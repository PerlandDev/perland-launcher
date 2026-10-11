//! Files a pack's author blocks other launchers from downloading (CurseForge, spec §3.4). The
//! player downloads them in the browser, and the launcher finds them by hash.
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

use super::ManualFile;
use crate::{download::hash::sha1_file, error::Result};

/// The largest file looked at.
pub const MAX_CANDIDATE: u64 = 512 << 20;

/// The player's Downloads folder, if the OS has one.
pub fn downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

/// Looks for the wanted files in a folder, again and again. It remembers each file's digests
/// by size and modified time, so a poll hashes only files that are new or changed.
#[derive(Debug, Default)]
pub struct Scanner {
    seen: HashMap<PathBuf, Seen>,
    /// How many hash computations ran (for tests).
    hashed: usize,
}

#[derive(Debug)]
struct Seen {
    len: u64,
    modified: Option<SystemTime>,
    /// Computed on first use; `Some("")` if the file couldn't be read.
    sha1: Option<String>,
    /// For wanted files without a sha1: did this file match, by the wanted file's path.
    verdicts: HashMap<String, bool>,
}

/// One scan with a fresh `Scanner`.
pub async fn scan(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
    Scanner::default().scan(dir, wanted).await
}

impl Scanner {
    /// Files directly in `dir` that are one of `wanted`, matched by hash. A file with the
    /// expected name is tried first, then any other `.jar` or `.zip`, because browsers rename
    /// duplicates (`mod (1).jar`).
    pub async fn scan(
        &mut self,
        dir: &Path,
        wanted: &[ManualFile],
    ) -> Result<Vec<(ManualFile, PathBuf)>> {
        let (dir, wanted) = (dir.to_path_buf(), wanted.to_vec());
        let mut state = std::mem::take(self);
        let (state, found) = tokio::task::spawn_blocking(move || {
            let found = state.scan_blocking(&dir, &wanted);
            (state, found)
        })
        .await
        .map_err(std::io::Error::other)?;
        *self = state;
        found
    }

    fn scan_blocking(
        &mut self,
        dir: &Path,
        wanted: &[ManualFile],
    ) -> Result<Vec<(ManualFile, PathBuf)>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut candidates: Vec<(String, PathBuf)> = Vec::new();
        let mut present = HashSet::new();
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if !meta.is_file() || meta.len() > MAX_CANDIDATE {
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if !(lower.ends_with(".jar") || lower.ends_with(".zip")) {
                continue;
            }
            let path = entry.path();
            let modified = meta.modified().ok();
            let unchanged = self
                .seen
                .get(&path)
                .is_some_and(|s| s.len == meta.len() && s.modified == modified);
            if !unchanged {
                self.seen.insert(
                    path.clone(),
                    Seen {
                        len: meta.len(),
                        modified,
                        sha1: None,
                        verdicts: HashMap::new(),
                    },
                );
            }
            present.insert(path.clone());
            candidates.push((name, path));
        }
        self.seen.retain(|path, _| present.contains(path));
        candidates.sort();
        let mut found = Vec::new();
        for file in wanted {
            if file.hashes.is_empty() {
                continue; // nothing to prove a match with
            }
            let mut ordered: Vec<&(String, PathBuf)> = candidates.iter().collect();
            ordered.sort_by_key(|(name, _)| *name != file.file_name);
            for (_, path) in ordered {
                if self.matches(path, file) {
                    found.push((file.clone(), path.clone()));
                    break;
                }
            }
        }
        Ok(found)
    }

    fn matches(&mut self, path: &Path, file: &ManualFile) -> bool {
        let hashed = &mut self.hashed;
        let Some(seen) = self.seen.get_mut(path) else {
            return false;
        };
        match &file.hashes.sha1 {
            Some(expected) => {
                if seen.sha1.is_none() {
                    seen.sha1 = Some(sha1_file(path).unwrap_or_default());
                    *hashed += 1;
                }
                seen.sha1
                    .as_deref()
                    .is_some_and(|s| !s.is_empty() && s.eq_ignore_ascii_case(expected))
            }
            None => *seen.verdicts.entry(file.path.clone()).or_insert_with(|| {
                *hashed += 1;
                file.hashes.verify_file(path).unwrap_or(false)
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::download::{
        hash::{curseforge_murmur2, sha1_hex},
        Hashes,
    };

    fn wanted(file_name: &str, hashes: Hashes) -> ManualFile {
        ManualFile {
            path: format!("mods/{file_name}"),
            name: file_name.into(),
            file_name: file_name.into(),
            page_url: "https://www.curseforge.com/projects/1".into(),
            hashes,
        }
    }

    #[tokio::test]
    async fn finds_renamed_downloads_by_hash() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("cozy (1).jar"), b"cozy").unwrap();
        std::fs::write(dir.path().join("other.zip"), b"other").unwrap();
        std::fs::write(dir.path().join("cozy.txt"), b"cozy").unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/paint.jar"), b"paint").unwrap();
        let cozy = wanted("cozy.jar", Hashes::sha1(sha1_hex(b"cozy")));
        let other = wanted(
            "o.jar",
            Hashes {
                murmur2: Some(curseforge_murmur2(b"other")),
                ..Hashes::default()
            },
        );
        let paint = wanted("paint.jar", Hashes::sha1(sha1_hex(b"paint")));
        let empty = wanted("nothing.jar", Hashes::default());
        let found = scan(dir.path(), &[cozy.clone(), other.clone(), paint, empty])
            .await
            .unwrap();
        assert_eq!(
            found,
            vec![
                (cozy, dir.path().join("cozy (1).jar")),
                (other, dir.path().join("other.zip"))
            ]
        );
        assert!(scan(&dir.path().join("missing"), &[])
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn a_scanner_hashes_each_file_once_until_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("other.jar"), b"other").unwrap();
        let by_sha1 = wanted("mod.jar", Hashes::sha1(sha1_hex(b"mod")));
        let by_murmur = wanted(
            "cf.jar",
            Hashes {
                murmur2: Some(curseforge_murmur2(b"cf")),
                ..Hashes::default()
            },
        );
        let want = [by_sha1.clone(), by_murmur.clone()];
        let mut scanner = Scanner::default();
        assert!(scanner.scan(dir.path(), &want).await.unwrap().is_empty());
        let first = scanner.hashed;
        assert_eq!(first, 2, "other.jar: one sha1, one murmur2 check");
        assert!(scanner.scan(dir.path(), &want).await.unwrap().is_empty());
        assert_eq!(scanner.hashed, first, "nothing new: nothing hashed");

        std::fs::write(dir.path().join("mod (1).jar"), b"mod").unwrap();
        std::fs::write(dir.path().join("cf.jar"), b"cf").unwrap();
        let found = scanner.scan(dir.path(), &want).await.unwrap();
        let mut names: Vec<_> = found.iter().map(|(f, _)| f.file_name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["cf.jar".to_string(), "mod.jar".to_string()]);

        // A changed file is hashed again; a vanished one is forgotten.
        std::fs::remove_file(dir.path().join("mod (1).jar")).unwrap();
        std::fs::remove_file(dir.path().join("cf.jar")).unwrap();
        let before = scanner.hashed;
        std::fs::write(dir.path().join("other.jar"), b"other, but longer now").unwrap();
        assert!(scanner.scan(dir.path(), &want).await.unwrap().is_empty());
        assert_eq!(scanner.hashed, before + 2);
        assert_eq!(scanner.seen.len(), 1);
    }
}
