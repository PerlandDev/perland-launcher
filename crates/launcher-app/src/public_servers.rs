//! Perland's public server list (public-server-list spec §6.2): one page of cards, and a card's
//! icon. Nothing is written. A card's `token` is its invite's credential, so it only ever travels
//! to the window, never to a log.
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    instances::{ServerAddress, ServerStatus},
    invite::non_empty,
    public_list::{ListQuery, PublicServer},
    Result,
};

use crate::{
    app::AppCore,
    dto::{PublicServerCard, PublicServerPage, ServerGame, ServerPack, ServerPlayers},
    events::Host,
    runner::Runner,
    servers::server_source,
};

fn owned(s: &Option<String>) -> Option<String> {
    non_empty(s).map(str::to_string)
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// One page of the list, each server as a card. `invite` is the link Play joins, and
    /// `instance` the instance that already joins it (compared by canonical invite).
    pub async fn servers_list(&self, query: ListQuery) -> Result<PublicServerPage> {
        let fetched = self.runner.public_page(query).await?;
        let joined = self.joined_invites().await?;
        let mut servers = Vec::with_capacity(fetched.page.servers.len());
        for server in &fetched.page.servers {
            let invite = self.runner.public_invite(server).map(|l| l.canonical());
            let instance = instance_of(&joined, invite.as_deref());
            let address =
                ServerAddress::from_view(&server.address, &fetched.engine).map(|a| a.display());
            servers.push(card(server, invite, address, instance, false));
        }
        Ok(PublicServerPage {
            servers,
            total: fetched.page.total,
            facets: fetched.page.facets,
        })
    }

    /// One look at the instances for a whole page: each server instance's canonical invite and
    /// its id.
    pub(crate) async fn joined_invites(&self) -> Result<Vec<(String, String)>> {
        Ok(self
            .instances
            .list()
            .await?
            .into_iter()
            .filter_map(|i| server_source(&i).map(|(saved, _)| (saved.canonical(), i.id)))
            .collect())
    }

    /// A server's icon as a `data:image/png` URL, or `None`.
    pub async fn servers_icon(&self, token: &str, icon_version: &str) -> Option<String> {
        self.runner
            .public_icon(token.to_string(), icon_version.to_string())
            .await
            .map(|png| format!("data:image/png;base64,{}", STANDARD.encode(png)))
    }
}

/// The instance among `joined` that already joins `invite`.
pub(crate) fn instance_of(joined: &[(String, String)], invite: Option<&str>) -> Option<String> {
    let invite = invite?;
    joined
        .iter()
        .find(|(saved, _)| saved == invite)
        .map(|(_, id)| id.clone())
}

/// A listed server as a card. The public list folds anything but online and starting into
/// sleeping; Your servers (`keep_offline`) keeps `offline`, since an owned or invited server
/// can be stopped.
pub(crate) fn card(
    server: &PublicServer,
    invite: Option<String>,
    address: Option<String>,
    instance: Option<String>,
    keep_offline: bool,
) -> PublicServerCard {
    let status = match ServerStatus::parse(&server.status) {
        Some(ServerStatus::Offline) if keep_offline => ServerStatus::Offline,
        _ => server.status(),
    };
    PublicServerCard {
        token: server.token.clone(),
        joinable: server.joinable() && invite.is_some(),
        invite,
        address,
        name: server.name.trim().to_string(),
        icon_version: owned(&server.icon_version),
        game: ServerGame {
            version: owned(&server.game.version),
            loader: owned(&server.game.loader),
            loader_version: owned(&server.game.loader_version),
        },
        modded: server.modded,
        status,
        players: server.players.as_ref().map(ServerPlayers::from),
        whitelisted: server.whitelisted,
        pack: server.pack.as_ref().map(|p| ServerPack {
            name: p.name.trim().to_string(),
            version: owned(&p.version),
            mod_count: p.mod_count,
            state: p.state.clone(),
        }),
        categories: server.categories.clone(),
        language: server.language.clone(),
        region: owned(&server.region),
        instance,
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::ServerStatus,
        invite::{InviteAddress, InviteGame, InvitePack, InvitePlayers, Url},
        public_list::{FetchedPage, ListPage, PublicServer},
        ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::core,
        testing::{invite, FakeRunner},
    };

    use super::*;

    fn token(c: char) -> String {
        c.to_string().repeat(43)
    }

    fn pack() -> InvitePack {
        InvitePack {
            name: "Survival Pack".into(),
            version: Some("1.0.4".into()),
            minecraft: "1.20.1".into(),
            loader: Some("fabric".into()),
            loader_version: Some("0.16.14".into()),
            mod_count: 142,
            state: "ready".into(),
            zip_state: Some("ready".into()),
            commit: Some("c1".into()),
        }
    }

    fn modded(c: char) -> PublicServer {
        PublicServer {
            token: token(c),
            name: " Survival ".into(),
            icon_version: Some("v1".into()),
            address: InviteAddress {
                host: Some("play.example.com".into()),
                port: 25566,
                display: None,
            },
            game: InviteGame {
                kind: "fabric".into(),
                version: Some("1.20.1".into()),
                loader: Some("fabric".into()),
                loader_version: Some("0.16.14".into()),
            },
            modded: true,
            status: "online".into(),
            players: Some(InvitePlayers {
                online: 3,
                max: Some(20),
            }),
            pack: Some(pack()),
            categories: vec!["survival".into()],
            language: "en".into(),
            ..PublicServer::default()
        }
    }

    fn serve_page(runner: &FakeRunner, servers: Vec<PublicServer>) {
        let total = servers.len() as u32;
        *runner.public_page.lock().unwrap() = Some(Ok(FetchedPage {
            engine: Url::parse("https://api.example.com").unwrap(),
            page: ListPage {
                servers,
                total,
                ..ListPage::default()
            },
        }));
    }

    #[tokio::test]
    async fn cards_carry_the_join_details() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve_page(&runner, vec![modded('A')]);
        let page = core.servers_list(ListQuery::default()).await.unwrap();
        assert_eq!(page.total, 1);
        let card = &page.servers[0];
        assert_eq!(card.token, token('A'));
        assert_eq!(card.invite.as_deref(), Some(invite('A').as_str()));
        assert!(card.joinable);
        assert_eq!(card.address.as_deref(), Some("play.example.com:25566"));
        assert_eq!(card.name, "Survival");
        assert_eq!(card.icon_version.as_deref(), Some("v1"));
        assert_eq!(card.game.loader.as_deref(), Some("fabric"));
        assert_eq!(card.status, ServerStatus::Online);
        assert_eq!(card.players.as_ref().unwrap().online, 3);
        assert_eq!(card.pack.as_ref().unwrap().mod_count, 142);
        assert_eq!(card.instance, None);
        assert!(core.instances_list().await.unwrap().instances.is_empty());
    }

    #[tokio::test]
    async fn the_query_reaches_the_runner() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve_page(&runner, vec![]);
        let query = ListQuery {
            modded: Some(true),
            offset: 50,
            ..ListQuery::default()
        };
        core.servers_list(query.clone()).await.unwrap();
        assert_eq!(runner.public_queries.lock().unwrap().as_slice(), [query]);
    }

    #[tokio::test]
    async fn a_server_already_joined_names_its_instance() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        crate::testing::serve(&runner, &invite('A'), Ok(crate::testing::server_view("s")));
        let joined = core.join_add(&invite('A')).await.unwrap();
        serve_page(&runner, vec![modded('A'), modded('B')]);
        let page = core.servers_list(ListQuery::default()).await.unwrap();
        assert_eq!(
            page.servers[0].instance.as_deref(),
            Some(joined.id.as_str())
        );
        assert_eq!(page.servers[1].instance, None);
    }

    #[tokio::test]
    async fn a_modded_server_without_a_pack_is_not_joinable() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut bare = modded('A');
        bare.pack = None;
        serve_page(&runner, vec![bare]);
        let page = core.servers_list(ListQuery::default()).await.unwrap();
        let card = &page.servers[0];
        assert!(!card.joinable);
        assert!(card.modded);
        // The invite and address stay: the card shows "Join from Minecraft".
        assert!(card.invite.is_some());
        assert_eq!(card.address.as_deref(), Some("play.example.com:25566"));
    }

    #[tokio::test]
    async fn a_server_without_an_invite_link_is_not_joinable() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        serve_page(&runner, vec![modded('A')]);
        *runner.public_invites.lock().unwrap() = false;
        let page = core.servers_list(ListQuery::default()).await.unwrap();
        assert_eq!(page.servers[0].invite, None);
        assert!(!page.servers[0].joinable);
    }

    #[tokio::test]
    async fn an_address_without_a_host_uses_the_engine_host() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut server = modded('A');
        server.address.host = None;
        serve_page(&runner, vec![server]);
        let page = core.servers_list(ListQuery::default()).await.unwrap();
        assert_eq!(
            page.servers[0].address.as_deref(),
            Some("api.example.com:25566")
        );
    }

    #[tokio::test]
    async fn an_unknown_status_is_sleeping() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut server = modded('A');
        server.status = "napping".into();
        serve_page(&runner, vec![server]);
        let page = core.servers_list(ListQuery::default()).await.unwrap();
        assert_eq!(page.servers[0].status, ServerStatus::Sleeping);
    }

    #[tokio::test]
    async fn errors_pass_through() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        *runner.public_page.lock().unwrap() = Some(Err(LauncherError::pack(
            ErrorCode::ServerListClosed,
            "closed",
        )));
        let err = core.servers_list(ListQuery::default()).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ServerListClosed);
    }

    #[tokio::test]
    async fn the_icon_is_a_data_url_or_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert_eq!(core.servers_icon(&token('A'), "v1").await, None);
        *runner.icon.lock().unwrap() = Some(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec());
        let icon = core.servers_icon(&token('A'), "v1").await.unwrap();
        assert!(icon.starts_with("data:image/png;base64,iVBORw0KGgo"));
    }
}
