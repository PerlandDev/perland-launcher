//! "Play this with friends" (spec `2026-10-21-play-with-friends-design.md`, and
//! `2026-10-21-free-server-settings-design.md` §4.2): the player's free server made from one of
//! their instances. A vanilla instance is a vanilla server on its exact version, an instance
//! installed from a Modrinth modpack is that pack by its version id, and any other modded
//! instance is a server pack the launcher writes (Modrinth CDN mods and config files only,
//! `share::write_server`), checks against the Engine's numbers before anything is sent, and
//! uploads; the Engine checks it again. A server's own instance is refused.
use launcher_core::{
    instances::{Instance, SourceRef},
    perland::{CreateFreeServer, FreeLimits, FreeServerSettings, Session},
    share::{self, PackMeta},
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{FreeServerCard, FriendsCheck},
    events::Host,
    free_servers::{
        checked_settings, checked_version, invalid, invalid_name, valid_name, valid_version_id,
    },
    perland::{host_of, language_tag},
    runner::Runner,
};

/// The Engine does not take a pack from the launcher (an older Engine, or the operator left it
/// off), so the button should not have shown.
fn packs_closed() -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "This Perland server can't be made from your own mods yet.",
    )
}

/// The Engine doesn't check the version (an older Engine): it would run the newest release, not
/// the instance's, so the button should not have shown.
fn versions_closed() -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "This Perland server can't make a server on your instance's version yet.",
    )
}

fn linked_to_a_server() -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "This instance belongs to a server, so it can't become another one.",
    )
}

/// The instance's stored Modrinth version id isn't one the Engine takes (a damaged instance).
fn bad_pack_version() -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "This modpack's Modrinth version can't be used for a free server. Reinstall the modpack from Browse.",
    )
    .with_param("field", "pack")
}

fn needs_a_loader() -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "Only modded instances can become a server this way.",
    )
}

fn too_heavy(host: &str, required: u32, limit: u32) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ModpackTooHeavy,
        "This modpack needs more memory than a free server has.",
    )
    .with_param("host", host)
    .with_param("requiredMemoryMb", required.to_string())
    .with_param("limitMemoryMb", limit.to_string())
}

fn too_big() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ShareTooLarge,
        "Your settings are too big for a free server.",
    )
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The Engine's free-server limits, refused when it takes no pack.
    async fn pack_limits(&self, session: &Session) -> Result<(FreeLimits, u64)> {
        let player = self.runner.player();
        let state = self
            .free_call(session, |e, t| player.free_server(e, t))
            .await?
            .ok_or_else(packs_closed)?;
        let max = state.limits.pack.ok_or_else(packs_closed)?.max_bytes;
        Ok((state.limits, max))
    }

    /// Instance `id`, refused when it belongs to a server.
    pub(crate) async fn friends_instance(&self, id: &str) -> Result<Instance> {
        let instance = self.instances.load(id).await?;
        if matches!(instance.source, SourceRef::Perland { .. }) || instance.server.is_some() {
            return Err(linked_to_a_server());
        }
        Ok(instance)
    }

    async fn modded_instance(&self, id: &str) -> Result<Instance> {
        let instance = self.friends_instance(id).await?;
        if instance.game.loader.is_none() {
            return Err(needs_a_loader());
        }
        Ok(instance)
    }

    /// What a free server made from instance `id`'s server pack would hold, against the
    /// Engine's limits. Uploads nothing; the plan is kept for the create that usually follows.
    /// For the pack path only: vanilla and Modrinth instances have nothing to check.
    pub async fn friends_check(&self, id: &str) -> Result<FriendsCheck> {
        let instance = self.modded_instance(id).await?;
        let session = self.free_session().await?;
        let (limits, _) = self.pack_limits(&session).await?;
        let plan = self.share_plan(&instance).await?;
        let check = share::server_check(&plan);
        {
            let mut plans = self.share_plans.lock().unwrap_or_else(|e| e.into_inner());
            plans.clear();
            plans.insert(instance.id.clone(), plan);
        }
        Ok(FriendsCheck {
            name: instance.name,
            mods: check.mods,
            server_mods: check.server_mods,
            required_mb: check.required_mb,
            limit_mb: limits.memory_mb.modded,
            not_on_modrinth: check.not_on_modrinth,
            settings: check.settings,
        })
    }

    /// Create the player's free server from instance `id`, with `settings` (left out: every
    /// default). No loader: a vanilla server on the instance's Minecraft version, on an Engine
    /// that checks it (`limits.settings`). Installed from a Modrinth modpack: that pack by its
    /// version id, nothing uploaded. Any other modded instance: write the server pack, refuse
    /// what the Engine would (mods not on Modrinth, too heavy, too big), then upload it.
    pub async fn friends_create(
        &self,
        id: &str,
        name: &str,
        settings: Option<FreeServerSettings>,
        language: &str,
    ) -> Result<FreeServerCard> {
        let name = name.trim();
        if !valid_name(name) {
            return Err(invalid_name());
        }
        let settings = checked_settings(settings)?;
        let instance = self.friends_instance(id).await?;
        let request = |kind: &str, version, modrinth_version_id| CreateFreeServer {
            name: name.to_string(),
            kind: kind.to_string(),
            modrinth_version_id,
            version,
            settings: settings.clone(),
            language: language_tag(language).to_string(),
        };
        if instance.game.loader.is_none() {
            let version =
                checked_version(Some(instance.game.minecraft.clone()))?.ok_or_else(invalid)?;
            let session = self.free_session().await?;
            let player = self.runner.player();
            let state = self
                .free_call(&session, |e, t| player.free_server(e, t))
                .await?;
            if state.and_then(|s| s.limits.settings).is_none() {
                return Err(versions_closed());
            }
            return self
                .free_server_create_checked(request("vanilla", Some(version), None))
                .await;
        }
        if let SourceRef::Modrinth { version_id, .. } = &instance.source {
            if !valid_version_id(version_id) {
                return Err(bad_pack_version());
            }
            return self
                .free_server_create_checked(request("modrinth", None, Some(version_id.clone())))
                .await;
        }
        let session = self.free_session().await?;
        let (limits, max_bytes) = self.pack_limits(&session).await?;
        let kept = self
            .share_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&instance.id);
        let plan = match kept {
            Some(plan) => plan,
            None => self.share_plan(&instance).await?,
        };
        let check = share::server_check(&plan);
        if check.not_on_modrinth.is_empty() && check.required_mb > limits.memory_mb.modded {
            return Err(too_heavy(
                host_of(&session.engine),
                check.required_mb,
                limits.memory_mb.modded,
            ));
        }
        // Inside `.perland/`, which no scan reads, and removed once read.
        let out = self
            .instances
            .perland_dir(&instance.id)
            .join("server-pack.mrpack");
        let meta = PackMeta {
            name: instance.name.clone(),
            version_id: share::today_utc(),
            minecraft: instance.game.minecraft.clone(),
            loader: instance.game.loader.clone(),
        };
        share::write_server(&self.instances.game_dir(&instance.id), &plan, meta, &out).await?;
        let pack = tokio::fs::read(&out).await;
        let _ = tokio::fs::remove_file(&out).await;
        let pack = pack?;
        if pack.len() as u64 > max_bytes {
            return Err(too_big());
        }
        let player = self.runner.player();
        let (name, language) = (name.to_string(), language_tag(language).to_string());
        let created = self
            .free_call(&session, |e, t| {
                player.create_free_server_from_pack(e, t, name, language, pack, settings)
            })
            .await?;
        Ok(self.free_card_of(&created).await)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use launcher_core::{
        accounts::BuildKind,
        instances::LoaderSpec,
        instances::SourceRef,
        invite::Url,
        meta::loaders::LoaderKind,
        perland::{
            CreateFreeServer, FreeServerSettings, FreeSettingsLimits, PackLimits, Profile, Session,
        },
        share::Linked,
        ErrorCode,
    };
    use serde_json::Value;

    use crate::{
        app::tests::{core, TestCore},
        dto::NewInstance,
        testing::{free_state, player_token, FakeRunner},
    };

    const CDN: &str = "https://cdn.modrinth.com/data/AAAA/versions/v1/known.jar";

    fn sign_in(core: &TestCore) {
        core.perland_store
            .save(&Session {
                engine: Url::parse("https://api.example.com").unwrap(),
                token: player_token('s'),
                profile: Profile {
                    name: "Steve".into(),
                    email: None,
                    discord: true,
                    github: false,
                    minecraft: None,
                },
            })
            .unwrap();
    }

    fn takes_packs(runner: &FakeRunner, modded_mb: u32) {
        let mut state = free_state(None);
        state.limits.memory_mb.modded = modded_mb;
        state.limits.pack = Some(PackLimits {
            max_bytes: 64 << 20,
        });
        *runner.player.free.lock().unwrap() = Ok(Some(state));
    }

    async fn instance(core: &TestCore, loader: bool, files: &[(&str, &[u8])]) -> String {
        let instance = core
            .instance_create(NewInstance {
                name: "Sky Friends".into(),
                minecraft: "1.20.1".into(),
                loader: loader.then(|| LoaderSpec {
                    kind: LoaderKind::Fabric,
                    version: "0.16.14".into(),
                }),
            })
            .await
            .unwrap();
        let game = core.instances.game_dir(&instance.id);
        for (rel, body) in files {
            let path = game.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        instance.id
    }

    /// An Engine that checks the version and takes settings, and no packs.
    fn takes_settings(runner: &FakeRunner) {
        let mut state = free_state(None);
        state.limits.settings = Some(FreeSettingsLimits {
            game_modes: vec!["survival".into(), "hardcore".into()],
            difficulties: vec!["easy".into(), "hard".into()],
            seed_max_bytes: 64,
            motd_max_bytes: 64,
        });
        *runner.player.free.lock().unwrap() = Ok(Some(state));
    }

    async fn with_source(core: &TestCore, id: &str, source: SourceRef) {
        let mut stored = core.instances.load(id).await.unwrap();
        stored.source = source;
        core.instances.update(&stored).await.unwrap();
    }

    fn knows(runner: &FakeRunner, body: &[u8], url: &str) {
        let sha1 = launcher_core::download::hash::sha1_hex(body);
        runner.browser.share_linked.lock().unwrap().insert(
            sha1.clone(),
            Linked {
                sha1,
                sha512: launcher_core::download::hash::sha512_hex(body),
                size: body.len() as u64,
                url: url.into(),
                env: None,
            },
        );
    }

    #[tokio::test]
    async fn check_names_mods_not_on_modrinth() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        takes_packs(&runner, 6144);
        knows(&runner, b"known", CDN);
        let id = instance(
            &core,
            true,
            &[
                ("mods/known.jar", b"known"),
                ("mods/own.jar", b"own"),
                ("config/a.toml", b"a = 1"),
            ],
        )
        .await;
        let check = core.friends_check(&id).await.unwrap();
        assert_eq!((check.mods, check.server_mods, check.settings), (2, 1, 1));
        assert_eq!((check.required_mb, check.limit_mb), (2560, 6144));
        assert_eq!(check.not_on_modrinth, ["own.jar"]);
        assert!(runner
            .player
            .calls_to("create_free_server_from_pack")
            .is_empty());

        let error = core
            .friends_create(&id, "Amigos", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::FreeServerModsNotOnModrinth);
        assert!(runner.player.uploaded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_uploads_the_server_pack() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        takes_packs(&runner, 6144);
        knows(&runner, b"known", CDN);
        let id = instance(
            &core,
            true,
            &[
                ("mods/known.jar", b"known"),
                ("config/a.toml", b"a = 1"),
                ("options.txt", b"v:1"),
            ],
        )
        .await;
        core.friends_check(&id).await.unwrap();
        let settings = FreeServerSettings {
            difficulty: Some("hard".into()),
            whitelist: Some(true),
            ..Default::default()
        };
        let card = core
            .friends_create(&id, " Amigos ", Some(settings.clone()), "es")
            .await
            .unwrap();
        assert_eq!(
            *runner.player.uploaded_settings.lock().unwrap(),
            [Some(settings)]
        );
        assert!(runner.player.calls_to("create_free_server").is_empty());
        assert_eq!(card.card.name, "Survival");
        // The check's plan was reused: no second Modrinth lookup.
        assert_eq!(runner.browser.share_lookups.lock().unwrap().len(), 1);

        let pack = runner.player.uploaded.lock().unwrap()[0].clone();
        let call = &runner.player.calls_to("create_free_server_from_pack")[0];
        assert_eq!(
            call.args,
            [
                player_token('s'),
                "Amigos".into(),
                "es".into(),
                launcher_core::download::hash::sha256_hex(&pack),
            ]
        );
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(pack)).unwrap();
        let mut names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["modrinth.index.json", "overrides/config/a.toml"]);
        let mut index = String::new();
        zip.by_name("modrinth.index.json")
            .unwrap()
            .read_to_string(&mut index)
            .unwrap();
        let index: Value = serde_json::from_str(&index).unwrap();
        assert_eq!(index["files"][0]["downloads"][0], CDN);
    }

    #[tokio::test]
    async fn refusals_upload_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        // Too heavy: 2560 MiB needed, 2048 there.
        takes_packs(&runner, 2048);
        knows(&runner, b"known", CDN);
        let id = instance(&core, true, &[("mods/known.jar", b"known")]).await;
        let error = core
            .friends_create(&id, "Amigos", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ModpackTooHeavy);
        assert_eq!(
            error.params.get("requiredMemoryMb").map(String::as_str),
            Some("2560")
        );

        // A bad name, and a vanilla instance.
        takes_packs(&runner, 6144);
        let error = core
            .friends_create(&id, "a b", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);
        let vanilla = instance(&core, false, &[]).await;
        let error = core.friends_check(&vanilla).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);

        // An Engine that takes no pack.
        *runner.player.free.lock().unwrap() = Ok(Some(free_state(None)));
        let error = core.friends_check(&id).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);
        assert!(runner.player.uploaded.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn vanilla_instance_creates_on_its_version() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        takes_settings(&runner);
        let id = instance(&core, false, &[("options.txt", b"v:1")]).await;
        let settings = FreeServerSettings {
            pvp: Some(false),
            ..Default::default()
        };
        core.friends_create(&id, "Amigos", Some(settings.clone()), "es")
            .await
            .unwrap();
        assert_eq!(
            *runner.player.create_requests.lock().unwrap(),
            [CreateFreeServer {
                name: "Amigos".into(),
                kind: "vanilla".into(),
                modrinth_version_id: None,
                version: Some("1.20.1".into()),
                settings: Some(settings),
                language: "es".into(),
            }]
        );
        assert!(runner.player.uploaded.lock().unwrap().is_empty());
        assert!(runner.browser.share_lookups.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn vanilla_instance_needs_an_engine_that_checks_the_version() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        // Packs on, settings off: an older Engine would run LATEST, not the instance's version.
        takes_packs(&runner, 6144);
        let id = instance(&core, false, &[]).await;
        let error = core
            .friends_create(&id, "Amigos", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);
        assert!(runner.player.calls_to("create_free_server").is_empty());
    }

    #[tokio::test]
    async fn modrinth_instance_creates_by_its_version_id() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        // No pack uploads and no settings on this Engine: neither is needed.
        *runner.player.free.lock().unwrap() = Ok(Some(free_state(None)));
        let id = instance(&core, true, &[("mods/own.jar", b"own")]).await;
        with_source(
            &core,
            &id,
            SourceRef::Modrinth {
                project_id: "1KVo5zza".into(),
                version_id: "Yp8wLY1P".into(),
            },
        )
        .await;
        core.friends_create(&id, "Amigos", None, "en")
            .await
            .unwrap();
        let sent = runner.player.create_requests.lock().unwrap().clone();
        assert_eq!(
            sent,
            [CreateFreeServer {
                name: "Amigos".into(),
                kind: "modrinth".into(),
                modrinth_version_id: Some("Yp8wLY1P".into()),
                version: None,
                settings: None,
                language: "en".into(),
            }]
        );
        assert!(runner
            .player
            .calls_to("create_free_server_from_pack")
            .is_empty());
        assert!(runner.browser.share_lookups.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn refusals_before_anything_is_sent() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        takes_settings(&runner);

        // Linked to a server.
        let server = instance(&core, false, &[]).await;
        with_source(
            &core,
            &server,
            SourceRef::Perland {
                invite: format!("https://play.example.com/{}", "A".repeat(43)),
                engine: "https://api.example.com".into(),
            },
        )
        .await;
        let error = core
            .friends_create(&server, "Amigos", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);
        let error = core.friends_check(&server).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);

        // Bad settings.
        let vanilla = instance(&core, false, &[]).await;
        let error = core
            .friends_create(
                &vanilla,
                "Amigos",
                Some(FreeServerSettings {
                    max_players: Some(0),
                    ..Default::default()
                }),
                "en",
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);
        assert_eq!(
            error.params.get("field").map(String::as_str),
            Some("settings.maxPlayers")
        );
        assert!(runner.player.calls.lock().unwrap().is_empty());

        // A bad name names its field.
        let error = core
            .friends_create(&vanilla, "a b", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.params.get("field").map(String::as_str), Some("name"));

        // A stored Modrinth version id the Engine wouldn't take: said plainly, not as a name problem.
        let pack = instance(&core, true, &[]).await;
        with_source(
            &core,
            &pack,
            SourceRef::Modrinth {
                project_id: "1KVo5zza".into(),
                version_id: "not/an id".into(),
            },
        )
        .await;
        let error = core
            .friends_create(&pack, "Amigos", None, "en")
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidFreeServer);
        assert!(!error.message.contains("name"), "{}", error.message);
        assert_eq!(error.params.get("field").map(String::as_str), Some("pack"));
        assert!(runner.player.calls_to("create_free_server").is_empty());
    }
}
