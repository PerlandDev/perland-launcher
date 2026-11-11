//! The invite's mod list (`GET /api/v1/invites/<token>/mods`) against a wiremock Engine: the
//! trust step counts where a server's mods come from with it. Fake tokens only.
use launcher_core::{
    invite::{InviteClient, InviteLimits, InviteLink, ModSource},
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn link(server: &MockServer) -> InviteLink {
    InviteLink::parse(&format!("{}/{TOKEN}", server.uri())).unwrap()
}

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

async fn engine(status: u16, body: Value) -> MockServer {
    let server = MockServer::builder().start().await;
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/api/v1/invites/{TOKEN}/mods")))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    server
}

fn client() -> InviteClient {
    InviteClient::new(launcher_core::http::client("test").unwrap())
}

fn listed() -> Value {
    envelope(json!({
        "commit": "3f9a",
        "enriched": true,
        "mods": [
            { "name": "Sodium", "side": "client", "folder": "mods", "source": "modrinth",
              "url": "https://modrinth.com/mod/AANobbMI" },
            { "name": "JEI", "side": "both", "folder": "mods", "source": "curseforge" },
            { "name": "Lithium", "side": "both", "folder": "mods", "source": "modrinth" },
            { "name": "Big Tool", "side": "both", "folder": "mods", "source": "url" },
            { "name": "custom.jar", "side": "both", "folder": "mods", "source": "uploaded" },
            { "name": "Server Only", "side": "server", "folder": "mods", "source": "modrinth" },
            { "name": "Odd", "side": "both", "folder": "mods", "source": "somewhere-new" }
        ]
    }))
}

#[tokio::test]
async fn the_mod_list_reads_each_source_and_counts_what_installs_here() {
    let server = engine(200, listed()).await;
    let mods = client().mods(&link(&server)).await.unwrap();
    assert_eq!(mods.commit, "3f9a");
    assert_eq!(mods.mods.len(), 7);
    assert_eq!(mods.mods[0].source, ModSource::Modrinth);
    assert_eq!(mods.mods[4].source, ModSource::Uploaded);
    assert_eq!(mods.mods[6].source, ModSource::Other);
    let counts = mods.counts();
    // The server-only mod never reaches the player's computer.
    assert_eq!(counts.total, 6);
    assert_eq!(counts.modrinth, 2);
    assert_eq!(counts.curseforge, 1);
    assert_eq!(counts.url, 1);
    assert_eq!(counts.uploaded, 1);
    assert_eq!(counts.other, 1);
    assert_eq!(mods.uploaded_names(), vec!["custom.jar".to_string()]);
}

#[tokio::test]
async fn mod_list_failures_name_the_host_never_the_token() {
    for (status, code) in [
        (404, ErrorCode::InviteGone),
        (429, ErrorCode::InviteRateLimited),
        (500, ErrorCode::HostTrouble),
    ] {
        let server = engine(status, json!({ "success": false, "data": null })).await;
        let e = client().mods(&link(&server)).await.unwrap_err();
        assert_eq!(e.code, code, "{status}");
        let shown = format!("{e:?}");
        assert!(!shown.contains(TOKEN), "{shown}");
    }
}

#[tokio::test]
async fn a_list_that_is_too_large_or_not_json_is_unexpected() {
    let server = engine(200, listed()).await;
    let small = client().with_limits(InviteLimits {
        mods: 64,
        ..InviteLimits::default()
    });
    let e = small.mods(&link(&server)).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::MetaUnexpected);

    let server = engine(200, json!("<html>")).await;
    let e = client().mods(&link(&server)).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::MetaUnexpected);
}
