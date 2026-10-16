//! Modrinth's `.mrpack`: a zip holding `modrinth.index.json` (files to download) plus
//! `overrides/` and `client-overrides/` copied into the game folder. Written from the published
//! format. Archive contents are untrusted: paths go through `normalize_rel`, and sizes are
//! capped and checked while reading, so a zip that lies about its sizes can't run away.
use std::{collections::BTreeMap, io::Read, path::Path};

use serde::Deserialize;
use sha1::{Digest, Sha1};

use super::{
    check_pack_url, game_from, invalid, unsafe_path, unsupported, DesiredState, FileMode,
    FileSource, OptionalFile, PackAbout, PackFile, Side, Sources,
};
use crate::{
    download::{hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    fsutil::normalize_rel,
    instances::SourceRef,
};

const INDEX: &str = "modrinth.index.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub files: Vec<MrFile>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MrFile {
    pub path: String,
    pub hashes: MrHashes,
    #[serde(default)]
    pub env: Option<MrEnv>,
    #[serde(default)]
    pub downloads: Vec<String>,
    #[serde(default)]
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrHashes {
    pub sha1: String,
    #[serde(default)]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MrEnv {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

/// Size caps for reading an archive, in bytes.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub index: u64,
    pub entry: u64,
    pub total: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            index: 16 << 20,
            entry: 1 << 30,
            total: 8 << 30,
        }
    }
}

pub fn parse_index(bytes: &[u8]) -> Result<MrIndex> {
    let index: MrIndex = serde_json::from_slice(bytes).map_err(|e| invalid(INDEX, e))?;
    if index.format_version != 1 {
        return Err(unsupported(
            "mrpack_format",
            format!("mrpack format {}", index.format_version),
        )
        .with_param("format", index.format_version));
    }
    if index.game != "minecraft" {
        return Err(
            unsupported("mrpack_game", format!("packs for {}", index.game))
                .with_param("game", &index.game),
        );
    }
    Ok(index)
}

fn env_of(env: Option<&MrEnv>) -> (Side, Option<OptionalFile>) {
    let Some(env) = env else {
        return (Side::Both, None);
    };
    let side = match (env.client.as_deref(), env.server.as_deref()) {
        (Some("unsupported"), _) => Side::Server,
        (_, Some("unsupported")) => Side::Client,
        _ => Side::Both,
    };
    let optional = (env.client.as_deref() == Some("optional")).then_some(OptionalFile {
        default: true,
        description: None,
    });
    (side, optional)
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

fn index_files(index: &MrIndex) -> Result<Vec<PackFile>> {
    index
        .files
        .iter()
        .map(|f| {
            let path = normalize_rel(&f.path).ok_or_else(|| unsafe_path(&f.path))?;
            let mut hashes = Hashes::from_format("sha1", &f.hashes.sha1)
                .ok_or_else(|| invalid(INDEX, format!("{path}: malformed sha1")))?;
            if let Some(sha512) = &f.hashes.sha512 {
                hashes.sha512 = Hashes::from_format("sha512", sha512)
                    .and_then(|h| h.sha512)
                    .ok_or_else(|| invalid(INDEX, format!("{path}: malformed sha512")))?
                    .into();
            }
            let urls: Vec<String> = f
                .downloads
                .iter()
                .filter(|u| check_pack_url(u).is_ok())
                .cloned()
                .collect();
            if urls.is_empty() {
                return Err(invalid(INDEX, format!("{path} has no https download")));
            }
            let (side, optional) = env_of(f.env.as_ref());
            Ok(PackFile {
                name: file_name(&path),
                path,
                source: FileSource::Remote { urls },
                hashes,
                size: f.file_size,
                side,
                optional,
                mode: FileMode::Replace,
                manual: None,
            })
        })
        .collect()
}

/// An entry of the archive is damaged (it won't inflate, or it changed), which is not a disk
/// problem.
fn corrupt(file: &str, detail: impl std::fmt::Display) -> LauncherError {
    LauncherError::pack(
        ErrorCode::MrpackCorrupt,
        format!("This .mrpack file is damaged ({file})"),
    )
    .with_param("file", file)
    .with_fix("Download the pack again")
    .with_detail(detail)
}

/// The whole archive is damaged, so there is no entry to name. The `context` param makes i18next
/// pick `errors.mrpack_corrupt.message_archive`, the message without a file name.
fn zip_error(e: zip::result::ZipError) -> LauncherError {
    LauncherError::pack(ErrorCode::MrpackCorrupt, "This .mrpack file is damaged")
        .with_param("context", "archive")
        .with_fix("Download the pack again")
        .with_detail(e)
}

/// The sha1 and length of everything `reader` yields.
fn hash_reader(mut reader: impl Read) -> std::io::Result<(String, u64)> {
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        total += n as u64;
    }
    Ok((hex::encode(hasher.finalize()), total))
}

/// Read a `.mrpack`: the files its index lists plus its override entries.
pub fn read_archive(
    path: &Path,
    stamp: String,
    archive: Hashes,
    limits: Limits,
) -> Result<DesiredState> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path)?).map_err(zip_error)?;
    let index = {
        let entry = zip.by_name(INDEX).map_err(|e| invalid(INDEX, e))?;
        let mut bytes = Vec::new();
        entry
            .take(limits.index + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| corrupt(INDEX, e))?;
        if bytes.len() as u64 > limits.index {
            return Err(invalid(INDEX, "too large"));
        }
        parse_index(&bytes)?
    };
    let mut files: BTreeMap<String, PackFile> = index_files(&index)?
        .into_iter()
        .map(|f| (f.path.clone(), f))
        .collect();
    let mut total = 0u64;
    for prefix in ["overrides/", "client-overrides/"] {
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).map_err(zip_error)?;
            if entry.is_dir() {
                continue;
            }
            let name = entry.name().to_string();
            let Some(rel) = name.strip_prefix(prefix) else {
                continue;
            };
            let path = normalize_rel(rel).ok_or_else(|| unsafe_path(rel))?;
            let declared = entry.size();
            total = total.saturating_add(declared);
            if declared > limits.entry || total > limits.total {
                return Err(invalid(&path, "larger than a pack file may be"));
            }
            let (sha1, size) =
                hash_reader((&mut entry).take(declared + 1)).map_err(|e| corrupt(&path, e))?;
            if size != declared {
                return Err(invalid(&path, "its size doesn't match the archive"));
            }
            let mode = if path.starts_with("mods/") {
                FileMode::Replace
            } else {
                FileMode::ReplaceUnlessModified
            };
            files.insert(
                path.clone(),
                PackFile {
                    name: file_name(&path),
                    path,
                    source: FileSource::Archive { entry: name },
                    hashes: Hashes::sha1(sha1),
                    size: Some(size),
                    side: Side::Both,
                    optional: None,
                    mode,
                    manual: None,
                },
            );
        }
    }
    let (minecraft, loader) = game_from(&index.dependencies)?;
    let desired = DesiredState {
        name: index.name.trim().chars().take(64).collect(),
        pack_version: index.version_id,
        minecraft,
        loader,
        files: files.into_values().collect(),
        stamp,
        about: PackAbout {
            description: index.summary,
            ..PackAbout::default()
        },
        archive: Some(archive),
    };
    desired.check_paths()?;
    Ok(desired)
}

/// Copy one archive entry to `dest`, checking its sha1 on the way (the cached archive could
/// have been damaged since it was read).
pub fn extract_entry(archive: &Path, entry: &str, dest: &Path, sha1: &str) -> Result<()> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive)?).map_err(zip_error)?;
    let mut source = zip.by_name(entry).map_err(|e| invalid(entry, e))?;
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut out = std::fs::File::create(dest)?;
    let mut hasher = Sha1::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = source.read(&mut buf).map_err(|e| corrupt(entry, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        std::io::Write::write_all(&mut out, &buf[..n])?;
    }
    drop(out);
    if !hex::encode(hasher.finalize()).eq_ignore_ascii_case(sha1) {
        let _ = std::fs::remove_file(dest);
        return Err(corrupt(entry, "it changed since the pack was read"));
    }
    Ok(())
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

/// A `.mrpack` the player picked: keep a copy in the object cache and read it.
pub async fn import_file(src: &Sources, path: &Path) -> Result<(SourceRef, DesiredState)> {
    let p = path.to_path_buf();
    let sha1 = tokio::task::spawn_blocking(move || sha1_file(&p))
        .await
        .map_err(join_error)??;
    src.cache.insert_file(path, &Hashes::sha1(&sha1)).await?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "pack.mrpack".into());
    let desired = resolve_cached(src, &sha1).await?;
    Ok((SourceRef::Mrpack { name, sha1 }, desired))
}

/// Read an imported `.mrpack` again from the object cache.
pub async fn resolve_cached(src: &Sources, sha1: &str) -> Result<DesiredState> {
    let hashes = Hashes::sha1(sha1);
    let path = src.cache.get(&hashes).await.ok_or_else(|| {
        LauncherError::pack(ErrorCode::PackNotFound, "The imported pack file is gone")
            .with_fix("Import the .mrpack file again")
    })?;
    let stamp = sha1.to_string();
    tokio::task::spawn_blocking(move || read_archive(&path, stamp, hashes, Limits::default()))
        .await
        .map_err(join_error)?
}
