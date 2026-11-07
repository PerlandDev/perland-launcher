//! The player's wardrobe: skins made or imported in the launcher, kept in `<root>/skins`.
//! `library.json` lists them and each texture is `<id>.png`, always a normalised 64 × 64 PNG.
//! A damaged list is set aside and the wardrobe starts empty, so the screen always opens.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::{ErrorCode, LauncherError, Result};
use crate::fsutil::write_atomic;
use crate::instances::validate_name;
use crate::skins_png;

/// The arm width a skin is drawn for: classic arms are 4 texels wide, slim ones 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkinModel {
    #[default]
    Classic,
    Slim,
}

impl SkinModel {
    /// The name Minecraft services use for it.
    pub fn variant(self) -> &'static str {
        match self {
            SkinModel::Classic => "classic",
            SkinModel::Slim => "slim",
        }
    }
}

/// One skin in the wardrobe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkinEntry {
    pub id: String,
    pub name: String,
    pub model: SkinModel,
    /// Unix seconds of the last change.
    pub updated: i64,
    /// The account (dashed uuid) that last wore it from the launcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worn_by: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LibraryFile {
    #[serde(default)]
    skins: Vec<SkinEntry>,
}

const LIST: &str = "library.json";

/// The skins folder. Every change is read, changed and written under one lock.
pub struct SkinLibrary {
    dir: PathBuf,
    lock: Mutex<()>,
}

fn not_found(id: &str) -> LauncherError {
    LauncherError::disk(
        ErrorCode::SkinNotFound,
        "That skin isn't in your wardrobe anymore.",
    )
    .with_param("id", id)
}

fn new_id() -> String {
    let mut bytes = [0u8; 6];
    getrandom::fill(&mut bytes).expect("the operating system has no random source");
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("skin-{hex}")
}

/// Ids are always `skin-` and hex, so one can never name a path outside the folder.
fn valid_id(id: &str) -> bool {
    id.strip_prefix("skin-")
        .is_some_and(|hex| !hex.is_empty() && hex.chars().all(|c| c.is_ascii_hexdigit()))
}

fn now() -> i64 {
    crate::instances::unix_now()
}

impl SkinLibrary {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            lock: Mutex::new(()),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn png(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.png"))
    }

    async fn read(&self) -> Result<LibraryFile> {
        let path = self.dir.join(LIST);
        let bytes = match tokio::fs::read(&path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(LibraryFile::default()),
            Err(e) => return Err(e.into()),
        };
        match serde_json::from_slice::<LibraryFile>(&bytes) {
            Ok(mut file) => {
                file.skins.retain(|s| valid_id(&s.id));
                let mut kept = Vec::with_capacity(file.skins.len());
                for skin in file.skins {
                    if tokio::fs::try_exists(self.png(&skin.id))
                        .await
                        .unwrap_or(false)
                    {
                        kept.push(skin);
                    }
                }
                file.skins = kept;
                Ok(file)
            }
            Err(e) => {
                tracing::warn!("the skin library list is damaged, setting it aside: {e}");
                let _ = tokio::fs::rename(&path, self.dir.join("library.json.damaged")).await;
                Ok(LibraryFile::default())
            }
        }
    }

    async fn write(&self, file: &LibraryFile) -> Result<()> {
        let json = serde_json::to_vec_pretty(file).map_err(|e| {
            LauncherError::disk(
                ErrorCode::DiskWriteFailed,
                "The skin list couldn't be saved",
            )
            .with_detail(e.to_string())
        })?;
        write_atomic(&self.dir.join(LIST), &json).await
    }

    /// Every skin, the most recently changed first.
    pub async fn list(&self) -> Result<Vec<SkinEntry>> {
        let _held = self.lock.lock().await;
        let mut skins = self.read().await?.skins;
        skins.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        Ok(skins)
    }

    /// A new skin named `name` with the texture `png` (checked and normalised first).
    pub async fn create(&self, name: &str, model: SkinModel, png: &[u8]) -> Result<SkinEntry> {
        let name = validate_name(name)?;
        let png = skins_png::normalise(png)?;
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        let entry = SkinEntry {
            id: new_id(),
            name,
            model,
            updated: now(),
            worn_by: None,
        };
        write_atomic(&self.png(&entry.id), &png).await?;
        file.skins.push(entry.clone());
        self.write(&file).await?;
        Ok(entry)
    }

    /// The texture of `id`, as PNG bytes.
    pub async fn texture(&self, id: &str) -> Result<Vec<u8>> {
        if !valid_id(id) {
            return Err(not_found(id));
        }
        match tokio::fs::read(self.png(id)).await {
            Ok(bytes) => Ok(bytes),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(not_found(id)),
            Err(e) => Err(e.into()),
        }
    }

    async fn change(
        &self,
        id: &str,
        edit: impl FnOnce(&mut SkinEntry),
    ) -> Result<(LibraryFile, SkinEntry)> {
        let mut file = self.read().await?;
        let entry = file
            .skins
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or_else(|| not_found(id))?;
        edit(entry);
        entry.updated = now().max(entry.updated + 1);
        let entry = entry.clone();
        Ok((file, entry))
    }

    /// Replaces the texture of `id` with `png` (checked and normalised first).
    pub async fn save_texture(&self, id: &str, png: &[u8]) -> Result<SkinEntry> {
        let png = skins_png::normalise(png)?;
        let _held = self.lock.lock().await;
        let (file, entry) = self.change(id, |_| {}).await?;
        write_atomic(&self.png(id), &png).await?;
        self.write(&file).await?;
        Ok(entry)
    }

    /// Renames `id` and/or changes its arm model.
    pub async fn update(
        &self,
        id: &str,
        name: Option<&str>,
        model: Option<SkinModel>,
    ) -> Result<SkinEntry> {
        let name = name.map(validate_name).transpose()?;
        let _held = self.lock.lock().await;
        let (file, entry) = self
            .change(id, |e| {
                if let Some(name) = name {
                    e.name = name;
                }
                if let Some(model) = model {
                    e.model = model;
                }
            })
            .await?;
        self.write(&file).await?;
        Ok(entry)
    }

    /// A copy of `id` named `name`.
    pub async fn duplicate(&self, id: &str, name: &str) -> Result<SkinEntry> {
        let name = validate_name(name)?;
        let png = self.texture(id).await?;
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        let model = file
            .skins
            .iter()
            .find(|s| s.id == id)
            .ok_or_else(|| not_found(id))?
            .model;
        let entry = SkinEntry {
            id: new_id(),
            name,
            model,
            updated: now(),
            worn_by: None,
        };
        write_atomic(&self.png(&entry.id), &png).await?;
        file.skins.push(entry.clone());
        self.write(&file).await?;
        Ok(entry)
    }

    /// Removes `id` and its texture.
    pub async fn delete(&self, id: &str) -> Result<()> {
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        let before = file.skins.len();
        file.skins.retain(|s| s.id != id);
        if file.skins.len() == before {
            return Err(not_found(id));
        }
        self.write(&file).await?;
        let _ = tokio::fs::remove_file(self.png(id)).await;
        Ok(())
    }

    /// Records that `uuid` now wears `id`; no other skin keeps that account's lamp.
    pub async fn mark_worn(&self, id: &str, uuid: &str) -> Result<()> {
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        if !file.skins.iter().any(|s| s.id == id) {
            return Err(not_found(id));
        }
        for skin in &mut file.skins {
            if skin.id == id {
                skin.worn_by = Some(uuid.to_string());
            } else if skin.worn_by.as_deref() == Some(uuid) {
                skin.worn_by = None;
            }
        }
        self.write(&file).await
    }
}

#[cfg(test)]
mod tests {
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

    #[tokio::test]
    async fn create_then_list_and_texture() {
        let (_dir, lib) = library();
        let entry = lib
            .create("  Mine  ", SkinModel::Slim, &skin([1, 2, 3, 255]))
            .await
            .unwrap();
        assert_eq!(entry.name, "Mine");
        assert!(valid_id(&entry.id));
        assert_eq!(lib.list().await.unwrap(), vec![entry.clone()]);
        let texels = skins_png::decode(&lib.texture(&entry.id).await.unwrap()).unwrap();
        assert_eq!(*texels.get_pixel(8, 8), Rgba([1, 2, 3, 255]));
    }

    #[tokio::test]
    async fn create_refuses_a_bad_texture_and_name() {
        let (_dir, lib) = library();
        let e = lib
            .create("A", SkinModel::Classic, b"nope")
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
        let e = lib
            .create("   ", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::NameInvalid);
        assert!(lib.list().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_texture_bumps_updated() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        let saved = lib
            .save_texture(&entry.id, &skin([9, 9, 9, 255]))
            .await
            .unwrap();
        assert!(saved.updated > entry.updated);
        let texels = skins_png::decode(&lib.texture(&entry.id).await.unwrap()).unwrap();
        assert_eq!(*texels.get_pixel(8, 8), Rgba([9, 9, 9, 255]));
    }

    #[tokio::test]
    async fn update_renames_and_changes_model() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        let entry = lib
            .update(&entry.id, Some("B"), Some(SkinModel::Slim))
            .await
            .unwrap();
        assert_eq!((entry.name.as_str(), entry.model), ("B", SkinModel::Slim));
    }

    #[tokio::test]
    async fn duplicate_copies_texture() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Slim, &skin([4, 4, 4, 255]))
            .await
            .unwrap();
        let copy = lib.duplicate(&entry.id, "A copy").await.unwrap();
        assert_ne!(copy.id, entry.id);
        assert_eq!(copy.model, SkinModel::Slim);
        assert_eq!(
            lib.texture(&copy.id).await.unwrap(),
            lib.texture(&entry.id).await.unwrap()
        );
        assert_eq!(lib.list().await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn delete_removes_png() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        lib.delete(&entry.id).await.unwrap();
        assert!(lib.list().await.unwrap().is_empty());
        assert!(!lib.png(&entry.id).exists());
    }

    #[tokio::test]
    async fn mark_worn_moves_lamp() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        let b = lib
            .create("B", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        lib.mark_worn(&a.id, "u1").await.unwrap();
        lib.mark_worn(&b.id, "u1").await.unwrap();
        let skins = lib.list().await.unwrap();
        let worn: Vec<_> = skins
            .iter()
            .filter(|s| s.worn_by.is_some())
            .map(|s| &s.id)
            .collect();
        assert_eq!(worn, vec![&b.id]);
    }

    #[tokio::test]
    async fn unknown_id_is_not_found() {
        let (_dir, lib) = library();
        for id in ["skin-abc", "../library", ""] {
            assert_eq!(
                lib.texture(id).await.unwrap_err().code,
                ErrorCode::SkinNotFound
            );
        }
        assert_eq!(
            lib.delete("skin-abc").await.unwrap_err().code,
            ErrorCode::SkinNotFound
        );
        let e = lib
            .save_texture("skin-abc", &skin([0; 4]))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinNotFound);
    }

    #[tokio::test]
    async fn damaged_library_starts_empty() {
        let (_dir, lib) = library();
        tokio::fs::create_dir_all(lib.dir()).await.unwrap();
        tokio::fs::write(lib.dir().join(LIST), b"{ not json")
            .await
            .unwrap();
        assert!(lib.list().await.unwrap().is_empty());
        assert!(lib.dir().join("library.json.damaged").exists());
        lib.create("A", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        assert_eq!(lib.list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn entry_without_png_is_dropped() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]))
            .await
            .unwrap();
        tokio::fs::remove_file(lib.png(&entry.id)).await.unwrap();
        assert!(lib.list().await.unwrap().is_empty());
    }
}
