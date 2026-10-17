//! The public server list against a wiremock Engine. The mock server is both the Perland
//! site (`/config.json`) and the Engine (`/api/v1/…`). Fake tokens only.
use launcher_core::{
    instances::ServerStatus,
    invite::{InviteClient, InviteLimits, Url},
    public_list::{site_url, ListQuery, ListSort, PublicListClient, PAGE_SIZE},
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const LIST: &str = "/api/v1/public-servers";

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn site(server: &MockServer) -> Url {
    site_url(&server.uri()).unwrap()
}

fn client_with(server: &MockServer, limits: InviteLimits) -> PublicListClient {
    let http = launcher_core::http::client("test").unwrap();
    PublicListClient::new(InviteClient::new(http).with_limits(limits), site(server))
}

fn client(server: &MockServer) -> PublicListClient {
    client_with(server, InviteLimits::default())
}

async fn mount_config(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })),
        )
        .mount(server)
        .await;
}

async fn mount_list(server: &MockServer, data: Value) {
    Mock::given(method("GET"))
        .and(path(LIST))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(data)))
        .mount(server)
        .await;
}

fn minimal_page() -> Value {
    json!({ "servers": [{ "token": TOKEN, "name": "x", "status": "online" }], "total": 1 })
}

async fn requests_to(server: &MockServer, route: &str) -> Vec<wiremock::Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == route)
        .collect()
}

#[tokio::test]
async fn reads_the_engine_from_the_site_config() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_list(&server, minimal_page()).await;
    let client = client(&server);
    let first = client.page(&ListQuery::default()).await.unwrap();
    client.page(&ListQuery::default()).await.unwrap();
    assert_eq!(first.engine.as_str().trim_end_matches('/'), server.uri());
    assert_eq!(first.page.total, 1);
    assert_eq!(requests_to(&server, "/config.json").await.len(), 1);
    assert_eq!(requests_to(&server, LIST).await.len(), 2);
}

#[tokio::test]
async fn sends_only_set_filters() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_list(&server, minimal_page()).await;
    let client = client(&server);
    let query = ListQuery {
        sort: ListSort::Newest,
        modded: Some(false),
        minecraft: Some("1.21.1".into()),
        has_room: true,
        category: Some("smp".into()),
        language: Some("es".into()),
        offset: 50,
        ..ListQuery::default()
    };
    client.page(&query).await.unwrap();
    client.page(&ListQuery::default()).await.unwrap();
    let hits = requests_to(&server, LIST).await;
    assert_eq!(
        hits[0].url.query(),
        Some("sort=newest&modded=false&minecraft=1.21.1&hasRoom=true&category=smp&language=es&limit=50&offset=50")
    );
    assert_eq!(hits[1].url.query(), Some("limit=50&offset=0"));
    assert_eq!(PAGE_SIZE, 50);
}

#[tokio::test]
async fn parses_a_full_and_a_minimal_server() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_list(
        &server,
        json!({
            "servers": [
                {
                    "token": TOKEN,
                    "inviteUrl": format!("https://play.perland.online/{TOKEN}"),
                    "joinUrl": "perland://join?invite=x",
                    "name": "asalith",
                    "motd": "§6Asalith§r\nModded survival",
                    "iconVersion": "a1b2c3d4",
                    "address": { "host": "play.perland.online", "port": 25566, "display": "play.perland.online:25566" },
                    "game": { "type": "fabric", "version": "1.21.1", "loader": "fabric", "loaderVersion": "0.16.5" },
                    "modded": true,
                    "status": "online",
                    "players": { "online": 7, "max": 20 },
                    "whitelisted": false,
                    "pack": { "name": "Asalith Fields", "version": "1.0.4", "minecraft": "1.21.1", "loader": "fabric", "loaderVersion": "0.16.5", "modCount": 142, "state": "ready", "zipState": "ready", "commit": "3f9a" },
                    "categories": ["smp", "modded-tech"],
                    "language": "es",
                    "region": "sa-east",
                    "listedAt": "2026-10-16T20:00:00Z"
                },
                { "token": TOKEN, "name": "x", "status": "online" }
            ],
            "total": 2,
            "facets": {
                "categories": ["survival", "smp"],
                "languages": ["en", "es"],
                "minecraftVersions": ["1.21.1", "1.20.1"],
                "regions": ["sa-east"]
            }
        }),
    )
    .await;
    let page = client(&server)
        .page(&ListQuery::default())
        .await
        .unwrap()
        .page;
    assert_eq!(page.total, 2);
    assert_eq!(page.facets.minecraft_versions, ["1.21.1", "1.20.1"]);
    assert_eq!(page.facets.regions, ["sa-east"]);
    let full = &page.servers[0];
    assert_eq!(full.name, "asalith");
    assert_eq!(full.address.port, 25566);
    assert_eq!(full.game.loader.as_deref(), Some("fabric"));
    assert_eq!(full.players.as_ref().unwrap().max, Some(20));
    assert_eq!(full.pack.as_ref().unwrap().mod_count, 142);
    assert_eq!(full.categories, ["smp", "modded-tech"]);
    assert_eq!(full.region.as_deref(), Some("sa-east"));
    assert_eq!(full.icon_version.as_deref(), Some("a1b2c3d4"));
    assert!(full.modded);
    assert_eq!(full.status(), ServerStatus::Online);
    let minimal = &page.servers[1];
    assert_eq!(minimal.name, "x");
    assert!(minimal.pack.is_none() && minimal.players.is_none() && minimal.invite_url.is_none());
    assert!(minimal.categories.is_empty());
    assert!(page.facets.categories == ["survival", "smp"]);
    // Only the name of a server is ever printed.
    assert!(!format!("{full:?}").contains(TOKEN));
}

#[tokio::test]
async fn unknown_status_reads_as_sleeping() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_list(
        &server,
        json!({ "servers": [
            { "token": TOKEN, "name": "a", "status": "waking" },
            { "token": TOKEN, "name": "b", "status": "starting" },
            { "token": TOKEN, "name": "c" }
        ], "total": 3 }),
    )
    .await;
    let page = client(&server)
        .page(&ListQuery::default())
        .await
        .unwrap()
        .page;
    assert_eq!(page.servers[0].status(), ServerStatus::Sleeping);
    assert_eq!(page.servers[1].status(), ServerStatus::Starting);
    assert_eq!(page.servers[2].status(), ServerStatus::Sleeping);
}

async fn list_answers(status: u16) -> (MockServer, launcher_core::LauncherError) {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(LIST))
        .respond_with(ResponseTemplate::new(status).set_body_string("not found"))
        .mount(&server)
        .await;
    let err = client(&server)
        .page(&ListQuery::default())
        .await
        .map(|_| ())
        .unwrap_err();
    (server, err)
}

#[tokio::test]
async fn closed_list_is_server_list_closed() {
    assert_eq!(list_answers(404).await.1.code, ErrorCode::ServerListClosed);
    assert_eq!(list_answers(503).await.1.code, ErrorCode::ServerListClosed);

    // A site config that answers but names no Engine.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount(&server)
        .await;
    let err = client(&server)
        .page(&ListQuery::default())
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerListClosed);
}

#[tokio::test]
async fn an_unreachable_site_stays_offline() {
    // A port nobody listens on.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let http = launcher_core::http::client("test").unwrap();
    let client = PublicListClient::new(
        InviteClient::new(http),
        site_url(&format!("http://127.0.0.1:{port}")).unwrap(),
    );
    let err = client
        .page(&ListQuery::default())
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
}

#[tokio::test]
async fn rate_limit_and_trouble() {
    assert_eq!(list_answers(429).await.1.code, ErrorCode::InviteRateLimited);
    assert_eq!(list_answers(500).await.1.code, ErrorCode::HostTrouble);
    assert_eq!(list_answers(502).await.1.code, ErrorCode::HostTrouble);
    assert_eq!(list_answers(400).await.1.code, ErrorCode::MetaUnexpected);

    // A body over the cap, and one that isn't a list.
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(LIST))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b' '; 600 << 10]))
        .mount(&server)
        .await;
    let err = client(&server)
        .page(&ListQuery::default())
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);

    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(LIST))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
        .mount(&server)
        .await;
    let err = client(&server)
        .page(&ListQuery::default())
        .await
        .map(|_| ())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
}

#[tokio::test]
async fn no_error_names_a_token() {
    // A token inside the body must not come out through any error.
    let mut errors = Vec::new();
    for status in [400, 404, 429, 500, 503] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(LIST))
            .respond_with(ResponseTemplate::new(status).set_body_string(TOKEN))
            .mount(&server)
            .await;
        errors.push(
            client(&server)
                .page(&ListQuery::default())
                .await
                .map(|_| ())
                .unwrap_err(),
        );
    }
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(LIST))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("{{\"servers\":\"{TOKEN}\"")),
        )
        .mount(&server)
        .await;
    errors.push(
        client(&server)
            .page(&ListQuery::default())
            .await
            .map(|_| ())
            .unwrap_err(),
    );
    for e in errors {
        assert!(!format!("{e:?}").contains(TOKEN), "{e:?}");
        assert!(!serde_json::to_string(&e).unwrap().contains(TOKEN));
    }
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n0000";

#[tokio::test]
async fn icon_checks_png_and_caches() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let icon_path = format!("/api/v1/invites/{TOKEN}/icon");
    Mock::given(method("GET"))
        .and(path(icon_path.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG))
        .mount(&server)
        .await;
    let cached = client(&server);
    assert_eq!(cached.icon(TOKEN, "v1").await.as_deref(), Some(PNG));
    assert_eq!(cached.icon(TOKEN, "v1").await.as_deref(), Some(PNG));
    assert_eq!(requests_to(&server, &icon_path).await.len(), 1);
    // A new icon version is another entry.
    assert!(cached.icon(TOKEN, "v2").await.is_some());
    assert_eq!(requests_to(&server, &icon_path).await.len(), 2);
    // A token that can't be one asks nothing.
    assert_eq!(cached.icon("short", "v1").await, None);

    let missing = MockServer::start().await;
    mount_config(&missing).await;
    Mock::given(method("GET"))
        .and(path(icon_path.as_str()))
        .respond_with(ResponseTemplate::new(404))
        .mount(&missing)
        .await;
    assert_eq!(client(&missing).icon(TOKEN, "v1").await, None);

    let fake = MockServer::start().await;
    mount_config(&fake).await;
    Mock::given(method("GET"))
        .and(path(icon_path.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"GIF89a".to_vec()))
        .mount(&fake)
        .await;
    assert_eq!(client(&fake).icon(TOKEN, "v1").await, None);
}

fn server_with(invite_url: Option<&str>, token: &str) -> launcher_core::public_list::PublicServer {
    serde_json::from_value(json!({ "token": token, "inviteUrl": invite_url, "name": "x" })).unwrap()
}

#[tokio::test]
async fn invite_for_prefers_invite_url() {
    let server = MockServer::start().await;
    let client = client(&server);
    let own = format!("https://play.example.com/{TOKEN}");
    let link = client.invite_for(&server_with(Some(&own), TOKEN)).unwrap();
    assert_eq!(link.canonical(), own);

    let link = client.invite_for(&server_with(None, TOKEN)).unwrap();
    assert_eq!(link.canonical(), format!("{}/{TOKEN}", server.uri()));

    assert!(client.invite_for(&server_with(None, "bad token")).is_none());
    // An unusable inviteUrl falls back to the site.
    let link = client
        .invite_for(&server_with(Some("https://play.example.com/short"), TOKEN))
        .unwrap();
    assert_eq!(link.canonical(), format!("{}/{TOKEN}", server.uri()));
}

#[test]
fn joinable_follows_the_join_rule() {
    let vanilla: launcher_core::public_list::PublicServer = serde_json::from_value(json!({
        "token": TOKEN, "name": "v", "game": { "type": "vanilla", "version": "1.21.1" }
    }))
    .unwrap();
    assert!(vanilla.joinable());
    let pack = json!({ "name": "P", "minecraft": "1.21.1", "modCount": 3, "state": "ready" });
    let with_pack: launcher_core::public_list::PublicServer = serde_json::from_value(json!({
        "token": TOKEN, "name": "f", "modded": true,
        "game": { "type": "fabric", "version": "1.21.1", "loader": "fabric" }, "pack": pack
    }))
    .unwrap();
    assert!(with_pack.joinable());
    let without: launcher_core::public_list::PublicServer = serde_json::from_value(json!({
        "token": TOKEN, "name": "f", "modded": true,
        "game": { "type": "fabric", "version": "1.21.1", "loader": "fabric" }
    }))
    .unwrap();
    assert!(!without.joinable());
}

#[test]
fn site_urls_are_checked_and_reduced_to_their_origin() {
    assert_eq!(
        site_url("https://play.perland.online/some/path/")
            .unwrap()
            .as_str(),
        "https://play.perland.online/"
    );
    assert!(site_url("http://localhost:8080").is_some());
    assert!(site_url("http://example.com").is_none());
    assert!(site_url("https://u:p@example.com").is_none());
    assert!(site_url("https://example.com/?a=b").is_none());
    assert!(site_url("https://example.com/#x").is_none());
    assert!(site_url("not a url").is_none());
}
