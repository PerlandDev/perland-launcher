//! The player's Minecraft account and their totals per server (Minecraft link spec §6.2):
//! a link code to type in the game, the poll that sees the link land, unlinking, and the stats
//! matched to the instances that join each server. Every call goes to the session's Engine.
//! Nothing here logs a token, a link code or an invite token.
use std::collections::HashMap;

use launcher_core::{
    invite::Url,
    perland::{Profile, Session},
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{LinkCodeView, PerlandStats, ServerStatsView},
    events::Host,
    runner::Runner,
    servers::server_source,
};

/// What a call that needs a session answers without one: what the Engine's 401 answers.
fn signed_out() -> LauncherError {
    LauncherError::auth(
        ErrorCode::PerlandSignedOut,
        "You were signed out of Perland.",
    )
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The session, or `perland_signed_out` without one.
    async fn signed_in(&self) -> Result<Session> {
        self.perland_session().await.ok_or_else(signed_out)
    }

    /// `answer` as it is, after forgetting `session` when it is a 401.
    async fn forget_on_401<T>(&self, session: &Session, answer: Result<T>) -> Result<T> {
        if answer
            .as_ref()
            .is_err_and(|e| e.code == ErrorCode::PerlandSignedOut)
        {
            self.drop_session(&session.token).await;
        }
        answer
    }

    /// A fresh code to type in the game. It replaces the player's previous one.
    pub async fn perland_link_code(&self) -> Result<LinkCodeView> {
        let session = self.signed_in().await?;
        let answer = self
            .runner
            .player()
            .link_code(session.engine.clone(), session.token.clone())
            .await;
        let code = self.forget_on_401(&session, answer).await?;
        Ok(LinkCodeView {
            code: code.code,
            expires_in: code.expires_in,
        })
    }

    /// While a code shows: ask `/me` (not throttled like `perland_status`), keep the fresher
    /// profile and answer the linked Minecraft name, if the link landed.
    pub async fn perland_link_poll(&self) -> Result<Option<String>> {
        let session = self.signed_in().await?;
        let answer = self
            .runner
            .player()
            .me(session.engine.clone(), session.token.clone())
            .await;
        let me = self.forget_on_401(&session, answer).await?;
        let profile = Profile::from_player(&me);
        let minecraft = profile.minecraft.clone();
        self.refresh_profile(&session.token, profile).await;
        self.mark_checked(&session.token).await;
        Ok(minecraft)
    }

    /// Unlink the Minecraft account, then forget its name in the saved profile.
    pub async fn perland_unlink(&self) -> Result<()> {
        let session = self.signed_in().await?;
        let answer = self
            .runner
            .player()
            .unlink(session.engine.clone(), session.token.clone())
            .await;
        self.forget_on_401(&session, answer).await?;
        self.edit_profile(&session.token, |profile| profile.minecraft = None)
            .await;
        Ok(())
    }

    /// The player's totals on each server whose invite is on, each with the instance that joins
    /// it. `None` when signed out, when the Engine doesn't have them (the feature is off), and
    /// after a 401 (which also signs the player out).
    pub async fn perland_stats(&self) -> Result<Option<PerlandStats>> {
        let Some(session) = self.perland_session().await else {
            return Ok(None);
        };
        let answer = self
            .runner
            .player()
            .stats(session.engine.clone(), session.token.clone())
            .await;
        let stats = match self.forget_on_401(&session, answer).await {
            Ok(Some(stats)) => stats,
            Ok(None) => return Ok(None),
            Err(e) if e.code == ErrorCode::PerlandSignedOut => return Ok(None),
            Err(e) => return Err(e),
        };
        let instances = self.instances_by_invite_token(&session.engine).await?;
        let servers = stats
            .servers
            .into_iter()
            .filter_map(|server| {
                let token = server.invite?.token;
                Some(ServerStatsView {
                    instance: instances.get(&token).cloned(),
                    token,
                    name: server.name,
                    playtime_seconds: server.playtime_seconds,
                    advancements: server.advancements,
                })
            })
            .collect();
        Ok(Some(PerlandStats { servers }))
    }

    /// Each server instance's invite token and its id, for the instances whose invite belongs
    /// to `engine`: a token only names a server on its own Engine.
    async fn instances_by_invite_token(&self, engine: &Url) -> Result<HashMap<String, String>> {
        let mut found = HashMap::new();
        for instance in self.instances.list().await? {
            let Some((link, saved)) = server_source(&instance) else {
                continue;
            };
            if Url::parse(&saved).is_ok_and(|saved| saved.origin() == engine.origin()) {
                found.entry(link.token().to_string()).or_insert(instance.id);
            }
        }
        Ok(found)
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        invite::Url,
        perland::{MinecraftAccount, PlayerStats, Profile, ServerStats, Session, StatsInvite},
        ErrorCode, LauncherError, Result,
    };

    use crate::{
        app::tests::{core, TestCore},
        dto::{LinkCodeView, PerlandStats, ServerStatsView},
        testing::{invite, player_token, sample_player, serve, server_view, FakeRunner},
    };

    const ENGINE: &str = "https://api.example.com";

    fn engine(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    fn token(c: char) -> String {
        c.to_string().repeat(43)
    }

    fn sign_in(core: &TestCore, engine_url: &str, minecraft: Option<&str>) {
        core.perland_store
            .save(&Session {
                engine: engine(engine_url),
                token: player_token('s'),
                profile: Profile {
                    name: "Alex".into(),
                    email: Some("alex@example.com".into()),
                    discord: false,
                    minecraft: minecraft.map(str::to_string),
                },
            })
            .unwrap();
    }

    fn signed_out() -> LauncherError {
        LauncherError::auth(
            ErrorCode::PerlandSignedOut,
            "You were signed out of Perland.",
        )
    }

    fn server(name: &str, invite: Option<char>, playtime: u64, advancements: u32) -> ServerStats {
        ServerStats {
            name: name.into(),
            invite: invite.map(|c| StatsInvite { token: token(c) }),
            playtime_seconds: playtime,
            advancements,
        }
    }

    fn serve_stats(runner: &FakeRunner, stats: Result<Option<PlayerStats>>) {
        *runner.player.stats.lock().unwrap() = stats;
    }

    fn moi() -> MinecraftAccount {
        MinecraftAccount {
            uuid: "069a79f4-44e9-4726-a5be-fca90e38aaf5".into(),
            name: "Moi".into(),
            linked_at: "2026-10-17T00:00:00Z".into(),
        }
    }

    #[tokio::test]
    async fn stats_signed_out_is_none_and_calls_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert_eq!(core.perland_stats().await.unwrap(), None);
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn stats_drops_servers_without_invite_and_names_the_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, Some("Moi"));
        serve(&runner, &invite('A'), Ok(server_view("s")));
        let joined = core.join_add(&invite('A')).await.unwrap();
        serve_stats(
            &runner,
            Ok(Some(PlayerStats {
                minecraft: Some(moi()),
                servers: vec![
                    server("Asalith", Some('A'), 7200, 12),
                    server("Private", None, 60, 1),
                ],
            })),
        );
        let stats = core.perland_stats().await.unwrap().expect("stats");
        assert_eq!(
            stats,
            PerlandStats {
                servers: vec![ServerStatsView {
                    token: token('A'),
                    instance: Some(joined.id),
                    name: "Asalith".into(),
                    playtime_seconds: 7200,
                    advancements: 12,
                }],
            }
        );
        let call = &runner.player.calls_to("stats")[0];
        assert_eq!(call.engine, engine(ENGINE));
        assert_eq!(call.args, [player_token('s')]);
    }

    #[tokio::test]
    async fn stats_unmatched_token_has_no_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, Some("Moi"));
        serve(&runner, &invite('A'), Ok(server_view("s")));
        core.join_add(&invite('A')).await.unwrap();
        serve_stats(
            &runner,
            Ok(Some(PlayerStats {
                minecraft: Some(moi()),
                servers: vec![server("Other", Some('B'), 0, 3)],
            })),
        );
        let stats = core.perland_stats().await.unwrap().expect("stats");
        assert_eq!(stats.servers.len(), 1);
        assert_eq!(stats.servers[0].token, token('B'));
        assert_eq!(stats.servers[0].instance, None);
    }

    #[tokio::test]
    async fn stats_ignores_an_instance_of_another_engine() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        // The instance's invite was found through api.example.com; the session lives elsewhere.
        sign_in(&core, "https://other.example.org", Some("Moi"));
        serve(&runner, &invite('A'), Ok(server_view("s")));
        core.join_add(&invite('A')).await.unwrap();
        serve_stats(
            &runner,
            Ok(Some(PlayerStats {
                minecraft: Some(moi()),
                servers: vec![server("Asalith", Some('A'), 60, 1)],
            })),
        );
        let stats = core.perland_stats().await.unwrap().expect("stats");
        assert_eq!(stats.servers[0].instance, None);
    }

    #[tokio::test]
    async fn stats_none_from_engine_is_none() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, None);
        serve_stats(&runner, Ok(None));
        assert_eq!(core.perland_stats().await.unwrap(), None);
        assert!(core.perland_session().await.is_some(), "the session stays");
    }

    #[tokio::test]
    async fn stats_401_forgets_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, Some("Moi"));
        serve_stats(&runner, Err(signed_out()));
        assert_eq!(core.perland_stats().await.unwrap(), None);
        assert!(core.perland_store.load().is_none(), "the store is empty");
        assert_eq!(core.perland_status().await.unwrap().account, None);
    }

    #[tokio::test]
    async fn stats_other_errors_pass_through() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, None);
        serve_stats(
            &runner,
            Err(LauncherError::network(
                ErrorCode::Offline,
                "Couldn't reach api.example.com",
            )),
        );
        let err = core.perland_stats().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Offline);
        assert!(core.perland_session().await.is_some(), "the session stays");
    }

    #[tokio::test]
    async fn link_code_signed_out_is_perland_signed_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let err = core.perland_link_code().await.unwrap_err();
        assert_eq!(err, signed_out());
        let err = core.perland_unlink().await.unwrap_err();
        assert_eq!(err, signed_out());
        let err = core.perland_link_poll().await.unwrap_err();
        assert_eq!(err, signed_out());
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn link_code_goes_to_the_session_engine() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://engine-a.example.com", None);
        let code = core.perland_link_code().await.unwrap();
        assert_eq!(
            code,
            LinkCodeView {
                code: "ABCD-EFGH".into(),
                expires_in: 600,
            }
        );
        let calls = runner.player.calls_to("link_code");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].engine, engine("https://engine-a.example.com"));
        assert_eq!(calls[0].args, [player_token('s')]);
    }

    #[tokio::test]
    async fn link_code_401_forgets_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, None);
        *runner.player.link_code.lock().unwrap() = Err(signed_out());
        let err = core.perland_link_code().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_session().await.is_none());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn link_poll_returns_the_name_and_status_carries_it() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, None);
        *runner.player.me.lock().unwrap() = Ok(launcher_core::perland::Player {
            minecraft: Some(moi()),
            ..sample_player()
        });
        assert_eq!(
            core.perland_link_poll().await.unwrap().as_deref(),
            Some("Moi")
        );
        let call = &runner.player.calls_to("me")[0];
        assert_eq!(call.engine, engine(ENGINE));
        assert_eq!(
            core.perland_store
                .load()
                .unwrap()
                .profile
                .minecraft
                .as_deref(),
            Some("Moi"),
            "the linked name is saved for the next launch"
        );
        let account = core.perland_status().await.unwrap().account.unwrap();
        assert_eq!(account.minecraft.as_deref(), Some("Moi"));
        assert_eq!(
            runner.player.calls_to("me").len(),
            1,
            "the poll counts as the status's /me"
        );
    }

    #[tokio::test]
    async fn link_poll_not_linked_yet_is_none() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, None);
        assert_eq!(core.perland_link_poll().await.unwrap(), None);
    }

    #[tokio::test]
    async fn link_poll_401_forgets_and_errors() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, None);
        *runner.player.me.lock().unwrap() = Err(signed_out());
        let err = core.perland_link_poll().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_session().await.is_none());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn unlink_clears_the_name() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, Some("Moi"));
        core.perland_unlink().await.unwrap();
        let call = &runner.player.calls_to("unlink")[0];
        assert_eq!(call.engine, engine(ENGINE));
        assert_eq!(call.args, [player_token('s')]);
        let session = core.perland_session().await.unwrap();
        assert_eq!(session.profile.minecraft, None);
        assert_eq!(
            session.profile.name, "Alex",
            "the rest of the profile stays"
        );
        assert_eq!(core.perland_store.load().unwrap().profile.minecraft, None);
    }

    #[tokio::test]
    async fn unlink_failure_keeps_the_name() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, Some("Moi"));
        *runner.player.unlink.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::Offline,
            "Couldn't reach api.example.com",
        ));
        let err = core.perland_unlink().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::Offline);
        let session = core.perland_session().await.unwrap();
        assert_eq!(session.profile.minecraft.as_deref(), Some("Moi"));
    }

    #[tokio::test]
    async fn unlink_401_forgets_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, ENGINE, Some("Moi"));
        *runner.player.unlink.lock().unwrap() = Err(signed_out());
        let err = core.perland_unlink().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_session().await.is_none());
    }
}
