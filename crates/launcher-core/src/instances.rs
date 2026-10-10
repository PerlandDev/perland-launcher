use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    meta::loaders::LoaderKind,
};

pub const MAX_NAME_CHARS: usize = 64;
/// Longest slug made from a name, before any `-N` suffix for duplicates.
pub const MAX_SLUG_LEN: usize = 48;
/// Longest valid id: a capped slug plus room for a `-N` suffix.
pub const MAX_ID_LEN: usize = 64;
pub const MIN_MEMORY_MB: u32 = 512;
pub const MAX_MEMORY_MB: u32 = 65_536;

/// The trimmed name, if it is 1–64 characters.
pub fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    let chars = trimmed.chars().count();
    if (1..=MAX_NAME_CHARS).contains(&chars) {
        Ok(trimmed.to_string())
    } else {
        Err(LauncherError::disk(
            ErrorCode::NameInvalid,
            "Instance names must be 1–64 characters",
        )
        .with_param("max", MAX_NAME_CHARS))
    }
}

pub fn validate_memory(memory_mb: u32) -> Result<()> {
    if (MIN_MEMORY_MB..=MAX_MEMORY_MB).contains(&memory_mb) {
        Ok(())
    } else {
        Err(LauncherError::java(
            ErrorCode::MemoryOutOfRange,
            format!("Memory must be between {MIN_MEMORY_MB} and {MAX_MEMORY_MB} MB"),
        )
        .with_param("min", MIN_MEMORY_MB)
        .with_param("max", MAX_MEMORY_MB))
    }
}

pub fn validate_java(java: &JavaSettings) -> Result<()> {
    validate_memory(java.memory_mb)?;
    if let Some(path) = &java.path {
        crate::play::custom_java(path)?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DamagedInstance {
    pub id: String,
    pub error: LauncherError,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct InstanceListing {
    pub instances: Vec<Instance>,
    pub damaged: Vec<DamagedInstance>,
}

/// Where an instance's content comes from. `Mrpack` is an imported file, kept in the object
/// cache under its sha1; `name` is only for display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SourceRef {
    None,
    Packwiz {
        url: String,
    },
    Mrpack {
        name: String,
        sha1: String,
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

    /// The accessors below (`instance_dir`, `game_dir`, `perland_dir`) join `id` onto the store
    /// directory as-is: they expect an id already validated via `create`, `load` or `list`
    /// (or `is_valid_instance_id`).
    pub fn instance_dir(&self, id: &str) -> PathBuf {
        self.dir.join(id)
    }

    /// See `instance_dir`: expects an already validated id.
    pub fn game_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("minecraft")
    }

    /// See `instance_dir`: expects an already validated id.
    pub fn perland_dir(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join(".perland")
    }

    fn json_path(&self, id: &str) -> PathBuf {
        self.instance_dir(id).join("instance.json")
    }

    pub async fn create(&self, name: &str, game: GameSpec) -> Result<Instance> {
        let name = validate_name(name)?;
        let base = slugify(&name);
        let mut id = base.clone();
        let mut n = 2;
        while tokio::fs::try_exists(self.instance_dir(&id)).await? {
            id = format!("{base}-{n}");
            n += 1;
        }
        let instance = Instance {
            id,
            name,
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
        check_id(&instance.id)?;
        let bytes = serde_json::to_vec_pretty(instance).map_err(|e| {
            LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the instance")
                .with_param("item", "instance")
                .with_detail(e)
        })?;
        write_atomic(&self.json_path(&instance.id), &bytes).await
    }

    pub async fn load(&self, id: &str) -> Result<Instance> {
        check_id(id)?;
        let path = self.json_path(id);
        let bytes = tokio::fs::read(&path).await.map_err(|e| {
            LauncherError::disk(
                ErrorCode::InstanceNotFound,
                format!("Instance \"{id}\" wasn't found"),
            )
            .with_param("id", id)
            .with_detail(e)
        })?;
        serde_json::from_slice(&bytes).map_err(|e| {
            LauncherError::disk(
                ErrorCode::InstanceDamaged,
                format!("The settings file of \"{id}\" is damaged"),
            )
            .with_param("id", id)
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

    /// Every instance, plus the folders whose `instance.json` couldn't be read.
    pub async fn list_all(&self) -> Result<InstanceListing> {
        let mut listing = InstanceListing::default();
        let mut entries = match tokio::fs::read_dir(&self.dir).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(listing),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let id = entry.file_name().to_string_lossy().into_owned();
            // Folders the launcher could never have created are not ours to report.
            if !is_valid_instance_id(&id) || !is_instance_dir(&entry.path()) {
                continue;
            }
            match self.load(&id).await {
                Ok(instance) => listing.instances.push(instance),
                Err(error) => listing.damaged.push(DamagedInstance { id, error }),
            }
        }
        listing.instances.sort_by_key(|i| i.name.to_lowercase());
        listing.damaged.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(listing)
    }

    /// Save changes to an existing instance after validating them.
    pub async fn update(&self, instance: &Instance) -> Result<Instance> {
        check_id(&instance.id)?;
        let mut instance = instance.clone();
        instance.name = validate_name(&instance.name)?;
        validate_java(&instance.java)?;
        self.load(&instance.id).await?;
        self.save(&instance).await?;
        Ok(instance)
    }

    /// Change the display name only; the id and folder stay the same.
    pub async fn rename(&self, id: &str, name: &str) -> Result<Instance> {
        let mut instance = self.load(id).await?;
        instance.name = validate_name(name)?;
        self.save(&instance).await?;
        Ok(instance)
    }

    pub async fn delete(&self, id: &str) -> Result<()> {
        check_id(id)?;
        let dir = self.instance_dir(id);
        if !tokio::fs::try_exists(&dir).await? {
            return Err(LauncherError::disk(
                ErrorCode::InstanceNotFound,
                format!("Instance \"{id}\" wasn't found"),
            )
            .with_param("id", id));
        }
        tokio::fs::remove_dir_all(&dir).await?;
        Ok(())
    }
}

/// True for exactly the ids `create` can produce: a slug (capped at `MAX_SLUG_LEN`) plus an
/// optional `-N` suffix. Rejects path separators, `.`/`..`, absolute paths and Windows device names.
pub fn is_valid_instance_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_LEN && finish_slug(slug_core(id), None) == id
}

fn check_id(id: &str) -> Result<()> {
    if is_valid_instance_id(id) {
        Ok(())
    } else {
        Err(
            LauncherError::disk(ErrorCode::InstanceIdInvalid, "That instance id isn't valid")
                .with_detail(id),
        )
    }
}

fn is_instance_dir(path: &Path) -> bool {
    path.join("instance.json").is_file()
}

const WINDOWS_RESERVED: [&str; 22] = [
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// A folder-safe id: lowercase ASCII letters/digits joined by single dashes, at most
/// `MAX_SLUG_LEN` long; never empty, never a Windows device name.
pub fn slugify(name: &str) -> String {
    finish_slug(slug_core(name), Some(MAX_SLUG_LEN))
}

fn slug_core(name: &str) -> String {
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
    slug
}

fn finish_slug(mut slug: String, cap: Option<usize>) -> String {
    if let Some(cap) = cap {
        slug.truncate(cap); // ASCII only, so any index is a char boundary
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
    fn instance_id_validation() {
        for ok in ["pack-2", "con-instance", "instance"] {
            assert!(is_valid_instance_id(ok), "{ok}");
        }
        for bad in [
            "CON", "Pack", "a--b", "a-", "", ".", "..", "../x", "a/b", "C:\\x", "/abs",
        ] {
            assert!(!is_valid_instance_id(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn load_rejects_unsafe_ids() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path().join("instances"));
        for bad in ["../x", "a/b", "", ".", "C:\\x", "/abs"] {
            let err = store.load(bad).await.unwrap_err();
            assert_eq!(err.category, ErrorCategory::Disk, "{bad}");
        }
    }

    #[tokio::test]
    async fn save_rejects_unsafe_id_and_writes_nothing_outside() {
        let tmp = tempfile::tempdir().unwrap();
        let store_dir = tmp.path().join("instances");
        std::fs::create_dir_all(&store_dir).unwrap();
        let store = InstanceStore::new(&store_dir);
        let mut instance = store.create("Pack", vanilla("1.21.1")).await.unwrap();
        instance.id = "../evil".into();
        let err = store.save(&instance).await.unwrap_err();
        assert_eq!(err.category, ErrorCategory::Disk);
        assert!(!tmp.path().join("evil").exists());
        let entries: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().collect();
        assert_eq!(
            entries.len(),
            1,
            "only the store dir may exist in the parent"
        );
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

    #[test]
    fn mrpack_source_names_the_cached_archive() {
        let s = SourceRef::Mrpack {
            name: "Pack-1.0.mrpack".into(),
            sha1: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
        };
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            r#"{"kind":"mrpack","name":"Pack-1.0.mrpack","sha1":"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"}"#
        );
    }

    use crate::ErrorCode;

    #[tokio::test]
    async fn long_names_cap_slug_and_suffix_stays_valid() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let name = "A".repeat(60);
        let first = store.create(&name, vanilla("1.21.1")).await.unwrap();
        assert_eq!(first.id.len(), MAX_SLUG_LEN);
        let second = store.create(&name, vanilla("1.21.1")).await.unwrap();
        assert_eq!(second.id, format!("{}-2", first.id));
        assert!(is_valid_instance_id(&second.id), "{}", second.id);
        assert_eq!(store.load(&second.id).await.unwrap().name, name);
    }

    #[tokio::test]
    async fn names_are_validated() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        for bad in ["", "   ", &"x".repeat(65)] {
            let err = store.create(bad, vanilla("1.21.1")).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::NameInvalid, "{bad:?}");
        }
        assert_eq!(validate_name("  ñandú  ").unwrap(), "ñandú");
    }

    #[test]
    fn memory_bounds() {
        assert!(validate_memory(MIN_MEMORY_MB).is_ok());
        assert!(validate_memory(MAX_MEMORY_MB).is_ok());
        assert_eq!(
            validate_memory(256).unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
        assert_eq!(
            validate_memory(MAX_MEMORY_MB + 1).unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
    }

    #[tokio::test]
    async fn list_all_reports_damaged_instances() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        store.create("Good", vanilla("1.21.1")).await.unwrap();
        let bad = store.create("Bad", vanilla("1.21.1")).await.unwrap();
        std::fs::write(store.instance_dir(&bad.id).join("instance.json"), "{oops").unwrap();
        std::fs::create_dir_all(tmp.path().join("Not An Id")).unwrap();
        std::fs::write(tmp.path().join("Not An Id/instance.json"), "{}").unwrap();
        let listing = store.list_all().await.unwrap();
        assert_eq!(listing.instances.len(), 1);
        assert_eq!(listing.instances[0].name, "Good");
        assert_eq!(listing.damaged.len(), 1);
        assert_eq!(listing.damaged[0].id, "bad");
        assert_eq!(listing.damaged[0].error.code, ErrorCode::InstanceDamaged);
    }

    #[tokio::test]
    async fn update_rename_and_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path());
        let mut instance = store.create("Pack", vanilla("1.21.1")).await.unwrap();
        instance.java.memory_mb = 6144;
        instance.game.minecraft = "1.20.1".into();
        let saved = store.update(&instance).await.unwrap();
        assert_eq!(store.load("pack").await.unwrap(), saved);

        instance.java.memory_mb = 100;
        assert_eq!(
            store.update(&instance).await.unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
        instance.java.memory_mb = 4096;
        instance.java.path = Some(tmp.path().join("missing-java"));
        assert_eq!(
            store.update(&instance).await.unwrap_err().code,
            ErrorCode::JavaPathMissing
        );

        let renamed = store.rename("pack", "  New name ").await.unwrap();
        assert_eq!(renamed.name, "New name");
        assert_eq!(renamed.id, "pack", "rename keeps the id and folder");

        store.delete("pack").await.unwrap();
        assert!(!store.instance_dir("pack").exists());
        assert_eq!(
            store.delete("pack").await.unwrap_err().code,
            ErrorCode::InstanceNotFound
        );
        assert_eq!(
            store.delete("../x").await.unwrap_err().code,
            ErrorCode::InstanceIdInvalid
        );
    }
}
