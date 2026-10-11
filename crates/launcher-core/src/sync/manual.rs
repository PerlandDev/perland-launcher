//! Files a pack's author blocks other launchers from downloading (CurseForge, spec §3.4). The
//! player downloads them in the browser, and the launcher finds them by hash.
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use super::ManualFile;
use crate::{download::hash::sha1_file, error::Result};

/// The largest file looked at.
pub const MAX_CANDIDATE: u64 = 512 << 20;

/// The player's Downloads folder, if the OS has one.
pub fn downloads_dir() -> Option<PathBuf> {
    dirs::download_dir()
}

/// Files directly in `dir` that are one of `wanted`, matched by hash. A file with the expected
/// name is tried first, then any other `.jar` or `.zip`, because browsers rename duplicates
/// (`mod (1).jar`).
pub async fn scan(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
    let (dir, wanted) = (dir.to_path_buf(), wanted.to_vec());
    tokio::task::spawn_blocking(move || scan_blocking(&dir, &wanted))
        .await
        .map_err(std::io::Error::other)?
}

fn scan_blocking(dir: &Path, wanted: &[ManualFile]) -> Result<Vec<(ManualFile, PathBuf)>> {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut candidates: Vec<(String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || meta.len() > MAX_CANDIDATE {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".jar") || lower.ends_with(".zip") {
            candidates.push((name, entry.path()));
        }
    }
    candidates.sort();
    let mut sha1s: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut found = Vec::new();
    for file in wanted {
        if file.hashes.is_empty() {
            continue; // nothing to prove a match with
        }
        let mut ordered: Vec<&(String, PathBuf)> = candidates.iter().collect();
        ordered.sort_by_key(|(name, _)| *name != file.file_name);
        for (_, path) in ordered {
            let matches = match &file.hashes.sha1 {
                Some(expected) => sha1s
                    .entry(path.clone())
                    .or_insert_with(|| sha1_file(path).ok())
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(expected)),
                None => file.hashes.verify_file(path).unwrap_or(false),
            };
            if matches {
                found.push((file.clone(), path.clone()));
                break;
            }
        }
    }
    Ok(found)
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
}
