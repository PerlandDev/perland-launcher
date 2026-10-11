//! What content an instance already has, by Modrinth project (spec §3.5). Files are named by
//! their sha1 through `POST /version_files`, so nothing Browse installs needs a record.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::api::{Api, DependencyType};
use crate::{
    download::hash::sha1_file,
    error::{LauncherError, Result},
    fsutil::path_key,
    sync::lockfile::Managed,
};

/// Content folders and the file endings Browse looks at in each.
const FOLDERS: [(&str, &[&str]); 3] = [
    ("mods", &[".jar", ".jar.disabled"]),
    ("resourcepacks", &[".zip"]),
    ("shaderpacks", &[".zip"]),
];

/// Remembers each file's sha1 by (path, size, modified time), so asking again only reads new
/// or changed files.
#[derive(Debug, Default)]
pub struct FileHashes {
    memo: HashMap<PathBuf, (u64, SystemTime, String)>,
    hashed: usize,
}

impl FileHashes {
    /// The file's sha1, or `None` when it is gone or isn't a file.
    pub async fn sha1(&mut self, path: &Path) -> Result<Option<String>> {
        let meta = match tokio::fs::metadata(path).await {
            Ok(meta) if meta.is_file() => meta,
            Ok(_) => return Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let modified = meta.modified()?;
        if let Some((size, when, sha1)) = self.memo.get(path) {
            if *size == meta.len() && *when == modified {
                return Ok(Some(sha1.clone()));
            }
        }
        let owned = path.to_path_buf();
        let sha1 = tokio::task::spawn_blocking(move || sha1_file(&owned))
            .await
            .map_err(|e| LauncherError::from(std::io::Error::other(e)))??;
        self.hashed += 1;
        self.memo
            .insert(path.to_path_buf(), (meta.len(), modified, sha1.clone()));
        Ok(Some(sha1))
    }

    /// How many files were actually read (not answered from memory).
    pub fn hashed(&self) -> usize {
        self.hashed
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledProject {
    /// Where it is, relative to the game folder.
    pub path: String,
    /// Sync owns the file (a pack's mod).
    pub managed: bool,
    /// Projects the installed version says it can't run with.
    pub incompatible: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Installed {
    pub projects: HashMap<String, InstalledProject>,
}

impl Installed {
    pub fn project_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.projects.keys().cloned().collect();
        ids.sort();
        ids
    }
}

/// Every content file in the game folder, as canonical relative paths, sorted.
pub async fn content_files(game_dir: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    for (folder, endings) in FOLDERS {
        let mut entries = match tokio::fs::read_dir(game_dir.join(folder)).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if endings.iter().any(|e| lower.ends_with(e)) {
                out.push(format!("{folder}/{name}"));
            }
        }
    }
    out.sort();
    Ok(out)
}

pub async fn identify(
    api: &Api,
    game_dir: &Path,
    managed: Option<&Managed>,
    hashes: &mut FileHashes,
) -> Result<Installed> {
    let from_lockfile: HashMap<String, String> = managed
        .map(|m| {
            m.files
                .iter()
                .filter(|(_, f)| !f.sha1.is_empty())
                .map(|(p, f)| (path_key(p), f.sha1.to_ascii_lowercase()))
                .collect()
        })
        .unwrap_or_default();
    let mut by_hash: HashMap<String, (String, bool)> = HashMap::new();
    for rel in content_files(game_dir).await? {
        let (sha1, is_managed) = match from_lockfile.get(&path_key(&rel)) {
            Some(sha1) => (sha1.clone(), true),
            None => match hashes.sha1(&game_dir.join(&rel)).await? {
                Some(sha1) => (sha1, false),
                None => continue,
            },
        };
        by_hash.entry(sha1).or_insert((rel, is_managed));
    }
    if by_hash.is_empty() {
        return Ok(Installed::default());
    }
    let mut asked: Vec<String> = by_hash.keys().cloned().collect();
    asked.sort();
    let versions = api.version_files(&asked).await?;
    let mut installed = Installed::default();
    for (sha1, version) in versions {
        let Some((path, managed)) = by_hash.get(&sha1.to_ascii_lowercase()) else {
            continue;
        };
        let incompatible = version
            .dependencies
            .iter()
            .filter(|d| d.dependency_type == DependencyType::Incompatible)
            .filter_map(|d| d.project_id.clone())
            .collect();
        installed
            .projects
            .entry(version.project_id.clone())
            .or_insert(InstalledProject {
                path: path.clone(),
                managed: *managed,
                incompatible,
            });
    }
    Ok(installed)
}
