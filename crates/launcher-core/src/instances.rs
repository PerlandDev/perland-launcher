use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{LauncherError, Result},
    fsutil::write_atomic,
    meta::loaders::LoaderKind,
};

/// Where an instance's content comes from. Plan 1 creates only `None`; packs arrive in Plan 3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceRef {
    None,
    Packwiz {
        url: String,
    },
    Mrpack {
        location: String,
    },
    #[serde(rename_all = "camelCase")]
    Modrinth {
        project_id: String,
        version_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoaderSpec {
    pub kind: LoaderKind,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSpec {
    pub minecraft: String,
    #[serde(default)]
    pub loader: Option<LoaderSpec>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdatePolicy {
    Auto,
    #[default]
    Ask,
    Pinned,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaSettings {
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default = "default_memory_mb")]
    pub memory_mb: u32,
}

fn default_memory_mb() -> u32 {
    4096
}

impl Default for JavaSettings {
    fn default() -> Self {
        Self {
            path: None,
            memory_mb: default_memory_mb(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub source: SourceRef,
    pub game: GameSpec,
    #[serde(default)]
    pub update_policy: UpdatePolicy,
    #[serde(default)]
    pub optional_choices: BTreeMap<String, bool>,
    #[serde(default)]
    pub java: JavaSettings,
}

pub struct InstanceStore {
    dir: PathBuf,
}

impl InstanceStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    pub fn game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("minecraft")
    }

    pub fn perland_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join(".perland")
    }

    fn json_path(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("instance.json")
    }

    pub async fn create(&self, name: &str, game: GameSpec) -> Result<Instance> {
        let base = slugify(name);
        let mut id = base.clone();
        let mut n = 2;
        while tokio::fs::try_exists(self.instance_dir(&id)).await? {
            id = format!("{base}-{n}");
            n += 1;
        }
        let instance = Instance {
            id,
            name: name.trim().to_string(),
            source: SourceRef::None,
            game,
            update_policy: UpdatePolicy::Ask,
            optional_choices: BTreeMap::new(),
            java: JavaSettings::default(),
        };
        tokio::fs::create_dir_all(self.game_dir(&instance.id)).await?;
        tokio::fs::create_dir_all(self.perland_dir(&instance.id)).await?;
        self.save(&instance).await?;
        Ok(instance)
    }

    pub async fn save(&self, instance: &Instance) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(instance)
            .map_err(|e| LauncherError::disk("Couldn't save the instance").with_detail(e))?;
        write_atomic(&self.json_path(&instance.id), &bytes).await
    }

    pub async fn load(&self, id: &str) -> Result<Instance> {
        let path = self.json_path(id);
        let bytes = tokio::fs::read(&path).await.map_err(|e| {
            LauncherError::disk(format!("Instance \"{id}\" wasn't found")).with_detail(e)
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            LauncherError::disk(format!("The settings file of \"{id}\" is damaged"))
                .with_fix("Restore instance.json from a backup or recreate the instance")
                .with_detail(e)
        })
    }

    pub async fn list(&self) -> Result<Vec<Instance>> {
        let mut out = Vec::new();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let id = entry.file_name().to_string_lossy().into_owned();
            if is_instance_dir(&entry.path()) {
                if let Ok(instance) = self.load(&id).await {
                    out.push(instance);
                }
            }
        }
        out.sort_by_key(|i| i.name.to_lowercase());
        Ok(out)
    }
}

fn is_instance_dir(path: &Path) -> bool {
    path.join("instance.json").is_file()
}

const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A folder-safe id: lowercase ASCII letters/digits joined by single dashes; never empty, never a Windows device name.
pub fn slugify(name: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for c in name.trim().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "instance".to_string()
    } else if WINDOWS_RESERVED.contains(&slug.as_str()) {
        format!("{slug}-instance")
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCategory;

    fn vanilla(mc: &str) -> GameSpec {
        GameSpec {
            minecraft: mc.into(),
            loader: None,
        }
    }

    #[test]
    fn slugify_handles_reserved_and_symbols() {
        assert_eq!(slugify("Asalith Fields"), "asalith-fields");
        assert_eq!(slugify("  ¡Hola Mundo!  "), "hola-mundo");
        assert_eq!(slugify("!!!"), "instance");
        assert_eq!(slugify("CON"), "con-instance");
        assert_eq!(slugify("lpt1"), "lpt1-instance");
        assert_eq!(slugify("Fabric 1.21 / test"), "fabric-1-21-test");
    }

    #[tokio::test]
    async fn create_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let game = GameSpec {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        };
        let created = store.create("Asalith Fields", game).await.unwrap();
        assert_eq!(created.id, "asalith-fields");
        assert!(store.game_dir("asalith-fields").is_dir());
        assert!(store.perland_dir("asalith-fields").is_dir());
        assert_eq!(store.load("asalith-fields").await.unwrap(), created);
        let raw =
            std::fs::read_to_string(store.instance_dir("asalith-fields").join("instance.json"))
                .unwrap();
        assert!(raw.contains("\"updatePolicy\": \"ask\""), "{raw}");
        assert!(raw.contains("\"kind\": \"none\""), "{raw}");
        assert!(raw.contains("\"memoryMb\": 4096"), "{raw}");
    }

    #[tokio::test]
    async fn duplicate_names_get_suffix() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        assert_eq!(
            store.create("Pack", vanilla("1.21.1")).await.unwrap().id,
            "pack"
        );
        assert_eq!(
            store.create("Pack", vanilla("1.21.1")).await.unwrap().id,
            "pack-2"
        );
        assert_eq!(
            store.create("pack!", vanilla("1.21.1")).await.unwrap().id,
            "pack-3"
        );
    }

    #[tokio::test]
    async fn list_is_sorted_and_tolerates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path().join("instances"));
        assert!(store.list().await.unwrap().is_empty());
        store.create("zeta", vanilla("1.21.1")).await.unwrap();
        store.create("Alpha", vanilla("1.21.1")).await.unwrap();
        let names: Vec<String> = store
            .list()
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert_eq!(names, vec!["Alpha", "zeta"]);
    }

    #[tokio::test]
    async fn missing_instance_is_disk_error() {
        let tmp = tempfile::tempdir().unwrap();
        let err = InstanceStore::new(tmp.path())
            .load("nope")
            .await
            .unwrap_err();
        assert_eq!(err.category, ErrorCategory::Disk);
    }

    #[test]
    fn modrinth_source_uses_camel_case() {
        let s = SourceRef::Modrinth {
            project_id: "abc".into(),
            version_id: "def".into(),
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"kind":"modrinth","projectId":"abc","versionId":"def"}"#
        );
    }
}
