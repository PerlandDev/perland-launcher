//! Files a player drops on the launcher's window: what each one is, judged by its name and,
//! for a zip, by what is inside it. A zip with a `shaders/` folder at its root or one folder
//! down is a shader pack; one with `pack.mcmeta` at its root is a resource pack.
use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DroppedKind {
    Mod,
    ResourcePack,
    ShaderPack,
    Modpack,
    /// A `.zip` that is neither kind of pack.
    UnknownZip,
    /// Anything else: another file type, a folder, a file that is gone.
    Other,
}

/// What `path` is. Reads the file system (a zip's directory), so call it off the async runtime.
pub fn classify(path: &Path) -> DroppedKind {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !path.is_file() {
        return DroppedKind::Other;
    }
    if name.ends_with(".jar") {
        DroppedKind::Mod
    } else if name.ends_with(".mrpack") {
        DroppedKind::Modpack
    } else if name.ends_with(".zip") {
        zip_kind(path)
    } else {
        DroppedKind::Other
    }
}

/// Only the zip's central directory is read, never its files.
fn zip_kind(path: &Path) -> DroppedKind {
    let Ok(archive) = std::fs::File::open(path).map(zip::ZipArchive::new) else {
        return DroppedKind::UnknownZip;
    };
    let Ok(archive) = archive else {
        return DroppedKind::UnknownZip;
    };
    let mut resource = false;
    for name in archive.file_names() {
        let Ok(name) = name else { continue };
        let parts: Vec<&str> = name.split(['/', '\\']).collect();
        // `shaders/...` or `<folder>/shaders/...`: the folder itself, not a file named shaders.
        let shaders =
            parts.len() >= 2 && parts[0] == "shaders" || parts.len() >= 3 && parts[1] == "shaders";
        if shaders {
            return DroppedKind::ShaderPack;
        }
        resource |= parts == ["pack.mcmeta"];
    }
    if resource {
        DroppedKind::ResourcePack
    } else {
        DroppedKind::UnknownZip
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::write_zip;

    fn file(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"x").unwrap();
        path
    }

    fn zip(dir: &Path, name: &str, entries: &[&str]) -> std::path::PathBuf {
        let path = dir.join(name);
        let entries: Vec<(&str, &[u8])> = entries.iter().map(|e| (*e, &b"x"[..])).collect();
        write_zip(&path, &entries);
        path
    }

    #[test]
    fn jars_and_modpacks_go_by_their_extension_in_any_case() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(classify(&file(tmp.path(), "sodium.jar")), DroppedKind::Mod);
        assert_eq!(classify(&file(tmp.path(), "IRIS.JAR")), DroppedKind::Mod);
        assert_eq!(
            classify(&file(tmp.path(), "Sky Islands.mrpack")),
            DroppedKind::Modpack
        );
        assert_eq!(classify(&file(tmp.path(), "notes.txt")), DroppedKind::Other);
        assert_eq!(classify(&file(tmp.path(), "skin.png")), DroppedKind::Other);
    }

    #[test]
    fn a_folder_or_a_missing_file_is_other() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("folder.jar")).unwrap();
        assert_eq!(classify(&tmp.path().join("folder.jar")), DroppedKind::Other);
        assert_eq!(classify(&tmp.path().join("gone.zip")), DroppedKind::Other);
    }

    #[test]
    fn a_zip_with_shaders_at_its_root_or_one_folder_down_is_a_shader_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let root = zip(tmp.path(), "a.zip", &["shaders/final.fsh"]);
        let nested = zip(
            tmp.path(),
            "b.ZIP",
            &["BSL v8/shaders/composite.fsh", "BSL v8/readme.txt"],
        );
        // Shader packs often carry a pack.mcmeta too: the shaders folder decides.
        let both = zip(tmp.path(), "c.zip", &["pack.mcmeta", "shaders/x.fsh"]);
        for path in [root, nested, both] {
            assert_eq!(classify(&path), DroppedKind::ShaderPack, "{path:?}");
        }
    }

    #[test]
    fn a_zip_with_pack_mcmeta_at_its_root_is_a_resource_pack() {
        let tmp = tempfile::tempdir().unwrap();
        let pack = zip(
            tmp.path(),
            "Faithful.zip",
            &["pack.mcmeta", "assets/minecraft/x.png"],
        );
        assert_eq!(classify(&pack), DroppedKind::ResourcePack);
    }

    #[test]
    fn any_other_zip_is_unknown() {
        let tmp = tempfile::tempdir().unwrap();
        // pack.mcmeta one folder down doesn't load in the game; shaders two folders down neither.
        let nested = zip(tmp.path(), "a.zip", &["Faithful/pack.mcmeta"]);
        let deep = zip(tmp.path(), "b.zip", &["a/b/shaders/x.fsh"]);
        let photos = zip(tmp.path(), "c.zip", &["photo.png"]);
        let named = zip(tmp.path(), "d.zip", &["my_shaders/x.fsh"]);
        for path in [nested, deep, photos, named] {
            assert_eq!(classify(&path), DroppedKind::UnknownZip, "{path:?}");
        }
        // Not a zip at all, whatever its name says.
        assert_eq!(
            classify(&file(tmp.path(), "fake.zip")),
            DroppedKind::UnknownZip
        );
    }
}
