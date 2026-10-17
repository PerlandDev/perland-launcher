//! Which mods a jar is, read from its own metadata. Sync uses it to tell a player's jar that is
//! another version of one of the pack's mods (set aside: the pack's version wins) from a mod the
//! player added (kept).
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

/// Metadata files are small; a bigger entry isn't one.
const MAX_ENTRY: u64 = 1024 * 1024;

/// The mod ids `jar` declares (Fabric, Quilt, Forge, NeoForge), lowercased. Empty when it isn't a
/// readable jar or declares none: such a jar is never taken for another version of anything.
/// Blocking.
pub(crate) fn mod_ids(jar: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Ok(file) = std::fs::File::open(jar) else {
        return ids;
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return ids;
    };
    let mut entry = |name: &str| -> Option<String> {
        let mut out = String::new();
        archive
            .by_name(name)
            .ok()?
            .take(MAX_ENTRY)
            .read_to_string(&mut out)
            .ok()?;
        Some(out)
    };
    if let Some(text) = entry("fabric.mod.json") {
        ids.extend(json_id(&text, &["id"]));
    }
    if let Some(text) = entry("quilt.mod.json") {
        ids.extend(json_id(&text, &["quilt_loader", "id"]));
    }
    for name in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
        if let Some(text) = entry(name) {
            ids.extend(toml_ids(&text));
        }
    }
    ids
}

fn json_id(text: &str, at: &[&str]) -> Option<String> {
    let mut value: &serde_json::Value = &serde_json::from_str(text).ok()?;
    for key in at {
        value = value.get(key)?;
    }
    clean(value.as_str()?)
}

/// Every `[[mods]] modId` of a Forge or NeoForge `mods.toml`.
fn toml_ids(text: &str) -> Vec<String> {
    let Ok(doc) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let Some(mods) = doc.get("mods").and_then(|m| m.as_array()) else {
        return Vec::new();
    };
    mods.iter()
        .filter_map(|m| m.get("modId")?.as_str())
        .filter_map(clean)
        .collect()
}

fn clean(id: &str) -> Option<String> {
    let id = id.trim().to_lowercase();
    // `${mod_id}` is an unexpanded build placeholder, not an id.
    (!id.is_empty() && !id.contains('$')).then_some(id)
}

/// The `players` jars (game-folder relative) that declare a mod one of `pack` declares. Blocking.
pub(crate) fn same_mods(game_dir: &Path, players: &[String], pack: &[PathBuf]) -> Vec<String> {
    let pack_ids: BTreeSet<String> = pack.iter().flat_map(|p| mod_ids(p)).collect();
    if pack_ids.is_empty() {
        return Vec::new();
    }
    players
        .iter()
        .filter(|rel| !mod_ids(&game_dir.join(rel)).is_disjoint(&pack_ids))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::write_zip;

    fn ids(entries: &[(&str, &str)]) -> Vec<String> {
        let dir = tempfile::tempdir().unwrap();
        let jar = dir.path().join("m.jar");
        write_zip(&jar, entries);
        mod_ids(&jar).into_iter().collect()
    }

    #[test]
    fn reads_fabric_and_quilt_ids() {
        assert_eq!(
            ids(&[("fabric.mod.json", r#"{"id": "Sodium", "version": "1"}"#)]),
            vec!["sodium"]
        );
        assert_eq!(
            ids(&[("quilt.mod.json", r#"{"quilt_loader": {"id": "qsl"}}"#)]),
            vec!["qsl"]
        );
    }

    #[test]
    fn reads_every_forge_and_neoforge_mod_id() {
        let toml =
            "modLoader = \"javafml\"\n[[mods]]\nmodId = \"jei\"\n[[mods]]\nmodId = \"jei_api\"\n";
        assert_eq!(ids(&[("META-INF/mods.toml", toml)]), vec!["jei", "jei_api"]);
        assert_eq!(
            ids(&[("META-INF/neoforge.mods.toml", toml)]),
            vec!["jei", "jei_api"]
        );
    }

    #[test]
    fn a_jar_without_readable_metadata_is_no_mod() {
        assert!(ids(&[("fabric.mod.json", "{not json")]).is_empty());
        assert!(ids(&[("META-INF/mods.toml", "[[mods]]\nmodId = \"${mod_id}\"")]).is_empty());
        assert!(ids(&[("readme.txt", "hi")]).is_empty());
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("x.jar"), b"not a zip").unwrap();
        assert!(mod_ids(&dir.path().join("x.jar")).is_empty());
        assert!(mod_ids(&dir.path().join("gone.jar")).is_empty());
    }

    #[test]
    fn same_mods_matches_by_id_not_by_file_name() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::create_dir_all(game.join("mods")).unwrap();
        let fabric = |id: &str| format!(r#"{{"id": "{id}"}}"#);
        write_zip(
            &game.join("mods/zoom-old.jar"),
            &[("fabric.mod.json", fabric("zoom"))],
        );
        write_zip(
            &game.join("mods/zoom-addon.jar"),
            &[("fabric.mod.json", fabric("zoom_addon"))],
        );
        std::fs::write(game.join("mods/zoom.jar"), b"not a zip").unwrap();
        let pack = dir.path().join("pack.jar");
        write_zip(&pack, &[("fabric.mod.json", fabric("zoom"))]);
        let players =
            ["mods/zoom-addon.jar", "mods/zoom-old.jar", "mods/zoom.jar"].map(String::from);

        assert_eq!(
            same_mods(game, &players, std::slice::from_ref(&pack)),
            vec!["mods/zoom-old.jar"]
        );
        assert!(same_mods(game, &players, &[]).is_empty());
    }
}
