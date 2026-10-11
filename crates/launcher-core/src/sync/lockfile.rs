//! The two files sync keeps in `.perland/`: `pack.json`, the resolved pack that is installed
//! (so a pinned or offline instance can repair itself and optional toggles need no network),
//! and `managed.json`, what sync wrote. `managed.json` is always written last (spec §3.3).
use std::{
    collections::{BTreeMap, HashSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{
    download::Hashes,
    error::{ErrorCode, LauncherError, Result},
    fsutil::{normalize_rel, path_key, write_atomic},
    sources::{DesiredState, FileMode},
};

pub const MANAGED_FILE: &str = "managed.json";
pub const PACK_FILE: &str = "pack.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedFile {
    pub name: String,
    /// The sha1 of the bytes sync wrote. Empty for a player's file that sync kept in place:
    /// it never matches, so the file stays the player's.
    pub sha1: String,
    pub size: u64,
    pub mode: FileMode,
    /// The hashes the pack declared, for changelogs ("updated" means these changed).
    pub declared: Hashes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Managed {
    pub version: u32,
    pub stamp: String,
    pub pack_version: String,
    /// The optional-file choices this state was built with.
    #[serde(default)]
    pub choices: BTreeMap<String, bool>,
    #[serde(default)]
    pub files: BTreeMap<String, ManagedFile>,
}

fn save_error(item: &str, e: impl std::fmt::Display) -> LauncherError {
    LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the pack's state")
        .with_param("item", item)
        .with_detail(e)
}

impl Managed {
    pub const VERSION: u32 = 1;

    /// `None` when there is no lockfile yet, or it can't be read. Sync then treats every file
    /// on disk that the pack doesn't list as the player's, and rewrites the lockfile. Entries
    /// whose path isn't canonical are dropped: sync moves lockfile paths to the trash, so a
    /// hand-edited `../` must never reach it.
    pub async fn load(perland_dir: &Path) -> Option<Managed> {
        let bytes = tokio::fs::read(perland_dir.join(MANAGED_FILE)).await.ok()?;
        let mut managed: Managed = serde_json::from_slice(&bytes).ok()?;
        if managed.version != Self::VERSION {
            return None;
        }
        managed
            .files
            .retain(|path, _| normalize_rel(path).as_deref() == Some(path.as_str()));
        Some(managed)
    }

    pub async fn save(&self, perland_dir: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| save_error("managed", e))?;
        write_atomic(&perland_dir.join(MANAGED_FILE), &bytes).await
    }
}

pub async fn load_pack(perland_dir: &Path) -> Option<DesiredState> {
    let bytes = tokio::fs::read(perland_dir.join(PACK_FILE)).await.ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub async fn save_pack(perland_dir: &Path, desired: &DesiredState) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(desired).map_err(|e| save_error("pack", e))?;
    write_atomic(&perland_dir.join(PACK_FILE), &bytes).await
}

/// "Your additions": jars in `mods/` that sync doesn't manage. Sync never touches these.
pub async fn additions(game_dir: &Path, managed: Option<&Managed>) -> Result<Vec<String>> {
    let mut out = Vec::new();
    // Keys compare as the file system does: a pack that renamed `foo.jar` to `Foo.jar` still
    // owns the `foo.jar` on disk.
    let managed_keys: HashSet<String> = managed
        .map(|m| m.files.keys().map(|k| path_key(k)).collect())
        .unwrap_or_default();
    let mut entries = match tokio::fs::read_dir(game_dir.join("mods")).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if !entry.file_type().await?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        if !(lower.ends_with(".jar") || lower.ends_with(".jar.disabled")) {
            continue;
        }
        let key = path_key(&format!("mods/{name}"));
        let key = key.strip_suffix(".disabled").unwrap_or(&key);
        if !managed_keys.contains(key) {
            out.push(name);
        }
    }
    out.sort_by_key(|n| n.to_lowercase());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::{FileMode, PackAbout};

    fn managed() -> Managed {
        let mut files = BTreeMap::new();
        files.insert(
            "mods/a.jar".to_string(),
            ManagedFile {
                name: "A".into(),
                sha1: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
                size: 5,
                mode: FileMode::Replace,
                declared: Hashes::sha1("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
            },
        );
        Managed {
            version: Managed::VERSION,
            stamp: "s1".into(),
            pack_version: "1.0.3".into(),
            choices: BTreeMap::from([("mods/opt.jar".to_string(), false)]),
            files,
        }
    }

    #[tokio::test]
    async fn managed_round_trips_and_tolerates_damage() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(Managed::load(dir.path()).await, None);
        managed().save(dir.path()).await.unwrap();
        assert_eq!(Managed::load(dir.path()).await, Some(managed()));
        let raw = std::fs::read_to_string(dir.path().join(MANAGED_FILE)).unwrap();
        assert!(raw.contains("\"packVersion\": \"1.0.3\""), "{raw}");
        std::fs::write(dir.path().join(MANAGED_FILE), "{oops").unwrap();
        assert_eq!(Managed::load(dir.path()).await, None);
        let mut future = managed();
        future.version = 99;
        future.save(dir.path()).await.unwrap();
        assert_eq!(
            Managed::load(dir.path()).await,
            None,
            "a newer launcher's lockfile isn't guessed at"
        );

        let mut tampered = managed();
        let entry = tampered.files["mods/a.jar"].clone();
        tampered
            .files
            .insert("../../outside.txt".into(), entry.clone());
        tampered.files.insert("mods\\b.jar".into(), entry);
        tampered.save(dir.path()).await.unwrap();
        let loaded = Managed::load(dir.path()).await.unwrap();
        assert_eq!(loaded.files.keys().collect::<Vec<_>>(), vec!["mods/a.jar"]);
    }

    #[tokio::test]
    async fn pack_snapshot_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_pack(dir.path()).await, None);
        let desired = DesiredState {
            name: "Asalith Fields".into(),
            pack_version: "1.0.3".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files: vec![],
            stamp: "abc".into(),
            about: PackAbout::default(),
            archive: None,
        };
        save_pack(dir.path(), &desired).await.unwrap();
        assert_eq!(load_pack(dir.path()).await, Some(desired));
    }

    #[tokio::test]
    async fn additions_are_unmanaged_jars_in_mods() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for f in ["a.jar", "Mine.jar", "old.jar.disabled", "notes.txt"] {
            std::fs::write(mods.join(f), "x").unwrap();
        }
        std::fs::create_dir_all(dir.path().join("config")).unwrap();
        std::fs::write(dir.path().join("config/c.jar"), "x").unwrap();
        let found = additions(dir.path(), Some(&managed())).await.unwrap();
        assert_eq!(found, vec!["Mine.jar", "old.jar.disabled"]);
        let mut all = additions(dir.path(), None).await.unwrap();
        all.sort();
        assert_eq!(all, vec!["Mine.jar", "a.jar", "old.jar.disabled"]);
        let empty = tempfile::tempdir().unwrap();
        assert!(additions(empty.path(), None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn additions_match_managed_paths_ignoring_case() {
        // A pack renamed `mods/foo.jar` to `mods/Foo.jar` with the same bytes: plan() keeps the
        // on-disk `foo.jar` while the lockfile records `mods/Foo.jar`. It is still the pack's.
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for f in ["foo.jar", "BAR.jar.disabled", "mine.jar"] {
            std::fs::write(mods.join(f), "x").unwrap();
        }
        let mut lock = managed();
        let entry = lock.files["mods/a.jar"].clone();
        lock.files.insert("mods/Foo.jar".into(), entry.clone());
        lock.files.insert("mods/bar.jar".into(), entry);
        let found = additions(dir.path(), Some(&lock)).await.unwrap();
        assert_eq!(found, vec!["mine.jar"]);
    }
}
