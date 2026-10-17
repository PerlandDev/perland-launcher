//! The plain jar files in an instance's `mods/` folder: list, turn on or off, add, remove.
//! A disabled mod is the same file with `.disabled` appended, which Fabric and Quilt ignore.
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::safe_join,
};

const DISABLED: &str = ".disabled";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalMod {
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
}

fn mods_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("mods")
}

/// `(name without .disabled, enabled)` when `file_name` is one plain path component ending in
/// `.jar` (any case), optionally followed by `.disabled`.
fn jar_name(file_name: &str) -> Option<(String, bool)> {
    let plain = !file_name.is_empty()
        && file_name != "."
        && file_name != ".."
        && !file_name.contains(['/', '\\', ':', '\0']);
    if !plain {
        return None;
    }
    let (base, enabled) = match file_name.strip_suffix(DISABLED) {
        Some(base) => (base, false),
        None => (file_name, true),
    };
    base.to_ascii_lowercase()
        .ends_with(".jar")
        .then(|| (base.to_string(), enabled))
}

fn invalid(file_name: &str) -> LauncherError {
    LauncherError::disk(ErrorCode::ModNameInvalid, "That isn't a mod file name")
        .with_detail(file_name)
}

fn not_found(file_name: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::ModNotFound,
        format!("{file_name} isn't in this instance anymore"),
    )
    .with_param("file", file_name)
}

fn exists(base: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::ModExists,
        format!("{base} is already in this instance"),
    )
    .with_param("file", base)
}

fn path_in(dir: &Path, file_name: &str) -> Result<PathBuf> {
    safe_join(dir, file_name).ok_or_else(|| invalid(file_name))
}

/// The jar `file_name` names in `mods/`, and its name without `.disabled`.
pub(crate) fn jar_path(game_dir: &Path, file_name: &str) -> Result<(PathBuf, String)> {
    let (base, _) = jar_name(file_name).ok_or_else(|| invalid(file_name))?;
    Ok((path_in(&mods_dir(game_dir), file_name)?, base))
}

/// Refuse a name already present in either state, so on and off copies never coexist.
async fn ensure_free(dir: &Path, base: &str) -> Result<()> {
    for candidate in [base.to_string(), format!("{base}{DISABLED}")] {
        if tokio::fs::try_exists(path_in(dir, &candidate)?).await? {
            return Err(exists(base));
        }
    }
    Ok(())
}

pub async fn list(game_dir: &Path) -> Result<Vec<LocalMod>> {
    let mut entries = match tokio::fs::read_dir(mods_dir(game_dir)).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let Some((_, enabled)) = jar_name(&file_name) else {
            continue;
        };
        let meta = entry.metadata().await?;
        if meta.is_file() {
            out.push(LocalMod {
                file_name,
                enabled,
                size: meta.len(),
            });
        }
    }
    out.sort_by_key(|m| m.file_name.to_lowercase());
    Ok(out)
}

pub async fn set_enabled(game_dir: &Path, file_name: &str, enabled: bool) -> Result<LocalMod> {
    let (base, currently) = jar_name(file_name).ok_or_else(|| invalid(file_name))?;
    let dir = mods_dir(game_dir);
    let from = path_in(&dir, file_name)?;
    let meta = tokio::fs::metadata(&from)
        .await
        .map_err(|_| not_found(file_name))?;
    if !meta.is_file() {
        return Err(not_found(file_name));
    }
    if currently == enabled {
        return Ok(LocalMod {
            file_name: file_name.to_string(),
            enabled,
            size: meta.len(),
        });
    }
    let to_name = if enabled {
        base.clone()
    } else {
        format!("{base}{DISABLED}")
    };
    let to = path_in(&dir, &to_name)?;
    if tokio::fs::try_exists(&to).await? {
        return Err(exists(&base));
    }
    tokio::fs::rename(&from, &to).await?;
    Ok(LocalMod {
        file_name: to_name,
        enabled,
        size: meta.len(),
    })
}

/// Copy a jar into `mods/` (via a `.part` file, so a half-copied mod is never loaded).
pub async fn add(game_dir: &Path, src: &Path) -> Result<LocalMod> {
    let name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let not_jar = || {
        LauncherError::disk(ErrorCode::ModNotJar, "Only .jar files can be added as mods")
            .with_param("file", &name)
    };
    let base = match jar_name(&name) {
        Some((base, true)) => base,
        _ => return Err(not_jar()),
    };
    let meta = tokio::fs::metadata(src)
        .await
        .map_err(|_| not_found(&name))?;
    if !meta.is_file() {
        return Err(not_jar());
    }
    let dir = mods_dir(game_dir);
    tokio::fs::create_dir_all(&dir).await?;
    ensure_free(&dir, &base).await?;
    let dest = path_in(&dir, &base)?;
    let part = path_in(&dir, &format!("{base}.part"))?;
    if let Err(e) = tokio::fs::copy(src, &part).await {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(e.into());
    }
    tokio::fs::rename(&part, &dest).await?;
    Ok(LocalMod {
        file_name: base,
        enabled: true,
        size: meta.len(),
    })
}

pub async fn remove(game_dir: &Path, file_name: &str) -> Result<()> {
    jar_name(file_name).ok_or_else(|| invalid(file_name))?;
    let path = path_in(&mods_dir(game_dir), file_name)?;
    if !path.is_file() {
        return Err(not_found(file_name));
    }
    tokio::fs::remove_file(&path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    fn jar(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[tokio::test]
    async fn list_finds_enabled_and_disabled_jars_only() {
        let tmp = tempfile::tempdir().unwrap();
        let mods = tmp.path().join("mods");
        std::fs::create_dir_all(mods.join("sub.jar")).unwrap(); // a folder, not a mod
        jar(&mods, "b.jar", b"12");
        jar(&mods, "A.jar.disabled", b"1");
        jar(&mods, "notes.txt", b"x");
        jar(&mods, "half.jar.part", b"x");
        let list = list(tmp.path()).await.unwrap();
        assert_eq!(
            list,
            vec![
                LocalMod {
                    file_name: "A.jar.disabled".into(),
                    enabled: false,
                    size: 1
                },
                LocalMod {
                    file_name: "b.jar".into(),
                    enabled: true,
                    size: 2
                },
            ]
        );
    }

    #[tokio::test]
    async fn missing_mods_folder_is_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(list(tmp.path()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn toggle_renames_both_ways() {
        let tmp = tempfile::tempdir().unwrap();
        jar(&tmp.path().join("mods"), "sodium.jar", b"abc");
        let off = set_enabled(tmp.path(), "sodium.jar", false).await.unwrap();
        assert_eq!(off.file_name, "sodium.jar.disabled");
        assert!(!off.enabled);
        assert!(tmp.path().join("mods/sodium.jar.disabled").is_file());
        let on = set_enabled(tmp.path(), "sodium.jar.disabled", true)
            .await
            .unwrap();
        assert_eq!(on.file_name, "sodium.jar");
        assert_eq!(on.size, 3);
        assert_eq!(
            set_enabled(tmp.path(), "gone.jar", false)
                .await
                .unwrap_err()
                .code,
            ErrorCode::ModNotFound
        );
    }

    #[tokio::test]
    async fn add_accepts_uppercase_jar_and_refuses_disabled_twin() {
        let tmp = tempfile::tempdir().unwrap();
        let downloads = tmp.path().join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let game = tmp.path().join("game");
        let src = jar(&downloads, "Sodium.JAR", b"abcd");
        let added = add(&game, &src).await.unwrap();
        assert_eq!(
            added,
            LocalMod {
                file_name: "Sodium.JAR".into(),
                enabled: true,
                size: 4
            }
        );
        set_enabled(&game, "Sodium.JAR", false).await.unwrap();
        assert_eq!(
            add(&game, &src).await.unwrap_err().code,
            ErrorCode::ModExists
        );
        assert_eq!(list(&game).await.unwrap().len(), 1, "no on/off twins");
    }

    #[tokio::test]
    async fn add_refuses_non_jars_and_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let notes = jar(tmp.path(), "notes.txt", b"x");
        assert_eq!(
            add(tmp.path(), &notes).await.unwrap_err().code,
            ErrorCode::ModNotJar
        );
        let folder = tmp.path().join("folder.jar");
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(
            add(tmp.path(), &folder).await.unwrap_err().code,
            ErrorCode::ModNotJar
        );
    }

    #[tokio::test]
    async fn names_cannot_escape_the_mods_folder() {
        let tmp = tempfile::tempdir().unwrap();
        for bad in ["../x.jar", "a/b.jar", "C:x.jar", "..", ""] {
            assert_eq!(
                remove(tmp.path(), bad).await.unwrap_err().code,
                ErrorCode::ModNameInvalid,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn remove_deletes_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        jar(&tmp.path().join("mods"), "x.jar.disabled", b"1");
        remove(tmp.path(), "x.jar.disabled").await.unwrap();
        assert!(list(tmp.path()).await.unwrap().is_empty());
        assert_eq!(
            remove(tmp.path(), "x.jar").await.unwrap_err().code,
            ErrorCode::ModNotFound
        );
    }
}
