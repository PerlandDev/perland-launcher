//! Earlier versions of each wardrobe skin, kept in `<skins>/versions/<id>/`: `versions.json` and one PNG per
//! version. Keyed by skin id only, so it doesn't care how `library.json` lists skins. See
//! `docs/superpowers/specs/2026-11-07-skin-version-history-design.md`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{not_found, now, valid_id, SkinEntry, SkinLibrary, SkinModel};
use crate::error::{ErrorCode, LauncherError, Result};
use crate::fsutil::write_atomic;

/// A save this long after the skin's last change starts a new editing session.
pub(super) const IDLE_SECS: i64 = 5 * 60;
/// Unworn versions kept per skin.
pub(super) const CAP: usize = 30;
/// Worn versions kept per skin, on top of [`CAP`].
pub(super) const WORN_CAP: usize = 10;
pub(super) const INDEX: &str = "versions.json";

/// Why a version was kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionReason {
    /// The texture before an editing session, or when the player left a skin they changed.
    Edited,
    Worn,
    Exported,
    /// Only in histories from before a restore stopped keeping the restored texture a second time.
    Restored,
}

/// One earlier version of a skin, as the history list shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinVersion {
    pub id: String,
    /// Unix seconds the texture was last changed before it was kept.
    pub taken: i64,
    pub reason: VersionReason,
    /// Worn from the launcher; kept longer than the others.
    pub worn: bool,
    /// The skin's texture now is this version (only the newest such version).
    pub current: bool,
    /// The arm model the texture was drawn for; `None` for a version kept before arm models were recorded.
    pub model: Option<SkinModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Record {
    id: String,
    taken: i64,
    reason: VersionReason,
    #[serde(default)]
    worn: bool,
    sha256: String,
    /// The arm model the texture was drawn for; absent in versions kept before it was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<SkinModel>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    /// Oldest first.
    #[serde(default)]
    versions: Vec<Record>,
}

fn version_not_found(id: &str, version: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::SkinVersionNotFound,
        "That version of the skin isn't kept anymore.",
    )
    .with_param("id", id)
    .with_param("version", version)
}

/// Version ids are digits only, so one can never name a path outside the folder.
fn valid_version(version: &str) -> bool {
    !version.is_empty() && version.len() <= 20 && version.chars().all(|c| c.is_ascii_digit())
}

fn sha256(png: &[u8]) -> String {
    hex::encode(Sha256::digest(png))
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub(super) fn versions_dir(skins: &Path, id: &str) -> PathBuf {
    skins.join("versions").join(id)
}

fn png_of(dir: &Path, version: &str) -> PathBuf {
    dir.join(format!("{version}.png"))
}

/// The index of `dir`, without versions whose PNG is gone. A damaged index is set aside and rebuilt from the PNGs.
async fn load(dir: &Path) -> Result<Index> {
    let path = dir.join(INDEX);
    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => Some(bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let parsed = bytes.map(|b| serde_json::from_slice::<Index>(&b));
    let mut index = match parsed {
        None => Index::default(),
        Some(Ok(index)) => index,
        Some(Err(e)) => {
            tracing::warn!("a skin's version list is damaged, rebuilding it: {e}");
            let _ = tokio::fs::rename(&path, dir.join("versions.json.damaged")).await;
            rebuild(dir).await
        }
    };
    let mut kept = Vec::with_capacity(index.versions.len());
    for record in index.versions {
        if valid_version(&record.id)
            && tokio::fs::try_exists(png_of(dir, &record.id))
                .await
                .unwrap_or(false)
        {
            kept.push(record);
        }
    }
    index.versions = kept;
    Ok(index)
}

/// An index read off the PNGs in `dir`: the time is in the name, the rest is lost.
async fn rebuild(dir: &Path) -> Index {
    let mut versions = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            let Some(stem) = path
                .extension()
                .filter(|x| *x == "png")
                .and_then(|_| path.file_stem())
                .and_then(|s| s.to_str())
                .filter(|s| valid_version(s))
            else {
                continue;
            };
            let Ok(png) = tokio::fs::read(&path).await else {
                continue;
            };
            let ms: i64 = stem.parse().unwrap_or(0);
            versions.push(Record {
                id: stem.to_string(),
                taken: ms / 1000,
                reason: VersionReason::Edited,
                worn: false,
                sha256: sha256(&png),
                // Lost with the index.
                model: None,
            });
        }
    }
    versions.sort_by_key(|r| r.id.parse::<u64>().unwrap_or(0));
    Index { versions }
}

async fn store(dir: &Path, index: &Index) -> Result<()> {
    let json = serde_json::to_vec_pretty(index).map_err(|e| {
        LauncherError::disk(
            ErrorCode::DiskWriteFailed,
            "The skin's version list couldn't be saved",
        )
        .with_detail(e.to_string())
    })?;
    write_atomic(&dir.join(INDEX), &json).await
}

/// Drops the oldest versions past the caps, worn and unworn counted apart.
async fn prune(dir: &Path, index: &mut Index) {
    let (mut worn, mut unworn) = (0, 0);
    let mut dropped = Vec::new();
    // Newest first, so the first ones past a cap are the oldest.
    for record in index.versions.iter().rev() {
        let count = if record.worn { &mut worn } else { &mut unworn };
        *count += 1;
        if *count > if record.worn { WORN_CAP } else { CAP } {
            dropped.push(record.id.clone());
        }
    }
    index.versions.retain(|r| !dropped.contains(&r.id));
    for id in dropped {
        let _ = tokio::fs::remove_file(png_of(dir, &id)).await;
    }
}

/// Keeps `png`, drawn for `model`, as a version of the skin in `dir`, unless a version with the same texture and
/// arms is kept already, so the history never lists one twice (then a worn keep just marks that one worn).
async fn keep(
    dir: &Path,
    png: &[u8],
    model: SkinModel,
    taken: i64,
    reason: VersionReason,
) -> Result<()> {
    let mut index = load(dir).await?;
    let hash = sha256(png);
    let worn = reason == VersionReason::Worn;
    if let Some(same) = index
        .versions
        .iter_mut()
        .rev()
        .find(|r| r.sha256 == hash && r.model.is_none_or(|m| m == model))
    {
        if worn && !same.worn {
            same.worn = true;
            store(dir, &index).await?;
        }
        return Ok(());
    }
    let last = index
        .versions
        .last()
        .and_then(|r| r.id.parse::<u64>().ok())
        .unwrap_or(0);
    let id = now_ms().max(last + 1).to_string();
    write_atomic(&png_of(dir, &id), png).await?;
    index.versions.push(Record {
        id,
        taken,
        reason,
        worn,
        sha256: hash,
        model: Some(model),
    });
    prune(dir, &mut index).await;
    store(dir, &index).await
}

impl SkinLibrary {
    fn versions_of(&self, id: &str) -> PathBuf {
        versions_dir(&self.dir, id)
    }

    /// The skin `id`, or not found, under the lock already held.
    pub(super) async fn entry_held(&self, id: &str) -> Result<SkinEntry> {
        if !valid_id(id) {
            return Err(not_found(id));
        }
        self.read()
            .await?
            .skins
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| not_found(id))
    }

    /// Before a save: when it starts a new editing session (or the skin has no versions yet), keeps the texture
    /// about to be overwritten, dated `last_change`.
    pub(super) async fn keep_before_save(&self, id: &str, last_change: i64) -> Result<()> {
        let dir = self.versions_of(id);
        let index = load(&dir).await?;
        if !index.versions.is_empty() && now() - last_change < IDLE_SECS {
            return Ok(());
        }
        let png = self.texture(id).await?;
        // Already the newest version: after a switch of arms its texture is kept with the arms it was drawn for,
        // and the skin's model is already the new one, so it mustn't be kept again with those.
        if index
            .versions
            .last()
            .is_some_and(|r| r.sha256 == sha256(&png))
        {
            return Ok(());
        }
        let model = self.entry_held(id).await?.model;
        keep(&dir, &png, model, last_change, VersionReason::Edited).await
    }

    /// Keeps the texture of `id` now, under the lock already held.
    pub(super) async fn keep_now(&self, id: &str, reason: VersionReason) -> Result<()> {
        let entry = self.entry_held(id).await?;
        let png = self.texture(id).await?;
        keep(
            &self.versions_of(id),
            &png,
            entry.model,
            entry.updated,
            reason,
        )
        .await
    }

    pub(super) async fn forget_versions(&self, id: &str) {
        let _ = tokio::fs::remove_dir_all(self.versions_of(id)).await;
    }

    /// Forgets every kept version of `id`. The skin itself stays as it is.
    pub async fn clear_versions(&self, id: &str) -> Result<()> {
        let _held = self.lock.lock().await;
        self.entry_held(id).await?;
        match tokio::fs::remove_dir_all(self.versions_of(id)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    /// Keeps the texture of `id` as it is now (leaving a changed skin, exporting it).
    pub async fn checkpoint(&self, id: &str, reason: VersionReason) -> Result<()> {
        let _held = self.lock.lock().await;
        self.keep_now(id, reason).await
    }

    /// The versions of `id`, newest first.
    pub async fn versions(&self, id: &str) -> Result<Vec<SkinVersion>> {
        let _held = self.lock.lock().await;
        let model = self.entry_held(id).await?.model;
        let index = load(&self.versions_of(id)).await?;
        // Only the newest version with the texture and the arms now is `current`.
        let mut now = self.texture(id).await.map(|png| sha256(&png)).ok();
        Ok(index
            .versions
            .into_iter()
            .rev()
            .map(|r| SkinVersion {
                current: now.as_deref() == Some(r.sha256.as_str())
                    && r.model.is_none_or(|m| m == model)
                    && now.take().is_some(),
                id: r.id,
                taken: r.taken,
                reason: r.reason,
                worn: r.worn,
                model: r.model,
            })
            .collect())
    }

    /// The texture of version `version` of `id`, as PNG bytes.
    pub async fn version_texture(&self, id: &str, version: &str) -> Result<Vec<u8>> {
        if !valid_id(id) {
            return Err(not_found(id));
        }
        if !valid_version(version) {
            return Err(version_not_found(id, version));
        }
        match tokio::fs::read(png_of(&self.versions_of(id), version)).await {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(version_not_found(id, version))
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Makes version `version` the texture of `id`. The texture it replaces is kept first, so a restore can always be
    /// undone; the restored one is in the history already, so it isn't kept again.
    pub async fn restore(&self, id: &str, version: &str) -> Result<SkinEntry> {
        let png = self.version_texture(id, version).await?;
        let _held = self.lock.lock().await;
        let model = self.model_of(id, version).await?;
        self.keep_now(id, VersionReason::Edited).await?;
        // The version's arms come back with its texture; one kept before arms were recorded keeps the skin's.
        let (file, entry) = self
            .change(id, |e| {
                if let Some(model) = model {
                    e.model = model;
                }
            })
            .await?;
        write_atomic(&self.png(id), &png).await?;
        self.write(&file).await?;
        Ok(entry)
    }

    /// The arm model version `version` of `id` was drawn for, if it was recorded.
    async fn model_of(&self, id: &str, version: &str) -> Result<Option<SkinModel>> {
        Ok(load(&self.versions_of(id))
            .await?
            .versions
            .into_iter()
            .find(|r| r.id == version)
            .and_then(|r| r.model))
    }

    /// A new skin named `name` from version `version` of `id`: same arms, same wardrobe, no history.
    pub async fn copy_version(&self, id: &str, version: &str, name: &str) -> Result<SkinEntry> {
        let png = self.version_texture(id, version).await?;
        if !valid_id(id) {
            return Err(not_found(id));
        }
        // The copy stays in the source's wardrobe, as a skin the player made (never seeded), with the version's arms.
        let source = self.entry(id).await?;
        let model = {
            let _held = self.lock.lock().await;
            self.model_of(id, version).await?
        }
        .unwrap_or(source.model);
        self.create(name, model, &png, source.owner.as_deref())
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::*;
    use image::{Rgba, RgbaImage};

    fn skin(texel: [u8; 4]) -> Vec<u8> {
        let mut image = RgbaImage::new(64, 64);
        image.put_pixel(8, 8, Rgba(texel));
        skins_png::encode(&image).unwrap()
    }

    fn library() -> (tempfile::TempDir, SkinLibrary) {
        let dir = tempfile::tempdir().unwrap();
        let lib = SkinLibrary::new(dir.path().join("skins"));
        (dir, lib)
    }

    /// Pretends `id` was last changed `secs` ago, so the next save starts a new session.
    async fn age(lib: &SkinLibrary, id: &str, secs: i64) {
        let mut file = lib.read().await.unwrap();
        for s in &mut file.skins {
            if s.id == id {
                s.updated -= secs;
            }
        }
        lib.write(&file).await.unwrap();
    }

    async fn texel_of(lib: &SkinLibrary, id: &str, version: &str) -> Rgba<u8> {
        let png = lib.version_texture(id, version).await.unwrap();
        *skins_png::decode(&png).unwrap().get_pixel(8, 8)
    }

    #[tokio::test]
    async fn first_save_keeps_the_original() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        assert!(lib.versions(&a.id).await.unwrap().is_empty());
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].reason, VersionReason::Edited);
        assert_eq!(versions[0].taken, a.updated);
        assert!(!versions[0].current);
        assert_eq!(
            texel_of(&lib, &a.id, &versions[0].id).await,
            Rgba([1, 1, 1, 255])
        );
    }

    #[tokio::test]
    async fn clearing_forgets_every_version_and_keeps_the_skin() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        age(&lib, &a.id, IDLE_SECS + 5).await;
        lib.save_texture(&a.id, &skin([3, 3, 3, 255]))
            .await
            .unwrap();
        assert_eq!(lib.versions(&a.id).await.unwrap().len(), 2);
        lib.clear_versions(&a.id).await.unwrap();
        assert!(lib.versions(&a.id).await.unwrap().is_empty());
        let png = lib.texture(&a.id).await.unwrap();
        assert_eq!(
            *skins_png::decode(&png).unwrap().get_pixel(8, 8),
            Rgba([3, 3, 3, 255])
        );
        // Nothing to clear is not a problem; an unknown skin is.
        lib.clear_versions(&a.id).await.unwrap();
        assert_eq!(
            lib.clear_versions("nope").await.unwrap_err().code,
            ErrorCode::SkinNotFound
        );
        // The next session starts a history again.
        age(&lib, &a.id, IDLE_SECS + 5).await;
        lib.save_texture(&a.id, &skin([4, 4, 4, 255]))
            .await
            .unwrap();
        assert_eq!(lib.versions(&a.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn saves_in_one_session_keep_one_version() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        for n in 2..6 {
            lib.save_texture(&a.id, &skin([n, n, n, 255]))
                .await
                .unwrap();
        }
        assert_eq!(lib.versions(&a.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_save_after_a_pause_keeps_the_last_session() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        age(&lib, &a.id, IDLE_SECS + 5).await;
        lib.save_texture(&a.id, &skin([3, 3, 3, 255]))
            .await
            .unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(
            texel_of(&lib, &a.id, &versions[0].id).await,
            Rgba([2, 2, 2, 255])
        );
        assert_eq!(
            texel_of(&lib, &a.id, &versions[1].id).await,
            Rgba([1, 1, 1, 255])
        );
    }

    #[tokio::test]
    async fn checkpoint_keeps_the_texture_now_once() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Exported)
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].reason, VersionReason::Exported);
        assert!(versions[0].current);
    }

    #[tokio::test]
    async fn a_texture_already_in_the_history_is_not_kept_again() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        // Back to the first texture: it is in the history already, further down.
        lib.save_texture(&a.id, &skin([1, 1, 1, 255]))
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        lib.mark_worn(&a.id, "u1").await.unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 2);
        // The version already kept is the skin now, and wearing it marks it.
        assert!(!versions[0].current && versions[1].current);
        assert!(versions[1].worn);
        assert_eq!(
            texel_of(&lib, &a.id, &versions[1].id).await,
            Rgba([1, 1, 1, 255])
        );
    }

    #[tokio::test]
    async fn wearing_marks_the_version_worn() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        lib.mark_worn(&a.id, "u1").await.unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert!(versions[0].worn);
        assert_eq!(versions[0].reason, VersionReason::Edited);

        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        lib.mark_worn(&a.id, "u1").await.unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].reason, VersionReason::Worn);
        assert!(versions[0].worn && versions[0].current);
    }

    #[tokio::test]
    async fn a_version_keeps_the_arm_model_its_texture_was_drawn_for() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Slim, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        // Switching the arms keeps the texture as it was, with the arms it was drawn for.
        lib.update(&a.id, None, Some(SkinModel::Classic))
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].model, Some(SkinModel::Classic));
        assert!(versions[0].current);
        assert_eq!(versions[1].model, Some(SkinModel::Slim));
        assert_eq!(
            texel_of(&lib, &a.id, &versions[1].id).await,
            Rgba([1, 1, 1, 255])
        );
    }

    #[tokio::test]
    async fn switching_arms_on_a_texture_that_fits_both_is_a_version_of_its_own() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.update(&a.id, None, Some(SkinModel::Slim))
            .await
            .unwrap();
        lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        let models: Vec<_> = versions.iter().map(|v| v.model).collect();
        assert_eq!(models, [Some(SkinModel::Slim), Some(SkinModel::Classic)]);
        // The same texels with other arms are not the skin now.
        assert!(versions[0].current && !versions[1].current);
    }

    #[tokio::test]
    async fn restoring_or_copying_a_version_brings_its_arms_back() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Slim, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.update(&a.id, None, Some(SkinModel::Classic))
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        let slim = versions
            .iter()
            .find(|v| v.model == Some(SkinModel::Slim))
            .unwrap()
            .id
            .clone();
        let copy = lib.copy_version(&a.id, &slim, "Copy").await.unwrap();
        assert_eq!(copy.model, SkinModel::Slim);
        let restored = lib.restore(&a.id, &slim).await.unwrap();
        assert_eq!(restored.model, SkinModel::Slim);
        assert_eq!(lib.entry(&a.id).await.unwrap().model, SkinModel::Slim);
    }

    #[tokio::test]
    async fn a_version_kept_before_arm_models_were_recorded_has_none() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Slim, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        let dir = versions_dir(&lib.dir, &a.id);
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(png_of(&dir, "5"), skin([9, 9, 9, 255]))
            .await
            .unwrap();
        tokio::fs::write(
            dir.join(INDEX),
            r#"{"versions":[{"id":"5","taken":1,"reason":"edited","sha256":"x"}]}"#,
        )
        .await
        .unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions[0].model, None);
        // Restoring it keeps the arms the skin has.
        assert_eq!(
            lib.restore(&a.id, "5").await.unwrap().model,
            SkinModel::Slim
        );
    }

    #[tokio::test]
    async fn restore_keeps_the_current_texture_and_adds_no_copy() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let original = lib.versions(&a.id).await.unwrap()[0].id.clone();
        let entry = lib.restore(&a.id, &original).await.unwrap();
        assert!(entry.updated > a.updated);
        let now = skins_png::decode(&lib.texture(&a.id).await.unwrap()).unwrap();
        assert_eq!(*now.get_pixel(8, 8), Rgba([1, 1, 1, 255]));

        let versions = lib.versions(&a.id).await.unwrap();
        let reasons: Vec<_> = versions.iter().map(|v| v.reason).collect();
        assert_eq!(reasons, vec![VersionReason::Edited, VersionReason::Edited]);
        // The texture the restore replaced is kept, so the restore can be undone.
        assert_eq!(
            texel_of(&lib, &a.id, &versions[0].id).await,
            Rgba([2, 2, 2, 255])
        );
        // The restored version is in the history already: it is the skin now, not a second copy of it.
        assert_eq!(versions[1].id, original);
        assert!(versions[1].current && !versions[0].current);
    }

    #[tokio::test]
    async fn copy_from_a_version_is_a_new_skin_with_no_history() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Slim, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let original = lib.versions(&a.id).await.unwrap()[0].id.clone();
        let copy = lib.copy_version(&a.id, &original, "Old A").await.unwrap();
        assert_ne!(copy.id, a.id);
        assert_eq!((copy.name.as_str(), copy.model), ("Old A", SkinModel::Slim));
        let texels = skins_png::decode(&lib.texture(&copy.id).await.unwrap()).unwrap();
        assert_eq!(*texels.get_pixel(8, 8), Rgba([1, 1, 1, 255]));
        assert!(lib.versions(&copy.id).await.unwrap().is_empty());
        // The source is untouched.
        let source = skins_png::decode(&lib.texture(&a.id).await.unwrap()).unwrap();
        assert_eq!(*source.get_pixel(8, 8), Rgba([2, 2, 2, 255]));
    }

    #[tokio::test]
    async fn copy_from_a_version_stays_in_the_source_wardrobe() {
        let (_dir, lib) = library();
        let a = lib
            .create_seeded("A", SkinModel::Classic, &skin([1, 1, 1, 255]), Some("u1"))
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let original = lib.versions(&a.id).await.unwrap()[0].id.clone();
        let copy = lib.copy_version(&a.id, &original, "Old A").await.unwrap();
        assert_eq!(copy.owner.as_deref(), Some("u1"));
        assert!(!copy.seeded);
    }

    #[tokio::test]
    async fn duplicate_starts_a_fresh_history() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let copy = lib.duplicate(&a.id, "B").await.unwrap();
        assert!(lib.versions(&copy.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_removes_the_versions() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        assert!(versions_dir(lib.dir(), &a.id).exists());
        lib.delete(&a.id).await.unwrap();
        assert!(!versions_dir(lib.dir(), &a.id).exists());
    }

    #[tokio::test]
    async fn pruning_keeps_the_newest_and_the_worn() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([0, 0, 0, 255]), None)
            .await
            .unwrap();
        lib.mark_worn(&a.id, "u1").await.unwrap();
        let worn = lib.versions(&a.id).await.unwrap()[0].id.clone();
        for n in 1..=(CAP as u8 + 2) {
            lib.save_texture(&a.id, &skin([n, 0, 0, 255]))
                .await
                .unwrap();
            lib.checkpoint(&a.id, VersionReason::Edited).await.unwrap();
        }
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.iter().filter(|v| !v.worn).count(), CAP);
        assert!(versions.iter().any(|v| v.id == worn));
        assert_eq!(
            texel_of(&lib, &a.id, &versions[0].id).await[0],
            CAP as u8 + 2
        );
        let pngs = std::fs::read_dir(versions_dir(lib.dir(), &a.id))
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|x| x == "png")
            })
            .count();
        assert_eq!(pngs, CAP + 1);
    }

    #[tokio::test]
    async fn damaged_index_is_set_aside_and_rebuilt() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let dir = versions_dir(lib.dir(), &a.id);
        std::fs::write(dir.join(INDEX), b"{ nope").unwrap();
        let versions = lib.versions(&a.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(
            texel_of(&lib, &a.id, &versions[0].id).await,
            Rgba([1, 1, 1, 255])
        );
        assert!(dir.join("versions.json.damaged").exists());
    }

    #[tokio::test]
    async fn a_version_without_its_png_is_dropped() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_texture(&a.id, &skin([2, 2, 2, 255]))
            .await
            .unwrap();
        let version = lib.versions(&a.id).await.unwrap()[0].id.clone();
        std::fs::remove_file(versions_dir(lib.dir(), &a.id).join(format!("{version}.png")))
            .unwrap();
        assert!(lib.versions(&a.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_versions_and_skins_are_not_found() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        for version in ["123", "../library", "", "abc"] {
            assert_eq!(
                lib.version_texture(&a.id, version).await.unwrap_err().code,
                ErrorCode::SkinVersionNotFound
            );
            assert_eq!(
                lib.restore(&a.id, version).await.unwrap_err().code,
                ErrorCode::SkinVersionNotFound
            );
        }
        for id in ["skin-abc", "../x"] {
            assert_eq!(
                lib.versions(id).await.unwrap_err().code,
                ErrorCode::SkinNotFound
            );
        }
    }
}
