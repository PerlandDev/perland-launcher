//! The enabled jars in `mods/` and the mods they declare, so a cause can name a mod by its own
//! name and the card can turn its file off. Only each jar's directory and metadata are read.
use std::{
    io::Read,
    path::{Path, PathBuf},
};

use super::{CrashCause, ModRef};

/// The largest metadata file read from a jar.
const MAX_METADATA_BYTES: u64 = 1 << 20;

/// One mod a jar declares (a Forge jar may declare several).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JarMod {
    pub file: String,
    pub id: String,
    pub name: Option<String>,
}

fn enabled_jars(game_dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(game_dir.join("mods")) else {
        return Vec::new();
    };
    let mut jars: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            name.to_ascii_lowercase()
                .ends_with(".jar")
                .then(|| (name, e.path()))
        })
        .collect();
    jars.sort();
    jars
}

fn open(path: &Path) -> Option<zip::ZipArchive<std::fs::File>> {
    zip::ZipArchive::new(std::fs::File::open(path).ok()?).ok()
}

fn read_entry(jar: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Option<String> {
    let entry = jar.by_name(name).ok()?;
    if !entry.is_file() || entry.size() > MAX_METADATA_BYTES {
        return None;
    }
    let mut text = String::new();
    entry
        .take(MAX_METADATA_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn text(value: Option<&serde_json::Value>) -> Option<String> {
    value?.as_str().map(str::to_string)
}

/// `(id, name)` of each mod a jar's Fabric, Quilt, Forge or NeoForge metadata declares.
fn declared(jar: &mut zip::ZipArchive<std::fs::File>) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let json = |jar: &mut _, name| {
        read_entry(jar, name).and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
    };
    if let Some(v) = json(jar, "fabric.mod.json") {
        out.extend(text(v.get("id")).map(|id| (id, text(v.get("name")))));
    }
    if let Some(v) = json(jar, "quilt.mod.json").and_then(|v| v.get("quilt_loader").cloned()) {
        let name = v.get("metadata").and_then(|m| text(m.get("name")));
        out.extend(text(v.get("id")).map(|id| (id, name)));
    }
    for file in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        let Some(table) = read_entry(jar, file).and_then(|t| t.parse::<toml::Table>().ok()) else {
            continue;
        };
        let mods = table.get("mods").and_then(|m| m.as_array());
        for m in mods.into_iter().flatten() {
            let field = |key| m.get(key).and_then(|v| v.as_str()).map(str::to_string);
            out.extend(field("modId").map(|id| (id, field("displayName"))));
        }
    }
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Every mod the enabled jars declare, in file name order.
pub fn scan(game_dir: &Path) -> Vec<JarMod> {
    enabled_jars(game_dir)
        .into_iter()
        .flat_map(|(file, path)| {
            let mods = open(&path)
                .map(|mut jar| declared(&mut jar))
                .unwrap_or_default();
            mods.into_iter().map(move |(id, name)| JarMod {
                file: file.clone(),
                id,
                name,
            })
        })
        .collect()
}

/// The enabled jar that holds `entry` (a mixin config) at its root.
pub fn owner_of_entry(game_dir: &Path, entry: &str) -> Option<String> {
    enabled_jars(game_dir)
        .into_iter()
        .find_map(|(file, path)| open(&path)?.index_for_name(entry).map(|_| file))
}

fn fill(module: &mut ModRef, jars: &[JarMod]) {
    if let Some(jar) = jars.iter().find(|j| j.id == module.id) {
        module.file = Some(jar.file.clone());
        if jar.name.is_some() {
            module.name = jar.name.clone();
        }
    }
}

/// Give each mod the cause names its jar's file and name, when an enabled jar declares it.
pub fn resolve(cause: &mut CrashCause, jars: &[JarMod]) {
    match cause {
        CrashCause::ModMissingDependency { module, .. }
        | CrashCause::ModWrongVersion { module }
        | CrashCause::ModFailed { module } => fill(module, jars),
        CrashCause::ModsClash { module, other } => {
            fill(module, jars);
            fill(other, jars);
        }
        _ => {}
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub(crate) fn jar(dir: &Path, file: &str, entries: &[(&str, &str)]) {
        std::fs::create_dir_all(dir.join("mods")).unwrap();
        let out = std::fs::File::create(dir.join("mods").join(file)).unwrap();
        let mut zip = zip::ZipWriter::new(out);
        for (name, body) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn scan_reads_fabric_quilt_and_forge_ids() {
        let tmp = tempfile::tempdir().unwrap();
        jar(
            tmp.path(),
            "sodium.jar",
            &[("fabric.mod.json", r#"{"id":"sodium","name":"Sodium"}"#)],
        );
        jar(
            tmp.path(),
            "q.jar",
            &[(
                "quilt.mod.json",
                r#"{"quilt_loader":{"id":"qmod","metadata":{"name":"Q Mod"}}}"#,
            )],
        );
        jar(
            tmp.path(),
            "create.jar",
            &[(
                "META-INF/mods.toml",
                "modLoader=\"javafml\"\n[[mods]]\nmodId=\"create\"\ndisplayName=\"Create\"\n",
            )],
        );
        jar(
            tmp.path(),
            "broken.jar",
            &[("fabric.mod.json", "{not json")],
        );
        let mods = scan(tmp.path());
        let ids: Vec<_> = mods
            .iter()
            .map(|m| (m.file.as_str(), m.id.as_str(), m.name.as_deref()))
            .collect();
        assert_eq!(
            ids,
            [
                ("create.jar", "create", Some("Create")),
                ("q.jar", "qmod", Some("Q Mod")),
                ("sodium.jar", "sodium", Some("Sodium")),
            ]
        );
    }

    #[test]
    fn disabled_jars_are_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        jar(
            tmp.path(),
            "sodium.jar.disabled",
            &[("fabric.mod.json", r#"{"id":"sodium"}"#)],
        );
        assert!(scan(tmp.path()).is_empty());
        assert!(scan(&tmp.path().join("nowhere")).is_empty());
    }

    #[test]
    fn mixin_config_finds_its_jar() {
        let tmp = tempfile::tempdir().unwrap();
        jar(tmp.path(), "a.jar", &[("a.mixins.json", "{}")]);
        jar(tmp.path(), "create.jar", &[("create.mixins.json", "{}")]);
        assert_eq!(
            owner_of_entry(tmp.path(), "create.mixins.json").as_deref(),
            Some("create.jar")
        );
        assert_eq!(owner_of_entry(tmp.path(), "none.mixins.json"), None);
    }

    #[test]
    fn unknown_mod_has_no_file() {
        let jars = [JarMod {
            file: "sodium.jar".into(),
            id: "sodium".into(),
            name: Some("Sodium".into()),
        }];
        let mut cause = CrashCause::ModsClash {
            module: ModRef {
                id: "sodium".into(),
                name: None,
                file: None,
            },
            other: ModRef {
                id: "optifabric".into(),
                name: Some("OptiFabric".into()),
                file: None,
            },
        };
        resolve(&mut cause, &jars);
        let CrashCause::ModsClash { module, other } = cause else {
            unreachable!()
        };
        assert_eq!(module.file.as_deref(), Some("sodium.jar"));
        assert_eq!(module.name.as_deref(), Some("Sodium"));
        assert_eq!(other.file, None);
        assert_eq!(other.name.as_deref(), Some("OptiFabric"));
    }
}
