//! Joining Perland servers by invite link (join-by-link spec §2.2): previewing an invite,
//! adding it as an instance, its live status, and moving it to a new link. Tauri only forwards
//! these commands.
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    instances::{unix_now, Instance, ServerLink, SourceRef, UpdatePolicy},
    invite::{EngineView, InviteLink},
    sources::{perland, PackInput},
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::{busy, AppCore},
    dto::{JoinPreview, ServerPlayers, ServerStatusView},
    events::Host,
    runner::Runner,
};

/// A server instance's saved invite, and the Engine it last used.
pub(crate) fn server_source(instance: &Instance) -> Option<(InviteLink, String)> {
    match &instance.source {
        SourceRef::Perland { invite, engine } => InviteLink::parse(invite)
            .ok()
            .map(|link| (link, engine.clone())),
        _ => None,
    }
}

fn has_pack() -> LauncherError {
    LauncherError::pack(
        ErrorCode::LinkHasPack,
        "This instance already gets its mods from a modpack",
    )
    .with_fix("Link a vanilla or imported instance, or join the server as a new instance")
}

fn not_a_server() -> LauncherError {
    LauncherError::pack(
        ErrorCode::NoPack,
        "This instance doesn't come from a server invite",
    )
}

/// A successful read: refresh the link, or start one for an instance that has none yet.
pub(crate) fn apply_view(server: &mut Option<ServerLink>, ev: &EngineView) {
    match server {
        Some(link) => link.refresh(&ev.view, &ev.engine, unix_now()),
        None => *server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now())),
    }
}

/// Whether two links differ in anything but when they were last read.
fn differs(before: &Option<ServerLink>, after: &Option<ServerLink>) -> bool {
    match (before, after) {
        (Some(before), Some(after)) => {
            let mut same_time = after.clone();
            same_time.last_seen = before.last_seen;
            same_time != *before
        }
        (None, None) => false,
        _ => true,
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The instance that already joins `link`'s server, compared by canonical invite.
    pub(crate) async fn instance_for_invite(&self, link: &InviteLink) -> Result<Option<Instance>> {
        let canonical = link.canonical();
        Ok(self.instances.list().await?.into_iter().find(|instance| {
            server_source(instance).is_some_and(|(saved, _)| saved.canonical() == canonical)
        }))
    }

    /// What Join server shows: the view and the icon. Nothing else is downloaded, and nothing
    /// is written.
    pub async fn join_preview(&self, link: &str) -> Result<JoinPreview> {
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        perland::supported(&link, &ev)?;
        let icon = self
            .runner
            .invite_icon(link.clone())
            .await
            .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)));
        let existing = self.instance_for_invite(&link).await?.map(|i| i.id);
        Ok(JoinPreview::new(&link, &ev, icon, existing))
    }

    /// Add the server as an instance, or return the one that already joins it. Nothing
    /// installs: the first Play does that.
    pub async fn join_add(&self, link: &str) -> Result<Instance> {
        let link = InviteLink::parse(link)?;
        if let Some(existing) = self.instance_for_invite(&link).await? {
            return Ok(existing);
        }
        let created = self
            .runner
            .add_pack(PackInput::Perland { link: link.clone() })
            .await?;
        let mut instance = self.instances.load(&created.id).await?;
        instance.update_policy = UpdatePolicy::Auto;
        // The view was just read by `add_pack`, so this is the shared copy.
        if let Ok(ev) = self.runner.invite_view(link, None).await {
            instance.server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now()));
        }
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// Change a server instance's `ServerLink` on disk and return it. The instance is reloaded
    /// first, so a rename or a sync made meanwhile stays. It is saved only on a real change:
    /// `last_seen` alone moving on every poll doesn't count.
    /// With `skip_when_busy`, the disk is left alone while the instance has a run: a run's own
    /// saves (sync, after-sync) rewrite the whole instance, and a poll must not race them. The
    /// returned link is still the fresh one.
    pub(crate) async fn update_server(
        &self,
        id: &str,
        skip_when_busy: bool,
        change: impl FnOnce(&mut Option<ServerLink>),
    ) -> Result<Option<ServerLink>> {
        let mut instance = self.instances.load(id).await?;
        let before = instance.server.clone();
        change(&mut instance.server);
        if differs(&before, &instance.server) && !(skip_when_busy && self.is_busy(id)) {
            self.instances.save(&instance).await?;
        }
        Ok(instance.server)
    }

    /// The last read said `invite_gone`: only `gone` changes; the address stays for joining.
    pub(crate) async fn mark_server_gone(
        &self,
        id: &str,
        skip_when_busy: bool,
    ) -> Result<Option<ServerLink>> {
        self.update_server(id, skip_when_busy, |server| {
            if let Some(link) = server {
                link.mark_gone();
            }
        })
        .await
    }

    /// The live status, through the shared 15 s view. It takes no lock, so it works while the
    /// game runs. A dead link is an answer (`gone`); anything else failing is an error, after
    /// which the UI polls less often.
    pub async fn server_status(&self, id: &str) -> Result<ServerStatusView> {
        let instance = self.instances.load(id).await?;
        let (link, engine) = server_source(&instance).ok_or_else(not_a_server)?;
        match self.runner.invite_view(link, Some(engine)).await {
            Ok(ev) => {
                let saved = self.update_server(id, true, |s| apply_view(s, &ev)).await?;
                Ok(ServerStatusView {
                    status: ev.view.status(),
                    players: ev.view.players.as_ref().map(ServerPlayers::from),
                    whitelisted: ev.view.whitelisted,
                    pack_state: ev.view.pack.as_ref().map(|p| p.state.clone()),
                    gone: false,
                    address: saved.and_then(|s| s.address),
                })
            }
            Err(e) if e.code == ErrorCode::InviteGone => {
                let last = self.mark_server_gone(id, true).await?;
                Ok(ServerStatusView {
                    status: None,
                    players: None,
                    whitelisted: last.as_ref().is_some_and(|s| s.whitelisted),
                    pack_state: None,
                    gone: true,
                    address: last.and_then(|s| s.address),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// "Link to a server": make a vanilla or imported instance a server instance. Its files
    /// stay; the next Play syncs to the server's pack, which keeps the player's own mods
    /// (link-instance spec §3, amended for #64). Refused while the instance has a run.
    pub async fn server_link(&self, id: &str, link: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.instances.load(id).await?.source != SourceRef::None {
            return Err(has_pack());
        }
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        perland::supported(&link, &ev)?;
        // Reload after the network read, so a save made meanwhile stays.
        if self.is_busy(id) {
            return Err(busy(id));
        }
        let mut instance = self.instances.load(id).await?;
        if instance.source != SourceRef::None {
            return Err(has_pack());
        }
        instance.source = SourceRef::Perland {
            invite: link.canonical(),
            engine: ev.engine.to_string(),
        };
        instance.server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now()));
        instance.update_policy = UpdatePolicy::Auto;
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// "Use a new link": point the instance at another invite. The files stay; the next Play
    /// checks the new link and syncs.
    /// Refused while the instance has a run: its sync saves the whole instance.
    pub async fn server_relink(&self, id: &str, link: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        server_source(&self.instances.load(id).await?).ok_or_else(not_a_server)?;
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        perland::supported(&link, &ev)?;
        // Reload after the network read, so a save made meanwhile (a rename, a sync) stays. A
        // Play may have started meanwhile, too.
        if self.is_busy(id) {
            return Err(busy(id));
        }
        let mut instance = self.instances.load(id).await?;
        instance.source = SourceRef::Perland {
            invite: link.canonical(),
            engine: ev.engine.to_string(),
        };
        instance.server = Some(ServerLink::from_view(&ev.view, &ev.engine, unix_now()));
        instance.update_policy = UpdatePolicy::Auto;
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// "Unlink": the instance stops following its server and is an ordinary one again. Every
    /// file stays (worlds, mods, configs); only the link and the pack's records go, so Play no
    /// longer syncs and the player can remove what the pack installed.
    /// Refused while the instance has a run: its sync saves the whole instance.
    pub async fn server_unlink(&self, id: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        let mut instance = self.instances.load(id).await?;
        server_source(&instance).ok_or_else(not_a_server)?;
        // The records first: if the save below fails, the instance stays linked and its next
        // sync only writes them again.
        launcher_core::sync::forget_pack(&self.instances.perland_dir(id)).await?;
        instance.source = SourceRef::None;
        instance.server = None;
        self.instances.save(&instance).await?;
        Ok(instance)
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::{ServerStatus, SourceRef, UpdatePolicy},
        ErrorCode, LauncherError,
    };

    use launcher_core::invite::InviteLink;

    use crate::{
        app::tests::core,
        dto::NewInstance,
        testing::{invite, serve, server_view},
    };

    #[tokio::test]
    async fn join_preview_reads_the_invite_and_writes_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        // A PNG's signature, then its IHDR chunk's length and type.
        *runner.icon.lock().unwrap() = Some(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec());
        let preview = core.join_preview(&invite('A')).await.unwrap();
        assert_eq!(preview.host, "play.example.com");
        assert_eq!(preview.name, "survival");
        assert_eq!(preview.kind, "stack");
        assert_eq!(preview.status, Some(ServerStatus::Online));
        assert_eq!(preview.players.as_ref().unwrap().max, Some(20));
        assert_eq!(preview.pack.as_ref().unwrap().mod_count, 2);
        assert_eq!(
            preview.team.as_ref().unwrap().accent.as_deref(),
            Some("violet")
        );
        assert_eq!(
            preview.address.as_ref().unwrap().display(),
            "play.example.com:25566"
        );
        assert!(preview
            .icon
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,iVBORw0KGgo"));
        assert_eq!(preview.existing, None);
        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert!(runner.added.lock().unwrap().is_empty(), "nothing is added");
    }

    #[tokio::test]
    async fn join_preview_refuses_networks_with_their_address() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut network = server_view("hub");
        network.view.kind = "network".into();
        network.view.pack = None;
        network.view.address.port = 25577;
        serve(&runner, &invite('A'), Ok(network));
        let err = core.join_preview(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteUnsupported);
        assert_eq!(err.params["address"], "play.example.com:25577");
    }

    /// Ruling R1: a modded stack that serves no pack can't be joined (never a vanilla instance).
    #[tokio::test]
    async fn join_preview_refuses_a_modded_stack_without_a_pack() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut modded = server_view("survival");
        modded.view.pack = None;
        assert_eq!(modded.view.game.loader.as_deref(), Some("fabric"));
        serve(&runner, &invite('A'), Ok(modded));
        let err = core.join_preview(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteUnsupported);
        assert_eq!(err.params["address"], "play.example.com:25566");
    }

    /// The refusal quotes the view's address text; an Engine that puts the invite link there
    /// must not get the token into the error.
    #[tokio::test]
    async fn a_refusal_never_quotes_the_invite_token() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let mut echo = server_view("hub");
        echo.view.kind = "network".into();
        echo.view.address.port = 0;
        echo.view.address.display = Some(invite('B'));
        serve(&runner, &invite('B'), Ok(echo));
        let token = "B".repeat(43);
        for err in [
            core.join_preview(&invite('B')).await.unwrap_err(),
            core.server_relink(&id, &invite('B')).await.unwrap_err(),
        ] {
            assert_eq!(err.code, ErrorCode::InviteUnsupported);
            let text = format!("{err:?} {}", serde_json::to_string(&err).unwrap());
            assert!(!text.contains(&token), "{text}");
        }
    }

    #[tokio::test]
    async fn junk_is_not_an_invite() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        for bad in ["https://play.example.com/short", "modrinth:sky", ""] {
            assert_eq!(
                core.join_preview(bad).await.unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
            assert_eq!(
                core.join_add(bad).await.unwrap_err().code,
                ErrorCode::InviteLinkInvalid,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn join_add_creates_an_automatic_server_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let instance = core.join_add(&invite('A')).await.unwrap();
        assert_eq!(instance.update_policy, UpdatePolicy::Auto);
        assert!(
            matches!(&instance.source, SourceRef::Perland { invite: i, .. } if *i == invite('A'))
        );
        let server = instance.server.clone().unwrap();
        assert_eq!(server.name, "survival");
        assert_eq!(server.accent.as_deref(), Some("violet"));
        assert_eq!(server.last_status, Some(ServerStatus::Online));
        assert_eq!(server.address.unwrap().display(), "play.example.com:25566");
        assert!(!server.gone);
        assert_eq!(core.instances.load(&instance.id).await.unwrap(), instance);
        let preview = core.join_preview(&invite('A')).await.unwrap();
        assert_eq!(preview.existing.as_deref(), Some(instance.id.as_str()));
    }

    #[tokio::test]
    async fn join_add_dedupes_case_and_slash_variants() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let first = core.join_add(&invite('A')).await.unwrap();
        let shouted = format!(
            "  {}/  ",
            invite('A').replace("play.example.com", "PLAY.Example.com:443")
        );
        let second = core.join_add(&shouted).await.unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(runner.added.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn server_status_refreshes_the_saved_link() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let mut asleep = server_view("survival");
        asleep.view.status = "sleeping".into();
        asleep.view.players = None;
        serve(&runner, &invite('A'), Ok(asleep));
        let status = core.server_status(&id).await.unwrap();
        assert_eq!(status.status, Some(ServerStatus::Sleeping));
        assert!(status.players.is_none() && !status.gone);
        assert_eq!(status.pack_state.as_deref(), Some("ready"));
        assert_eq!(
            core.instances
                .load(&id)
                .await
                .unwrap()
                .server
                .unwrap()
                .last_status,
            Some(ServerStatus::Sleeping)
        );
        assert_eq!(
            runner
                .engine_hints
                .lock()
                .unwrap()
                .last()
                .cloned()
                .flatten()
                .as_deref(),
            Some("https://api.example.com/"),
            "the saved Engine seeds the shared client"
        );
    }

    #[tokio::test]
    async fn a_dead_link_is_marked_and_keeps_its_address() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        serve(
            &runner,
            &invite('A'),
            Err(LauncherError::pack(ErrorCode::InviteGone, "gone")),
        );
        let status = core.server_status(&id).await.unwrap();
        assert!(status.gone && status.status.is_none());
        assert_eq!(status.address.unwrap().display(), "play.example.com:25566");
        assert!(core.instances.load(&id).await.unwrap().server.unwrap().gone);
        // The owner turned the invite back on.
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        assert!(!core.server_status(&id).await.unwrap().gone);
    }

    #[tokio::test]
    async fn a_rate_limit_is_passed_on_and_changes_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let before = core.instances.load(&id).await.unwrap();
        serve(
            &runner,
            &invite('A'),
            Err(LauncherError::network(
                ErrorCode::InviteRateLimited,
                "slow down",
            )),
        );
        assert_eq!(
            core.server_status(&id).await.unwrap_err().code,
            ErrorCode::InviteRateLimited
        );
        assert_eq!(core.instances.load(&id).await.unwrap(), before);
    }

    #[tokio::test]
    async fn relinking_moves_the_instance_and_keeps_its_files() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        let world = core.instances.game_dir(&id).join("saves/home/level.dat");
        std::fs::create_dir_all(world.parent().unwrap()).unwrap();
        std::fs::write(&world, b"blocks").unwrap();
        core.update_server(&id, false, |s| s.as_mut().unwrap().mark_gone())
            .await
            .unwrap();
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let moved = core.server_relink(&id, &invite('B')).await.unwrap();
        assert!(matches!(&moved.source, SourceRef::Perland { invite: i, .. } if *i == invite('B')));
        assert!(!moved.server.as_ref().unwrap().gone);
        assert_eq!(moved.update_policy, UpdatePolicy::Auto);
        assert!(world.is_file(), "the files stay; the next Play syncs");
        assert_eq!(core.instances.load(&id).await.unwrap(), moved);
    }

    #[tokio::test]
    async fn unlinking_makes_an_ordinary_instance_and_keeps_every_file() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let linked = core.join_add(&invite('A')).await.unwrap();
        let id = linked.id.clone();
        let game = core.instances.game_dir(&id);
        for file in [
            "saves/home/level.dat",
            "mods/pack.jar",
            "mods/mine.jar",
            "config/pack.toml",
            "resourcepacks/Faithful.zip",
        ] {
            std::fs::create_dir_all(game.join(file).parent().unwrap()).unwrap();
            std::fs::write(game.join(file), b"kept").unwrap();
        }
        let perland = core.instances.perland_dir(&id);
        crate::testing::managed_for(&["mods/pack.jar", "resourcepacks/Faithful.zip"])
            .save(&perland)
            .await
            .unwrap();
        std::fs::write(perland.join("pack.json"), b"{}").unwrap();

        let plain = core.server_unlink(&id).await.unwrap();

        assert_eq!(plain.source, SourceRef::None);
        assert_eq!(plain.server, None);
        assert_eq!(plain.name, linked.name, "the player's name stays");
        assert_eq!(plain.game, linked.game, "the game stays as it is");
        for file in [
            "saves/home/level.dat",
            "mods/pack.jar",
            "mods/mine.jar",
            "config/pack.toml",
            "resourcepacks/Faithful.zip",
        ] {
            assert!(game.join(file).is_file(), "{file} stays");
        }
        assert_eq!(core.instances.load(&id).await.unwrap(), plain);
        // The pack no longer owns anything: the player can remove what it installed.
        let packs = core
            .content_list(
                &id,
                launcher_core::browse::compat::ContentKind::ResourcePack,
            )
            .await
            .unwrap();
        assert!(packs.iter().all(|f| !f.managed));
        assert!(!perland.join("pack.json").exists(), "no update checks");
        assert_eq!(
            core.instance_for_invite(&InviteLink::parse(&invite('A')).unwrap())
                .await
                .unwrap(),
            None,
            "Join server adds a new instance again"
        );
        assert_eq!(
            core.server_status(&id).await.unwrap_err().code,
            ErrorCode::NoPack
        );
        // Instance settings opened before the unlink can't bring the link back.
        assert_eq!(
            core.instance_update(linked).await.unwrap().source,
            SourceRef::None
        );
        // And it can be linked again, as any ordinary instance.
        let again = core.server_link(&id, &invite('A')).await.unwrap();
        assert!(matches!(again.source, SourceRef::Perland { .. }));
    }

    #[tokio::test]
    async fn only_server_instances_unlink_and_never_while_running() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let ordinary = plain(&core).await;
        assert_eq!(
            core.server_unlink(&ordinary.id).await.unwrap_err().code,
            ErrorCode::NoPack
        );
        let id = core.join_add(&invite('A')).await.unwrap().id;
        hold(&core, &id);
        let before = instance_json(&core, &id);
        assert_eq!(
            core.server_unlink(&id).await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        assert_eq!(instance_json(&core, &id), before);
        core.runs.lock().unwrap().clear();
        core.server_unlink(&id).await.unwrap();
    }

    async fn plain(core: &crate::app::tests::TestCore) -> launcher_core::instances::Instance {
        core.instance_create(NewInstance {
            name: "Imported".into(),
            minecraft: "1.19.2".into(),
            loader: None,
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn linking_turns_a_plain_instance_into_a_server_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let before = plain(&core).await;
        let world = core
            .instances
            .game_dir(&before.id)
            .join("saves/home/level.dat");
        std::fs::create_dir_all(world.parent().unwrap()).unwrap();
        std::fs::write(&world, b"blocks").unwrap();

        let linked = core.server_link(&before.id, &invite('A')).await.unwrap();

        assert!(
            matches!(&linked.source, SourceRef::Perland { invite: i, .. } if *i == invite('A'))
        );
        assert_eq!(linked.server.as_ref().unwrap().name, "survival");
        assert_eq!(linked.update_policy, UpdatePolicy::Auto);
        assert_eq!(linked.name, "Imported", "the player's name stays");
        assert_eq!(
            linked.game, before.game,
            "the first Play changes the version"
        );
        assert!(world.is_file());
        assert_eq!(core.instances.load(&before.id).await.unwrap(), linked);
        assert_eq!(
            core.instance_for_invite(&InviteLink::parse(&invite('A')).unwrap())
                .await
                .unwrap()
                .map(|i| i.id),
            Some(before.id),
            "Join server now finds it"
        );
    }

    #[tokio::test]
    async fn only_plain_instances_can_be_linked() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let server = core.join_add(&invite('A')).await.unwrap();
        let mut pack = plain(&core).await;
        pack.source = SourceRef::Mrpack {
            name: "Pack.mrpack".into(),
            sha1: "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into(),
        };
        core.instances.save(&pack).await.unwrap();
        for id in [&server.id, &pack.id] {
            let before = instance_json(&core, id);
            assert_eq!(
                core.server_link(id, &invite('A')).await.unwrap_err().code,
                ErrorCode::LinkHasPack
            );
            assert_eq!(instance_json(&core, id), before);
        }
    }

    #[tokio::test]
    async fn a_refused_link_writes_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut network = server_view("hub");
        network.view.kind = "network".into();
        network.view.pack = None;
        serve(&runner, &invite('A'), Ok(network));
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let target = plain(&core).await;
        let before = instance_json(&core, &target.id);
        assert_eq!(
            core.server_link(&target.id, &invite('A'))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InviteUnsupported
        );
        assert_eq!(
            core.server_link(&target.id, "https://play.example.com/short")
                .await
                .unwrap_err()
                .code,
            ErrorCode::InviteLinkInvalid
        );
        hold(&core, &target.id);
        assert_eq!(
            core.server_link(&target.id, &invite('B'))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceBusy
        );
        assert_eq!(instance_json(&core, &target.id), before);
    }

    #[tokio::test]
    async fn only_server_instances_have_a_status_or_a_link_to_move() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let plain = core
            .instance_create(NewInstance {
                name: "Plain".into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        assert_eq!(
            core.server_status(&plain.id).await.unwrap_err().code,
            ErrorCode::NoPack
        );
        assert_eq!(
            core.server_relink(&plain.id, &invite('B'))
                .await
                .unwrap_err()
                .code,
            ErrorCode::NoPack
        );
    }

    #[tokio::test]
    async fn instance_update_keeps_source_and_server_from_disk() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        // B is another server, so its link differs from A's in more than the token.
        let mut creative = server_view("creative");
        creative.view.address.port = 25570;
        serve(&runner, &invite('B'), Ok(creative));
        let stale = core.join_add(&invite('A')).await.unwrap();
        core.server_relink(&stale.id, &invite('B')).await.unwrap();
        // Instance settings was opened before the relink and saves now, with a new name.
        let mut edited = stale.clone();
        edited.name = "Our server".into();
        let saved = core.instance_update(edited).await.unwrap();
        assert_eq!(saved.name, "Our server");
        assert!(matches!(&saved.source, SourceRef::Perland { invite: i, .. } if *i == invite('B')));
        let server = saved.server.clone().unwrap();
        assert_eq!(
            server.name, "creative",
            "the stale copy's link is not written back"
        );
        assert_eq!(server.address.unwrap().display(), "play.example.com:25570");
        assert_eq!(core.instances.load(&stale.id).await.unwrap(), saved);
        // A form that sends no link at all doesn't erase it either.
        let mut bare = saved.clone();
        bare.server = None;
        assert_eq!(
            core.instance_update(bare).await.unwrap().server,
            saved.server
        );
    }

    #[tokio::test]
    async fn instance_update_keeps_servers_automatic() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let mut instance = core.join_add(&invite('A')).await.unwrap();
        instance.update_policy = UpdatePolicy::Pinned;
        let saved = core.instance_update(instance).await.unwrap();
        assert_eq!(saved.update_policy, UpdatePolicy::Auto);
    }

    fn instance_json(core: &crate::app::tests::TestCore, id: &str) -> Vec<u8> {
        std::fs::read(core.instances.instance_dir(id).join("instance.json")).unwrap()
    }

    /// Pretend a run holds the instance.
    fn hold(core: &crate::app::tests::TestCore, id: &str) {
        core.runs.lock().unwrap().insert(
            id.to_string(),
            crate::app::Run {
                phase: crate::events::RunStateKind::Running,
                abort: None,
                killer: None,
                cancel: launcher_core::sync::Cancel::default(),
                answer: None,
                offer: None,
                update_only: false,
                synced: None,
            },
        );
    }

    #[tokio::test]
    async fn a_busy_instance_answers_but_is_not_written() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        hold(&core, &id);
        let before = instance_json(&core, &id);
        let mut asleep = server_view("survival");
        asleep.view.status = "sleeping".into();
        serve(&runner, &invite('A'), Ok(asleep));
        let status = core.server_status(&id).await.unwrap();
        assert_eq!(status.status, Some(ServerStatus::Sleeping));
        assert_eq!(instance_json(&core, &id), before);
        serve(
            &runner,
            &invite('A'),
            Err(LauncherError::pack(ErrorCode::InviteGone, "gone")),
        );
        let status = core.server_status(&id).await.unwrap();
        assert!(status.gone);
        assert_eq!(status.address.unwrap().display(), "play.example.com:25566");
        assert_eq!(instance_json(&core, &id), before);
    }

    /// A run rewrites the whole instance when it syncs, so a relink meanwhile would be lost or
    /// would swap the source under the run.
    #[tokio::test]
    async fn a_running_instance_keeps_its_link() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        hold(&core, &id);
        let before = instance_json(&core, &id);
        assert_eq!(
            core.server_relink(&id, &invite('B'))
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceBusy
        );
        assert_eq!(instance_json(&core, &id), before);
        core.runs.lock().unwrap().clear();
        core.server_relink(&id, &invite('B')).await.unwrap();
    }

    #[tokio::test]
    async fn an_unchanged_poll_does_not_rewrite_the_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        // An old read time, so a poll that rewrote `last_seen` would show.
        core.update_server(&id, false, |s| s.as_mut().unwrap().last_seen = Some(1))
            .await
            .unwrap();
        let before = instance_json(&core, &id);
        core.server_status(&id).await.unwrap();
        core.server_status(&id).await.unwrap();
        assert_eq!(instance_json(&core, &id), before);
    }

    #[tokio::test]
    async fn a_real_change_is_saved_with_a_fresh_read_time() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A')).await.unwrap().id;
        core.update_server(&id, false, |s| s.as_mut().unwrap().last_seen = Some(1))
            .await
            .unwrap();
        let mut renamed = server_view("survival 2");
        renamed.view.status = "offline".into();
        serve(&runner, &invite('A'), Ok(renamed));
        core.server_status(&id).await.unwrap();
        let server = core.instances.load(&id).await.unwrap().server.unwrap();
        assert_eq!(server.name, "survival 2");
        assert_eq!(server.last_status, Some(ServerStatus::Offline));
        assert!(server.last_seen.unwrap() > 1);
    }
}
