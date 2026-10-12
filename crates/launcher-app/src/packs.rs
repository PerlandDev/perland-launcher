//! Pack commands other than playing: adding a pack, its status, its mods, manual downloads
//! and art (spec addendum §4.4). Runs and syncs live in `runs.rs`.
use std::{path::Path, sync::Arc};

use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    fsutil::path_key,
    instances::{Instance, SourceRef},
    mods,
    sources::{
        art::{sniff, MAX_ART_BYTES},
        PackInput,
    },
    sync::{
        lockfile::{self, Managed},
        plan,
    },
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    deep_link,
    dto::{
        ManualProgress, OptionalChoice, PackAddRequest, PackArtData, PackContents, PackMod,
        PackStatus, PackUpdate,
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
        let installed = lockfile::load_pack(&self.instances.perland_dir(id)).await;
        let (update, check_error) = match &installed {
            None => (None, None),
            Some(_) => match self.runner.find_update(instance).await {
                Ok(update) => (update.as_ref().map(PackUpdate::from), None),
                Err(error) => (None, Some(error)),
            },
        };
        Ok(PackStatus {
            installed_version: installed.map(|d| d.pack_version),
            update,
            check_error,
        })
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

    /// The pack's icon and banner from `.perland/art/`.
    pub async fn pack_art(&self, id: &str) -> Result<PackArtData> {
        self.instances.load(id).await?;
        let dir = self.instances.perland_dir(id).join("art");
        Ok(PackArtData {
            icon: art_url(&dir, "icon").await,
            banner: art_url(&dir, "banner").await,
        })
    }

    /// A `perland://` link arrived (at startup or from a second launch). A good one waits for
    /// the UI (`deep_link_take`); a bad one is only a warning.
    pub fn deep_link(&self, raw: &str) {
        match deep_link::parse(raw) {
            Ok(url) => {
                *self.pending_link.lock().unwrap_or_else(|e| e.into_inner()) = Some(url);
                self.host.emit(AppEvent::DeepLink);
            }
            Err(error) => self.host.emit(AppEvent::Warning {
                instance: None,
                error,
            }),
        }
    }

    /// The waiting pack link, once.
    pub fn deep_link_take(&self) -> Option<String> {
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
        instances::{SourceRef, UpdatePolicy},
        sources::PackInput,
        sync::manual::Scanner,
        ErrorCode, LauncherError,
    };

    use crate::{
        app::{tests::core, ManualState},
        dto::{NewInstance, OpenTarget, PackAddRequest, PackArtData},
        events::{AppEvent, RunStateKind::*},
        testing::{desired, exit, managed_for, manual_file, pack_file, pack_instance, pending},
    };

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
                core.java_test(bad).await.unwrap_err().code,
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
    async fn pack_art_comes_back_as_data_urls_of_real_images_only() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let id = pack_instance(&core, "Sky", None).await.id;
        assert_eq!(
            core.pack_art(&id).await.unwrap(),
            PackArtData {
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
        let data = core.pack_art(&id).await.unwrap();
        assert!(data
            .icon
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert!(data
            .banner
            .unwrap()
            .starts_with("data:image/svg+xml;base64,"));
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
            core.deep_link_take().as_deref(),
            Some("https://x.example/pack.toml")
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
}
