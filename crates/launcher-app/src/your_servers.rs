//! Your servers (sign-in and Your servers spec §5 and §6.2): the signed-in player's owned,
//! invited and followed servers, and the three calls that keep that list current. Every call
//! goes to the session's Engine, and an invite token is sent only when the invite's own Engine
//! is that one. Nothing here logs a token or an invite.
use launcher_core::{
    instances::ServerAddress, invite::InviteLink, perland::Relation, ErrorCode, Result,
};

use crate::{
    app::AppCore,
    dto::{YourServerCard, YourServersPage},
    events::Host,
    public_servers::{card, instance_of},
    runner::Runner,
};

#[derive(Clone, Copy)]
enum Call {
    Follow,
    Unfollow,
    Joined,
}

impl Call {
    fn name(self) -> &'static str {
        match self {
            Self::Follow => "follow",
            Self::Unfollow => "unfollow",
            Self::Joined => "joined",
        }
    }
}

fn relation_name(relation: Relation) -> &'static str {
    match relation {
        Relation::Owner => "owner",
        Relation::Invited => "invited",
        Relation::Followed => "followed",
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The player's servers, or `None` when signed out, when the Engine has the feature or
    /// accounts closed, and after a 401 (which also signs the player out).
    pub async fn your_servers(&self) -> Result<Option<YourServersPage>> {
        let Some(session) = self.perland_session().await else {
            return Ok(None);
        };
        let player = self.runner.player();
        // The free server shows as its own card (built from its own view), so its entry here
        // is left out. That view is read alongside; whatever it answers, the list stands.
        let (fetched, free) = tokio::join!(
            player.servers(session.engine.clone(), session.token.clone()),
            player.free_server(session.engine.clone(), session.token.clone()),
        );
        let free = free.ok().flatten().and_then(|state| state.server);
        let entries = match fetched {
            Ok(entries) => entries,
            Err(e) if e.code == ErrorCode::PerlandSignedOut => {
                self.drop_session(&session.token).await;
                return Ok(None);
            }
            Err(e)
                if matches!(
                    e.code,
                    ErrorCode::PlayerServersClosed | ErrorCode::PlayerAccountsClosed
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        };
        let joined = self.joined_invites().await?;
        let free_invite = free
            .as_ref()
            .and_then(|f| f.invite_url.as_deref())
            .and_then(|url| InviteLink::parse(url).ok())
            .map(|link| link.canonical());
        let servers = entries
            .iter()
            .filter_map(|entry| {
                let server = &entry.server;
                let invite = self.runner.public_invite(server).map(|l| l.canonical());
                let is_free = entry.relation == Relation::Owner
                    && free.as_ref().is_some_and(|f| match &free_invite {
                        Some(free_invite) => invite.as_ref() == Some(free_invite),
                        None => server.name.trim() == f.name.trim(),
                    });
                if is_free {
                    return None;
                }
                let instance = instance_of(&joined, invite.as_deref());
                let address =
                    ServerAddress::from_view(&server.address, &session.engine).map(|a| a.display());
                let mut card = card(server, invite, address, instance, true);
                // The list can't say which Modrinth pack a server runs; an instance that
                // already joins it can play it all the same.
                card.joinable |= card.instance.is_some();
                Some(YourServerCard {
                    card,
                    relation: relation_name(entry.relation).into(),
                    following: entry.following,
                })
            })
            .collect();
        Ok(Some(YourServersPage { servers }))
    }

    /// Tell the Engine the player follows the server `invite` names. Never fails.
    pub async fn your_servers_follow(&self, invite: &str) {
        self.quiet(Call::Follow, invite).await;
    }

    /// Tell the Engine the player stopped following it. A skipped call (signed out, another
    /// Engine) is `Ok`.
    pub async fn your_servers_unfollow(&self, invite: &str) -> Result<()> {
        self.relation_call(Call::Unfollow, invite).await
    }

    /// Tell the Engine the player joined the server `invite` names. Never fails.
    pub async fn your_servers_joined(&self, invite: &str) {
        self.quiet(Call::Joined, invite).await;
    }

    /// A call whose failure is only logged, by code.
    async fn quiet(&self, call: Call, invite: &str) {
        if let Err(e) = self.relation_call(call, invite).await {
            tracing::info!(code = ?e.code, call = call.name(), "Your servers call didn't go through");
        }
    }

    /// Send `call` for `invite` to the session's Engine, unless there is no session or the
    /// invite belongs to another Engine, which must not get the token.
    async fn relation_call(&self, call: Call, invite: &str) -> Result<()> {
        let link = InviteLink::parse(invite)?;
        let Some(session) = self.perland_session().await else {
            return Ok(());
        };
        let player = self.runner.player();
        let engine = player.engine_for(link.clone()).await?;
        if engine.origin() != session.engine.origin() {
            tracing::info!(
                call = call.name(),
                "an invite of another Engine; nothing sent to the Perland account's"
            );
            return Ok(());
        }
        let (engine, token, invite_token) = (
            session.engine.clone(),
            session.token.clone(),
            link.token().to_string(),
        );
        let answer = match call {
            Call::Follow => player.follow(engine, token, invite_token).await,
            Call::Unfollow => player.unfollow(engine, token, invite_token).await,
            Call::Joined => player.joined(engine, token, invite_token).await,
        };
        if let Err(e) = &answer {
            if e.code == ErrorCode::PerlandSignedOut {
                self.drop_session(&session.token).await;
            }
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::ServerStatus,
        invite::Url,
        invite::{InviteAddress, InviteGame},
        perland::{Profile, Relation, Session, YourServer},
        public_list::PublicServer,
        LauncherError,
    };

    use crate::{
        app::tests::{core, TestCore},
        testing::{free_state, free_view, invite, player_token, serve, server_view, FakeRunner},
    };

    use super::*;

    fn token(c: char) -> String {
        c.to_string().repeat(43)
    }

    fn sign_in(core: &TestCore, engine: &str) {
        core.perland_store
            .save(&Session {
                engine: Url::parse(engine).unwrap(),
                token: player_token('s'),
                profile: Profile {
                    name: "Steve".into(),
                    email: None,
                    discord: true,
                },
            })
            .unwrap();
    }

    fn listed(c: char, status: &str) -> PublicServer {
        PublicServer {
            token: token(c),
            name: "Survival".into(),
            address: InviteAddress {
                host: Some("play.example.com".into()),
                port: 25566,
                display: None,
            },
            game: InviteGame {
                kind: "paper".into(),
                version: Some("1.21.1".into()),
                ..InviteGame::default()
            },
            status: status.into(),
            language: "en".into(),
            ..PublicServer::default()
        }
    }

    fn entry(c: char, status: &str, relation: Relation, following: bool) -> YourServer {
        YourServer {
            server: listed(c, status),
            relation,
            following,
        }
    }

    fn serve_servers(runner: &FakeRunner, servers: Vec<YourServer>) {
        *runner.player.servers.lock().unwrap() = Ok(servers);
    }

    fn fail(runner: &FakeRunner, code: ErrorCode) {
        *runner.player.servers.lock().unwrap() = Err(LauncherError::network(code, "no"));
        *runner.player.relation.lock().unwrap() = Err(LauncherError::network(code, "no"));
    }

    #[tokio::test]
    async fn none_when_signed_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert_eq!(core.your_servers().await.unwrap(), None);
        assert!(runner.player.calls_to("servers").is_empty());
    }

    #[tokio::test]
    async fn none_when_servers_closed() {
        for code in [
            ErrorCode::PlayerServersClosed,
            ErrorCode::PlayerAccountsClosed,
        ] {
            let (core, runner, _host, _t) = core(BuildKind::Dev);
            sign_in(&core, "https://api.example.com");
            fail(&runner, code);
            assert_eq!(core.your_servers().await.unwrap(), None);
            assert!(
                core.perland_session().await.is_some(),
                "a closed feature keeps the session"
            );
        }
    }

    #[tokio::test]
    async fn your_servers_after_401_is_none_and_forgets() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        fail(&runner, ErrorCode::PerlandSignedOut);
        assert_eq!(core.your_servers().await.unwrap(), None);
        assert!(core.perland_session().await.is_none());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn other_errors_pass_through() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        fail(&runner, ErrorCode::InviteSiteUnreachable);
        let err = core.your_servers().await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteSiteUnreachable);
        assert!(core.perland_session().await.is_some());
    }

    #[tokio::test]
    async fn cards_keep_relation_following_and_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve(&runner, &invite('A'), Ok(server_view("s")));
        let joined = core.join_add(&invite('A')).await.unwrap();
        serve_servers(
            &runner,
            vec![
                entry('A', "online", Relation::Owner, true),
                entry('B', "online", Relation::Invited, false),
                entry('C', "online", Relation::Followed, true),
            ],
        );
        let page = core.your_servers().await.unwrap().unwrap();
        let cards = &page.servers;
        assert_eq!(cards.len(), 3);
        assert_eq!(cards[0].relation, "owner");
        assert!(cards[0].following);
        assert_eq!(cards[0].card.instance.as_deref(), Some(joined.id.as_str()));
        assert_eq!(cards[0].card.invite.as_deref(), Some(invite('A').as_str()));
        assert_eq!(cards[0].card.token, token('A'));
        assert_eq!(cards[1].relation, "invited");
        assert!(!cards[1].following);
        assert_eq!(cards[1].card.instance, None);
        assert_eq!(cards[2].relation, "followed");
        assert_eq!(cards[0].card.status, ServerStatus::Online);
        assert_eq!(
            cards[0].card.address.as_deref(),
            Some("play.example.com:25566")
        );
        let call = &runner.player.calls_to("servers")[0];
        assert_eq!(call.engine, Url::parse("https://api.example.com").unwrap());
        assert_eq!(call.args, [player_token('s')]);
    }

    #[tokio::test]
    async fn your_servers_hides_the_free_owner_entry() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_servers(
            &runner,
            vec![
                entry('A', "online", Relation::Owner, false),
                entry('F', "online", Relation::Owner, false),
                entry('B', "online", Relation::Invited, false),
            ],
        );
        *runner.player.free.lock().unwrap() = Ok(Some(free_state(Some(free_view("online")))));
        let page = core.your_servers().await.unwrap().unwrap();
        let tokens: Vec<_> = page.servers.iter().map(|c| c.card.token.clone()).collect();
        assert_eq!(
            tokens,
            [token('A'), token('B')],
            "the free server shows once, as its own card"
        );
        assert_eq!(runner.player.calls_to("free_server").len(), 1);
    }

    #[tokio::test]
    async fn your_servers_hides_by_name_without_invite() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let mut other = entry('C', "online", Relation::Owner, false);
        other.server.name = "Creative".into();
        serve_servers(
            &runner,
            vec![
                entry('A', "online", Relation::Owner, false),
                entry('B', "online", Relation::Invited, false),
                other,
            ],
        );
        let mut free = free_view("creating");
        free.invite_url = None;
        *runner.player.free.lock().unwrap() = Ok(Some(free_state(Some(free))));
        let page = core.your_servers().await.unwrap().unwrap();
        let tokens: Vec<_> = page.servers.iter().map(|c| c.card.token.clone()).collect();
        assert_eq!(
            tokens,
            [token('B'), token('C')],
            "only the owner entry named like the free server"
        );
    }

    #[tokio::test]
    async fn free_server_errors_leave_the_list_alone() {
        for answer in [
            Err(LauncherError::network(ErrorCode::HostTrouble, "trouble")),
            Err(LauncherError::auth(
                ErrorCode::PerlandSignedOut,
                "signed out",
            )),
            Ok(None),
        ] {
            let (core, runner, _host, _t) = core(BuildKind::Dev);
            sign_in(&core, "https://api.example.com");
            serve_servers(&runner, vec![entry('F', "online", Relation::Owner, false)]);
            *runner.player.free.lock().unwrap() = answer;
            let page = core.your_servers().await.unwrap().unwrap();
            assert_eq!(page.servers.len(), 1);
        }
    }

    #[tokio::test]
    async fn card_with_instance_is_joinable() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve(&runner, &invite('A'), Ok(server_view("s")));
        let joined = core.join_add(&invite('A')).await.unwrap();
        // The list doesn't say which modpack a modded server runs.
        let bare = |c: char| {
            let mut e = entry(c, "online", Relation::Followed, true);
            e.server.modded = true;
            e.server.game.loader = Some("fabric".into());
            e
        };
        serve_servers(&runner, vec![bare('A'), bare('B')]);
        let page = core.your_servers().await.unwrap().unwrap();
        assert_eq!(
            page.servers[0].card.instance.as_deref(),
            Some(joined.id.as_str())
        );
        assert!(
            page.servers[0].card.joinable,
            "Play plays the instance it has"
        );
        assert!(!page.servers[1].card.joinable);
    }

    #[tokio::test]
    async fn offline_stays_offline() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_servers(&runner, vec![entry('A', "offline", Relation::Owner, false)]);
        let page = core.your_servers().await.unwrap().unwrap();
        assert_eq!(page.servers[0].card.status, ServerStatus::Offline);
    }

    #[tokio::test]
    async fn unknown_status_is_sleeping() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_servers(
            &runner,
            vec![entry('A', "napping", Relation::Followed, true)],
        );
        let page = core.your_servers().await.unwrap().unwrap();
        assert_eq!(page.servers[0].card.status, ServerStatus::Sleeping);
    }

    #[tokio::test]
    async fn follow_sends_invite_token() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        core.your_servers_follow(&invite('A')).await;
        let calls = runner.player.calls_to("follow");
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].engine,
            Url::parse("https://api.example.com").unwrap()
        );
        assert_eq!(calls[0].args, [player_token('s'), token('A')]);
    }

    #[tokio::test]
    async fn follow_does_nothing_signed_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.your_servers_follow(&invite('A')).await;
        core.your_servers_joined(&invite('A')).await;
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn follow_skips_an_invite_of_another_engine() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        // The invite's site names api.example.com; the session lives on another Engine.
        sign_in(&core, "https://other.example.org");
        core.your_servers_follow(&invite('A')).await;
        core.your_servers_joined(&invite('A')).await;
        assert!(core.your_servers_unfollow(&invite('A')).await.is_ok());
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn follow_error_is_swallowed() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        fail(&runner, ErrorCode::TooManyPlayerServers);
        core.your_servers_follow(&invite('A')).await;
        core.your_servers_joined(&invite('A')).await;
        assert_eq!(runner.player.calls_to("follow").len(), 1);
        assert_eq!(runner.player.calls_to("joined").len(), 1);
        assert!(core.perland_session().await.is_some());
    }

    #[tokio::test]
    async fn follow_after_401_forgets_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        fail(&runner, ErrorCode::PerlandSignedOut);
        core.your_servers_follow(&invite('A')).await;
        assert!(core.perland_session().await.is_none());
    }

    #[tokio::test]
    async fn an_unparsable_invite_is_skipped_by_follow_and_joined() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        core.your_servers_follow("not a link").await;
        core.your_servers_joined("not a link").await;
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unfollow_sends_invite_token() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        core.your_servers_unfollow(&invite('A')).await.unwrap();
        let calls = runner.player.calls_to("unfollow");
        assert_eq!(calls[0].args, [player_token('s'), token('A')]);
    }

    #[tokio::test]
    async fn unfollow_reports_errors() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        fail(&runner, ErrorCode::InviteSiteUnreachable);
        let err = core.your_servers_unfollow(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteSiteUnreachable);
        let err = core.your_servers_unfollow("not a link").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteLinkInvalid);
    }

    #[tokio::test]
    async fn unfollow_after_401_forgets_the_session_and_reports() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        fail(&runner, ErrorCode::PerlandSignedOut);
        let err = core.your_servers_unfollow(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_session().await.is_none());
    }

    #[tokio::test]
    async fn an_invite_whose_engine_cant_be_found_sends_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        *runner.player.site.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::InviteSiteUnreachable,
            "Couldn't reach the invite's site",
        ));
        core.your_servers_follow(&invite('A')).await;
        core.your_servers_joined(&invite('A')).await;
        let err = core.your_servers_unfollow(&invite('A')).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteSiteUnreachable);
        assert!(runner.player.calls.lock().unwrap().is_empty());
        assert!(core.perland_session().await.is_some(), "the session stays");
    }

    #[tokio::test]
    async fn unfollow_signed_out_is_ok_and_sends_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.your_servers_unfollow(&invite('A')).await.unwrap();
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn joined_sends_invite_token() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        core.your_servers_joined(&invite('A')).await;
        let calls = runner.player.calls_to("joined");
        assert_eq!(calls[0].args, [player_token('s'), token('A')]);
    }
}
