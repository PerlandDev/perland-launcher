//! The file-system half of a sync: what is on disk, free space, the trash, and moving files.
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use super::{lockfile::Managed, plan::OnDisk};
use crate::{
    download::{hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    fsutil::path_key,
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

fn path_is_folder(path: &str) -> LauncherError {
    let shown: String = path.chars().take(120).collect();
    LauncherError::pack(
        ErrorCode::PackPathIsFolder,
        format!("The pack wants to put a file where you have a folder ({shown})"),
    )
    .with_param("path", shown)
    .with_fix("Move or rename that folder, then try again")
}

/// Told `(files checked, files to check)` as a survey goes: `0` before the first file, then once
/// per file.
pub(crate) type SurveyProgress = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Hash what's on disk at every path the pack wants or the lockfile names. A lockfile path that
/// differs from a wanted one only in letter case is left out: on a case-insensitive file system
/// it is the same file. A folder where the pack wants a file stops the sync before anything
/// moves; a folder at a path only the lockfile names is the player's, so it is left out too.
pub(crate) async fn survey(
    game_dir: &Path,
    wanted: &[&PackFile],
    managed: Option<&Managed>,
    progress: Option<SurveyProgress>,
) -> Result<BTreeMap<String, OnDisk>> {
    let mut jobs: Vec<(String, Option<Hashes>)> = wanted
        .iter()
        .map(|f| (f.path.clone(), Some(f.hashes.clone())))
        .collect();
    let mut seen: HashSet<String> = jobs.iter().map(|(p, _)| path_key(p)).collect();
    if let Some(m) = managed {
        for path in m.files.keys() {
            if seen.insert(path_key(path)) {
                jobs.push((path.clone(), None));
            }
        }
    }
    let game_dir = game_dir.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, OnDisk>> {
        let mut out = BTreeMap::new();
        let total = jobs.len() as u64;
        let report = |done: u64| {
            if let (Some(progress), true) = (&progress, total > 0) {
                progress(done, total);
            }
        };
        report(0);
        for (checked, (path, hashes)) in jobs.into_iter().enumerate() {
            let full = game_dir.join(&path);
            let state = match std::fs::metadata(&full) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(OnDisk::Missing),
                Err(e) => return Err(e.into()),
                Ok(meta) if !meta.is_file() => match hashes {
                    Some(_) => return Err(path_is_folder(&path)),
                    None => None,
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
                    Some(if same {
                        OnDisk::Same { sha1, size }
                    } else {
                        OnDisk::Different { sha1, size }
                    })
                }
            };
            if let Some(state) = state {
                out.insert(path, state);
            }
            report(checked as u64 + 1);
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

/// The jars directly in `game_dir/mods/` that no wanted file sits at (compared the way the file
/// system does), sorted: what a linked instance's first sync sets aside. Folders, `.disabled`
/// jars and other files are left alone; the loaders only load loose jars.
pub(crate) async fn unwanted_jars(game_dir: &Path, wanted: &[&PackFile]) -> Result<Vec<String>> {
    let Ok(mut entries) = tokio::fs::read_dir(game_dir.join("mods")).await else {
        return Ok(Vec::new());
    };
    let wanted: HashSet<String> = wanted.iter().map(|f| path_key(&f.path)).collect();
    let mut found = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let rel = format!("mods/{name}");
        let is_file = entry.file_type().await.is_ok_and(|t| t.is_file());
        if is_file && name.to_lowercase().ends_with(".jar") && !wanted.contains(&path_key(&rel)) {
            found.push(rel);
        }
    }
    found.sort();
    Ok(found)
}

/// Move `game_dir/rel` into `game_dir/<SET_ASIDE_DIR>/`, never over a file already there: a
/// taken name gets `-2`, `-3`, … before its extension.
pub(crate) async fn move_aside(game_dir: &Path, rel: &str) -> Result<()> {
    let from = game_dir.join(rel);
    let Some(name) = Path::new(rel).file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };
    let dir = game_dir.join(super::SET_ASIDE_DIR);
    tokio::fs::create_dir_all(&dir).await?;
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    let mut to = dir.join(name);
    let mut n = 2;
    while tokio::fs::symlink_metadata(&to).await.is_ok() {
        to = dir.join(format!("{stem}-{n}{ext}"));
        n += 1;
    }
    tokio::fs::rename(&from, &to).await?;
    Ok(())
}

/// Is `game_dir/alias` a file of its own, apart from `game_dir/wanted`? Not when it is gone,
/// and not when both names lead to one file (a case-insensitive file system).
pub(crate) fn is_separate(game_dir: &Path, alias: &str, wanted: &str) -> bool {
    let alias = game_dir.join(alias);
    if !alias.is_file() {
        return false;
    }
    match (
        std::fs::canonicalize(&alias),
        std::fs::canonicalize(game_dir.join(wanted)),
    ) {
        (Ok(a), Ok(w)) => a != w,
        _ => false,
    }
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
        let found = survey(dir.path(), &[&a, &b, &c], Some(&lock), None)
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

    #[tokio::test]
    async fn survey_reports_each_file_it_checks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.jar"), "a").unwrap();
        let a = pf("a.jar", Hashes::sha1(sha1_hex(b"a")));
        let b = pf("b.jar", Hashes::sha1(sha1_hex(b"b")));
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let log = seen.clone();
        let progress: SurveyProgress =
            Arc::new(move |done, total| log.lock().unwrap().push((done, total)));
        survey(dir.path(), &[&a, &b], None, Some(progress))
            .await
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), vec![(0, 2), (1, 2), (2, 2)]);
    }

    #[tokio::test]
    async fn survey_sees_a_case_only_rename_once() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mods")).unwrap();
        std::fs::write(dir.path().join("mods/foo.jar"), "foo").unwrap();
        let renamed = pf("mods/Foo.jar", Hashes::sha1(sha1_hex(b"foo")));
        let lock = managed(&[("mods/foo.jar", FileMode::Replace, 3)]);
        let found = survey(dir.path(), &[&renamed], Some(&lock), None)
            .await
            .unwrap();
        assert!(
            !found.contains_key("mods/foo.jar"),
            "the old spelling is the wanted file: {found:?}"
        );
    }

    #[test]
    fn a_case_alias_is_separate_only_as_a_file_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("mods/folder.jar")).unwrap();
        std::fs::write(dir.path().join("mods/foo.jar"), "old").unwrap();
        std::fs::write(dir.path().join("mods/bar.jar"), "bar").unwrap();
        assert!(is_separate(dir.path(), "mods/bar.jar", "mods/foo.jar"));
        assert!(!is_separate(dir.path(), "mods/foo.jar", "mods/foo.jar"));
        assert!(!is_separate(dir.path(), "mods/gone.jar", "mods/foo.jar"));
        assert!(
            !is_separate(dir.path(), "mods/folder.jar", "mods/foo.jar"),
            "folders are never moved"
        );
        std::fs::write(dir.path().join("mods/Foo.jar"), "new").unwrap();
        // Case-insensitive (Windows): one file under two names. Case-sensitive: two files.
        let one_file = std::fs::read(dir.path().join("mods/foo.jar")).unwrap() == b"new";
        assert_eq!(
            is_separate(dir.path(), "mods/foo.jar", "mods/Foo.jar"),
            !one_file
        );
        #[cfg(windows)]
        assert!(one_file);
    }

    #[tokio::test]
    async fn a_folder_where_the_pack_wants_a_file_stops_the_survey() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("saves/world.dat")).unwrap();
        std::fs::write(dir.path().join("saves/world.dat/level.dat"), "mine").unwrap();
        let wanted = pf("saves/world.dat", Hashes::sha1(sha1_hex(b"pack")));
        let err = survey(dir.path(), &[&wanted], None, None)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::PackPathIsFolder);
        assert_eq!(err.params["path"], "saves/world.dat");

        // A managed file the pack dropped that is now a folder: the player's, never trashed.
        let lock = managed(&[("saves/world.dat", FileMode::Replace, 3)]);
        let found = survey(dir.path(), &[], Some(&lock), None).await.unwrap();
        assert_eq!(found.get("saves/world.dat"), None);
    }

    #[tokio::test]
    async fn only_not_found_counts_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        // A path the OS refuses outright (an interior NUL) fails `metadata` with another error.
        let odd = pf("mods/a\0b.jar", Hashes::sha1(sha1_hex(b"x")));
        let err = survey(dir.path(), &[&odd], None, None).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::DiskIo);
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
