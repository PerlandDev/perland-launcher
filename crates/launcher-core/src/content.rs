//! The resource packs and shaders in an instance's `resourcepacks/` and `shaderpacks/` folders:
//! list them and remove the zips. (Adding goes through Browse, which writes the same folders.)
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{
    browse::compat::ContentKind,
    error::{ErrorCode, LauncherError, Result},
    fsutil::safe_component,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalPack {
    pub file_name: String,
    pub size: u64,
    pub folder: bool,
}

fn invalid(file_name: &str) -> LauncherError {
    LauncherError::disk(ErrorCode::ModNameInvalid, "That isn't a pack file name")
        .with_detail(file_name)
}

fn not_found(file_name: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::ModNotFound,
        format!("{file_name} isn't in this instance anymore"),
    )
    .with_param("file", file_name)
}

/// `<game_dir>/<resourcepacks|shaderpacks>`; any other kind has no pack folder here.
fn pack_dir(game_dir: &Path, kind: ContentKind) -> Result<PathBuf> {
    match kind {
        ContentKind::ResourcePack | ContentKind::Shader => {
            Ok(game_dir.join(kind.folder().unwrap_or_default()))
        }
        ContentKind::Modpack | ContentKind::Mod => Err(invalid(kind.project_type())),
    }
}

fn is_zip(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

/// The zips and folders in the kind's folder, sorted by name without regard to case. Hidden
/// names (including the files Browse is still writing) and other files are left out.
pub async fn list(game_dir: &Path, kind: ContentKind) -> Result<Vec<LocalPack>> {
    let mut entries = match tokio::fs::read_dir(pack_dir(game_dir, kind)?).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if file_name.starts_with('.') {
            continue;
        }
        let meta = entry.metadata().await?;
        if meta.is_dir() {
            out.push(LocalPack {
                file_name,
                size: 0,
                folder: true,
            });
        } else if meta.is_file() && is_zip(&file_name) {
            out.push(LocalPack {
                file_name,
                size: meta.len(),
                folder: false,
            });
        }
    }
    out.sort_by_key(|p| p.file_name.to_lowercase());
    Ok(out)
}

/// Delete one zip. Folders are never removed: they may be someone's work in progress.
pub async fn remove(game_dir: &Path, kind: ContentKind, file_name: &str) -> Result<()> {
    let dir = pack_dir(game_dir, kind)?;
    if !safe_component(file_name) || !is_zip(file_name) {
        return Err(invalid(file_name));
    }
    let path = dir.join(file_name);
    let meta = tokio::fs::symlink_metadata(&path)
        .await
        .map_err(|_| not_found(file_name))?;
    if meta.is_dir() {
        return Err(invalid(file_name));
    }
    if !meta.is_file() {
        return Err(not_found(file_name));
    }
    tokio::fs::remove_file(&path).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn put(game: &Path, folder: &str, name: &str, bytes: &[u8]) {
        let dir = game.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), bytes).unwrap();
    }

    fn pack(name: &str, size: u64, folder: bool) -> LocalPack {
        LocalPack {
            file_name: name.into(),
            size,
            folder,
        }
    }

    #[tokio::test]
    async fn list_returns_zips_and_folders_sorted_and_skips_the_rest() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path();
        put(g, "resourcepacks", "b.zip", b"12");
        put(g, "resourcepacks", "A.ZIP", b"1");
        put(g, "resourcepacks", "notes.txt", b"x");
        put(g, "resourcepacks", ".hidden.zip", b"x");
        put(g, "resourcepacks", ".Faithful.zip.123.perland-tmp", b"x");
        std::fs::create_dir_all(g.join("resourcepacks/Folder Pack")).unwrap();
        put(g, "shaderpacks", "other.zip", b"x"); // the other kind's folder
        assert_eq!(
            list(g, ContentKind::ResourcePack).await.unwrap(),
            vec![
                pack("A.ZIP", 1, false),
                pack("b.zip", 2, false),
                pack("Folder Pack", 0, true),
            ]
        );
        assert_eq!(
            list(g, ContentKind::Shader).await.unwrap(),
            vec![pack("other.zip", 1, false)]
        );
    }

    #[tokio::test]
    async fn a_missing_folder_is_an_empty_list() {
        let tmp = tempfile::tempdir().unwrap();
        for kind in [ContentKind::ResourcePack, ContentKind::Shader] {
            assert!(list(tmp.path(), kind).await.unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn kinds_without_a_pack_folder_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        for kind in [ContentKind::Mod, ContentKind::Modpack] {
            assert_eq!(
                list(tmp.path(), kind).await.unwrap_err().code,
                ErrorCode::ModNameInvalid
            );
            assert_eq!(
                remove(tmp.path(), kind, "a.zip").await.unwrap_err().code,
                ErrorCode::ModNameInvalid
            );
        }
    }

    #[tokio::test]
    async fn remove_deletes_a_zip() {
        let tmp = tempfile::tempdir().unwrap();
        put(tmp.path(), "shaderpacks", "Complementary.zip", b"z");
        put(tmp.path(), "shaderpacks", "Keep.zip", b"z");
        remove(tmp.path(), ContentKind::Shader, "Complementary.zip")
            .await
            .unwrap();
        let left = list(tmp.path(), ContentKind::Shader).await.unwrap();
        assert_eq!(left, vec![pack("Keep.zip", 1, false)]);
    }

    #[tokio::test]
    async fn remove_refuses_folders_and_unsafe_or_wrong_names() {
        let tmp = tempfile::tempdir().unwrap();
        let g = tmp.path();
        std::fs::create_dir_all(g.join("resourcepacks/Folder")).unwrap();
        std::fs::create_dir_all(g.join("resourcepacks/dir.zip")).unwrap();
        put(g, "resourcepacks", "a.txt", b"x");
        put(g, "resourcepacks", "x.zip", b"x");
        for bad in [
            "Folder",
            "dir.zip",
            "..\\x.zip",
            "../x.zip",
            "sub/x.zip",
            "CON.zip",
            "a.txt",
            "",
        ] {
            let err = remove(g, ContentKind::ResourcePack, bad).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::ModNameInvalid, "{bad:?}");
        }
        assert!(g.join("resourcepacks/Folder").is_dir());
        assert!(g.join("resourcepacks/dir.zip").is_dir());
        assert!(g.join("resourcepacks/a.txt").is_file());
        assert!(g.join("resourcepacks/x.zip").is_file());
    }

    #[tokio::test]
    async fn remove_reports_a_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let err = remove(tmp.path(), ContentKind::ResourcePack, "gone.zip")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ModNotFound);
        assert_eq!(err.params["file"], "gone.zip");
    }
}
