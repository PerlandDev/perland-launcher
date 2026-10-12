//! Server invites against a wiremock Engine. The mock server is both the invite site
//! (`/config.json`) and the Engine (`/api/v1/invites/…`). Fake tokens only.
mod common;

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use common::{hits, sources, write_zip};
use launcher_core::{
    events::{noop_sink, Event, EventSink},
    instances::SourceRef,
    invite::{InviteClient, InviteLimits, InviteLink, InviteTiming},
    meta::loaders::LoaderKind,
    sources::{perland, DesiredState, PackInput, UpdateCheck},
    ErrorCategory, ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{header, method, path},
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

/// A tiny `.mrpack` for Fabric 1.20.1 with one override file.
fn mrpack(dir: &std::path::Path, version: &str) -> Vec<u8> {
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": version, "name": "Survival Pack (index)",
        "files": [], "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.16.14" }
    }))
    .unwrap();
    let path = dir.join(format!("{version}.mrpack"));
    write_zip(
        &path,
        &[
            ("modrinth.index.json", &index[..]),
            ("overrides/config/server.txt", &b"motd=hi"[..]),
        ],
    );
    std::fs::read(path).unwrap()
}

async fn mount_pack(server: &MockServer, bytes: Vec<u8>, etag: &str) {
    for verb in ["GET", "HEAD"] {
        Mock::given(method(verb))
            .and(path(route("/pack")))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("etag", format!("\"{etag}\"").as_str())
                    .set_body_bytes(bytes.clone()),
            )
            .mount(server)
            .await;
    }
}

fn collect() -> (EventSink, Arc<Mutex<Vec<Event>>>) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    (Arc::new(move |e| s2.lock().unwrap().push(e)), seen)
}

fn source(server: &MockServer) -> SourceRef {
    SourceRef::Perland {
        invite: invite_url(server),
        engine: server.uri(),
    }
}

fn installed(stamp: &str) -> DesiredState {
    DesiredState {
        name: "survival".into(),
        pack_version: "1.0.3".into(),
        minecraft: "1.20.1".into(),
        loader: None,
        files: vec![],
        stamp: stamp.into(),
        about: Default::default(),
        archive: None,
    }
}

#[tokio::test]
async fn adding_reads_only_the_view_and_resolving_downloads_the_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;
    let src = sources(&server, tmp.path());

    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (source, shell) = src.open(&input).await.unwrap();
    assert_eq!(
        source,
        SourceRef::Perland {
            invite: invite_url(&server),
            engine: format!("{}/", server.uri()),
        }
    );
    assert_eq!(shell.name, "survival");
    assert_eq!(shell.minecraft, "1.20.1");
    assert_eq!(shell.loader.as_ref().unwrap().kind, LoaderKind::Fabric);
    assert!(shell.files.is_empty());
    assert_eq!(
        hits(&server, &route("/pack")).await,
        0,
        "Add downloads nothing"
    );

    let desired = src.resolve(&source).await.unwrap();
    assert_eq!(desired.stamp, "c1");
    assert_eq!(desired.name, "survival");
    assert_eq!(desired.pack_version, "1.0.4");
    assert_eq!(desired.loader.as_ref().unwrap().version, "0.16.14");
    let paths: Vec<_> = desired.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["config/server.txt"]);
    assert!(desired.archive.is_some());
    assert_eq!(
        desired.about.policy,
        Some(launcher_core::instances::UpdatePolicy::Auto)
    );
    assert_eq!(desired.about.description.as_deref(), Some("Be nice."));
    assert!(desired
        .about
        .art
        .icon
        .as_deref()
        .unwrap()
        .ends_with(&route("/icon")));
}

#[tokio::test]
async fn without_a_commit_the_stamp_is_the_pack_etag() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(None, "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "e7a9").await;
    let desired = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap();
    assert_eq!(desired.stamp, "e7a9");
}

#[tokio::test]
async fn the_installed_stamp_is_the_pack_etag_even_with_a_commit() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "e2").await;
    let desired = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap();
    assert_eq!(desired.stamp, "e2");
}

#[tokio::test]
async fn without_an_etag_the_installed_stamp_is_the_commit() {
    // HEAD answers without an ETag, or fails: the commit names what was downloaded.
    for head in [
        ResponseTemplate::new(200),
        ResponseTemplate::new(503).set_body_json(failure(None)),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_view(&server, modded_view(Some("c1"), "ready")).await;
        Mock::given(method("HEAD"))
            .and(path(route("/pack")))
            .respond_with(head)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(route("/pack")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(mrpack(tmp.path(), "1.0.4")))
            .mount(&server)
            .await;
        let desired = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
            .await
            .unwrap();
        assert_eq!(desired.stamp, "c1");
    }
}

#[tokio::test]
async fn a_vanilla_server_resolves_to_vanilla_and_follows_its_version() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("1.21.1")).await;
    let desired = sources(&server, tmp.path())
        .resolve(&source(&server))
        .await
        .unwrap();
    assert_eq!(desired.stamp, "vanilla:1.21.1");
    assert_eq!(
        (desired.minecraft.as_str(), desired.pack_version.as_str()),
        ("1.21.1", "1.21.1")
    );
    assert!(desired.loader.is_none() && desired.files.is_empty() && desired.archive.is_none());

    // Same version: up to date. The server moves to 1.21.4: an update (fresh sources, so the
    // 15 s shared view doesn't hide the change).
    let src = sources(&server, tmp.path());
    assert_eq!(
        src.check(&source(&server), &desired).await,
        UpdateCheck::UpToDate
    );
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("1.21.4")).await;
    match sources(&server, tmp.path())
        .check(&source(&server), &desired)
        .await
    {
        UpdateCheck::Available { stamp, version, .. } => {
            assert_eq!(
                (stamp.as_str(), version.as_str()),
                ("vanilla:1.21.4", "1.21.4")
            )
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_server_that_drops_its_modpack_offers_vanilla() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("1.20.1")).await;
    match sources(&server, tmp.path())
        .check(&source(&server), &installed("c1"))
        .await
    {
        UpdateCheck::Available { stamp, .. } => assert_eq!(stamp, "vanilla:1.20.1"),
        other => panic!("{other:?}"),
    }
    // And back: a vanilla instance whose server gained a modpack.
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c2"), "ready")).await;
    match sources(&server, tmp.path())
        .check(&source(&server), &installed("vanilla:1.20.1"))
        .await
    {
        UpdateCheck::Available {
            stamp, source: s, ..
        } => {
            assert_eq!(stamp, "c2");
            assert!(matches!(s, SourceRef::Perland { .. }));
        }
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_update_check_compares_commits() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let src = sources(&server, tmp.path());
    assert_eq!(
        src.check(&source(&server), &installed("c1")).await,
        UpdateCheck::UpToDate
    );
    match src.check(&source(&server), &installed("c0")).await {
        UpdateCheck::Available { stamp, version, .. } => {
            assert_eq!((stamp.as_str(), version.as_str()), ("c1", "1.0.4"))
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        hits(&server, &route("/pack")).await,
        0,
        "the commit answers it; no HEAD"
    );
}

#[tokio::test]
async fn without_a_commit_the_check_asks_head_pack() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(None, "ready")).await;
    Mock::given(method("HEAD"))
        .and(path(route("/pack")))
        .and(header("if-none-match", "\"c1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;
    Mock::given(method("HEAD"))
        .and(path(route("/pack")))
        .respond_with(ResponseTemplate::new(200).insert_header("etag", "\"c2\""))
        .mount(&server)
        .await;
    let src = sources(&server, tmp.path());
    assert_eq!(
        src.check(&source(&server), &installed("c1")).await,
        UpdateCheck::UpToDate
    );
    match src.check(&source(&server), &installed("c0")).await {
        UpdateCheck::Available { stamp, .. } => assert_eq!(stamp, "c2"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn the_check_never_fails_and_a_dead_link_stays_recognizable() {
    for (status, code) in [
        (404u16, ErrorCode::InviteGone),
        (500, ErrorCode::UpdateCheckFailed),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        mount_config(&server).await;
        Mock::given(method("GET"))
            .and(path(route("")))
            .respond_with(ResponseTemplate::new(status).set_body_json(failure(None)))
            .mount(&server)
            .await;
        match sources(&server, tmp.path())
            .check(&source(&server), &installed("c1"))
            .await
        {
            UpdateCheck::Unavailable(e) => assert_eq!(e.code, code, "{status}"),
            other => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn pack_failures_map_to_codes_with_the_engine_reason() {
    for (status, engine_code, want) in [
        (
            502u16,
            Some("modrinth_unavailable"),
            ErrorCode::ServerPackUnavailable,
        ),
        (
            413,
            Some("export_too_large"),
            ErrorCode::ServerPackUnavailable,
        ),
        (
            400,
            Some("invalid_modpack"),
            ErrorCode::ServerPackUnavailable,
        ),
        (404, None, ErrorCode::InviteGone),
        (429, None, ErrorCode::InviteRateLimited),
    ] {
        let tmp = tempfile::tempdir().unwrap();
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_view(&server, modded_view(Some("c1"), "ready")).await;
        Mock::given(method("GET"))
            .and(path(route("/pack")))
            .respond_with(ResponseTemplate::new(status).set_body_json(failure(engine_code)))
            .mount(&server)
            .await;
        let err = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
            .await
            .unwrap_err();
        assert_eq!(err.code, want, "{status}");
        if let Some(c) = engine_code {
            assert_eq!(err.detail.as_deref(), Some(c), "{status}");
        }
    }
}

#[tokio::test]
async fn an_unavailable_pack_is_refused_before_downloading() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "unavailable")).await;
    let err = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerPackUnavailable);
    assert_eq!(hits(&server, &route("/pack")).await, 0);
}

#[tokio::test]
async fn networks_are_refused_with_their_address() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, network_view()).await;
    let src = sources(&server, tmp.path());
    let err = perland::open(&src, &link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteUnsupported);
    assert_eq!(err.params["address"], "play.example.com:25577");
    let err = perland::resolve(&src, &link(&server), &noop_sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteUnsupported);
}

#[tokio::test]
async fn a_modded_stack_without_a_pack_is_refused_with_its_address() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(
        &server,
        json!({
            "kind": "stack", "name": "modded", "motd": "Mods",
            "address": { "host": "play.example.com", "port": 25566, "display": "play.example.com:25566" },
            "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric" },
            "status": "online", "whitelisted": false, "pack": null,
            "team": { "displayName": "", "accent": "" }
        }),
    )
    .await;
    let src = sources(&server, tmp.path());
    let err = perland::open(&src, &link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteUnsupported);
    assert_eq!(err.params["address"], "play.example.com:25566");
    let err = perland::resolve(&src, &link(&server), &noop_sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteUnsupported);
    assert_eq!(err.params["address"], "play.example.com:25566");
    // An installed instance still plays what it has.
    match src.check(&source(&server), &installed("c1")).await {
        UpdateCheck::Unavailable(e) => assert_eq!(e.code, ErrorCode::UpdateCheckFailed),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_pending_pack_is_polled_until_ready() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope(modded_view(Some("c1"), "pending"))),
        )
        .with_priority(1)
        .up_to_n_times(2)
        .mount(&server)
        .await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;
    let (sink, seen) = collect();
    let desired = perland::resolve(&sources(&server, tmp.path()), &link(&server), &sink)
        .await
        .unwrap();
    assert_eq!(desired.stamp, "c1");
    assert!(hits(&server, &route("")).await >= 3);
    let waits = seen
        .lock()
        .unwrap()
        .iter()
        .filter(|e| matches!(e, Event::Progress { task, .. } if task.key == "server_pack_wait"))
        .count();
    assert!(
        waits >= 1,
        "the run can say the server is preparing the modpack"
    );
}

#[tokio::test]
async fn a_pack_still_building_after_the_limit_is_pending() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "pending")).await;
    let started = std::time::Instant::now();
    let err = perland::resolve(&sources(&server, tmp.path()), &link(&server), &noop_sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerPackPending);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the poll limit is injectable"
    );
    assert_eq!(hits(&server, &route("/pack")).await, 0);
}
