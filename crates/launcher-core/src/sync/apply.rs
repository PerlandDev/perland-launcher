//! The file-system half of a sync: what is on disk, free space, the trash, and moving files.
// Task 15 (the apply driver) is the first caller; until then only the tests use these.
#![allow(dead_code)]
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{lockfile::Managed, plan::OnDisk};
use crate::{
    download::{hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    sources::{FileMode, PackFile},
};

/// Removed and replaced files stay recoverable this long (spec §3.2).
pub(crate) const TRASH_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Free space kept on top of what a sync needs.
pub(crate) const SPACE_MARGIN: u64 = 100 << 20;

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Delete trash folders (named by unix seconds) older than `max_age`. Best effort.
pub(crate) async fn purge_trash(trash: &Path, now: SystemTime, max_age: Duration) {
    let Ok(mut entries) = tokio::fs::read_dir(trash).await else {
        return;
    };
    let cutoff = unix_secs(now).saturating_sub(max_age.as_secs());
    while let Ok(Some(entry)) = entries.next_entry().await {
        let Ok(secs) = entry.file_name().to_string_lossy().parse::<u64>() else {
            continue;
        };
        if secs < cutoff {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

pub(crate) fn trash_folder(perland_dir: &Path, now: SystemTime) -> PathBuf {
    perland_dir.join("trash").join(unix_secs(now).to_string())
}

/// Cheap, with no hashing: every managed file still exists, and the files the pack owns outright
/// kept their size. Files the player may edit (preserved, kept overrides) only need to exist.
pub(crate) fn quick_check(game_dir: &Path, managed: &Managed) -> bool {
    managed.files.iter().all(
        |(path, entry)| match std::fs::metadata(game_dir.join(path)) {
            Ok(meta) => entry.mode != FileMode::Replace || meta.len() == entry.size,
            Err(_) => false,
        },
    )
}

/// Hash what's on disk at every path the pack wants or the lockfile names.
pub(crate) async fn survey(
    game_dir: &Path,
    wanted: &[&PackFile],
    managed: Option<&Managed>,
) -> Result<BTreeMap<String, OnDisk>> {
    let mut jobs: Vec<(String, Option<Hashes>)> = wanted
        .iter()
        .map(|f| (f.path.clone(), Some(f.hashes.clone())))
        .collect();
    let mut seen: HashSet<String> = jobs.iter().map(|(p, _)| p.clone()).collect();
    if let Some(m) = managed {
        for path in m.files.keys() {
            if seen.insert(path.clone()) {
                jobs.push((path.clone(), None));
            }
        }
    }
    let game_dir = game_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, OnDisk>> {
        let mut out = BTreeMap::new();
        for (path, hashes) in jobs {
            let full = game_dir.join(&path);
            let state = match std::fs::metadata(&full) {
                Err(_) => OnDisk::Missing,
                Ok(meta) if !meta.is_file() => OnDisk::Different {
                    sha1: String::new(),
                    size: 0,
                },
                Ok(meta) => {
                    let sha1 = sha1_file(&full)?;
                    let same = match &hashes {
                        None => false,
                        Some(h) => match &h.sha1 {
                            Some(expected) => expected.eq_ignore_ascii_case(&sha1),
                            None => h.verify_file(&full)?,
                        },
                    };
                    let size = meta.len();
                    if same {
                        OnDisk::Same { sha1, size }
                    } else {
                        OnDisk::Different { sha1, size }
                    }
                }
            };
            out.insert(path, state);
        }
        Ok(out)
    })
    .await
    .map_err(|e| LauncherError::from(std::io::Error::other(e)))?
}

pub(crate) fn ensure_space(needed: u64, free: u64) -> Result<()> {
    let needed = needed.saturating_add(SPACE_MARGIN);
    if needed <= free {
        return Ok(());
    }
    let mb = |bytes: u64| bytes.div_ceil(1 << 20);
    Err(LauncherError::disk(
        ErrorCode::NotEnoughSpace,
        format!(
            "Not enough free space: {} MB needed, {} MB free",
            mb(needed),
            mb(free)
        ),
    )
    .with_param("needed_mb", mb(needed))
    .with_param("free_mb", mb(free))
    .with_fix("Free up some space and try again"))
}

/// Free bytes on the volume holding `path`; when that can't be read, assume plenty: a full
/// disk still fails cleanly while writing.
pub(crate) fn free_space(path: &Path) -> u64 {
    fs4::available_space(path).unwrap_or(u64::MAX)
}

/// Move `game_dir/rel` to `trash_dir/rel`. A file that's already gone is fine.
pub(crate) async fn move_to_trash(game_dir: &Path, trash_dir: &Path, rel: &str) -> Result<()> {
    let from = game_dir.join(rel);
    if tokio::fs::symlink_metadata(&from).await.is_err() {
        return Ok(());
    }
    let to = trash_dir.join(rel);
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(&from, &to).await?;
    Ok(())
}

/// Move a staged file into place (same volume: the instance folder).
pub(crate) async fn place(staged: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::rename(staged, target).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::hash::{sha1_hex, sha256_hex},
        sources::{FileMode, FileSource, Side},
        sync::lockfile::ManagedFile,
        ErrorCode,
    };
    use std::collections::BTreeMap;

    fn pf(path: &str, hashes: Hashes) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.into(),
            source: FileSource::Remote { urls: vec![] },
            hashes,
            size: None,
            side: Side::Both,
            optional: None,
            mode: FileMode::Replace,
            manual: None,
        }
    }

    fn managed(files: &[(&str, FileMode, u64)]) -> Managed {
        Managed {
            version: Managed::VERSION,
            stamp: "s".into(),
            pack_version: "1".into(),
            choices: BTreeMap::new(),
            files: files
                .iter()
                .map(|(p, mode, size)| {
                    (
                        p.to_string(),
                        ManagedFile {
                            name: p.to_string(),
                            sha1: "x".into(),
                            size: *size,
                            mode: *mode,
                            declared: Hashes::sha1("x"),
                        },
                    )
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn old_trash_is_purged_and_recent_trash_kept() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(10 * 86_400);
        for name in ["86400", "860000", "notes"] {
            std::fs::create_dir_all(dir.path().join(name).join("mods")).unwrap();
        }
        purge_trash(dir.path(), now, TRASH_MAX_AGE).await;
        assert!(!dir.path().join("86400").exists(), "9 days old");
        assert!(dir.path().join("860000").exists(), "under a day old");
        assert!(dir.path().join("notes").exists(), "not ours to delete");
        purge_trash(&dir.path().join("missing"), now, TRASH_MAX_AGE).await;
        assert_eq!(
            trash_folder(Path::new("p"), now),
            Path::new("p").join("trash").join("864000")
        );
    }

    #[test]
    fn quick_check_needs_every_file_and_exact_sizes_for_pack_owned_ones() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mods")).unwrap();
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("mods/a.jar"), "12345").unwrap();
        std::fs::write(dir.path().join("config/c.toml"), "edited by the player").unwrap();
        let ok = managed(&[
            ("mods/a.jar", FileMode::Replace, 5),
            ("config/c.toml", FileMode::Preserve, 3),
        ]);
        assert!(quick_check(dir.path(), &ok));
        let resized = managed(&[("mods/a.jar", FileMode::Replace, 6)]);
        assert!(!quick_check(dir.path(), &resized));
        let missing = managed(&[("mods/b.jar", FileMode::Replace, 5)]);
        assert!(!quick_check(dir.path(), &missing));
    }

    #[tokio::test]
    async fn survey_hashes_what_is_there() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mods")).unwrap();
        std::fs::write(dir.path().join("mods/a.jar"), "hello").unwrap();
        std::fs::write(dir.path().join("mods/b.jar"), "jello").unwrap();
        std::fs::write(dir.path().join("mods/old.jar"), "old").unwrap();
        let a = pf("mods/a.jar", Hashes::sha1(sha1_hex(b"hello")));
        let b = pf(
            "mods/b.jar",
            Hashes {
                sha256: Some(sha256_hex(b"hello")),
                ..Hashes::default()
            },
        );
        let c = pf("mods/c.jar", Hashes::sha1(sha1_hex(b"c")));
        let lock = managed(&[("mods/old.jar", FileMode::Replace, 3)]);
        let found = survey(dir.path(), &[&a, &b, &c], Some(&lock))
            .await
            .unwrap();
        assert_eq!(
            found["mods/a.jar"],
            OnDisk::Same {
                sha1: sha1_hex(b"hello"),
                size: 5
            }
        );
        assert_eq!(
            found["mods/b.jar"],
            OnDisk::Different {
                sha1: sha1_hex(b"jello"),
                size: 5
            }
        );
        assert_eq!(found["mods/c.jar"], OnDisk::Missing);
        assert!(
            matches!(found["mods/old.jar"], OnDisk::Different { .. }),
            "present"
        );
    }

    #[test]
    fn free_space_is_checked_with_a_margin() {
        ensure_space(1 << 20, 200 << 20).unwrap();
        let err = ensure_space(1 << 30, 50 << 20).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotEnoughSpace);
        assert_eq!(err.params["needed_mb"], "1124");
        assert_eq!(err.params["free_mb"], "50");
        assert!(free_space(Path::new(".")) > 0);
    }

    #[tokio::test]
    async fn moves_create_folders_and_tolerate_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let trash = dir.path().join("trash/1");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/a.jar"), "a").unwrap();
        move_to_trash(&game, &trash, "mods/a.jar").await.unwrap();
        assert!(!game.join("mods/a.jar").exists());
        assert_eq!(
            std::fs::read_to_string(trash.join("mods/a.jar")).unwrap(),
            "a"
        );
        move_to_trash(&game, &trash, "mods/never.jar")
            .await
            .unwrap();

        let staged = dir.path().join("staging/config/x.toml");
        std::fs::create_dir_all(staged.parent().unwrap()).unwrap();
        std::fs::write(&staged, "x").unwrap();
        place(&staged, &game.join("config/x.toml")).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(game.join("config/x.toml")).unwrap(),
            "x"
        );
        assert!(!staged.exists());
    }
}
