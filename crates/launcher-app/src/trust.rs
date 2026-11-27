//! The trust step: before a server's modpack installs on the player's computer, the player is
//! told who runs the server, how many files it installs and where they come from, and that mods
//! are programs that run with the player's own access. The answer is kept on the instance, keyed
//! by the server (`instances::server_key`), not by the invite token an owner can regenerate.
//!
//! Every way in reaches this: Join server and `perland://join` ask `join_trust` before adding,
//! the server lists ask it before Play, and any Play of an instance whose server isn't trusted
//! yet is refused with `server_trust_needed`, which the UI answers with the same step. A server
//! without a modpack needs no step, and neither does one the player owns.
use launcher_core::{
    instances::{server_key, unix_now, Instance, ServerTrust, UpdatePolicy},
    invite::{EngineView, InviteLink},
    perland::Relation,
    sync::lockfile::PACK_FILE,
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::{busy, AppCore},
    dto::{ModSourceCounts, TrustPrompt},
    events::Host,
    runner::Runner,
    servers::server_source,
};

pub(crate) fn trust_needed(id: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ServerTrustNeeded,
        "Confirm you trust this server before its modpack installs",
    )
    .with_param("id", id)
}

fn not_a_server() -> LauncherError {
    LauncherError::pack(
        ErrorCode::NoPack,
        "This instance doesn't come from a server invite",
    )
}

/// Whether the server installs a modpack: an Engine pack or a Modrinth modpack version.
fn installs_a_pack(ev: &EngineView) -> bool {
    ev.view.pack.is_some() || ev.view.modrinth.is_some()
}

/// How a Play may go, as far as trust is concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Gate {
    /// Trusted, owned, or without a modpack: the run goes as usual.
    Open,
    /// Not trusted, but the server can't be read right now and a version is installed: the run
    /// plays that version and takes no update, so nothing new installs.
    InstalledOnly,
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The trust step for joining `link`, or `None` when none is needed. Nothing is written.
    /// `instance` is the instance the link is for (Use a new link, Link to a server): its trust
    /// counts when the link reaches the server it trusts.
    pub async fn join_trust(
        &self,
        link: &str,
        instance: Option<&str>,
    ) -> Result<Option<TrustPrompt>> {
        let link = InviteLink::parse(link)?;
        let ev = self.runner.invite_view(link.clone(), None).await?;
        launcher_core::sources::perland::supported(&link, &ev)?;
        let existing = match instance {
            Some(id) => Some(self.instances.load(id).await?),
            None => self.instance_for_invite(&link).await?,
        };
        self.prompt_for(&link, &ev, existing.as_ref()).await
    }

    /// The trust step for instance `id`'s server, or `None` when none is needed.
    pub async fn instance_trust(&self, id: &str) -> Result<Option<TrustPrompt>> {
        let instance = self.instances.load(id).await?;
        let (link, engine) = server_source(&instance).ok_or_else(not_a_server)?;
        let ev = self.runner.invite_view(link.clone(), Some(engine)).await?;
        self.prompt_for(&link, &ev, Some(&instance)).await
    }

    /// The player trusts instance `id`'s server, and chose whether its pack updates on its own.
    /// Refused while the instance has a run: a run saves the whole instance.
    pub async fn instance_trust_set(&self, id: &str, auto_update: bool) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        let instance = self.instances.load(id).await?;
        let (link, engine) = server_source(&instance).ok_or_else(not_a_server)?;
        let ev = self.runner.invite_view(link, Some(engine)).await?;
        if self.is_busy(id) {
            return Err(busy(id));
        }
        let mut instance = self.instances.load(id).await?;
        trust(&mut instance, &ev, auto_update);
        self.instances.save(&instance).await?;
        Ok(instance)
    }

    /// Take the trust back: the next Play asks again before anything installs from the server.
    /// The files already installed stay.
    pub async fn instance_trust_forget(&self, id: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        let mut instance = self.instances.load(id).await?;
        server_source(&instance).ok_or_else(not_a_server)?;
        if instance.server_trust.take().is_some() {
            self.instances.save(&instance).await?;
        }
        Ok(instance)
    }

    /// Before a run of `instance`: refused with `server_trust_needed` while its server installs
    /// a modpack the player hasn't trusted. A server the player owns is trusted on the spot.
    pub(crate) async fn trust_gate(&self, instance: &Instance) -> Result<Gate> {
        let Some((link, engine)) = server_source(instance) else {
            return Ok(Gate::Open);
        };
        if instance.server_trust.is_some() {
            return Ok(Gate::Open);
        }
        let ev = match self.runner.invite_view(link.clone(), Some(engine)).await {
            Ok(ev) => ev,
            Err(error) => {
                let installed =
                    tokio::fs::try_exists(self.instances.perland_dir(&instance.id).join(PACK_FILE))
                        .await
                        .unwrap_or(false);
                // Without the view no new pack can be read, so what's installed may play.
                return if installed {
                    Ok(Gate::InstalledOnly)
                } else {
                    Err(error)
                };
            }
        };
        if !installs_a_pack(&ev) {
            return Ok(Gate::Open);
        }
        if self.owns(&link).await {
            let mut saved = self.instances.load(&instance.id).await?;
            saved.server_trust = Some(ServerTrust {
                server: server_key(&ev.view, &ev.engine),
                at: unix_now(),
            });
            self.instances.save(&saved).await?;
            return Ok(Gate::Open);
        }
        Err(trust_needed(&instance.id))
    }

    async fn prompt_for(
        &self,
        link: &InviteLink,
        ev: &EngineView,
        existing: Option<&Instance>,
    ) -> Result<Option<TrustPrompt>> {
        if !installs_a_pack(ev) {
            return Ok(None);
        }
        if existing.is_some_and(|i| i.trusts(&server_key(&ev.view, &ev.engine))) {
            return Ok(None);
        }
        if self.owns(link).await {
            return Ok(None);
        }
        let view = &ev.view;
        let counts = match &view.pack {
            Some(_) => self
                .runner
                .invite_mods(link.clone(), Some(ev.engine.to_string()))
                .await
                .map_err(|e| tracing::info!(code = ?e.code, "trust step without the mod list"))
                .ok()
                .map(|mods| mods.counts()),
            None => None,
        };
        Ok(Some(TrustPrompt {
            name: view.shown_name(),
            team: Some(view.team.display_name.trim().to_string()).filter(|t| !t.is_empty()),
            pack: view
                .pack
                .as_ref()
                .map(|p| p.name.trim().to_string())
                .filter(|n| !n.is_empty()),
            mod_count: counts
                .map(|c| c.total)
                .or_else(|| view.pack.as_ref().map(|p| p.mod_count))
                .unwrap_or(0),
            sources: counts.map(ModSourceCounts::from),
            modrinth_pack: view.pack.is_none() && view.modrinth.is_some(),
            // Existing server instances update on their own; a new one starts on too (see the
            // PR's decisions: a server's pack must match the server to join it).
            auto_update: existing.is_none_or(|i| i.update_policy == UpdatePolicy::Auto),
        }))
    }

    /// Whether the signed-in player owns the server `link` reaches: their free server, or an
    /// Owner entry in Your servers. Signed out, or the list can't be read: no.
    pub(crate) async fn owns(&self, link: &InviteLink) -> bool {
        let Some(session) = self.perland_session().await else {
            return false;
        };
        let canonical = link.canonical();
        let player = self.runner.player();
        let (servers, free) = tokio::join!(
            player.servers(session.engine.clone(), session.token.clone()),
            player.free_server(session.engine.clone(), session.token.clone()),
        );
        let free_is = free
            .ok()
            .flatten()
            .and_then(|state| state.server)
            .and_then(|f| f.invite_url)
            .and_then(|url| InviteLink::parse(&url).ok())
            .is_some_and(|l| l.canonical() == canonical);
        free_is
            || servers.ok().is_some_and(|entries| {
                entries.iter().any(|e| {
                    e.relation == Relation::Owner
                        && self
                            .runner
                            .public_invite(&e.server)
                            .is_some_and(|l| l.canonical() == canonical)
                })
            })
    }
}

/// Record the trust step's answer on `instance` for the server `ev` describes.
pub(crate) fn trust(instance: &mut Instance, ev: &EngineView, auto_update: bool) {
    instance.server_trust = Some(ServerTrust {
        server: server_key(&ev.view, &ev.engine),
        at: unix_now(),
    });
    instance.update_policy = if auto_update {
        UpdatePolicy::Auto
    } else {
        UpdatePolicy::Ask
    };
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::UpdatePolicy,
        invite::{InviteModrinth, ModSource, Url},
        perland::{Profile, Relation, Session, YourServer},
        public_list::PublicServer,
        ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::{core, TestCore},
        testing::{invite, player_token, serve, serve_mods, server_view},
    };

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
                client: None,
            })
            .unwrap();
    }

    fn owned(c: char) -> YourServer {
        YourServer {
            server: PublicServer {
                token: c.to_string().repeat(43),
                name: "Survival".into(),
                ..PublicServer::default()
            },
            relation: Relation::Owner,
            following: false,
            manage: None,
        }
    }

    #[tokio::test]
    async fn the_trust_step_says_who_runs_it_and_where_its_mods_come_from() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        serve_mods(
            &runner,
            &invite('A'),
            &[
                ("Sodium", ModSource::Modrinth),
                ("JEI", ModSource::CurseForge),
                ("custom.jar", ModSource::Uploaded),
                ("Tool", ModSource::Url),
            ],
        );
        let prompt = core.join_trust(&invite('A'), None).await.unwrap().unwrap();
        assert_eq!(prompt.name, "survival");
        assert_eq!(prompt.team.as_deref(), Some("Acme"));
        assert_eq!(prompt.pack.as_deref(), Some("Survival Pack"));
        assert_eq!(prompt.mod_count, 4);
        let sources = prompt.sources.unwrap();
        assert_eq!(
            (
                sources.modrinth,
                sources.curseforge,
                sources.uploaded,
                sources.web
            ),
            (1, 1, 1, 1)
        );
        assert!(!prompt.modrinth_pack);
        assert!(
            prompt.auto_update,
            "a new server starts updating on its own"
        );
        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert!(runner.added.lock().unwrap().is_empty(), "nothing is added");
    }

    #[tokio::test]
    async fn without_the_mod_list_the_step_counts_from_the_view() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let prompt = core.join_trust(&invite('A'), None).await.unwrap().unwrap();
        assert_eq!(prompt.mod_count, 2);
        assert_eq!(prompt.sources, None);
    }

    #[tokio::test]
    async fn a_modrinth_modpack_server_says_so() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut ev = server_view("survival");
        ev.view.pack = None;
        ev.view.modrinth = Some(InviteModrinth {
            project_id: "1KVo5zza".into(),
            version_id: "Yp8wLY1P".into(),
        });
        serve(&runner, &invite('A'), Ok(ev));
        let prompt = core.join_trust(&invite('A'), None).await.unwrap().unwrap();
        assert!(prompt.modrinth_pack);
        assert_eq!(prompt.sources, None);
        assert_eq!(
            runner.mods_reads.load(std::sync::atomic::Ordering::SeqCst),
            0
        );
    }

    #[tokio::test]
    async fn a_vanilla_server_needs_no_step_and_plays() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut ev = server_view("creative");
        ev.view.pack = None;
        ev.view.game.loader = None;
        serve(&runner, &invite('A'), Ok(ev));
        assert_eq!(core.join_trust(&invite('A'), None).await.unwrap(), None);
        let id = core.join_add(&invite('A'), None).await.unwrap().id;
        let instance = core.instances.load(&id).await.unwrap();
        assert_eq!(core.trust_gate(&instance).await.unwrap(), super::Gate::Open);
    }

    #[tokio::test]
    async fn the_players_own_server_never_asks() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        *runner.public_invites.lock().unwrap() = true;
        sign_in(&core);
        *runner.player.servers.lock().unwrap() = Ok(vec![owned('A')]);
        assert_eq!(core.join_trust(&invite('A'), None).await.unwrap(), None);
        // An instance of it is trusted at its first Play, quietly.
        let id = core.join_add(&invite('A'), None).await.unwrap().id;
        let instance = core.instances.load(&id).await.unwrap();
        assert_eq!(core.trust_gate(&instance).await.unwrap(), super::Gate::Open);
        assert!(core
            .instances
            .load(&id)
            .await
            .unwrap()
            .server_trust
            .is_some());

        // Someone else's server, or a list that can't be read, asks.
        serve(&runner, &invite('B'), Ok(server_view("other")));
        assert!(core.join_trust(&invite('B'), None).await.unwrap().is_some());
        serve(&runner, &invite('C'), Ok(server_view("mine")));
        *runner.player.servers.lock().unwrap() =
            Err(LauncherError::network(ErrorCode::Offline, "no"));
        assert!(core.join_trust(&invite('C'), None).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn trust_is_remembered_for_the_server_not_the_link() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A'), Some(false)).await.unwrap().id;
        let joined = core.instances.load(&id).await.unwrap();
        assert_eq!(
            joined.server_trust.as_ref().unwrap().server,
            "address:play.example.com:25566"
        );
        assert_eq!(joined.update_policy, UpdatePolicy::Ask);
        assert_eq!(core.join_trust(&invite('A'), None).await.unwrap(), None);
        assert_eq!(core.instance_trust(&id).await.unwrap(), None);

        // The owner made a new link for the same server: still trusted, and still asking.
        serve(&runner, &invite('B'), Ok(server_view("survival")));
        assert_eq!(
            core.join_trust(&invite('B'), Some(&id)).await.unwrap(),
            None
        );
        assert!(core.join_trust(&invite('B'), None).await.unwrap().is_some());
        let moved = core.server_relink(&id, &invite('B'), None).await.unwrap();
        assert!(moved.server_trust.is_some());
        assert_eq!(moved.update_policy, UpdatePolicy::Ask);

        // A link to another server is another server.
        let mut other = server_view("creative");
        other.view.address.port = 25570;
        serve(&runner, &invite('C'), Ok(other));
        assert!(core
            .join_trust(&invite('C'), Some(&id))
            .await
            .unwrap()
            .is_some());
        let elsewhere = core.server_relink(&id, &invite('C'), None).await.unwrap();
        assert_eq!(elsewhere.server_trust, None);
        let prompt = core.instance_trust(&id).await.unwrap().unwrap();
        assert!(
            !prompt.auto_update,
            "the switch starts where the player left it"
        );
    }

    #[tokio::test]
    async fn trust_can_be_taken_back_and_only_its_commands_change_it() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve(&runner, &invite('A'), Ok(server_view("survival")));
        let id = core.join_add(&invite('A'), Some(true)).await.unwrap().id;
        let mut edited = core.instances.load(&id).await.unwrap();
        edited.server_trust = None;
        edited.update_policy = UpdatePolicy::Pinned;
        let saved = core.instance_update(edited).await.unwrap();
        assert!(saved.server_trust.is_some(), "settings can't drop it");
        assert_eq!(
            saved.update_policy,
            UpdatePolicy::Auto,
            "a server is never pinned"
        );
        let mut asking = saved.clone();
        asking.update_policy = UpdatePolicy::Ask;
        assert_eq!(
            core.instance_update(asking).await.unwrap().update_policy,
            UpdatePolicy::Ask
        );

        let forgot = core.instance_trust_forget(&id).await.unwrap();
        assert_eq!(forgot.server_trust, None);
        assert!(core.instance_trust(&id).await.unwrap().is_some());
        let instance = core.instances.load(&id).await.unwrap();
        assert_eq!(
            core.trust_gate(&instance).await.unwrap_err().code,
            ErrorCode::ServerTrustNeeded
        );
        // Unlinking forgets it too.
        core.instance_trust_set(&id, true).await.unwrap();
        assert_eq!(core.server_unlink(&id).await.unwrap().server_trust, None);
    }
}
