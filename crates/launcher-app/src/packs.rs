//! Pack commands other than playing: adding a pack, its status, its mods, manual downloads
//! and art (spec addendum §4.4). Runs and syncs live in `runs.rs`.
use std::{path::Path, sync::Arc};

use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    fsutil::{path_key, write_atomic},
    instances::{ArtMode, Focus, IconMode, Instance, SourceRef},
    mods,
    picture::{self, BACKGROUND_FIT, MAX_SOURCE_BYTES, THUMBNAIL_FIT},
    sources::{
        art::{sniff, MAX_ART_BYTES},
        PackInput, UpdateCheck,
    },
    sync::{
        lockfile::{self, Managed},
        plan,
    },
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::{busy, AppCore},
    deep_link::{self, DeepLink},
    dto::{
        ArtChoice, IconChoice, InstanceArtData, ManualProgress, OptionalChoice, PackAddRequest,
        PackContents, PackMod, PackStatus, PackUpdate,
    },
    events::{AppEvent, Host},
    runner::Runner,
};

fn no_pack() -> LauncherError {
    LauncherError::pack(ErrorCode::NoPack, "This instance doesn't come from a pack")
}

/// Each art file type and the media type the webview needs.
const ART: [(&str, &str); 4] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];

/// `<dir>/<stem>.<ext>` as a `data:` URL, if it is a real image of that type within the size
/// limit. The core wrote it, but the disk is the player's, so it is checked again here.
async fn art_url(dir: &Path, stem: &str) -> Option<String> {
    for (ext, mime) in ART {
        let path = dir.join(format!("{stem}.{ext}"));
        let Ok(meta) = tokio::fs::metadata(&path).await else {
            continue;
        };
        if !meta.is_file() || meta.len() > MAX_ART_BYTES as u64 {
            continue;
        }
        let Ok(bytes) = tokio::fs::read(&path).await else {
            continue;
        };
        if sniff(&bytes) == Some(ext) {
            return Some(format!("data:{mime};base64,{}", STANDARD.encode(&bytes)));
        }
    }
    None
}

/// Where the player's own picture lives, inside `.perland/`, and what it may be.
const CUSTOM_DIR: &str = "custom";
/// The background.
const CUSTOM_STEM: &str = "image";
/// The icon, once it is chosen apart from the background.
const ICON_STEM: &str = "icon";
const CUSTOM_EXTS: [&str; 3] = ["png", "jpg", "webp"];
const MAX_SEED_CHARS: usize = 64;

fn art_file_invalid() -> LauncherError {
    LauncherError::disk(
        ErrorCode::ArtFileInvalid,
        "That file can't be used as an instance image",
    )
    .with_fix("Pick a PNG, JPEG or WebP image.")
}

/// The picked file's bytes and real type (by signature, not by name), if it is a PNG, JPEG or
/// WebP. Read whole only up to `MAX_SOURCE_BYTES`.
async fn read_picture(path: &Path) -> Result<(Vec<u8>, &'static str)> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| art_file_invalid().with_detail(e))?;
    if !meta.is_file() {
        return Err(art_file_invalid().with_detail("not a file"));
    }
    if meta.len() > MAX_SOURCE_BYTES {
        return Err(art_file_invalid().with_detail("the file is too big"));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| art_file_invalid().with_detail(e))?;
    match sniff(&bytes) {
        Some(ext) if CUSTOM_EXTS.contains(&ext) => Ok((bytes, ext)),
        _ => Err(art_file_invalid().with_detail("not a PNG, JPEG or WebP image")),
    }
}

/// `read_picture`, then a picture bigger than the art limit (a 4K screenshot) scaled down to
/// what the launcher draws, so it is kept instead of refused.
async fn read_upload(path: &Path) -> Result<(Vec<u8>, &'static str)> {
    let (bytes, ext) = read_picture(path).await?;
    tokio::task::spawn_blocking(move || picture::fit(bytes, ext, MAX_ART_BYTES, BACKGROUND_FIT))
        .await
        .ok()
        .flatten()
        .ok_or_else(|| {
            art_file_invalid().with_detail("the picture couldn't be read or scaled down")
        })
}

/// Create `dir` (somewhere under `instance`) without ever creating `instance` itself: if the
/// instance folder is gone (a rename moved it), this is `instance_not_found`, not a new folder.
async fn make_dir_in_instance(instance: &Path, dir: &Path, id: &str) -> Result<()> {
    if !tokio::fs::metadata(instance)
        .await
        .is_ok_and(|m| m.is_dir())
    {
        return Err(LauncherError::disk(
            ErrorCode::InstanceNotFound,
            format!("Instance \"{id}\" wasn't found"),
        )
        .with_param("id", id));
    }
    tokio::fs::create_dir_all(dir).await?;
    Ok(())
}

/// Write `<dir>/<stem>.<ext>` atomically and remove the same picture's other types, so only
/// the new one is left.
async fn write_custom(dir: &Path, stem: &str, bytes: &[u8], ext: &str) -> Result<()> {
    write_atomic(&dir.join(format!("{stem}.{ext}")), bytes).await?;
    for other in CUSTOM_EXTS.iter().filter(|e| **e != ext) {
        let _ = tokio::fs::remove_file(dir.join(format!("{stem}.{other}"))).await;
    }
    Ok(())
}

/// Remove `<dir>/<stem>.*`, and `dir` too once nothing is left in it. Best effort: the saved
/// choice is what counts, a leftover file is only unused.
async fn remove_custom(dir: &Path, stem: &str) {
    for ext in CUSTOM_EXTS {
        let _ = tokio::fs::remove_file(dir.join(format!("{stem}.{ext}"))).await;
    }
    let _ = tokio::fs::remove_dir(dir).await;
}

/// Where the game saves screenshots, inside the game folder.
const SCREENSHOTS_DIR: &str = "screenshots";
/// "Change image…" offers this many of the newest screenshots.
const MAX_SCREENSHOTS: usize = 24;

/// `name` as a screenshot's path, if it is a plain file name: never a path that leaves the
/// screenshots folder.
fn screenshot_path(game: &Path, name: &str) -> Option<std::path::PathBuf> {
    let plain = !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
        && Path::new(name).file_name() == Some(std::ffi::OsStr::new(name));
    plain.then(|| game.join(SCREENSHOTS_DIR).join(name))
}

/// Whether a file name looks like a picture "Change image…" can use, by its extension only.
fn picture_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".webp"]
        .iter()
        .any(|ext| lower.ends_with(ext))
}

fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// A new instance from a pack link (https `pack.toml` or `modrinth:`) or a `.mrpack` file.
    /// Nothing but the art downloads until its first Play.
    pub async fn pack_add(&self, request: PackAddRequest) -> Result<Instance> {
        let input = match request {
            PackAddRequest::Link { url } => match PackInput::parse(&url)? {
                PackInput::MrpackFile { .. } => return Err(link_invalid()),
                // The UI routes invite links to Join server first; this is the same thing.
                PackInput::Perland { .. } => return self.join_add(&url).await,
                input => input,
            },
            PackAddRequest::File { path } => {
                let is_mrpack = path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("mrpack"));
                let is_file = tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file());
                if !is_mrpack || !is_file {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    return Err(LauncherError::pack(
                        ErrorCode::PackInvalid,
                        format!("{name} isn't a .mrpack file"),
                    )
                    .with_param("file", name));
                }
                PackInput::MrpackFile { path }
            }
        };
        self.runner.add_pack(input).await
    }

    /// The instance `id`, if it comes from a pack.
    pub(crate) async fn pack_instance(&self, id: &str) -> Result<Instance> {
        let instance = self.instances.load(id).await?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        Ok(instance)
    }

    /// The banner's Update: take the pack's update without playing.
    pub async fn pack_update(self: &Arc<Self>, id: &str) -> Result<()> {
        let instance = self.pack_instance(id).await?;
        let checks = self.checks_for_updates(&instance).await;
        self.start_run(id, instance, None, checks)
    }

    /// For the banner: the installed version and the update on offer. A failed check is
    /// reported in `check_error`, not as an error: the banner just says it couldn't check.
    pub async fn pack_status(&self, id: &str) -> Result<PackStatus> {
        let instance = self.pack_instance(id).await?;
        let held = instance.update_hold.clone();
        let installed = lockfile::load_pack(&self.instances.perland_dir(id)).await;
        let (update, check_error) = match &installed {
            None => (None, None),
            // A server's check is the commit in its view, so the banner never downloads a pack.
            Some(current) if matches!(instance.source, SourceRef::Perland { .. }) => {
                match self.runner.check_update(instance).await {
                    UpdateCheck::UpToDate => (None, None),
                    UpdateCheck::Available { version, .. } => (
                        Some(PackUpdate {
                            from_version: Some(current.pack_version.clone()),
                            to_version: version,
                            added: vec![],
                            updated: vec![],
                            removed: vec![],
                            kept: vec![],
                        }),
                        None,
                    ),
                    UpdateCheck::Unavailable(error) => {
                        if error.code == ErrorCode::InviteGone {
                            // The banner is a lockless read: while a run holds the instance,
                            // the run's own check marks it.
                            let _ = self.mark_server_gone(id, true).await;
                        }
                        (None, Some(error))
                    }
                }
            }
            Some(_) => match self
                .runner
                .find_update(instance, launcher_core::events::noop_sink())
                .await
            {
                Ok(update) => (update.as_ref().map(PackUpdate::from), None),
                Err(error) => (None, Some(error)),
            },
        };
        // Going back held the instance at its version; once the source is back there too,
        // there is nothing left to differ (spec §5.8).
        let installed_version = installed.map(|d| d.pack_version);
        let mut hold_lifted = false;
        if let Some(hold) = &held {
            let caught_up = update.is_none()
                && check_error.is_none()
                && installed_version.as_deref() == Some(hold.version.as_str());
            if caught_up {
                hold_lifted = self.lift_hold(id).await;
            }
        }
        Ok(PackStatus {
            installed_version,
            update,
            check_error,
            hold_lifted,
        })
    }

    /// Clear the instance's update hold. Lockless like the banner's poll, so the instance is
    /// reloaded first and left alone while a run rewrites it; the next poll tries again.
    /// `true` when the hold was lifted and saved.
    async fn lift_hold(&self, id: &str) -> bool {
        if self.is_busy(id) {
            return false;
        }
        let Ok(mut instance) = self.instances.load(id).await else {
            return false;
        };
        if instance.update_hold.take().is_none() {
            return false;
        }
        match self.instances.save(&instance).await {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(instance = id, %error, "couldn't lift the update hold");
                false
            }
        }
    }

    /// A pack instance's mods: the pack's own (from `pack.json`, client side, under `mods/`)
    /// and the jars the player added.
    pub async fn pack_contents(&self, id: &str) -> Result<PackContents> {
        let instance = self.pack_instance(id).await?;
        let perland = self.instances.perland_dir(id);
        let game_dir = self.instances.game_dir(id);
        let installed = lockfile::load_pack(&perland).await;
        let mut managed: Vec<PackMod> = match &installed {
            None => Vec::new(),
            Some(pack) => {
                let choices = plan::effective_choices(pack, &instance.optional_choices);
                pack.files
                    .iter()
                    .filter(|f| f.side.on_client() && f.path.starts_with("mods/"))
                    .map(|f| PackMod {
                        path: f.path.clone(),
                        name: f.name.clone(),
                        optional: f.optional.as_ref().map(|o| OptionalChoice {
                            enabled: choices.get(&f.path).copied().unwrap_or(o.default),
                            description: o.description.clone(),
                        }),
                    })
                    .collect()
            }
        };
        managed.sort_by_key(|m| m.name.to_lowercase());
        let lock = Managed::load(&perland).await;
        let names = lockfile::additions(&game_dir, lock.as_ref()).await?;
        let additions = mods::list(&game_dir)
            .await?
            .into_iter()
            .filter(|m| names.contains(&m.file_name))
            .collect();
        Ok(PackContents { managed, additions })
    }

    /// Turn an optional pack mod on or off. It applies on the next sync (Play or Update),
    /// offline too, from `pack.json` and the cache.
    pub async fn pack_set_optional(&self, id: &str, path: &str, enabled: bool) -> Result<Instance> {
        let mut instance = self.pack_instance(id).await?;
        let installed = lockfile::load_pack(&self.instances.perland_dir(id)).await;
        let optional = installed.as_ref().is_some_and(|pack| {
            pack.files
                .iter()
                .any(|f| f.path == path && f.optional.is_some() && f.side.on_client())
        });
        if !optional {
            return Err(LauncherError::disk(
                ErrorCode::ModNotFound,
                format!("{path} isn't an optional mod of this pack"),
            )
            .with_param("file", path));
        }
        instance.optional_choices.insert(path.to_string(), enabled);
        self.instances.update(&instance).await
    }

    /// Mods that came with the pack are the pack's: the player can't turn them off or remove
    /// them (sync would only put them back). Paths compare as the file system does, so a pack
    /// that renamed `foo.jar` to `Foo.jar` still owns the `foo.jar` on disk.
    pub(crate) async fn refuse_managed(&self, id: &str, file: &str) -> Result<()> {
        let key = path_key(&format!("mods/{file}"));
        let key = key.strip_suffix(".disabled").unwrap_or(&key);
        self.refuse_if_owned(id, key, file.strip_suffix(".disabled").unwrap_or(file))
            .await
    }

    /// The same for a file in another content folder (`resourcepacks`, `shaderpacks`), which
    /// has no on/off state, so the name is compared as it is.
    pub(crate) async fn refuse_managed_in(&self, id: &str, folder: &str, file: &str) -> Result<()> {
        self.refuse_if_owned(id, &path_key(&format!("{folder}/{file}")), file)
            .await
    }

    async fn refuse_if_owned(&self, id: &str, key: &str, shown: &str) -> Result<()> {
        let managed = Managed::load(&self.instances.perland_dir(id)).await;
        if managed.is_some_and(|m| m.files.keys().any(|k| path_key(k) == key)) {
            return Err(LauncherError::disk(
                ErrorCode::ModManaged,
                format!("{shown} comes with the pack"),
            )
            .with_param("file", shown)
            .with_fix("Mods you add yourself can be turned off or removed"));
        }
        Ok(())
    }

    /// Look in the Downloads folder for the files the last sync couldn't download, and put
    /// what's there into the cache. The "Download these manually" dialog polls this.
    pub async fn manual_import(&self, id: &str) -> Result<ManualProgress> {
        self.instances.load(id).await?;
        let mut manual = self.manual.lock().await;
        let Some(state) = manual.get_mut(id) else {
            return Ok(ManualProgress {
                found: 0,
                total: 0,
                remaining: Vec::new(),
            });
        };
        if let Some(dir) = &self.config.downloads_dir {
            let found = state.scanner.scan(dir, &state.remaining).await?;
            if !found.is_empty() {
                self.runner.import_manual(found.clone()).await?;
                state
                    .remaining
                    .retain(|f| !found.iter().any(|(got, _)| got.path == f.path));
            }
        }
        Ok(ManualProgress {
            found: state.total - state.remaining.len(),
            total: state.total,
            remaining: state.remaining.clone(),
        })
    }

    /// The pictures the player sees for this instance, icon and background apart: their own
    /// files from `.perland/custom/`, nothing for generated ones, or the pack's art from
    /// `.perland/art/`. A never-chosen icon on a round-1 upload is that upload, so the one
    /// picture keeps filling both places until one of them changes.
    pub async fn instance_art(&self, id: &str) -> Result<InstanceArtData> {
        let instance = self.instances.load(id).await?;
        let perland = self.instances.perland_dir(id);
        let custom = perland.join(CUSTOM_DIR);
        let art = perland.join("art");
        let icon = match instance.icon_mode() {
            IconMode::Generated => None,
            IconMode::Custom if instance.icon.is_none() => art_url(&custom, CUSTOM_STEM).await,
            IconMode::Custom => art_url(&custom, ICON_STEM).await,
            IconMode::Auto => art_url(&art, "icon").await,
        };
        let banner = match instance.art {
            ArtMode::Generated => None,
            ArtMode::Custom => art_url(&custom, CUSTOM_STEM).await,
            ArtMode::Auto => art_url(&art, "banner").await,
        };
        Ok(InstanceArtData { icon, banner })
    }

    /// "Change image…", the background: a generated landscape, the pack's own art, or a file of
    /// the player's. Refused while the instance is busy or installing, like a rename. Every check
    /// runs before anything is written, so a bad file leaves the previous picture as it was. Only
    /// the picture fields change (a `save`, not an `update`), so a stale Java path doesn't block
    /// a picture. A never-chosen icon is pinned first, so the icon never changes with it.
    pub async fn instance_art_set(&self, id: &str, choice: ArtChoice) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        let custom = self.instances.perland_dir(id).join(CUSTOM_DIR);
        let upload = match &choice {
            ArtChoice::Upload { path } => Some(read_upload(path).await?),
            ArtChoice::Screenshot { name } => {
                let path = screenshot_path(&self.instances.game_dir(id), name)
                    .ok_or_else(|| art_file_invalid().with_detail("not a screenshot's name"))?;
                Some(read_upload(&path).await?)
            }
            _ => None,
        };
        // Loaded after the upload is read, right before writing: a rename that moved the folder
        // meanwhile is refused here instead of writing into the old place.
        let mut instance = self.instances.load(id).await?;
        let icon = instance.icon_mode();
        match choice {
            ArtChoice::Generated { seed } => {
                if seed.is_empty() || seed.chars().count() > MAX_SEED_CHARS {
                    return Err(art_file_invalid().with_detail("the picture name is not valid"));
                }
                instance.art = ArtMode::Generated;
                instance.art_seed = Some(seed);
            }
            ArtChoice::Pack => {
                if instance.source == SourceRef::None {
                    return Err(no_pack());
                }
                instance.art = ArtMode::Auto;
            }
            ArtChoice::Upload { .. } | ArtChoice::Screenshot { .. } => {
                instance.art = ArtMode::Custom
            }
        }
        // A new picture starts from the default position.
        instance.banner_focus = None;
        if instance.icon.is_none() {
            if icon == IconMode::Custom {
                self.pin_legacy_icon(id, &custom).await?;
            }
            instance.icon = Some(icon);
        }
        let Some((bytes, ext)) = upload else {
            self.instances.save(&instance).await?;
            // The saved choice is what counts; a leftover file is only unused.
            remove_custom(&custom, CUSTOM_STEM).await;
            return Ok(instance);
        };
        make_dir_in_instance(&self.instances.instance_dir(id), &custom, id).await?;
        write_custom(&custom, CUSTOM_STEM, &bytes, ext).await?;
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// "Change image…", the screenshots: the file names of the newest pictures in the game's
    /// screenshots folder, newest first. Chosen by name and size only, so a long folder lists
    /// quickly; each picture is checked when it is read.
    pub async fn instance_screenshots(&self, id: &str) -> Result<Vec<String>> {
        self.instances.load(id).await?;
        let dir = self.instances.game_dir(id).join(SCREENSHOTS_DIR);
        let Ok(mut entries) = tokio::fs::read_dir(&dir).await else {
            return Ok(Vec::new());
        };
        let mut found = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            let Ok(meta) = entry.metadata().await else {
                continue;
            };
            if !picture_name(&name) || !meta.is_file() || meta.len() > MAX_SOURCE_BYTES {
                continue;
            }
            let modified = meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            found.push((modified, name));
        }
        found.sort_by(|a, b| b.cmp(a));
        Ok(found
            .into_iter()
            .take(MAX_SCREENSHOTS)
            .map(|(_, name)| name)
            .collect())
    }

    /// One screenshot's thumbnail as a JPEG `data:` URL, or `None` when it is gone or isn't a
    /// usable picture. A thumbnail, so a page of 4K screenshots stays light.
    pub async fn instance_screenshot(&self, id: &str, name: &str) -> Result<Option<String>> {
        self.instances.load(id).await?;
        let Some(path) = screenshot_path(&self.instances.game_dir(id), name) else {
            return Ok(None);
        };
        let Ok((bytes, _)) = read_picture(&path).await else {
            return Ok(None);
        };
        let thumbnail =
            tokio::task::spawn_blocking(move || picture::thumbnail(&bytes, THUMBNAIL_FIT))
                .await
                .ok()
                .flatten();
        Ok(thumbnail.map(|jpeg| format!("data:image/jpeg;base64,{}", STANDARD.encode(jpeg))))
    }

    /// "Change image…", the background's position: which part of a picture background stays in
    /// view (`None` is the default). The same refusals as `instance_art_set`, and only this field
    /// is saved.
    pub async fn instance_banner_focus_set(
        &self,
        id: &str,
        focus: Option<Focus>,
    ) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        if focus.is_some_and(|f| f.x > Focus::MAX || f.y > Focus::MAX) {
            return Err(LauncherError::disk(
                ErrorCode::BannerFocusInvalid,
                "That position is outside the picture",
            ));
        }
        let mut instance = self.instances.load(id).await?;
        instance.banner_focus = focus;
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// A round-1 upload is the icon as well as the background: copy it to `icon.<ext>` before
    /// the background changes, so the icon stays.
    async fn pin_legacy_icon(&self, id: &str, custom: &Path) -> Result<()> {
        for ext in CUSTOM_EXTS {
            let image = custom.join(format!("{CUSTOM_STEM}.{ext}"));
            if !tokio::fs::metadata(&image).await.is_ok_and(|m| m.is_file()) {
                continue;
            }
            let bytes = tokio::fs::read(&image).await?;
            make_dir_in_instance(&self.instances.instance_dir(id), custom, id).await?;
            return write_custom(custom, ICON_STEM, &bytes, ext).await;
        }
        Ok(())
    }

    /// "Change image…", the icon: the generated monogram, the pack's own icon, or a file of the
    /// player's. The same refusals, checks and writes as `instance_art_set`; the background
    /// stays as it is.
    pub async fn instance_icon_set(&self, id: &str, choice: IconChoice) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        let custom = self.instances.perland_dir(id).join(CUSTOM_DIR);
        let upload = match &choice {
            IconChoice::Upload { path } => Some(read_upload(path).await?),
            _ => None,
        };
        // Loaded right before writing, as in `instance_art_set`.
        let mut instance = self.instances.load(id).await?;
        instance.icon = Some(match choice {
            IconChoice::Generated => IconMode::Generated,
            IconChoice::Pack => {
                if instance.source == SourceRef::None {
                    return Err(no_pack());
                }
                IconMode::Auto
            }
            IconChoice::Upload { .. } => IconMode::Custom,
        });
        let Some((bytes, ext)) = upload else {
            self.instances.save(&instance).await?;
            remove_custom(&custom, ICON_STEM).await;
            return Ok(instance);
        };
        make_dir_in_instance(&self.instances.instance_dir(id), &custom, id).await?;
        write_custom(&custom, ICON_STEM, &bytes, ext).await?;
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// A `perland://` link arrived (at startup or from a second launch). A good one waits for
    /// the UI (`deep_link_take`); a bad one is only a warning.
    pub fn deep_link(&self, raw: &str) {
        match deep_link::parse(raw) {
            Ok(link) => {
                *self.pending_link.lock().unwrap_or_else(|e| e.into_inner()) = Some(link);
                self.host.emit(AppEvent::DeepLink);
            }
            Err(error) => self.host.emit(AppEvent::Warning {
                instance: None,
                error,
            }),
        }
    }

    /// The waiting link, once.
    pub fn deep_link_take(&self) -> Option<DeepLink> {
        self.pending_link
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use launcher_core::{
        accounts::BuildKind,
        instances::{ArtMode, Focus, IconMode, SourceRef, UpdatePolicy},
        sources::{art::MAX_ART_BYTES, PackInput, UpdateCheck},
        sync::{lockfile, manual::Scanner},
        ErrorCode, LauncherError,
    };

    use super::MAX_SCREENSHOTS;
    use crate::{
        app::{tests::core, ManualState},
        deep_link::DeepLink,
        dto::{ArtChoice, IconChoice, InstanceArtData, NewInstance, OpenTarget, PackAddRequest},
        events::{AppEvent, RunStateKind::*},
        testing::{
            desired, exit, invite, managed_for, manual_file, pack_file, pack_instance, pending,
            serve, server_view,
        },
    };
    use launcher_core::picture::MAX_SOURCE_BYTES;

    #[tokio::test]
    async fn pack_add_hands_invite_links_to_join_add() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let added = core
            .pack_add(PackAddRequest::Link { url: invite('A') })
            .await
            .unwrap();
        assert!(matches!(added.source, SourceRef::Perland { .. }));
        assert!(added.server.is_some());
        let again = core
            .pack_add(PackAddRequest::Link { url: invite('A') })
            .await
            .unwrap();
        assert_eq!(again.id, added.id);
        assert_eq!(runner.added.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_servers_pack_status_comes_from_its_view() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        lockfile::save_pack(&core.instances.perland_dir(&id), &desired("1.0.4", vec![]))
            .await
            .unwrap();
        runner.update_checks.lock().unwrap().insert(
            id.clone(),
            UpdateCheck::Available {
                source: SourceRef::None,
                stamp: "c2".into(),
                version: "1.0.5".into(),
            },
        );
        let checks = runner.checks.load(Ordering::SeqCst);
        let update = core.pack_status(&id).await.unwrap().update.unwrap();
        assert_eq!(
            (update.from_version.as_deref(), update.to_version.as_str()),
            (Some("1.0.4"), "1.0.5")
        );
        assert!(update.added.is_empty() && update.updated.is_empty() && update.removed.is_empty());
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "no find_update: the banner never downloads a pack"
        );
        runner.update_checks.lock().unwrap().insert(
            id.clone(),
            UpdateCheck::Unavailable(LauncherError::pack(ErrorCode::InviteGone, "gone")),
        );
        assert_eq!(
            core.pack_status(&id)
                .await
                .unwrap()
                .check_error
                .unwrap()
                .code,
            ErrorCode::InviteGone
        );
        assert!(core.instances.load(&id).await.unwrap().server.unwrap().gone);
    }

    #[tokio::test]
    async fn the_banner_update_syncs_without_playing() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        core.pack_update(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(host.states(&id), vec![Checking, Queued, Syncing, Idle]);
        assert_eq!(
            *runner.syncs.lock().unwrap(),
            vec![(id.clone(), Some("1.0.4".to_string()))]
        );
        let exit = host.last_exit(&id).unwrap();
        assert!(exit.update_only && !exit.is_crash() && exit.error.is_none());
        assert!(runner.prepared.lock().unwrap().is_empty(), "no game");
    }

    #[tokio::test]
    async fn pack_update_refuses_vanilla_and_busy_instances() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        core.account_add_offline("Steve").await.unwrap();
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(
            core.pack_update(&plain.id).await.unwrap_err().code,
            ErrorCode::NoPack
        );
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            core.pack_update(&id).await.unwrap_err().code,
            ErrorCode::AlreadyRunning
        );
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn pack_status_reports_the_update_or_why_not() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", Some(&desired("1.0.3", vec![])))
            .await
            .id;
        let status = core.pack_status(&id).await.unwrap();
        assert_eq!(status.installed_version.as_deref(), Some("1.0.3"));
        assert!(status.update.is_none() && status.check_error.is_none());

        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.clone(), Ok(Some(pending("1.0.3", "1.0.4"))));
        assert_eq!(
            core.pack_status(&id)
                .await
                .unwrap()
                .update
                .unwrap()
                .to_version,
            "1.0.4"
        );

        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::network(
                ErrorCode::UpdateCheckFailed,
                "Couldn't check for updates",
            )),
        );
        assert_eq!(
            core.pack_status(&id)
                .await
                .unwrap()
                .check_error
                .unwrap()
                .code,
            ErrorCode::UpdateCheckFailed
        );

        let never = pack_instance(&core, "Fresh", None).await.id;
        let checks = runner.checks.load(Ordering::SeqCst);
        let status = core.pack_status(&never).await.unwrap();
        assert!(status.installed_version.is_none() && status.update.is_none());
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "nothing installed to compare against"
        );
    }

    #[tokio::test]
    async fn pack_add_takes_links_and_mrpack_files_only() {
        let (core, runner, _host, tmp) = core(BuildKind::Dev);
        let link = |url: &str| PackAddRequest::Link { url: url.into() };
        let url = "https://perlanddev.github.io/asalith-modpack/pack.toml";
        let added = core.pack_add(link(url)).await.unwrap();
        assert_eq!(added.source, SourceRef::Packwiz { url: url.into() });
        core.pack_add(link("modrinth:sky")).await.unwrap();

        let mrpack = tmp.path().join("Sky Islands.MRPACK");
        std::fs::write(&mrpack, b"zip").unwrap();
        core.pack_add(PackAddRequest::File {
            path: mrpack.clone(),
        })
        .await
        .unwrap();
        assert_eq!(
            *runner.added.lock().unwrap(),
            vec![
                PackInput::Packwiz { url: url.into() },
                PackInput::Modrinth {
                    project: "sky".into(),
                    version: None
                },
                PackInput::MrpackFile { path: mrpack },
            ]
        );

        // A link is never a file, and never plain http.
        for bad in [
            "C:/Downloads/pack.mrpack",
            "http://evil.example/pack.toml",
            "pack.toml",
        ] {
            assert_eq!(
                core.pack_add(link(bad)).await.unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad}"
            );
        }
        // A file is only ever a regular .mrpack file.
        let exe = tmp.path().join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        let folder = tmp.path().join("folder.mrpack");
        std::fs::create_dir_all(&folder).unwrap();
        for bad in [exe, folder, tmp.path().join("missing.mrpack")] {
            assert_eq!(
                core.pack_add(PackAddRequest::File { path: bad.clone() })
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::PackInvalid,
                "{}",
                bad.display()
            );
        }
        assert_eq!(
            runner.added.lock().unwrap().len(),
            3,
            "refused input never reaches the core"
        );
    }

    /// "Sky" with JEI (managed, on disk), Shaders (optional, off by default), a config file,
    /// and the player's own sodium.jar.
    async fn sky(core: &crate::app::tests::TestCore) -> String {
        let installed = desired(
            "1.0.3",
            vec![
                pack_file("mods/jei.jar", "JEI", None),
                pack_file("mods/shaders.jar", "Shaders", Some(false)),
                pack_file("config/a.toml", "a.toml", None),
            ],
        );
        let id = pack_instance(core, "Sky", Some(&installed)).await.id;
        managed_for(&["mods/jei.jar", "config/a.toml"])
            .save(&core.instances.perland_dir(&id))
            .await
            .unwrap();
        let mods = core.instances.game_dir(&id).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("jei.jar"), b"jei").unwrap();
        std::fs::write(mods.join("sodium.jar"), b"sodium").unwrap();
        id
    }

    #[tokio::test]
    async fn pack_contents_splits_the_pack_from_the_players_additions() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = sky(&core).await;
        let contents = core.pack_contents(&id).await.unwrap();
        let names: Vec<_> = contents.managed.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["JEI", "Shaders"], "mods only, sorted");
        assert!(contents.managed[0].optional.is_none());
        let shaders = contents.managed[1].optional.as_ref().unwrap();
        assert!(!shaders.enabled);
        assert_eq!(shaders.description.as_deref(), Some("Optional Shaders"));
        let additions: Vec<_> = contents
            .additions
            .iter()
            .map(|m| m.file_name.as_str())
            .collect();
        assert_eq!(additions, vec!["sodium.jar"]);
    }

    #[tokio::test]
    async fn optional_choices_save_for_the_next_sync() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = sky(&core).await;
        let saved = core
            .pack_set_optional(&id, "mods/shaders.jar", true)
            .await
            .unwrap();
        assert_eq!(saved.optional_choices.get("mods/shaders.jar"), Some(&true));
        let contents = core.pack_contents(&id).await.unwrap();
        assert!(contents.managed[1].optional.as_ref().unwrap().enabled);
        for not_optional in ["mods/jei.jar", "mods/nope.jar", "config/a.toml"] {
            assert_eq!(
                core.pack_set_optional(&id, not_optional, false)
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::ModNotFound,
                "{not_optional}"
            );
        }
    }

    #[tokio::test]
    async fn pack_mods_cannot_be_toggled_or_removed_but_additions_can() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = sky(&core).await;
        let refused = core
            .mod_set_enabled(&id, "jei.jar", false)
            .await
            .unwrap_err();
        assert_eq!(refused.code, ErrorCode::ModManaged);
        assert_eq!(refused.params["file"], "jei.jar");
        assert_eq!(
            core.mod_remove(&id, "jei.jar").await.unwrap_err().code,
            ErrorCode::ModManaged
        );
        core.mod_set_enabled(&id, "sodium.jar", false)
            .await
            .unwrap();
        core.mod_remove(&id, "sodium.jar.disabled").await.unwrap();
    }

    #[tokio::test]
    async fn a_settings_save_keeps_the_saved_pack_source() {
        // The settings form was loaded before a sync moved the source on: saving it must not
        // put the old source back.
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let mut stale = core.instances.load(&id).await.unwrap();
        stale.source = SourceRef::Packwiz {
            url: "https://elsewhere.example/pack.toml".into(),
        };
        stale.update_policy = UpdatePolicy::Auto;
        let saved = core.instance_update(stale).await.unwrap();
        let expected = SourceRef::Packwiz {
            url: "https://example.com/pack.toml".into(),
        };
        assert_eq!(saved.source, expected);
        assert_eq!(saved.update_policy, UpdatePolicy::Auto);
        assert_eq!(core.instances.load(&id).await.unwrap().source, expected);
    }

    #[tokio::test]
    async fn pack_mods_are_recognised_whatever_their_case_on_disk() {
        // The pack renamed `mods/foo.jar` to `mods/Foo.jar` with the same bytes: sync kept the
        // on-disk `foo.jar` while the lockfile records `mods/Foo.jar`.
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let installed = desired("1.0.4", vec![pack_file("mods/Foo.jar", "Foo", None)]);
        let id = pack_instance(&core, "Sky", Some(&installed)).await.id;
        managed_for(&["mods/Foo.jar"])
            .save(&core.instances.perland_dir(&id))
            .await
            .unwrap();
        let mods = core.instances.game_dir(&id).join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("foo.jar"), b"foo").unwrap();
        assert!(core.pack_contents(&id).await.unwrap().additions.is_empty());
        let refused = core
            .mod_set_enabled(&id, "foo.jar", false)
            .await
            .unwrap_err();
        assert_eq!(refused.code, ErrorCode::ModManaged);
        for file in ["foo.jar", "FOO.jar.disabled"] {
            assert_eq!(
                core.mod_remove(&id, file).await.unwrap_err().code,
                ErrorCode::ModManaged,
                "{file}"
            );
        }
        assert!(mods.join("foo.jar").exists());
    }

    #[tokio::test]
    async fn webview_paths_must_be_the_right_kind_of_file() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let tool = tmp.path().join("tool.exe");
        std::fs::write(&tool, b"MZ").unwrap();
        let folder = tmp.path().join("java");
        std::fs::create_dir_all(&folder).unwrap();
        for bad in [tool.clone(), folder] {
            assert_eq!(
                core.java_test(bad, None).await.unwrap_err().code,
                ErrorCode::JavaTestFailed
            );
        }
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(
            core.mods_add(&plain.id, vec![tool]).await.unwrap_err().code,
            ErrorCode::ModNotJar
        );
    }

    #[tokio::test]
    async fn manual_import_finds_downloads_and_imports_each_once() {
        let (core, runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        assert_eq!(
            core.manual_import(&id).await.unwrap().total,
            0,
            "nothing pending"
        );
        let (a, b) = (manual_file("a.jar", b"aaa"), manual_file("b.jar", b"bbb"));
        core.manual.lock().await.insert(
            id.clone(),
            ManualState {
                total: 2,
                remaining: vec![a, b],
                scanner: Scanner::default(),
            },
        );
        let downloads = tmp.path().join("Downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let progress = core.manual_import(&id).await.unwrap();
        assert_eq!((progress.found, progress.total), (0, 2));
        std::fs::write(downloads.join("a (1).jar"), b"aaa").unwrap();
        let progress = core.manual_import(&id).await.unwrap();
        assert_eq!((progress.found, progress.remaining.len()), (1, 1));
        core.manual_import(&id).await.unwrap();
        assert_eq!(
            runner.imported.lock().unwrap().len(),
            1,
            "a found file is imported once"
        );
        std::fs::write(downloads.join("b.jar"), b"bbb").unwrap();
        let progress = core.manual_import(&id).await.unwrap();
        assert_eq!((progress.found, progress.total), (2, 2));
        assert!(progress.remaining.is_empty());
        assert_eq!(core.folder(OpenTarget::Downloads).await.unwrap(), downloads);
    }

    #[tokio::test]
    async fn instance_art_comes_back_as_data_urls_of_real_images_only() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        assert_eq!(
            core.instance_art(&id).await.unwrap(),
            InstanceArtData {
                icon: None,
                banner: None
            }
        );
        let art = core.instances.perland_dir(&id).join("art");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("icon.png"), b"\x89PNG\r\n\x1a\n\0rest").unwrap();
        std::fs::write(art.join("banner.png"), b"<html>not a png</html>").unwrap();
        std::fs::write(
            art.join("banner.svg"),
            b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .unwrap();
        let data = core.instance_art(&id).await.unwrap();
        assert!(data
            .icon
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(data
            .banner
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0rest";
    const WEBP: &[u8] = b"RIFF\0\0\0\0WEBPrest";

    /// A file of the player's own, outside the instance.
    fn picked(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    fn custom_files(core: &crate::app::tests::TestCore, id: &str) -> Vec<String> {
        let dir = core.instances.perland_dir(id).join("custom");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|d| {
                d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[tokio::test]
    async fn custom_image_wins_over_pack_art_and_survives_a_sync() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let art = core.instances.perland_dir(&id).join("art");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("icon.png"), b"\x89PNG\r\n\x1a\n\0pack").unwrap();
        let pack_icon = core.instance_art(&id).await.unwrap().icon.unwrap();
        let file = picked(tmp.path(), "mine.png", PNG);
        let set = core
            .instance_art_set(&id, ArtChoice::Upload { path: file })
            .await
            .unwrap();
        assert_eq!(set.art, ArtMode::Custom);
        assert_eq!(core.instances.load(&id).await.unwrap().art, ArtMode::Custom);
        // A sync rewrites the pack's art; the player's background stays.
        std::fs::write(art.join("banner.png"), b"\x89PNG\r\n\x1a\n\0again").unwrap();
        let data = core.instance_art(&id).await.unwrap();
        assert_eq!(data.banner, Some(png_url(PNG)));
        // A new background never takes the icon with it: the pack's icon is pinned.
        assert_eq!(set.icon, Some(IconMode::Auto));
        assert_eq!(data.icon, Some(pack_icon));
        assert_eq!(custom_files(&core, &id), ["image.png"]);
    }

    #[tokio::test]
    async fn generated_choice_hides_pack_art_and_sets_the_seed() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let art = core.instances.perland_dir(&id).join("art");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("icon.png"), PNG).unwrap();
        core.instance_art_set(
            &id,
            ArtChoice::Upload {
                path: picked(tmp.path(), "mine.png", PNG),
            },
        )
        .await
        .unwrap();
        let set = core
            .instance_art_set(
                &id,
                ArtChoice::Generated {
                    seed: "dunes".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            (set.art, set.art_seed.as_deref()),
            (ArtMode::Generated, Some("dunes"))
        );
        // The background is generated; the icon stays the pack's.
        assert_eq!(
            core.instance_art(&id).await.unwrap(),
            InstanceArtData {
                icon: Some(png_url(PNG)),
                banner: None
            }
        );
        assert!(custom_files(&core, &id).is_empty());
        assert!(!core.instances.perland_dir(&id).join("custom").exists());
        // Back to the pack's own art.
        std::fs::write(art.join("banner.png"), PNG).unwrap();
        let set = core.instance_art_set(&id, ArtChoice::Pack).await.unwrap();
        assert_eq!(set.art, ArtMode::Auto);
        assert!(core.instance_art(&id).await.unwrap().banner.is_some());
    }

    #[tokio::test]
    async fn a_bad_seed_changes_nothing() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        for seed in ["".to_string(), "x".repeat(65)] {
            let err = core
                .instance_art_set(&id, ArtChoice::Generated { seed })
                .await
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::ArtFileInvalid);
        }
        let now = core.instances.load(&id).await.unwrap();
        assert_eq!((now.art, now.art_seed), (ArtMode::Auto, None));
        core.instance_art_set(
            &id,
            ArtChoice::Generated {
                seed: "x".repeat(64),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn pack_choice_needs_a_pack() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        core.instance_art_set(
            &plain.id,
            ArtChoice::Generated {
                seed: "dunes".into(),
            },
        )
        .await
        .unwrap();
        let err = core
            .instance_art_set(&plain.id, ArtChoice::Pack)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::NoPack);
        assert_eq!(
            core.instances.load(&plain.id).await.unwrap().art,
            ArtMode::Generated
        );
    }

    #[tokio::test]
    async fn uploads_are_checked() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        core.instance_art_set(
            &id,
            ArtChoice::Upload {
                path: picked(tmp.path(), "first.png", PNG),
            },
        )
        .await
        .unwrap();
        let sparse = tmp.path().join("huge.png");
        let file = std::fs::File::create(&sparse).unwrap();
        file.set_len(MAX_SOURCE_BYTES + 1).unwrap();
        drop(file);
        let folder = tmp.path().join("folder.png");
        std::fs::create_dir(&folder).unwrap();
        for bad in [
            picked(tmp.path(), "fake.png", b"MZ\x90\0this is an exe"),
            picked(
                tmp.path(),
                "vector.png",
                b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
            ),
            picked(tmp.path(), "anim.png", b"GIF89a\x01\0\x01\0"),
            sparse,
            folder,
            tmp.path().join("missing.png"),
        ] {
            let err = core
                .instance_art_set(&id, ArtChoice::Upload { path: bad.clone() })
                .await
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::ArtFileInvalid, "{bad:?}");
            assert_eq!(err.fix.as_deref(), Some("Pick a PNG, JPEG or WebP image."));
            // The previous picture and the mode are untouched.
            assert_eq!(core.instances.load(&id).await.unwrap().art, ArtMode::Custom);
            assert_eq!(custom_files(&core, &id), ["image.png"]);
        }
        // Never set: a bad first upload leaves the mode as it was.
        let other = pack_instance(&core, "Other", None).await.id;
        core.instance_art_set(
            &other,
            ArtChoice::Upload {
                path: tmp.path().join("missing.png"),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            core.instances.load(&other).await.unwrap().art,
            ArtMode::Auto
        );
        assert!(!core.instances.perland_dir(&other).join("custom").exists());
    }

    #[tokio::test]
    async fn upload_replaces_a_previous_upload_of_another_type() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        for (name, bytes) in [("a.png", PNG), ("b.webp", WEBP)] {
            core.instance_art_set(
                &id,
                ArtChoice::Upload {
                    path: picked(tmp.path(), name, bytes),
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(custom_files(&core, &id), ["image.webp"]);
        assert!(core
            .instance_art(&id)
            .await
            .unwrap()
            .banner
            .unwrap()
            .starts_with("data:image/webp;base64,"));
    }

    #[tokio::test]
    async fn the_picture_follows_the_bytes_not_the_name() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        core.instance_art_set(
            &id,
            ArtChoice::Upload {
                path: picked(tmp.path(), "photo.jpg", WEBP),
            },
        )
        .await
        .unwrap();
        assert_eq!(custom_files(&core, &id), ["image.webp"]);
    }

    #[tokio::test]
    async fn a_missing_java_doesnt_block_an_image_change() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let mut stored = core.instances.load(&id).await.unwrap();
        stored.java.path = Some(tmp.path().join("uninstalled/java.exe"));
        core.instances.save(&stored).await.unwrap();
        let set = core
            .instance_art_set(
                &id,
                ArtChoice::Generated {
                    seed: "dunes".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(set.art, ArtMode::Generated);
        let file = picked(tmp.path(), "mine.png", PNG);
        let set = core
            .instance_art_set(&id, ArtChoice::Upload { path: file })
            .await
            .unwrap();
        assert_eq!(set.art, ArtMode::Custom);
        assert_eq!(custom_files(&core, &id), ["image.png"]);
        let set = core.instance_art_set(&id, ArtChoice::Pack).await.unwrap();
        assert_eq!(set.art, ArtMode::Auto);
        // The player's other settings are kept as they were.
        assert_eq!(core.instances.load(&id).await.unwrap().java, stored.java);
    }

    #[tokio::test]
    async fn art_changes_are_refused_while_busy_or_installing() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let file = picked(tmp.path(), "mine.png", PNG);
        core.runs.lock().unwrap().insert(
            id.clone(),
            crate::app::Run {
                phase: Running,
                abort: None,
                killer: None,
                cancel: launcher_core::sync::Cancel::default(),
                answer: None,
                offer: None,
                update_only: false,
                synced: None,
            },
        );
        let upload = ArtChoice::Upload { path: file };
        let err = core
            .instance_art_set(&id, upload.clone())
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InstanceBusy);
        core.runs.lock().unwrap().clear();
        core.installing.lock().unwrap().insert(id.clone());
        let err = core.instance_art_set(&id, upload).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ContentInstalling);
        let err = core
            .instance_art_set(&id, ArtChoice::Pack)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ContentInstalling);
        let now = core.instances.load(&id).await.unwrap();
        assert_eq!(now.art, ArtMode::Auto);
        assert!(!core.instances.perland_dir(&id).join("custom").exists());
    }

    /// What `instance_art` sends for these PNG bytes.
    fn png_url(bytes: &[u8]) -> String {
        use base64::{engine::general_purpose::STANDARD, Engine};
        format!("data:image/png;base64,{}", STANDARD.encode(bytes))
    }

    /// A screenshot the game saved, `age` seconds old.
    fn screenshot(
        core: &crate::app::tests::TestCore,
        id: &str,
        name: &str,
        bytes: &[u8],
        age: u64,
    ) {
        let dir = core.instances.game_dir(id).join("screenshots");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        set_age(&path, age);
    }

    /// Make `path` look `age` seconds old.
    fn set_age(path: &std::path::Path, age: u64) {
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(age);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    /// A real 1×1 PNG, for what has to decode.
    fn tiny_png() -> Vec<u8> {
        use base64::{engine::general_purpose::STANDARD, Engine};
        STANDARD
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==")
            .unwrap()
    }

    #[tokio::test]
    async fn screenshots_list_the_images_newest_first() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        assert!(core.instance_screenshots(&id).await.unwrap().is_empty());
        screenshot(&core, &id, "old.png", PNG_A, 300);
        screenshot(&core, &id, "new.png", &tiny_png(), 10);
        screenshot(&core, &id, "mid.webp", WEBP, 100);
        screenshot(&core, &id, "notes.txt", b"hello", 0);
        screenshot(&core, &id, "fake.png", b"<html>not a png</html>", 0);
        std::fs::create_dir_all(core.instances.game_dir(&id).join("screenshots/dir.png")).unwrap();
        // Bigger than a background may be, but a 4K screenshot is: it is scaled down when picked.
        let big = core.instances.game_dir(&id).join("screenshots/big.png");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(MAX_ART_BYTES as u64 + 1)
            .unwrap();
        set_age(&big, 200);
        let huge = core.instances.game_dir(&id).join("screenshots/huge.png");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(MAX_SOURCE_BYTES + 1)
            .unwrap();
        assert_eq!(
            core.instance_screenshots(&id).await.unwrap(),
            ["fake.png", "new.png", "mid.webp", "big.png", "old.png"],
            "by name and size only: a fake is caught when it is read"
        );
        // A small thumbnail, not the whole picture.
        assert!(core
            .instance_screenshot(&id, "new.png")
            .await
            .unwrap()
            .unwrap()
            .starts_with("data:image/jpeg;base64,"));
        assert_eq!(
            core.instance_screenshot(&id, "big.png").await.unwrap(),
            None
        );
        assert_eq!(
            core.instance_screenshot(&id, "fake.png").await.unwrap(),
            None
        );
        assert_eq!(
            core.instance_screenshot(&id, "gone.png").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn screenshots_keep_to_the_newest_few() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        for n in 0..MAX_SCREENSHOTS + 3 {
            screenshot(&core, &id, &format!("{n:02}.png"), PNG, n as u64);
        }
        let names = core.instance_screenshots(&id).await.unwrap();
        assert_eq!(names.len(), MAX_SCREENSHOTS);
        assert_eq!(names[0], "00.png");
    }

    #[tokio::test]
    async fn a_screenshot_becomes_the_background() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        screenshot(&core, &id, "2026-10-17_12.00.00.png", PNG_A, 0);
        let set = core
            .instance_art_set(
                &id,
                ArtChoice::Screenshot {
                    name: "2026-10-17_12.00.00.png".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(set.art, ArtMode::Custom);
        assert_eq!(
            core.instance_art(&id).await.unwrap().banner,
            Some(png_url(PNG_A))
        );
        // A copy: the game's own screenshot can go and the background stays.
        std::fs::remove_file(
            core.instances
                .game_dir(&id)
                .join("screenshots/2026-10-17_12.00.00.png"),
        )
        .unwrap();
        assert_eq!(
            core.instance_art(&id).await.unwrap().banner,
            Some(png_url(PNG_A))
        );
    }

    #[tokio::test]
    async fn a_screenshot_name_never_leaves_the_folder() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        screenshot(&core, &id, "real.png", PNG, 0);
        std::fs::write(core.instances.game_dir(&id).join("outside.png"), PNG).unwrap();
        for name in [
            "../outside.png",
            "..\\outside.png",
            "",
            ".",
            "..",
            "/etc/passwd",
            "C:x.png",
        ] {
            let err = core
                .instance_art_set(&id, ArtChoice::Screenshot { name: name.into() })
                .await
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::ArtFileInvalid, "{name}");
            assert_eq!(
                core.instance_screenshot(&id, name).await.unwrap(),
                None,
                "{name}"
            );
        }
        assert_eq!(core.instances.load(&id).await.unwrap().art, ArtMode::Auto);
    }

    const PNG_A: &[u8] = b"\x89PNG\r\n\x1a\n\0A";
    const PNG_B: &[u8] = b"\x89PNG\r\n\x1a\n\0B";

    #[tokio::test]
    async fn icon_and_background_uploads_are_separate() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let set = core
            .instance_icon_set(
                &id,
                IconChoice::Upload {
                    path: picked(tmp.path(), "a.png", PNG_A),
                },
            )
            .await
            .unwrap();
        assert_eq!(set.icon, Some(IconMode::Custom));
        let set = core
            .instance_art_set(
                &id,
                ArtChoice::Upload {
                    path: picked(tmp.path(), "b.png", PNG_B),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            (set.icon, set.art),
            (Some(IconMode::Custom), ArtMode::Custom)
        );
        assert_eq!(
            core.instance_art(&id).await.unwrap(),
            InstanceArtData {
                icon: Some(png_url(PNG_A)),
                banner: Some(png_url(PNG_B)),
            }
        );
        assert_eq!(custom_files(&core, &id), ["icon.png", "image.png"]);
        // A landscape background keeps the icon.
        core.instance_art_set(
            &id,
            ArtChoice::Generated {
                seed: "dunes".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            core.instance_art(&id).await.unwrap(),
            InstanceArtData {
                icon: Some(png_url(PNG_A)),
                banner: None,
            }
        );
        assert_eq!(custom_files(&core, &id), ["icon.png"]);
        assert_eq!(
            core.instances.load(&id).await.unwrap().icon,
            Some(IconMode::Custom)
        );
    }

    #[tokio::test]
    async fn legacy_upload_fills_both_until_changed() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        // A round-1 instance.json: a custom picture and no `icon` key at all.
        let mut json = serde_json::to_value(core.instances.load(&id).await.unwrap()).unwrap();
        json["art"] = "custom".into();
        json.as_object_mut().unwrap().remove("icon");
        std::fs::write(
            core.instances.instance_dir(&id).join("instance.json"),
            serde_json::to_vec(&json).unwrap(),
        )
        .unwrap();
        let custom = core.instances.perland_dir(&id).join("custom");
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(custom.join("image.png"), PNG).unwrap();
        assert_eq!(core.instances.load(&id).await.unwrap().icon, None);
        assert_eq!(
            core.instance_art(&id).await.unwrap(),
            InstanceArtData {
                icon: Some(png_url(PNG)),
                banner: Some(png_url(PNG)),
            }
        );
        // A new background pins the old picture as the icon first.
        let set = core
            .instance_art_set(
                &id,
                ArtChoice::Generated {
                    seed: "dunes".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(set.icon, Some(IconMode::Custom));
        assert_eq!(
            core.instances.load(&id).await.unwrap().icon,
            Some(IconMode::Custom)
        );
        assert_eq!(
            core.instance_art(&id).await.unwrap(),
            InstanceArtData {
                icon: Some(png_url(PNG)),
                banner: None,
            }
        );
        assert!(custom.join("icon.png").exists());
        assert_eq!(custom_files(&core, &id), ["icon.png"]);
    }

    #[tokio::test]
    async fn pack_icon_choice() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        let err = core
            .instance_icon_set(&plain.id, IconChoice::Pack)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::NoPack);
        assert_eq!(core.instances.load(&plain.id).await.unwrap().icon, None);

        let id = pack_instance(&core, "Sky", None).await.id;
        let art = core.instances.perland_dir(&id).join("art");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("icon.png"), PNG_B).unwrap();
        let upload = IconChoice::Upload {
            path: picked(tmp.path(), "a.png", PNG_A),
        };
        core.instance_icon_set(&id, upload.clone()).await.unwrap();
        assert_eq!(custom_files(&core, &id), ["icon.png"]);
        let set = core.instance_icon_set(&id, IconChoice::Pack).await.unwrap();
        assert_eq!(set.icon, Some(IconMode::Auto));
        assert_eq!(
            core.instance_art(&id).await.unwrap().icon,
            Some(png_url(PNG_B))
        );
        assert!(custom_files(&core, &id).is_empty());

        core.instance_icon_set(&id, upload).await.unwrap();
        let set = core
            .instance_icon_set(&id, IconChoice::Generated)
            .await
            .unwrap();
        assert_eq!(set.icon, Some(IconMode::Generated));
        assert_eq!(
            core.instances.load(&id).await.unwrap().icon,
            Some(IconMode::Generated)
        );
        // Even with pack art there, a generated icon sends no image.
        assert_eq!(core.instance_art(&id).await.unwrap().icon, None);
        assert!(custom_files(&core, &id).is_empty());
    }

    #[tokio::test]
    async fn icon_uploads_are_checked() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        core.instance_icon_set(&id, IconChoice::Generated)
            .await
            .unwrap();
        let fake = picked(tmp.path(), "fake.png", b"MZ\x90\0this is an exe");
        let err = core
            .instance_icon_set(&id, IconChoice::Upload { path: fake.clone() })
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ArtFileInvalid);
        assert_eq!(err.fix.as_deref(), Some("Pick a PNG, JPEG or WebP image."));
        assert_eq!(
            core.instances.load(&id).await.unwrap().icon,
            Some(IconMode::Generated)
        );
        assert!(!core.instances.perland_dir(&id).join("custom").exists());
        // A good icon stays as it was after a bad one.
        core.instance_icon_set(
            &id,
            IconChoice::Upload {
                path: picked(tmp.path(), "good.png", PNG),
            },
        )
        .await
        .unwrap();
        core.instance_icon_set(&id, IconChoice::Upload { path: fake })
            .await
            .unwrap_err();
        assert_eq!(
            core.instances.load(&id).await.unwrap().icon,
            Some(IconMode::Custom)
        );
        assert_eq!(custom_files(&core, &id), ["icon.png"]);
        // Like the background, the bytes pick the type and a new type replaces the old file.
        core.instance_icon_set(
            &id,
            IconChoice::Upload {
                path: picked(tmp.path(), "photo.jpg", WEBP),
            },
        )
        .await
        .unwrap();
        assert_eq!(custom_files(&core, &id), ["icon.webp"]);
    }

    #[tokio::test]
    async fn icon_set_refused_while_busy() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        core.runs.lock().unwrap().insert(
            id.clone(),
            crate::app::Run {
                phase: Running,
                abort: None,
                killer: None,
                cancel: launcher_core::sync::Cancel::default(),
                answer: None,
                offer: None,
                update_only: false,
                synced: None,
            },
        );
        let upload = IconChoice::Upload {
            path: picked(tmp.path(), "mine.png", PNG),
        };
        for choice in [IconChoice::Generated, upload.clone()] {
            let err = core.instance_icon_set(&id, choice).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::InstanceBusy);
        }
        core.runs.lock().unwrap().clear();
        core.installing.lock().unwrap().insert(id.clone());
        for choice in [IconChoice::Pack, upload] {
            let err = core.instance_icon_set(&id, choice).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::ContentInstalling);
        }
        assert_eq!(core.instances.load(&id).await.unwrap().icon, None);
        assert!(!core.instances.perland_dir(&id).join("custom").exists());
    }

    #[tokio::test]
    async fn banner_focus_is_saved_and_reset_by_a_new_background() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        core.instance_art_set(
            &id,
            ArtChoice::Upload {
                path: picked(tmp.path(), "mine.png", PNG),
            },
        )
        .await
        .unwrap();
        let focus = Focus { x: 20, y: 80 };
        let set = core
            .instance_banner_focus_set(&id, Some(focus))
            .await
            .unwrap();
        assert_eq!(set.banner_focus, Some(focus));
        let loaded = core.instances.load(&id).await.unwrap();
        assert_eq!(loaded.banner_focus, Some(focus));
        assert_eq!(loaded.art, ArtMode::Custom, "only the position changes");
        // Back to the default.
        let set = core.instance_banner_focus_set(&id, None).await.unwrap();
        assert_eq!(set.banner_focus, None);
        core.instance_banner_focus_set(&id, Some(focus))
            .await
            .unwrap();
        // Any new background starts from the default position.
        let set = core
            .instance_art_set(&id, ArtChoice::Generated { seed: "sky".into() })
            .await
            .unwrap();
        assert_eq!(set.banner_focus, None);
        assert_eq!(core.instances.load(&id).await.unwrap().banner_focus, None);
    }

    #[tokio::test]
    async fn banner_focus_over_100_is_refused() {
        let (core, _runner, _host, _tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        let path = core.instances.instance_dir(&id).join("instance.json");
        let before = std::fs::read(&path).unwrap();
        for focus in [Focus { x: 101, y: 0 }, Focus { x: 0, y: 255 }] {
            let err = core
                .instance_banner_focus_set(&id, Some(focus))
                .await
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::BannerFocusInvalid);
        }
        assert_eq!(std::fs::read(&path).unwrap(), before);
        // The edges are inside the picture.
        let edge = Focus { x: 100, y: 0 };
        let set = core
            .instance_banner_focus_set(&id, Some(edge))
            .await
            .unwrap();
        assert_eq!(set.banner_focus, Some(edge));
    }

    #[tokio::test]
    async fn banner_focus_refused_while_busy() {
        let (core, _runner, _host, _tmp) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        core.runs.lock().unwrap().insert(
            id.clone(),
            crate::app::Run {
                phase: Running,
                abort: None,
                killer: None,
                cancel: launcher_core::sync::Cancel::default(),
                answer: None,
                offer: None,
                update_only: false,
                synced: None,
            },
        );
        let focus = Some(Focus { x: 20, y: 80 });
        let err = core
            .instance_banner_focus_set(&id, focus)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InstanceBusy);
        core.runs.lock().unwrap().clear();
        core.installing.lock().unwrap().insert(id.clone());
        let err = core
            .instance_banner_focus_set(&id, focus)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ContentInstalling);
        assert_eq!(core.instances.load(&id).await.unwrap().banner_focus, None);
    }

    #[tokio::test]
    async fn a_good_deep_link_waits_for_the_ui() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        core.deep_link("perland://pack?url=https%3A%2F%2Fx.example%2Fpack.toml");
        assert!(host
            .events
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, AppEvent::DeepLink)));
        assert_eq!(
            core.deep_link_take(),
            Some(DeepLink::Pack {
                url: "https://x.example/pack.toml".into()
            })
        );
        assert_eq!(core.deep_link_take(), None, "taken once");
    }

    #[tokio::test]
    async fn a_bad_deep_link_warns_and_keeps_nothing() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        core.deep_link("perland://pack?url=http%3A%2F%2Fx.example%2Fpack.toml");
        assert_eq!(host.warnings(), vec![ErrorCode::PackLinkInvalid]);
        assert_eq!(core.deep_link_take(), None);
    }
    #[tokio::test]
    async fn a_join_link_waits_typed_and_a_bad_one_warns() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        let token = "A".repeat(43);
        core.deep_link(&format!(
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}"
        ));
        assert_eq!(
            core.deep_link_take(),
            Some(DeepLink::Join {
                invite: format!("https://play.example.com/{token}")
            })
        );
        core.deep_link("perland://join?invite=https%3A%2F%2Fplay.example.com%2Fshort");
        assert_eq!(host.warnings(), vec![ErrorCode::InviteLinkInvalid]);
        assert_eq!(core.deep_link_take(), None);
    }
}
