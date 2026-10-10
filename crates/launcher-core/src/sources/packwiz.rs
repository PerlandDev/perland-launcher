//! packwiz packs: `pack.toml` → `index.toml` → one `.pw.toml` metafile per mod. Written from
//! packwiz's published format. Everything here is pure; fetching is in `resolve` (Task 9).
use std::collections::BTreeMap;

use futures_util::{StreamExt, TryStreamExt};
use serde::{de::DeserializeOwned, Deserialize};

use super::{
    check_pack_url, curseforge, encode_path, game_from, invalid, sidecar::parse_sidecar,
    unsafe_path, unsupported, DesiredState, FileMode, FileSource, ManualDownload, OptionalFile,
    PackAbout, PackFile, Side, SourceConfig, Sources, UpdateCheck,
};
use crate::{
    download::{hash::sha256_hex, host_of, Hashes},
    error::{ErrorCode, LauncherError, Result},
    fsutil::{normalize_rel, safe_component},
    instances::SourceRef,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct PackToml {
    pub name: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub pack_format: Option<String>,
    pub index: IndexRef,
    #[serde(default)]
    pub versions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexRef {
    pub file: String,
    pub hash_format: String,
    pub hash: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexToml {
    pub hash_format: String,
    #[serde(default)]
    pub files: Vec<IndexEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct IndexEntry {
    pub file: String,
    pub hash: String,
    #[serde(default)]
    pub hash_format: Option<String>,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub metafile: bool,
    #[serde(default)]
    pub preserve: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MetaToml {
    pub name: String,
    pub filename: String,
    #[serde(default)]
    pub side: Option<String>,
    pub download: MetaDownload,
    #[serde(default)]
    pub update: Option<MetaUpdate>,
    #[serde(default)]
    pub option: Option<MetaOption>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MetaDownload {
    #[serde(default)]
    pub url: Option<String>,
    pub hash_format: String,
    pub hash: String,
    #[serde(default)]
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetaUpdate {
    #[serde(default)]
    pub curseforge: Option<CfUpdate>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CfUpdate {
    pub file_id: u64,
    pub project_id: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetaOption {
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub description: Option<String>,
}

fn toml_from<T: DeserializeOwned>(bytes: &[u8], file: &str) -> Result<T> {
    let text = std::str::from_utf8(bytes).map_err(|e| invalid(file, e))?;
    toml::from_str(text).map_err(|e| invalid(file, e))
}

pub fn parse_pack(bytes: &[u8]) -> Result<PackToml> {
    let pack: PackToml = toml_from(bytes, "pack.toml")?;
    if let Some(format) = &pack.pack_format {
        if !format.starts_with("packwiz:1.") {
            return Err(unsupported(&format!("pack format {format}")));
        }
    }
    Ok(pack)
}

pub fn parse_index(bytes: &[u8]) -> Result<IndexToml> {
    toml_from(bytes, "index.toml")
}

pub fn parse_meta(bytes: &[u8], file: &str) -> Result<MetaToml> {
    toml_from(bytes, file)
}

/// A pack's hash, checked: an unknown format can't be verified, and a malformed value is damage.
pub fn entry_hashes(format: &str, value: &str, file: &str) -> Result<Hashes> {
    if !Hashes::is_known_format(format) {
        return Err(unsupported(&format!("{format} checksums")));
    }
    Hashes::from_format(format, value).ok_or_else(|| invalid(file, "malformed checksum"))
}

fn side(value: Option<&str>) -> Side {
    match value {
        Some("client") => Side::Client,
        Some("server") => Side::Server,
        _ => Side::Both,
    }
}

/// Pack files download over https (plain http only from this computer, for tests), like packs.
fn check_file_url(url: &str, file: &str) -> Result<String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| invalid(file, e))?;
    if check_pack_url(url).is_ok() {
        Ok(parsed.to_string())
    } else {
        Err(unsupported(&format!("{} downloads", parsed.scheme())))
    }
}

/// The file a metafile installs: `filename` in the metafile's own folder.
pub fn meta_file(
    cfg: &SourceConfig,
    meta_path: &str,
    meta: &MetaToml,
    preserve: bool,
) -> Result<PackFile> {
    if !safe_component(&meta.filename) {
        return Err(unsafe_path(&meta.filename));
    }
    let joined = match meta_path.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{}", meta.filename),
        None => meta.filename.clone(),
    };
    let path = normalize_rel(&joined).ok_or_else(|| unsafe_path(&joined))?;
    let hashes = entry_hashes(&meta.download.hash_format, &meta.download.hash, meta_path)?;
    let (urls, manual) = match meta.download.mode.as_deref().unwrap_or("url") {
        "" | "url" => {
            let url = meta
                .download
                .url
                .as_deref()
                .ok_or_else(|| invalid(meta_path, "no download url"))?;
            (vec![check_file_url(url, meta_path)?], None)
        }
        "metadata:curseforge" => {
            let cf = meta
                .update
                .as_ref()
                .and_then(|u| u.curseforge.as_ref())
                .ok_or_else(|| invalid(meta_path, "no [update.curseforge]"))?;
            (
                vec![curseforge::cdn_url(
                    &cfg.curseforge_cdn,
                    cf.file_id,
                    &meta.filename,
                )],
                Some(ManualDownload {
                    page_url: curseforge::page_url(&cfg.curseforge_site, cf.project_id),
                    file_name: meta.filename.clone(),
                }),
            )
        }
        other => return Err(unsupported(&format!("download mode {other}"))),
    };
    Ok(PackFile {
        path,
        name: meta.name.clone(),
        source: FileSource::Remote { urls },
        hashes,
        size: None,
        side: side(meta.side.as_deref()),
        optional: meta
            .option
            .as_ref()
            .filter(|o| o.optional)
            .map(|o| OptionalFile {
                default: o.default,
                description: o.description.clone(),
            }),
        mode: if preserve {
            FileMode::Preserve
        } else {
            FileMode::Replace
        },
        manual,
    })
}

/// A file served next to `index.toml` (configs, scripts), installed at its `alias` if it has one.
pub fn plain_file(
    index_url: &reqwest::Url,
    entry: &IndexEntry,
    default_format: &str,
) -> Result<PackFile> {
    let file = normalize_rel(&entry.file).ok_or_else(|| unsafe_path(&entry.file))?;
    let target = match &entry.alias {
        Some(alias) => normalize_rel(alias).ok_or_else(|| unsafe_path(alias))?,
        None => file.clone(),
    };
    let url = index_url
        .join(&encode_path(&file))
        .map_err(|e| invalid(&file, e))?;
    let format = entry.hash_format.as_deref().unwrap_or(default_format);
    let hashes = entry_hashes(format, &entry.hash, &file)?;
    let name = target.rsplit('/').next().unwrap_or(&target).to_string();
    Ok(PackFile {
        path: target,
        name,
        source: FileSource::Remote {
            urls: vec![url.to_string()],
        },
        hashes,
        size: None,
        side: Side::Both,
        optional: None,
        mode: if entry.preserve {
            FileMode::Preserve
        } else {
            FileMode::Replace
        },
        manual: None,
    })
}

/// How many metafiles are fetched at once.
const METAFILE_CONCURRENCY: usize = 8;

fn pack_not_found(e: LauncherError) -> LauncherError {
    if e.code == ErrorCode::MetaNotFound {
        LauncherError::pack(ErrorCode::PackNotFound, "That pack wasn't found")
            .with_fix("Check the link and try again")
            .with_detail(e.message)
    } else {
        e
    }
}

/// The pack as it is now. Every byte goes through `MetaClient`: `pack.toml` is revalidated with
/// its ETag, and `index.toml` and the metafiles are checked against their hashes, so unchanged
/// metafiles are never downloaded again and an offline resolve works from the cache.
pub async fn resolve(src: &Sources, pack_url: &str) -> Result<DesiredState> {
    let url = check_pack_url(pack_url)?;
    let pack_bytes = src
        .meta
        .get_bytes_validated(url.as_str(), |b| parse_pack(b).map(|_| ()))
        .await
        .map_err(pack_not_found)?;
    let pack = parse_pack(&pack_bytes)?;

    let index_file =
        normalize_rel(&pack.index.file).ok_or_else(|| unsafe_path(&pack.index.file))?;
    let index_url = url
        .join(&encode_path(&index_file))
        .map_err(|e| invalid("pack.toml", e))?;
    let index_hashes = entry_hashes(&pack.index.hash_format, &pack.index.hash, "pack.toml")?;
    let index_bytes = src
        .meta
        .get_verified_bytes(index_url.as_str(), &index_hashes)
        .await?;
    let index = parse_index(&index_bytes)?;

    let mut files: Vec<PackFile> = futures_util::stream::iter(
        index
            .files
            .iter()
            .map(|entry| index_entry(src, &index_url, entry, &index.hash_format)),
    )
    .buffered(METAFILE_CONCURRENCY)
    .try_collect()
    .await?;
    files.sort_by(|a, b| a.path.cmp(&b.path));

    let (minecraft, loader) = game_from(&pack.versions)?;
    let name = match pack.name.trim() {
        "" => "Pack".to_string(),
        n => n.chars().take(64).collect(),
    };
    let desired = DesiredState {
        name,
        pack_version: pack.version.unwrap_or_default(),
        minecraft,
        loader,
        files,
        stamp: sha256_hex(&pack_bytes),
        about: fetch_sidecar(src, &url).await,
        archive: None,
    };
    desired.check_paths()?;
    Ok(desired)
}

async fn index_entry(
    src: &Sources,
    index_url: &reqwest::Url,
    entry: &IndexEntry,
    default_format: &str,
) -> Result<PackFile> {
    if !entry.metafile {
        return plain_file(index_url, entry, default_format);
    }
    let file = normalize_rel(&entry.file).ok_or_else(|| unsafe_path(&entry.file))?;
    let format = entry.hash_format.as_deref().unwrap_or(default_format);
    let hashes = entry_hashes(format, &entry.hash, &file)?;
    let url = index_url
        .join(&encode_path(&file))
        .map_err(|e| invalid(&file, e))?;
    let bytes = src.meta.get_verified_bytes(url.as_str(), &hashes).await?;
    let meta = parse_meta(&bytes, &file)?;
    meta_file(&src.config, &file, &meta, entry.preserve)
}

/// `launcher.toml` next to `pack.toml`; anything other than a readable file means "none".
async fn fetch_sidecar(src: &Sources, pack_url: &reqwest::Url) -> PackAbout {
    let Ok(url) = pack_url.join("launcher.toml") else {
        return PackAbout::default();
    };
    match src.meta.get_bytes(url.as_str()).await {
        Ok(bytes) => std::str::from_utf8(&bytes)
            .map(|text| parse_sidecar(text, pack_url.as_str()))
            .unwrap_or_default(),
        Err(_) => PackAbout::default(),
    }
}

/// The cheap check (spec §3.1): fetch `pack.toml` fresh and compare its sha256. Never fails:
/// problems come back as `Unavailable` so the player can still play what they have.
pub async fn check(src: &Sources, pack_url: &str, installed_stamp: &str) -> UpdateCheck {
    let unavailable = |detail: String| {
        UpdateCheck::Unavailable(
            LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
                .with_fix("You can still play the version you have")
                .with_detail(detail),
        )
    };
    let url = match check_pack_url(pack_url) {
        Ok(url) => url,
        Err(e) => return UpdateCheck::Unavailable(e),
    };
    let response = match src.http.get(url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => return unavailable(e.without_url().to_string()),
    };
    if !response.status().is_success() {
        return unavailable(format!(
            "{} answered {}",
            host_of(url.as_str()),
            response.status()
        ));
    }
    let bytes = match response.bytes().await {
        Ok(b) => b,
        Err(e) => return unavailable(e.without_url().to_string()),
    };
    let pack = match parse_pack(&bytes) {
        Ok(p) => p,
        Err(e) => return UpdateCheck::Unavailable(e),
    };
    let stamp = sha256_hex(&bytes);
    if stamp == installed_stamp {
        UpdateCheck::UpToDate
    } else {
        UpdateCheck::Available {
            source: SourceRef::Packwiz {
                url: pack_url.to_string(),
            },
            stamp,
            version: pack.version.unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        sources::{FileMode, FileSource, Side},
        ErrorCode,
    };

    const PACK: &str = r#"name = "Asalith Fields"
author = "PerlandDev"
version = "1.0.3"
pack-format = "packwiz:1.1.0"

[index]
file = "index.toml"
hash-format = "sha256"
hash = "30a9c85d91a505ab9c8c6ca9380f705dcc91f74c80e58e16a3d5e244f58226ee"

[versions]
forge = "47.4.22"
minecraft = "1.20.1"

[options]
acceptable-game-versions = ["1.20.1"]
"#;

    const APPLESKIN: &str = r#"name = "AppleSkin"
filename = "appleskin-forge-mc1.20.1-2.5.1.jar"
side = "both"

[download]
url = "https://cdn.modrinth.com/data/EsAfCjCV/versions/XdXDExVF/appleskin-forge-mc1.20.1-2.5.1.jar"
hash-format = "sha512"
hash = "3176f928fb6c60d27f5edb610ca4ab0236175404eebf32797a1ec6cdd794bb294a260231579578536d03571377e43c6181adc75d8dd2c0733f17fce4f7e636b9"

[update]
[update.modrinth]
mod-id = "EsAfCjCV"
version = "XdXDExVF"
"#;

    const COZY: &str = r#"name = "Lucky's Cozy Home: Stone & Waterworks Update"
filename = "cozy_home-3.0.4-forge-1.20.1.jar"
side = "both"

[download]
hash-format = "sha1"
hash = "9b5c0143e9dca96ddcf999f94c8aa0a30495f49f"
mode = "metadata:curseforge"

[update]
[update.curseforge]
file-id = 5512563
project-id = 825733
"#;

    fn cfg() -> SourceConfig {
        SourceConfig::default()
    }

    #[test]
    fn reads_the_asalith_pack_toml() {
        let pack = parse_pack(PACK.as_bytes()).unwrap();
        assert_eq!(pack.name, "Asalith Fields");
        assert_eq!(pack.version.as_deref(), Some("1.0.3"));
        assert_eq!(pack.index.file, "index.toml");
        assert_eq!(pack.index.hash_format, "sha256");
        assert_eq!(pack.versions["forge"], "47.4.22");
    }

    #[test]
    fn pack_format_and_syntax_are_checked() {
        let future = PACK.replace("packwiz:1.1.0", "packwiz:2.0.0");
        assert_eq!(
            parse_pack(future.as_bytes()).unwrap_err().code,
            ErrorCode::PackUnsupported
        );
        let err = parse_pack(b"name = [").unwrap_err();
        assert_eq!(err.code, ErrorCode::PackInvalid);
        assert_eq!(err.params["file"], "pack.toml");
        assert_eq!(
            parse_pack(&[0xff, 0xfe]).unwrap_err().code,
            ErrorCode::PackInvalid
        );
    }

    #[test]
    fn index_entries_keep_their_flags() {
        let index = parse_index(
            b"hash-format = \"sha256\"\n\n[[files]]\nfile = \"mods/a.pw.toml\"\nhash = \"00\"\nmetafile = true\n\n\
              [[files]]\nfile = \"config/x.toml\"\nhash = \"11\"\nhash-format = \"sha1\"\npreserve = true\nalias = \"config/y.toml\"\n",
        )
        .unwrap();
        assert_eq!(index.hash_format, "sha256");
        assert!(index.files[0].metafile && !index.files[0].preserve);
        assert!(index.files[1].preserve);
        assert_eq!(index.files[1].hash_format.as_deref(), Some("sha1"));
        assert_eq!(index.files[1].alias.as_deref(), Some("config/y.toml"));
    }

    #[test]
    fn a_modrinth_metafile_installs_next_to_itself() {
        let meta = parse_meta(APPLESKIN.as_bytes(), "mods/appleskin.pw.toml").unwrap();
        let f = meta_file(&cfg(), "mods/appleskin.pw.toml", &meta, false).unwrap();
        assert_eq!(f.path, "mods/appleskin-forge-mc1.20.1-2.5.1.jar");
        assert_eq!(f.name, "AppleSkin");
        assert_eq!(
            f.source,
            FileSource::Remote {
                urls: vec!["https://cdn.modrinth.com/data/EsAfCjCV/versions/XdXDExVF/appleskin-forge-mc1.20.1-2.5.1.jar".into()]
            }
        );
        assert!(f.hashes.sha512.is_some());
        assert_eq!(
            (f.side, f.mode, f.optional.clone(), f.manual.clone()),
            (Side::Both, FileMode::Replace, None, None)
        );
    }

    #[test]
    fn a_curseforge_metafile_uses_the_cdn_and_names_its_page() {
        let meta = parse_meta(COZY.as_bytes(), "mods/cozy-home.pw.toml").unwrap();
        let f = meta_file(&cfg(), "mods/cozy-home.pw.toml", &meta, false).unwrap();
        assert_eq!(
            f.source,
            FileSource::Remote {
                urls: vec!["https://mediafilez.forgecdn.net/files/5512/563/cozy_home-3.0.4-forge-1.20.1.jar".into()]
            }
        );
        let manual = f.manual.unwrap();
        assert_eq!(
            manual.page_url,
            "https://www.curseforge.com/projects/825733"
        );
        assert_eq!(manual.file_name, "cozy_home-3.0.4-forge-1.20.1.jar");
        assert_eq!(
            f.hashes.sha1.as_deref(),
            Some("9b5c0143e9dca96ddcf999f94c8aa0a30495f49f")
        );
    }

    #[test]
    fn metafile_sides_options_and_preserve() {
        let text = APPLESKIN.replace("side = \"both\"", "side = \"client\"")
            + "\n[option]\noptional = true\ndefault = false\ndescription = \"Shows food values\"\n";
        let meta = parse_meta(text.as_bytes(), "mods/a.pw.toml").unwrap();
        let f = meta_file(&cfg(), "mods/a.pw.toml", &meta, true).unwrap();
        assert_eq!(f.side, Side::Client);
        assert_eq!(f.mode, FileMode::Preserve);
        let opt = f.optional.unwrap();
        assert!(!opt.default);
        assert_eq!(opt.description.as_deref(), Some("Shows food values"));
        let server = APPLESKIN.replace("side = \"both\"", "side = \"server\"");
        let meta = parse_meta(server.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(
            meta_file(&cfg(), "mods/a.pw.toml", &meta, false)
                .unwrap()
                .side,
            Side::Server
        );
    }

    #[test]
    fn hostile_metafiles_are_refused() {
        for name in ["../evil.jar", "CON.jar", "a/b.jar", "x.jar:stream"] {
            let text = APPLESKIN.replace(
                "appleskin-forge-mc1.20.1-2.5.1.jar\"\nside",
                &format!("{name}\"\nside"),
            );
            let meta = parse_meta(text.as_bytes(), "mods/a.pw.toml").unwrap();
            assert_eq!(
                meta_file(&cfg(), "mods/a.pw.toml", &meta, false)
                    .unwrap_err()
                    .code,
                ErrorCode::PackUnsafePath,
                "{name}"
            );
        }
        let mode = COZY.replace("metadata:curseforge", "metadata:ftb");
        let meta = parse_meta(mode.as_bytes(), "mods/c.pw.toml").unwrap();
        assert_eq!(
            meta_file(&cfg(), "mods/c.pw.toml", &meta, false)
                .unwrap_err()
                .code,
            ErrorCode::PackUnsupported
        );
        let crc = APPLESKIN.replace("hash-format = \"sha512\"", "hash-format = \"crc32\"");
        let meta = parse_meta(crc.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(
            meta_file(&cfg(), "mods/a.pw.toml", &meta, false)
                .unwrap_err()
                .code,
            ErrorCode::PackUnsupported
        );
        let bad = APPLESKIN.replace("hash = \"3176", "hash = \"zz76");
        let meta = parse_meta(bad.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(
            meta_file(&cfg(), "mods/a.pw.toml", &meta, false)
                .unwrap_err()
                .code,
            ErrorCode::PackInvalid
        );
        let ftp = APPLESKIN.replace("url = \"https://", "url = \"ftp://");
        let meta = parse_meta(ftp.as_bytes(), "mods/a.pw.toml").unwrap();
        assert_eq!(
            meta_file(&cfg(), "mods/a.pw.toml", &meta, false)
                .unwrap_err()
                .code,
            ErrorCode::PackUnsupported
        );
    }

    #[test]
    fn plain_files_resolve_next_to_the_index() {
        let index_url = reqwest::Url::parse("https://example.com/pack/index.toml").unwrap();
        let entry = IndexEntry {
            file: "config/a b#1.toml".into(),
            hash: crate::download::hash::sha1_hex(b"x"),
            hash_format: Some("sha1".into()),
            alias: None,
            metafile: false,
            preserve: true,
        };
        let f = plain_file(&index_url, &entry, "sha256").unwrap();
        assert_eq!(f.path, "config/a b#1.toml");
        assert_eq!(f.name, "a b#1.toml");
        assert_eq!(f.mode, FileMode::Preserve);
        assert_eq!(
            f.source,
            FileSource::Remote {
                urls: vec!["https://example.com/pack/config/a%20b%231.toml".into()]
            }
        );
        let aliased = IndexEntry {
            alias: Some("config/real.toml".into()),
            ..entry.clone()
        };
        assert_eq!(
            plain_file(&index_url, &aliased, "sha256").unwrap().path,
            "config/real.toml"
        );
        let escaping = IndexEntry {
            file: "../../etc/passwd".into(),
            ..entry
        };
        assert_eq!(
            plain_file(&index_url, &escaping, "sha256")
                .unwrap_err()
                .code,
            ErrorCode::PackUnsafePath
        );
    }
}
