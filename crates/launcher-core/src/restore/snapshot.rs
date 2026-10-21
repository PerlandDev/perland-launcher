//! Copying a game folder into a restore point: archives are hard-linked, the rest is copied.
//! Blocking `std::fs`; callers run it in `spawn_blocking`.
use std::{
    fs,
    io::Result as IoResult,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use super::SnapFile;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LinkMode {
    /// Hard-link archives.
    Auto,
    /// Copy everything.
    Copy,
}

/// A hard link failed (FAT32, another volume, a network drive).
#[derive(Debug)]
pub(crate) struct LinkFailed;

/// Mod, pack and shader archives: `.jar` and `.zip`, also with a `.disabled` suffix. These are
/// replaced rather than rewritten in place, so a hard link can't leak an edit into the point.
pub(crate) fn is_archive(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = lower.strip_suffix(".disabled").unwrap_or(&lower);
    base.ends_with(".jar") || base.ends_with(".zip")
}

/// The files under `dir` (not symlinks, not folders), with their metadata.
fn walk(dir: &Path) -> IoResult<Vec<(PathBuf, fs::Metadata)>> {
    let mut out = Vec::new();
    // A linked folder (symlink or junction) is not followed.
    match fs::symlink_metadata(dir) {
        Ok(meta) if meta.file_type().is_symlink() => return Ok(out),
        _ => {}
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        for entry in entries {
            let path = entry?.path();
            let meta = fs::symlink_metadata(&path)?;
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                out.push((path, meta));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn rel_path(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .unwrap_or(path)
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Bytes `snapshot_folder` would copy for `game_dir/<top>` in `mode`.
pub(crate) fn copy_size(game_dir: &Path, top: &str, mode: LinkMode) -> IoResult<u64> {
    let mut total = 0u64;
    for (path, meta) in walk(&game_dir.join(top))? {
        let name = path.file_name().map(|n| n.to_string_lossy());
        let linked = mode == LinkMode::Auto && name.as_deref().is_some_and(is_archive);
        if !linked {
            total = total.saturating_add(meta.len());
        }
    }
    Ok(total)
}

/// Snapshot game_dir/<top> into dest/<top>. Archives are hard-linked unless `mode` is Copy or a
/// link fails (then the caller retries the whole point with Copy). Returns the files written.
pub(crate) fn snapshot_folder(
    game_dir: &Path,
    top: &str,
    dest: &Path,
    mode: LinkMode,
) -> IoResult<Result<Vec<SnapFile>, LinkFailed>> {
    let mut files = Vec::new();
    for (path, meta) in walk(&game_dir.join(top))? {
        let rel = rel_path(game_dir, &path);
        let to = dest.join(&rel);
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent)?;
        }
        let name = path.file_name().map(|n| n.to_string_lossy());
        let link = mode == LinkMode::Auto && name.as_deref().is_some_and(is_archive);
        if link {
            if fs::hard_link(&path, &to).is_err() {
                return Ok(Err(LinkFailed));
            }
        } else {
            fs::copy(&path, &to)?;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        files.push(SnapFile {
            path: rel,
            size: meta.len(),
            mtime,
            linked: link,
        });
    }
    Ok(Ok(files))
}

/// Bytes this point alone holds: copied files, and linked files game_dir no longer has as the same file.
pub(crate) fn held_bytes(point_files: &Path, game_dir: &Path, files: &[SnapFile]) -> u64 {
    files
        .iter()
        .filter(|f| {
            !f.linked
                || !same_file::is_same_file(point_files.join(&f.path), game_dir.join(&f.path))
                    .unwrap_or(false)
        })
        .map(|f| f.size)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn is_archive_names() {
        for name in ["a.jar", "a.zip", "a.jar.disabled", "A.JAR"] {
            assert!(is_archive(name), "{name}");
        }
        for name in ["a.toml", "a.json", "a.toml.disabled"] {
            assert!(!is_archive(name), "{name}");
        }
    }

    #[test]
    fn snapshot_links_jars_and_copies_config() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, dest) = (tmp.path().join("game"), tmp.path().join("dest"));
        write(&game.join("mods/a.jar"), b"jar");
        write(&game.join("config/sub/a.toml"), b"cfg");

        let mods = snapshot_folder(&game, "mods", &dest, LinkMode::Auto)
            .unwrap()
            .unwrap();
        let config = snapshot_folder(&game, "config", &dest, LinkMode::Auto)
            .unwrap()
            .unwrap();

        assert!(same_file::is_same_file(game.join("mods/a.jar"), dest.join("mods/a.jar")).unwrap());
        let copy = dest.join("config/sub/a.toml");
        assert!(!same_file::is_same_file(game.join("config/sub/a.toml"), &copy).unwrap());
        assert_eq!(fs::read(copy).unwrap(), b"cfg");
        assert_eq!(
            mods,
            vec![SnapFile {
                path: "mods/a.jar".into(),
                size: 3,
                mtime: mods[0].mtime,
                linked: true
            }]
        );
        assert_eq!(config[0].path, "config/sub/a.toml");
        assert!(!config[0].linked);
    }

    #[test]
    fn config_edit_does_not_leak_into_point() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, dest) = (tmp.path().join("game"), tmp.path().join("dest"));
        write(&game.join("config/a.toml"), b"old");
        snapshot_folder(&game, "config", &dest, LinkMode::Auto)
            .unwrap()
            .unwrap();

        // Rewritten in place, the way the game does.
        fs::write(game.join("config/a.toml"), b"new").unwrap();

        assert_eq!(fs::read(dest.join("config/a.toml")).unwrap(), b"old");
    }

    #[test]
    fn snapshot_falls_back_to_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, dest) = (tmp.path().join("game"), tmp.path().join("dest"));
        write(&game.join("mods/a.jar"), b"jar");

        let files = snapshot_folder(&game, "mods", &dest, LinkMode::Copy)
            .unwrap()
            .unwrap();

        assert!(files.iter().all(|f| !f.linked));
        assert!(
            !same_file::is_same_file(game.join("mods/a.jar"), dest.join("mods/a.jar")).unwrap()
        );
        assert_eq!(copy_size(&game, "mods", LinkMode::Copy).unwrap(), 3);
        assert_eq!(copy_size(&game, "mods", LinkMode::Auto).unwrap(), 0);
    }

    #[cfg(unix)]
    #[test]
    fn linked_folder_is_not_followed() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, dest) = (tmp.path().join("game"), tmp.path().join("dest"));
        write(&tmp.path().join("elsewhere/a.toml"), b"x");
        fs::create_dir_all(&game).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("elsewhere"), game.join("config")).unwrap();
        let files = snapshot_folder(&game, "config", &dest, LinkMode::Auto)
            .unwrap()
            .unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn missing_folder_gives_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        let files = snapshot_folder(tmp.path(), "mods", &dest, LinkMode::Auto)
            .unwrap()
            .unwrap();
        assert!(files.is_empty());
        assert!(!dest.join("mods").exists());
    }

    #[test]
    fn held_bytes_counts_copies_and_orphaned_links() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, dest) = (tmp.path().join("game"), tmp.path().join("dest"));
        write(&game.join("mods/a.jar"), b"jar");
        write(&game.join("config/a.toml"), b"cf");
        let mut files = snapshot_folder(&game, "mods", &dest, LinkMode::Auto)
            .unwrap()
            .unwrap();
        files.extend(
            snapshot_folder(&game, "config", &dest, LinkMode::Auto)
                .unwrap()
                .unwrap(),
        );

        assert_eq!(held_bytes(&dest, &game, &files), 2);
        fs::remove_file(game.join("mods/a.jar")).unwrap();
        assert_eq!(held_bytes(&dest, &game, &files), 5);
    }
}
