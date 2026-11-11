//! The players' wardrobes: skins made or imported in the launcher, kept in `<root>/skins`.
//! `library.json` lists them and each texture is `<id>.png`, always a normalised 64 × 64 PNG.
//! Every account has its own wardrobe (an entry's `owner`); a skin made with no account signed
//! in is loose until an account adopts it. A damaged list is set aside and the wardrobe starts
//! empty, so the screen always opens. See `docs/superpowers/specs/2026-11-07-per-account-wardrobes-design.md`.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::error::{ErrorCode, LauncherError, Result};
use crate::fsutil::write_atomic;
use crate::instances::validate_name;
use crate::skin_catalog::Manifest;
use crate::skins_png;

mod versions;
pub use versions::{SkinVersion, VersionReason};

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

/// A catalog piece a skin wears, with the colours picked for its areas.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WornPiece {
    pub id: String,
    /// Area id → `#rrggbb`.
    pub colours: BTreeMap<String, String>,
}

/// How a skin was put together from the catalog. `<id>.png` is the finished image; the
/// player's own painting, when there is one, is kept beside it as `<id>.paint.png`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinLook {
    pub pieces: Vec<WornPiece>,
    pub painting: bool,
}

/// Most pieces one look may wear.
const MAX_PIECES: usize = 16;

fn look_invalid(why: &str) -> LauncherError {
    LauncherError::new(
        crate::error::ErrorCategory::Integrity,
        ErrorCode::SkinInvalid,
        "That look can't be saved.",
    )
    .with_detail(why.to_string())
}

fn valid_colour(c: &str) -> bool {
    c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|x| x.is_ascii_hexdigit())
}

fn check_look(look: &SkinLook) -> Result<()> {
    if look.pieces.len() > MAX_PIECES {
        return Err(look_invalid("too many pieces"));
    }
    let mut seen = BTreeSet::new();
    for piece in &look.pieces {
        if !seen.insert(piece.id.as_str()) {
            return Err(look_invalid("a piece is worn twice"));
        }
        if piece.colours.values().any(|c| !valid_colour(c)) {
            return Err(look_invalid("a colour isn't #rrggbb"));
        }
    }
    Ok(())
}

/// One skin in the wardrobe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkinEntry {
    pub id: String,
    pub name: String,
    pub model: SkinModel,
    /// Unix seconds of the last change.
    pub updated: i64,
    /// The account (dashed uuid) that last wore it from the launcher.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worn_by: Option<String>,
    /// The account (dashed uuid) whose wardrobe it's in; `None` while it's loose.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Put in an empty wardrobe by the launcher and never changed since: an account adopting
    /// loose skins drops it rather than keep a starter the player never chose.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub seeded: bool,
    /// The catalog pieces it was built from, when it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<SkinLook>,
}

#[derive(Debug, Serialize, Deserialize)]
struct LibraryFile {
    /// 1 (absent): one wardrobe for everyone. 2: an `owner` per entry. 3: each wardrobe in the
    /// player's order, the order of `skins`.
    #[serde(default = "version_one")]
    version: u32,
    #[serde(default)]
    skins: Vec<SkinEntry>,
}

impl Default for LibraryFile {
    fn default() -> Self {
        Self {
            version: VERSION,
            skins: Vec::new(),
        }
    }
}

fn version_one() -> u32 {
    1
}

const LIST: &str = "library.json";
const VERSION: u32 = 3;

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

    fn paint_png(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.paint.png"))
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
                if file.version < 2 {
                    // One wardrobe for everyone: a skin worn from the launcher goes to the account
                    // that wore it, the rest are loose until an account adopts them.
                    for skin in &mut file.skins {
                        skin.owner = skin.worn_by.clone();
                    }
                    file.version = 2;
                }
                if file.version < 3 {
                    // Each wardrobe showed its worn skin first, then the most recently changed: keep
                    // that as the player's order.
                    file.skins.sort_by(|a, b| {
                        let wears = |s: &SkinEntry| s.owner.is_some() && s.worn_by == s.owner;
                        wears(b)
                            .cmp(&wears(a))
                            .then_with(|| b.updated.cmp(&a.updated))
                            .then_with(|| a.id.cmp(&b.id))
                    });
                    file.version = VERSION;
                }
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

    /// The wardrobe of `owner` (the loose skins for `None`), in the player's order.
    pub async fn list(&self, owner: Option<&str>) -> Result<Vec<SkinEntry>> {
        let _held = self.lock.lock().await;
        Ok(wardrobe(self.read().await?.skins, owner))
    }

    /// The entry `id`, whoever's wardrobe it's in.
    pub async fn entry(&self, id: &str) -> Result<SkinEntry> {
        let _held = self.lock.lock().await;
        self.read()
            .await?
            .skins
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| not_found(id))
    }

    /// A new skin named `name` with the texture `png` (checked and normalised first), in the
    /// wardrobe of `owner`.
    pub async fn create(
        &self,
        name: &str,
        model: SkinModel,
        png: &[u8],
        owner: Option<&str>,
    ) -> Result<SkinEntry> {
        self.add(name, model, png, owner, false).await
    }

    /// Like [`Self::create`], for the starter the launcher puts in an empty wardrobe by itself.
    pub async fn create_seeded(
        &self,
        name: &str,
        model: SkinModel,
        png: &[u8],
        owner: Option<&str>,
    ) -> Result<SkinEntry> {
        self.add(name, model, png, owner, true).await
    }

    async fn add(
        &self,
        name: &str,
        model: SkinModel,
        png: &[u8],
        owner: Option<&str>,
        seeded: bool,
    ) -> Result<SkinEntry> {
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
            owner: owner.map(str::to_string),
            seeded,
            look: None,
        };
        write_atomic(&self.png(&entry.id), &png).await?;
        // A new skin goes on top of its wardrobe.
        file.skins.insert(0, entry.clone());
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
        entry.seeded = false;
        entry.updated = now().max(entry.updated + 1);
        let entry = entry.clone();
        Ok((file, entry))
    }

    /// Replaces the texture of `id` with `png` (checked and normalised first).
    pub async fn save_texture(&self, id: &str, png: &[u8]) -> Result<SkinEntry> {
        let png = skins_png::normalise(png)?;
        let _held = self.lock.lock().await;
        let mut last_change = 0;
        let (file, entry) = self.change(id, |e| last_change = e.updated).await?;
        self.keep_before_save(id, last_change).await?;
        write_atomic(&self.png(id), &png).await?;
        self.write(&file).await?;
        Ok(entry)
    }

    /// Saves the look of `id`: the finished `texture` (`<id>.png`) and, when the player painted
    /// on it, their `painting` (`<id>.paint.png`; removed when `None`). A history version of the
    /// finished image is taken as `save_texture` does. `look.painting` is derived from whether
    /// `painting` bytes are passed; the caller's value is ignored.
    pub async fn save_look(
        &self,
        id: &str,
        mut look: SkinLook,
        painting: Option<&[u8]>,
        texture: &[u8],
    ) -> Result<SkinEntry> {
        check_look(&look)?;
        let texture = skins_png::normalise(texture)?;
        let painting = painting.map(skins_png::normalise).transpose()?;
        look.painting = painting.is_some();
        let _held = self.lock.lock().await;
        let mut last_change = 0;
        let (file, entry) = self
            .change(id, |e| {
                last_change = e.updated;
                e.look = Some(look);
            })
            .await?;
        self.keep_before_save(id, last_change).await?;
        match &painting {
            Some(bytes) => write_atomic(&self.paint_png(id), bytes).await?,
            None => {
                let _ = tokio::fs::remove_file(self.paint_png(id)).await;
            }
        }
        write_atomic(&self.png(id), &texture).await?;
        self.write(&file).await?;
        Ok(entry)
    }

    /// Drops the look of `id` and its painting; `<id>.png` stays as a plain skin.
    pub async fn clear_look(&self, id: &str) -> Result<SkinEntry> {
        let _held = self.lock.lock().await;
        let (file, entry) = self.change(id, |e| e.look = None).await?;
        self.write(&file).await?;
        let _ = tokio::fs::remove_file(self.paint_png(id)).await;
        Ok(entry)
    }

    /// The player's own painting of `id`; `None` unless its look says it has one.
    pub async fn painting(&self, id: &str) -> Result<Option<Vec<u8>>> {
        if !valid_id(id) {
            return Err(not_found(id));
        }
        if !self.entry(id).await?.look.is_some_and(|l| l.painting) {
            return Ok(None);
        }
        match tokio::fs::read(self.paint_png(id)).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Every catalog file of every piece any skin's look wears, across all owners. Pieces the
    /// manifest doesn't list are skipped. No list yet is an empty set; a list that can't be read
    /// or parsed is an error, so nobody prunes files a look may still need.
    pub async fn look_hashes(&self, manifest: &Manifest) -> Result<BTreeSet<String>> {
        let _held = self.lock.lock().await;
        let mut out = BTreeSet::new();
        let bytes = match tokio::fs::read(self.dir.join(LIST)).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        let file = serde_json::from_slice::<LibraryFile>(&bytes).map_err(|e| {
            LauncherError::disk(ErrorCode::DiskIo, "The skin list couldn't be read")
                .with_detail(e.to_string())
        })?;
        for piece in file
            .skins
            .iter()
            .filter_map(|s| s.look.as_ref())
            .flat_map(|l| l.pieces.iter())
        {
            if let Some(found) = manifest.pieces.iter().find(|p| p.id == piece.id) {
                out.insert(found.thumb.clone());
                for files in found.files.values() {
                    out.insert(files.art.clone());
                    out.insert(files.mask.clone());
                }
            }
        }
        Ok(out)
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
        // A switch of arms keeps the texture first, with the arms it was drawn for: the redrawn arms that follow
        // are a new version, and the old one can be restored as it was.
        if let Some(model) = model {
            if self.entry_held(id).await?.model != model {
                self.keep_now(id, VersionReason::Edited).await?;
            }
        }
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
        let at = file
            .skins
            .iter()
            .position(|s| s.id == id)
            .ok_or_else(|| not_found(id))?;
        let source = &file.skins[at];
        let mut look = source.look.clone();
        let paint = if look.as_ref().is_some_and(|l| l.painting) {
            tokio::fs::read(self.paint_png(id)).await.ok()
        } else {
            None
        };
        if paint.is_none() {
            if let Some(l) = look.as_mut() {
                l.painting = false;
            }
        }
        let entry = SkinEntry {
            id: new_id(),
            name,
            model: source.model,
            updated: now(),
            worn_by: None,
            owner: source.owner.clone(),
            seeded: false,
            look,
        };
        write_atomic(&self.png(&entry.id), &png).await?;
        if let Some(paint) = paint {
            write_atomic(&self.paint_png(&entry.id), &paint).await?;
        }
        // A copy goes right below its source.
        file.skins.insert(at + 1, entry.clone());
        self.write(&file).await?;
        Ok(entry)
    }

    /// Puts the wardrobe of `owner` in the order of `ids`. Its skins left out of `ids` follow,
    /// in the order they had; ids that aren't in that wardrobe are ignored. Moving a skin isn't
    /// changing it: its time and versions stay as they are. Answers the wardrobe as it now stands.
    pub async fn reorder(&self, owner: Option<&str>, ids: &[String]) -> Result<Vec<SkinEntry>> {
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        let mine = |s: &SkinEntry| s.owner.as_deref() == owner;
        // The places this wardrobe's skins take in the list, which the new order fills.
        let places: Vec<usize> = (0..file.skins.len())
            .filter(|&i| mine(&file.skins[i]))
            .collect();
        let mut rest: Vec<SkinEntry> = places.iter().map(|&i| file.skins[i].clone()).collect();
        let mut order = Vec::with_capacity(rest.len());
        for id in ids {
            if let Some(at) = rest.iter().position(|s| &s.id == id) {
                order.push(rest.remove(at));
            }
        }
        order.append(&mut rest);
        let changed = places
            .iter()
            .zip(&order)
            .any(|(&i, s)| file.skins[i].id != s.id);
        for (&i, skin) in places.iter().zip(order) {
            file.skins[i] = skin;
        }
        if changed {
            self.write(&file).await?;
        }
        Ok(wardrobe(file.skins, owner))
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
        let _ = tokio::fs::remove_file(self.paint_png(id)).await;
        self.forget_versions(id).await;
        Ok(())
    }

    /// Records that `uuid` now wears `id`; no other skin keeps that account's lamp.
    pub async fn mark_worn(&self, id: &str, uuid: &str) -> Result<()> {
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        if !file.skins.iter().any(|s| s.id == id) {
            return Err(not_found(id));
        }
        worn(&mut file, id, uuid);
        self.write(&file).await?;
        self.keep_now(id, VersionReason::Worn).await
    }

    /// Moves every loose skin into the wardrobe of `uuid`, except a starter the player never
    /// changed, which is dropped instead: one marked seeded, or one never worn whose texels are
    /// exactly `starter` (the launcher's starter as a PNG: entries from before the mark existed).
    /// Answers how many moved.
    pub async fn adopt_loose(&self, uuid: &str, starter: Option<&[u8]>) -> Result<usize> {
        let starter = starter.and_then(|png| skins_png::decode(png).ok());
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        let mut dropped = Vec::new();
        for skin in file.skins.iter().filter(|s| s.owner.is_none()) {
            let untouched = skin.seeded
                || match &starter {
                    Some(starter) if skin.worn_by.is_none() => {
                        match tokio::fs::read(self.png(&skin.id)).await {
                            Ok(bytes) => skins_png::decode(&bytes).is_ok_and(|t| &t == starter),
                            Err(_) => false,
                        }
                    }
                    _ => false,
                };
            if untouched {
                dropped.push(skin.id.clone());
            }
        }
        file.skins.retain(|s| !dropped.contains(&s.id));
        let mut moved = 0;
        for skin in file.skins.iter_mut().filter(|s| s.owner.is_none()) {
            skin.owner = Some(uuid.to_string());
            moved += 1;
        }
        if moved > 0 || !dropped.is_empty() {
            self.write(&file).await?;
        }
        for id in dropped {
            let _ = tokio::fs::remove_file(self.png(&id)).await;
            let _ = tokio::fs::remove_file(self.paint_png(&id)).await;
            self.forget_versions(&id).await;
        }
        Ok(moved)
    }

    /// Records the skin `uuid` really wears (`png`, drawn for `model`): the entry in its wardrobe
    /// with the same texels and arm model, or a new one named `name`. Either way it's marked worn.
    pub async fn record_current(
        &self,
        uuid: &str,
        name: &str,
        model: SkinModel,
        png: &[u8],
    ) -> Result<SkinEntry> {
        let name = validate_name(name)?;
        let png = skins_png::normalise(png)?;
        let texels = skins_png::decode(&png)?;
        let _held = self.lock.lock().await;
        let mut file = self.read().await?;
        let mut found = None;
        for skin in file
            .skins
            .iter()
            .filter(|s| s.owner.as_deref() == Some(uuid) && s.model == model)
        {
            let same = match tokio::fs::read(self.png(&skin.id)).await {
                Ok(bytes) => skins_png::decode(&bytes).is_ok_and(|t| t == texels),
                Err(_) => false,
            };
            if same {
                found = Some(skin.id.clone());
                break;
            }
        }
        let id = match found {
            Some(id) => id,
            None => {
                let entry = SkinEntry {
                    id: new_id(),
                    name,
                    model,
                    updated: now(),
                    worn_by: None,
                    owner: Some(uuid.to_string()),
                    seeded: false,
                    look: None,
                };
                write_atomic(&self.png(&entry.id), &png).await?;
                let id = entry.id.clone();
                file.skins.insert(0, entry);
                id
            }
        };
        let changed = worn(&mut file, &id, uuid);
        let entry = file
            .skins
            .iter()
            .find(|s| s.id == id)
            .cloned()
            .ok_or_else(|| not_found(&id))?;
        if changed {
            self.write(&file).await?;
        }
        Ok(entry)
    }
}

/// Gives `uuid`'s lamp to `id` alone; answers whether anything changed.
fn worn(file: &mut LibraryFile, id: &str, uuid: &str) -> bool {
    let mut changed = false;
    for skin in &mut file.skins {
        if skin.id == id {
            if skin.worn_by.as_deref() != Some(uuid) {
                skin.worn_by = Some(uuid.to_string());
                changed = true;
            }
        } else if skin.worn_by.as_deref() == Some(uuid) {
            skin.worn_by = None;
            changed = true;
        }
    }
    changed
}

/// The skins in `owner`'s wardrobe, in the player's order.
fn wardrobe(mut skins: Vec<SkinEntry>, owner: Option<&str>) -> Vec<SkinEntry> {
    skins.retain(|s| s.owner.as_deref() == owner);
    skins
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
            .create("  Mine  ", SkinModel::Slim, &skin([1, 2, 3, 255]), None)
            .await
            .unwrap();
        assert_eq!(entry.name, "Mine");
        assert!(valid_id(&entry.id));
        assert_eq!(lib.list(None).await.unwrap(), vec![entry.clone()]);
        let texels = skins_png::decode(&lib.texture(&entry.id).await.unwrap()).unwrap();
        assert_eq!(*texels.get_pixel(8, 8), Rgba([1, 2, 3, 255]));
    }

    #[tokio::test]
    async fn create_refuses_a_bad_texture_and_name() {
        let (_dir, lib) = library();
        let e = lib
            .create("A", SkinModel::Classic, b"nope", None)
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
        let e = lib
            .create("   ", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::NameInvalid);
        assert!(lib.list(None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn save_texture_bumps_updated() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), None)
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
            .create("A", SkinModel::Classic, &skin([0; 4]), None)
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
            .create("A", SkinModel::Slim, &skin([4, 4, 4, 255]), None)
            .await
            .unwrap();
        let copy = lib.duplicate(&entry.id, "A copy").await.unwrap();
        assert_ne!(copy.id, entry.id);
        assert_eq!(copy.model, SkinModel::Slim);
        assert_eq!(
            lib.texture(&copy.id).await.unwrap(),
            lib.texture(&entry.id).await.unwrap()
        );
        assert_eq!(lib.list(None).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn delete_removes_png() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        lib.delete(&entry.id).await.unwrap();
        assert!(lib.list(None).await.unwrap().is_empty());
        assert!(!lib.png(&entry.id).exists());
    }

    #[tokio::test]
    async fn mark_worn_moves_lamp() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        let b = lib
            .create("B", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        lib.mark_worn(&a.id, "u1").await.unwrap();
        lib.mark_worn(&b.id, "u1").await.unwrap();
        let skins = lib.list(None).await.unwrap();
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
        assert!(lib.list(None).await.unwrap().is_empty());
        assert!(lib.dir().join("library.json.damaged").exists());
        lib.create("A", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        assert_eq!(lib.list(None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn each_account_lists_only_its_own_wardrobe() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), Some("u1"))
            .await
            .unwrap();
        let b = lib
            .create("B", SkinModel::Classic, &skin([0; 4]), Some("u2"))
            .await
            .unwrap();
        let loose = lib
            .create("L", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        assert_eq!(a.owner.as_deref(), Some("u1"));
        let ids = |v: Vec<SkinEntry>| v.into_iter().map(|e| e.id).collect::<Vec<_>>();
        assert_eq!(ids(lib.list(Some("u1")).await.unwrap()), vec![a.id.clone()]);
        assert_eq!(ids(lib.list(Some("u2")).await.unwrap()), vec![b.id.clone()]);
        assert_eq!(ids(lib.list(None).await.unwrap()), vec![loose.id]);
        let copy = lib.duplicate(&a.id, "A copy").await.unwrap();
        assert_eq!(copy.owner.as_deref(), Some("u1"));
    }

    fn ids(list: Vec<SkinEntry>) -> Vec<String> {
        list.into_iter().map(|e| e.id).collect()
    }

    #[tokio::test]
    async fn new_skins_go_on_top_and_changing_one_never_moves_it() {
        let (_dir, lib) = library();
        let old = lib
            .create("Old", SkinModel::Classic, &skin([0; 4]), Some("u1"))
            .await
            .unwrap();
        let new = lib
            .create("New", SkinModel::Classic, &skin([1; 4]), Some("u1"))
            .await
            .unwrap();
        assert_eq!(
            ids(lib.list(Some("u1")).await.unwrap()),
            [new.id.as_str(), old.id.as_str()]
        );
        lib.save_texture(old.id.as_str(), &skin([2; 4]))
            .await
            .unwrap();
        lib.mark_worn(old.id.as_str(), "u1").await.unwrap();
        lib.update(old.id.as_str(), Some("Renamed"), Some(SkinModel::Slim))
            .await
            .unwrap();
        assert_eq!(
            ids(lib.list(Some("u1")).await.unwrap()),
            [new.id.as_str(), old.id.as_str()]
        );
    }

    #[tokio::test]
    async fn a_copy_goes_right_below_its_source() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), Some("u1"))
            .await
            .unwrap();
        let b = lib
            .create("B", SkinModel::Classic, &skin([1; 4]), Some("u1"))
            .await
            .unwrap();
        let copy = lib.duplicate(b.id.as_str(), "B copy").await.unwrap();
        assert_eq!(
            ids(lib.list(Some("u1")).await.unwrap()),
            [b.id.as_str(), copy.id.as_str(), a.id.as_str()]
        );
    }

    #[tokio::test]
    async fn reorder_sets_one_wardrobe_order_and_leaves_the_rest() {
        let (_dir, lib) = library();
        let mut mine = Vec::new();
        let mut theirs = Vec::new();
        for i in 0..3u8 {
            mine.push(
                lib.create("Mine", SkinModel::Classic, &skin([i; 4]), Some("u1"))
                    .await
                    .unwrap(),
            );
            theirs.push(
                lib.create("Theirs", SkinModel::Classic, &skin([i; 4]), Some("u2"))
                    .await
                    .unwrap(),
            );
        }
        let theirs_before = ids(lib.list(Some("u2")).await.unwrap());
        let [a, b, c] = [&mine[0].id, &mine[1].id, &mine[2].id];
        let list = lib
            .reorder(Some("u1"), &[a.clone(), c.clone(), b.clone()])
            .await
            .unwrap();
        assert_eq!(ids(list), [a.as_str(), c.as_str(), b.as_str()]);
        assert_eq!(
            ids(lib.list(Some("u1")).await.unwrap()),
            [a.as_str(), c.as_str(), b.as_str()]
        );
        assert_eq!(ids(lib.list(Some("u2")).await.unwrap()), theirs_before);
        // Moving a skin isn't changing it: it keeps its time and no version is kept.
        let entry = lib.entry(a).await.unwrap();
        assert_eq!(entry.updated, mine[0].updated);
        assert!(lib.versions(a).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn reorder_puts_skins_it_wasnt_told_of_after_and_ignores_strangers() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), Some("u1"))
            .await
            .unwrap();
        let b = lib
            .create("B", SkinModel::Classic, &skin([1; 4]), Some("u1"))
            .await
            .unwrap();
        let c = lib
            .create("C", SkinModel::Classic, &skin([2; 4]), Some("u1"))
            .await
            .unwrap();
        let other = lib
            .create("Other", SkinModel::Classic, &skin([3; 4]), Some("u2"))
            .await
            .unwrap();
        // Shown [C, B, A]; the screen asks for A first, names someone else's skin and one that's gone.
        let list = lib
            .reorder(
                Some("u1"),
                &[a.id.clone(), other.id.clone(), "skin-0000".into()],
            )
            .await
            .unwrap();
        assert_eq!(ids(list), [a.id.as_str(), c.id.as_str(), b.id.as_str()]);
        assert_eq!(
            ids(lib.list(Some("u2")).await.unwrap()),
            [other.id.as_str()]
        );
    }

    #[tokio::test]
    async fn a_version_two_list_keeps_the_order_it_showed() {
        let (_dir, lib) = library();
        tokio::fs::create_dir_all(lib.dir()).await.unwrap();
        for id in ["skin-aa", "skin-bb", "skin-cc", "skin-dd"] {
            tokio::fs::write(lib.png(id), skin([0; 4])).await.unwrap();
        }
        // Version 2 showed the worn skin first, then the most recently changed.
        let v2 = r#"{"version":2,"skins":[
            {"id":"skin-aa","name":"Oldest","model":"classic","updated":1,"owner":"u1"},
            {"id":"skin-bb","name":"Newest","model":"classic","updated":9,"owner":"u1"},
            {"id":"skin-cc","name":"Worn","model":"classic","updated":2,"owner":"u1","wornBy":"u1"},
            {"id":"skin-dd","name":"Middle","model":"classic","updated":5,"owner":"u1"}
        ]}"#;
        tokio::fs::write(lib.dir().join(LIST), v2).await.unwrap();
        let shown = ["skin-cc", "skin-bb", "skin-dd", "skin-aa"];
        assert_eq!(ids(lib.list(Some("u1")).await.unwrap()), shown);
        // Written back as version 3 in that order, which a later change no longer reshuffles.
        lib.save_texture("skin-aa", &skin([1; 4])).await.unwrap();
        let raw: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(lib.dir().join(LIST)).await.unwrap()).unwrap();
        assert_eq!(raw["version"], 3);
        assert_eq!(ids(lib.list(Some("u1")).await.unwrap()), shown);
    }

    #[tokio::test]
    async fn a_version_one_list_moves_worn_skins_to_their_account() {
        let (_dir, lib) = library();
        tokio::fs::create_dir_all(lib.dir()).await.unwrap();
        for id in ["skin-aa", "skin-bb"] {
            tokio::fs::write(lib.png(id), skin([0; 4])).await.unwrap();
        }
        let v1 = r#"{"skins":[
            {"id":"skin-aa","name":"Worn","model":"slim","updated":2,"wornBy":"u1"},
            {"id":"skin-bb","name":"Loose","model":"classic","updated":1}
        ]}"#;
        tokio::fs::write(lib.dir().join(LIST), v1).await.unwrap();
        let mine = lib.list(Some("u1")).await.unwrap();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].id, "skin-aa", "the id is kept");
        assert_eq!(lib.list(None).await.unwrap()[0].id, "skin-bb");
        // Written back as the current version, so a later change of `wornBy` doesn't move it again.
        lib.mark_worn("skin-bb", "u1").await.unwrap();
        let raw: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(lib.dir().join(LIST)).await.unwrap()).unwrap();
        assert_eq!(raw["version"], VERSION);
        assert_eq!(lib.list(None).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn adopt_loose_gives_loose_skins_to_the_account() {
        let (_dir, lib) = library();
        let loose = lib
            .create("L", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        lib.create("Theirs", SkinModel::Classic, &skin([0; 4]), Some("u2"))
            .await
            .unwrap();
        assert_eq!(lib.adopt_loose("u1", None).await.unwrap(), 1);
        assert_eq!(lib.list(Some("u1")).await.unwrap()[0].id, loose.id);
        assert!(lib.list(None).await.unwrap().is_empty());
        assert_eq!(lib.list(Some("u2")).await.unwrap().len(), 1);
        assert_eq!(lib.adopt_loose("u1", None).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn an_untouched_seeded_starter_is_dropped_not_adopted() {
        let (_dir, lib) = library();
        let seeded = lib
            .create_seeded("The Perlander", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        assert!(seeded.seeded);
        let edited = lib
            .create_seeded("Edited", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        let edited = lib.save_texture(&edited.id, &skin([7; 4])).await.unwrap();
        assert!(!edited.seeded, "an edit makes it the player's own");
        let renamed = lib
            .create_seeded("Renamed", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        lib.update(&renamed.id, Some("Mine now"), None)
            .await
            .unwrap();
        let made = lib
            .create("Made", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        assert!(!made.seeded);

        assert_eq!(lib.adopt_loose("u1", None).await.unwrap(), 3);
        let mut mine: Vec<_> = lib
            .list(Some("u1"))
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        mine.sort();
        let mut want = vec![edited.id, renamed.id, made.id];
        want.sort();
        assert_eq!(mine, want);
        assert!(lib.list(None).await.unwrap().is_empty());
        assert!(!lib.png(&seeded.id).exists());
    }

    #[tokio::test]
    async fn a_version_one_starter_left_untouched_is_dropped_too() {
        let (_dir, lib) = library();
        tokio::fs::create_dir_all(lib.dir()).await.unwrap();
        let starter = skin([108, 171, 60, 255]);
        tokio::fs::write(lib.png("skin-aa"), &starter)
            .await
            .unwrap();
        tokio::fs::write(lib.png("skin-bb"), skin([1, 1, 1, 255]))
            .await
            .unwrap();
        tokio::fs::write(lib.png("skin-cc"), &starter)
            .await
            .unwrap();
        let v1 = r#"{"skins":[
            {"id":"skin-aa","name":"The Perlander","model":"classic","updated":3},
            {"id":"skin-bb","name":"Painted","model":"classic","updated":2},
            {"id":"skin-cc","name":"Worn one","model":"classic","updated":1,"wornBy":"u2"}
        ]}"#;
        tokio::fs::write(lib.dir().join(LIST), v1).await.unwrap();
        assert_eq!(lib.adopt_loose("u1", Some(&starter)).await.unwrap(), 1);
        let mine: Vec<_> = lib
            .list(Some("u1"))
            .await
            .unwrap()
            .into_iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(mine, vec!["skin-bb"]);
        assert!(!lib.png("skin-aa").exists());
        // Worn, so it went to its account and is kept, starter or not.
        assert_eq!(lib.list(Some("u2")).await.unwrap()[0].id, "skin-cc");
    }

    #[tokio::test]
    async fn a_starter_that_isnt_a_skin_drops_nothing() {
        let (_dir, lib) = library();
        lib.create("A", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        assert_eq!(lib.adopt_loose("u1", Some(b"nope")).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn record_current_adds_the_skin_once_and_marks_it_worn() {
        let (_dir, lib) = library();
        lib.create("Other", SkinModel::Slim, &skin([1, 2, 3, 255]), Some("u2"))
            .await
            .unwrap();
        let real = skin([1, 2, 3, 255]);
        let first = lib
            .record_current("u1", "Alex's skin", SkinModel::Slim, &real)
            .await
            .unwrap();
        assert_eq!(first.owner.as_deref(), Some("u1"));
        assert_eq!(first.worn_by.as_deref(), Some("u1"));
        assert_eq!(first.name, "Alex's skin");
        let again = lib
            .record_current("u1", "Alex's skin", SkinModel::Slim, &real)
            .await
            .unwrap();
        assert_eq!(again.id, first.id, "no duplicate on a second visit");
        assert_eq!(lib.list(Some("u1")).await.unwrap().len(), 1);
        // Another arm model is another skin.
        let classic = lib
            .record_current("u1", "Alex's skin", SkinModel::Classic, &real)
            .await
            .unwrap();
        assert_ne!(classic.id, first.id);
        let list = lib.list(Some("u1")).await.unwrap();
        assert_eq!(list[0].id, classic.id, "the skin worn now comes first");
        assert!(list[1].worn_by.is_none());
    }

    #[tokio::test]
    async fn record_current_finds_an_edited_copy_that_matches_again() {
        let (_dir, lib) = library();
        let mine = lib
            .create("Mine", SkinModel::Classic, &skin([0; 4]), Some("u1"))
            .await
            .unwrap();
        lib.save_texture(&mine.id, &skin([5, 5, 5, 255]))
            .await
            .unwrap();
        let found = lib
            .record_current("u1", "x", SkinModel::Classic, &skin([5, 5, 5, 255]))
            .await
            .unwrap();
        assert_eq!(found.id, mine.id);
        assert_eq!(found.name, "Mine");
    }

    #[tokio::test]
    async fn entry_without_png_is_dropped() {
        let (_dir, lib) = library();
        let entry = lib
            .create("A", SkinModel::Classic, &skin([0; 4]), None)
            .await
            .unwrap();
        tokio::fs::remove_file(lib.png(&entry.id)).await.unwrap();
        assert!(lib.list(None).await.unwrap().is_empty());
    }
    fn look_of(ids: &[&str]) -> SkinLook {
        SkinLook {
            pieces: ids
                .iter()
                .map(|id| WornPiece {
                    id: id.to_string(),
                    colours: BTreeMap::from([("main".to_string(), "#aabbcc".to_string())]),
                })
                .collect(),
            painting: false,
        }
    }

    fn paint_path(lib: &SkinLibrary, id: &str) -> PathBuf {
        lib.dir().join(format!("{id}.paint.png"))
    }

    fn texel_at(png: &[u8]) -> Rgba<u8> {
        *skins_png::decode(png).unwrap().get_pixel(8, 8)
    }

    #[tokio::test]
    async fn save_look_writes_three_files_together() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), Some("u"))
            .await
            .unwrap();
        let entry = lib
            .save_look(
                &a.id,
                look_of(&["hat-a"]),
                Some(&skin([9, 9, 9, 255])),
                &skin([2, 2, 2, 255]),
            )
            .await
            .unwrap();
        assert!(entry.look.as_ref().unwrap().painting);
        assert_eq!(
            texel_at(&lib.texture(&a.id).await.unwrap()),
            Rgba([2, 2, 2, 255])
        );
        assert_eq!(
            texel_at(&lib.painting(&a.id).await.unwrap().unwrap()),
            Rgba([9, 9, 9, 255])
        );
        assert_eq!(lib.entry(&a.id).await.unwrap().look, entry.look);
        assert_eq!(lib.versions(&a.id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn save_look_without_painting_removes_paint_file() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_look(
            &a.id,
            look_of(&[]),
            Some(&skin([9, 9, 9, 255])),
            &skin([2, 2, 2, 255]),
        )
        .await
        .unwrap();
        assert!(paint_path(&lib, &a.id).exists());
        let entry = lib
            .save_look(&a.id, look_of(&["x"]), None, &skin([3, 3, 3, 255]))
            .await
            .unwrap();
        assert!(!entry.look.unwrap().painting);
        assert!(!paint_path(&lib, &a.id).exists());
        assert_eq!(lib.painting(&a.id).await.unwrap(), None);
    }

    #[tokio::test]
    async fn save_look_validates() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        let many: Vec<String> = (0..17).map(|i| format!("p{i}")).collect();
        let many: Vec<&str> = many.iter().map(String::as_str).collect();
        for bad in [look_of(&many), look_of(&["a", "a"])] {
            let e = lib
                .save_look(&a.id, bad, None, &skin([2; 4]))
                .await
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::SkinInvalid);
        }
        let mut bad = look_of(&["a"]);
        bad.pieces[0].colours.insert("main".into(), "red".into());
        let e = lib
            .save_look(&a.id, bad, None, &skin([2; 4]))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::SkinInvalid);
        assert_eq!(lib.entry(&a.id).await.unwrap().look, None);
    }

    #[tokio::test]
    async fn clear_look_keeps_finished_png() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_look(
            &a.id,
            look_of(&["a"]),
            Some(&skin([9, 9, 9, 255])),
            &skin([2, 2, 2, 255]),
        )
        .await
        .unwrap();
        let entry = lib.clear_look(&a.id).await.unwrap();
        assert_eq!(entry.look, None);
        assert!(!paint_path(&lib, &a.id).exists());
        assert_eq!(
            texel_at(&lib.texture(&a.id).await.unwrap()),
            Rgba([2, 2, 2, 255])
        );
    }

    #[tokio::test]
    async fn delete_removes_paint_file() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_look(
            &a.id,
            look_of(&["a"]),
            Some(&skin([9, 9, 9, 255])),
            &skin([2, 2, 2, 255]),
        )
        .await
        .unwrap();
        lib.delete(&a.id).await.unwrap();
        assert!(!paint_path(&lib, &a.id).exists());
    }

    #[tokio::test]
    async fn duplicate_copies_look_and_painting() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        lib.save_look(
            &a.id,
            look_of(&["a"]),
            Some(&skin([9, 9, 9, 255])),
            &skin([2, 2, 2, 255]),
        )
        .await
        .unwrap();
        let b = lib.duplicate(&a.id, "B").await.unwrap();
        assert_eq!(b.look, lib.entry(&a.id).await.unwrap().look);
        assert_eq!(
            texel_at(&lib.painting(&b.id).await.unwrap().unwrap()),
            Rgba([9, 9, 9, 255])
        );
    }

    #[tokio::test]
    async fn old_list_without_look_loads() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), Some("u"))
            .await
            .unwrap();
        let raw = std::fs::read_to_string(lib.dir().join(LIST)).unwrap();
        // A look is an optional field, not a new version of the list: it is written as any list is.
        assert!(!raw.contains("look"));
        assert!(raw.contains(&format!("\"version\": {VERSION}")));
        assert_eq!(lib.entry(&a.id).await.unwrap().look, None);
    }

    #[tokio::test]
    async fn look_hashes_cover_all_owners() {
        use crate::skin_catalog::{CatalogFiles, CatalogPiece, CatalogText};
        let h = |c: char| c.to_string().repeat(64);
        let piece = |id: &str, c: char| CatalogPiece {
            id: id.into(),
            slot: "hat".into(),
            name: CatalogText {
                en: "A".into(),
                es: "A".into(),
            },
            tags: vec![],
            areas: vec![],
            files: BTreeMap::from([
                (
                    "classic".into(),
                    CatalogFiles {
                        art: h(c),
                        mask: h((c as u8 + 1) as char),
                    },
                ),
                (
                    "slim".into(),
                    CatalogFiles {
                        art: h((c as u8 + 2) as char),
                        mask: h((c as u8 + 3) as char),
                    },
                ),
            ]),
            thumb: h('f'),
            retired: false,
        };
        let manifest = Manifest {
            format_version: 1,
            version: h('0'),
            license: "CC0-1.0".into(),
            slots: vec![],
            tags: vec![],
            pieces: vec![piece("one", 'a'), piece("two", 'm'), piece("unworn", 'w')],
            looks: vec![],
            expressions: vec![],
            sheets: None,
        };
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1; 4]), Some("u1"))
            .await
            .unwrap();
        let b = lib
            .create("B", SkinModel::Classic, &skin([1; 4]), Some("u2"))
            .await
            .unwrap();
        lib.save_look(&a.id, look_of(&["one", "gone"]), None, &skin([2; 4]))
            .await
            .unwrap();
        lib.save_look(&b.id, look_of(&["two"]), None, &skin([2; 4]))
            .await
            .unwrap();
        let hashes = lib.look_hashes(&manifest).await.unwrap();
        let want: BTreeSet<String> = ['a', 'b', 'c', 'd', 'm', 'n', 'o', 'p', 'f']
            .iter()
            .map(|c| h(*c))
            .collect();
        assert_eq!(hashes, want);
    }
    #[tokio::test]
    async fn look_hashes_errors_on_a_damaged_list_and_is_empty_without_one() {
        use crate::skin_catalog::Manifest;
        let manifest = Manifest {
            format_version: 1,
            version: "0".repeat(64),
            license: "CC0-1.0".into(),
            slots: vec![],
            tags: vec![],
            pieces: vec![],
            looks: vec![],
            expressions: vec![],
            sheets: None,
        };
        let (_dir, lib) = library();
        assert!(lib.look_hashes(&manifest).await.unwrap().is_empty());
        std::fs::create_dir_all(lib.dir()).unwrap();
        std::fs::write(lib.dir().join(LIST), b"{ nope").unwrap();
        assert!(lib.look_hashes(&manifest).await.is_err());
        assert!(lib.dir().join(LIST).exists());
    }

    #[tokio::test]
    async fn orphan_paint_file_is_not_served_and_lost_paint_is_not_copied() {
        let (_dir, lib) = library();
        let a = lib
            .create("A", SkinModel::Classic, &skin([1, 1, 1, 255]), None)
            .await
            .unwrap();
        std::fs::write(paint_path(&lib, &a.id), skin([9; 4])).unwrap();
        assert_eq!(lib.painting(&a.id).await.unwrap(), None);
        lib.save_look(&a.id, look_of(&["a"]), Some(&skin([9; 4])), &skin([2; 4]))
            .await
            .unwrap();
        std::fs::remove_file(paint_path(&lib, &a.id)).unwrap();
        let b = lib.duplicate(&a.id, "B").await.unwrap();
        assert!(!b.look.unwrap().painting);
    }
}
