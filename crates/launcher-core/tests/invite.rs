#![allow(dead_code)]
//! Server invites against a wiremock Engine. The mock server is both the invite site
//! (`/config.json`) and the Engine (`/api/v1/invites/…`). Fake tokens only.
mod common;

use std::time::Duration;

use common::hits;
use launcher_core::{
    invite::{InviteClient, InviteLimits, InviteLink, InviteTiming},
    ErrorCategory, ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn invite_url(server: &MockServer) -> String {
    format!("{}/{TOKEN}", server.uri())
}

fn link(server: &MockServer) -> InviteLink {
    InviteLink::parse(&invite_url(server)).unwrap()
}

fn route(suffix: &str) -> String {
    format!("/api/v1/invites/{TOKEN}{suffix}")
}

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn failure(code: Option<&str>) -> Value {
    json!({
        "success": false,
        "data": code.map(|c| json!({ "code": c })),
        "error": "failed",
        "meta": { "requestId": "r" }
    })
}

fn modded_view(commit: Option<&str>, state: &str) -> Value {
    json!({
        "kind": "stack", "name": "survival", "motd": "§6Survival§r\nwith friends",
        "address": { "host": "play.example.com", "port": 25566, "display": "play.example.com:25566" },
        "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric", "loaderVersion": "0.16.14" },
        "status": "online", "players": { "online": 3, "max": 20 }, "whitelisted": false,
        "note": "Be nice.", "language": "en",
        "pack": {
            "name": "Survival Pack", "version": "1.0.4", "minecraft": "1.20.1",
            "loader": "fabric", "loaderVersion": "0.16.14", "modCount": 2,
            "state": state, "zipState": "ready", "commit": commit
        },
        "team": { "displayName": "Acme", "accent": "violet" },
        "launcher": { "downloadUrl": "https://example.com/launcher" }
    })
}

fn vanilla_view(version: &str) -> Value {
    json!({
        "kind": "stack", "name": "creative", "motd": "Build",
        "address": { "host": "play.example.com", "port": 25570, "display": "play.example.com:25570" },
        "game": { "type": "vanilla", "version": version },
        "status": "sleeping", "whitelisted": true, "language": "en",
        "pack": null, "team": { "displayName": "", "accent": "" }
    })
}

fn network_view() -> Value {
    json!({
        "kind": "network", "name": "hub", "motd": "Hub",
        "address": { "host": "play.example.com", "port": 25577, "display": "play.example.com:25577" },
        "game": { "type": "velocity" }, "status": "online", "players": { "online": 9 },
        "whitelisted": false, "language": "en", "pack": null,
        "team": { "displayName": "Acme", "accent": "cyan" }
    })
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

async fn mount_view(server: &MockServer, view: Value) {
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(view)))
        .mount(server)
        .await;
}

fn fast() -> InviteTiming {
    InviteTiming {
        view_ttl: Duration::from_secs(15),
        pack_poll_every: Duration::from_millis(10),
        pack_poll_for: Duration::from_millis(300),
    }
}

fn client() -> InviteClient {
    InviteClient::new(launcher_core::http::client("test").unwrap()).with_timing(fast())
}

#[tokio::test]
async fn config_json_finds_the_engine_and_the_view_maps() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c0ffee"), "ready")).await;
    let client = client();
    let ev = client.view(&link(&server)).await.unwrap();
    assert_eq!(ev.engine.as_str().trim_end_matches('/'), server.uri());
    let view = &ev.view;
    assert_eq!(view.name, "survival");
    assert_eq!(view.address.port, 25566);
    assert_eq!(view.players.as_ref().unwrap().max, Some(20));
    let pack = view.pack.as_ref().unwrap();
    assert_eq!(pack.commit.as_deref(), Some("c0ffee"));
    assert_eq!(pack.mod_count, 2);
    assert_eq!(view.team.accent, "violet");
    assert_eq!(
        view.launcher
            .as_ref()
            .and_then(|l| l.download_url.as_deref()),
        Some("https://example.com/launcher")
    );
    assert_eq!(
        view.status(),
        Some(launcher_core::instances::ServerStatus::Online)
    );
    client.view(&link(&server)).await.unwrap();
    assert_eq!(
        hits(&server, "/config.json").await,
        1,
        "the Engine is cached per site"
    );
    assert_eq!(
        hits(&server, &route("")).await,
        1,
        "the view is shared for 15 s"
    );
}

#[tokio::test]
async fn a_site_without_a_usable_config_is_unreachable() {
    let bodies: [Option<&str>; 5] = [
        None,
        Some("not json"),
        Some(r#"{"engineUrl":"http://engine.example.com"}"#),
        Some(r#"{"engineUrl":"https://user:pw@engine.example.com"}"#),
        Some("{}"),
    ];
    for body in bodies {
        let server = MockServer::start().await;
        if let Some(body) = body {
            Mock::given(method("GET"))
                .and(path("/config.json"))
                .respond_with(ResponseTemplate::new(200).set_body_string(body))
                .mount(&server)
                .await;
        }
        let err = client().engine_for(&link(&server)).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InviteSiteUnreachable, "{body:?}");
        assert_eq!(err.category, ErrorCategory::Network);
        assert_eq!(err.params["host"], "127.0.0.1");
    }
    // Too large for the cap (a tiny cap keeps the test input tiny).
    let server = MockServer::start().await;
    mount_config(&server).await;
    let small = client().with_limits(InviteLimits {
        config: 16,
        ..InviteLimits::default()
    });
    assert_eq!(
        small.engine_for(&link(&server)).await.unwrap_err().code,
        ErrorCode::InviteSiteUnreachable
    );
}

#[tokio::test]
async fn view_answers_map_to_codes_that_name_the_host() {
    for (status, code) in [
        (404u16, ErrorCode::InviteGone),
        (429, ErrorCode::InviteRateLimited),
        (500, ErrorCode::HostTrouble),
        (503, ErrorCode::HostTrouble),
    ] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(route("")))
            .respond_with(ResponseTemplate::new(status).set_body_json(failure(None)))
            .mount(&server)
            .await;
        let err = client().view(&link(&server)).await.unwrap_err();
        assert_eq!(err.code, code, "{status}");
        assert_eq!(err.params["host"], "127.0.0.1");
    }
}

#[tokio::test]
async fn the_view_is_shared_for_its_ttl_and_fresh_reads_skip_it() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = client().with_timing(InviteTiming {
        view_ttl: Duration::from_millis(50),
        ..fast()
    });
    let l = link(&server);
    client.view(&l).await.unwrap();
    client.view(&l).await.unwrap();
    assert_eq!(hits(&server, &route("")).await, 1);
    tokio::time::sleep(Duration::from_millis(80)).await;
    client.view(&l).await.unwrap();
    assert_eq!(hits(&server, &route("")).await, 2);
    client.view_fresh(&l).await.unwrap();
    assert_eq!(hits(&server, &route("")).await, 3);
}

#[tokio::test]
async fn callers_at_the_same_moment_share_one_request() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = client();
    let other = client.clone();
    let (l1, l2) = (link(&server), link(&server));
    let (a, b) = tokio::join!(client.view(&l1), other.view(&l2));
    assert_eq!(a.unwrap().view, b.unwrap().view);
    assert_eq!(hits(&server, &route("")).await, 1);
}

#[tokio::test]
async fn the_icon_is_a_small_png_or_nothing() {
    let png = b"\x89PNG\r\n\x1a\nabcd".to_vec();
    let small = |c: InviteClient| {
        c.with_limits(InviteLimits {
            icon: 16,
            ..InviteLimits::default()
        })
    };
    for (status, body, want) in [
        (200u16, png.clone(), Some(png.clone())),
        (200, b"<html>no</html>".to_vec(), None),
        (200, [png.clone(), b"0123456789".to_vec()].concat(), None),
        (404, Vec::new(), None),
    ] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(route("/icon")))
            .respond_with(ResponseTemplate::new(status).set_body_bytes(body))
            .mount(&server)
            .await;
        assert_eq!(small(client()).icon(&link(&server)).await, want, "{status}");
    }
}

#[tokio::test]
async fn a_view_with_odd_values_still_parses() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let motd = "§".repeat(2000);
    mount_view(
        &server,
        json!({
            "kind": "stack", "name": "odd", "motd": motd, "status": "exploding",
            "address": { "host": "-bad host", "port": 70000 },
            "game": { "type": "vanilla", "version": "1.21.1" },
            "team": { "accent": "red" }, "somethingNew": { "a": 1 }
        }),
    )
    .await;
    let view = client().view(&link(&server)).await.unwrap().view;
    assert_eq!(view.status(), None);
    assert_eq!(view.address.port, 70000);
    assert_eq!(view.team.accent, "red");
    assert!(view.players.is_none() && view.pack.is_none() && !view.whitelisted);
    assert!(view.launcher.is_none(), "the launcher block is optional");
}

#[tokio::test]
async fn a_captive_portal_page_is_unexpected_data() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(format!("<html>Sign in to Wi-Fi to reach /{TOKEN}</html>")),
        )
        .mount(&server)
        .await;
    let err = client().view(&link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
    let text = format!("{err:?} {}", serde_json::to_string(&err).unwrap());
    assert!(!text.contains(TOKEN), "{text}");

    // A view larger than the cap is unexpected too.
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let small = client().with_limits(InviteLimits {
        view: 64,
        ..InviteLimits::default()
    });
    assert_eq!(
        small.view(&link(&server)).await.unwrap_err().code,
        ErrorCode::MetaUnexpected
    );
}

#[tokio::test]
async fn an_unreachable_engine_is_forgotten_and_found_again() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = client();
    let l = link(&server);
    // An instance saved an Engine that has since moved (nothing listens on port 1).
    client.remember_engine(&l, "http://127.0.0.1:1");
    let err = client.view(&l).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
    assert_eq!(err.params["host"], "127.0.0.1");
    assert_eq!(client.view(&l).await.unwrap().view.name, "survival");
    assert_eq!(hits(&server, "/config.json").await, 1);
}
