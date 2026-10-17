//! Server invites against a wiremock Engine. The mock server is both the invite site
//! (`/config.json`) and the Engine (`/api/v1/invites/…`). Fake tokens only.
mod common;

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use common::{hits, sources, write_zip};
use launcher_core::{
    events::{noop_sink, Event, EventSink},
    instances::{GameSpec, InstanceStore, SourceRef, UpdatePolicy},
    invite::{InviteClient, InviteLimits, InviteLink, InviteTiming},
    locks::InstanceLock,
    meta::loaders::LoaderKind,
    sources::{perland, DesiredState, PackInput, Sources, UpdateCheck},
    sync::{BoxFuture, Cancel, GameInstall, SyncOutcome, Syncer},
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
        ..InviteTiming::default()
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

/// No view sharing, so every `view` reaches the Engine.
fn unshared_client() -> InviteClient {
    client().with_timing(InviteTiming {
        view_ttl: Duration::ZERO,
        ..fast()
    })
}

#[tokio::test]
async fn a_saved_engine_that_moved_is_found_again_through_config_json() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = unshared_client();
    let l = link(&server);
    // An instance saved an Engine that has since moved (nothing listens on port 1).
    client.remember_engine(&l, "http://127.0.0.1:1");
    let ev = client.view(&l).await.unwrap();
    assert_eq!(ev.view.name, "survival");
    assert_eq!(ev.engine.as_str().trim_end_matches('/'), server.uri());
    // Every later poll seeds the stale Engine again; the one config.json named wins.
    for _ in 0..3 {
        client.remember_engine(&l, "http://127.0.0.1:1");
        assert_eq!(client.view(&l).await.unwrap().view.name, "survival");
    }
    assert_eq!(hits(&server, "/config.json").await, 1);
}

#[tokio::test]
async fn a_saved_engine_that_answers_404_is_checked_against_config_json() {
    // The old host still answers, but no longer knows the invite.
    let old = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(404).set_body_json(failure(None)))
        .mount(&old)
        .await;
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    let client = unshared_client();
    let l = link(&server);
    client.remember_engine(&l, &old.uri());
    let ev = client.view(&l).await.unwrap();
    assert_eq!(ev.view.name, "survival", "not invite_gone");
    assert_eq!(ev.engine.as_str().trim_end_matches('/'), server.uri());
    client.remember_engine(&l, &old.uri());
    client.view(&l).await.unwrap();
    assert_eq!(hits(&old, &route("")).await, 1);
    assert_eq!(hits(&server, "/config.json").await, 1);
}

#[tokio::test]
async fn a_dead_link_on_the_engine_config_json_names_stays_gone() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(404).set_body_json(failure(None)))
        .mount(&server)
        .await;
    let client = unshared_client();
    let l = link(&server);
    for _ in 0..3 {
        client.remember_engine(&l, &server.uri());
        assert_eq!(
            client.view(&l).await.unwrap_err().code,
            ErrorCode::InviteGone
        );
    }
    assert_eq!(
        hits(&server, "/config.json").await,
        1,
        "the Engine is confirmed once, then trusted"
    );
}

#[tokio::test]
async fn an_unreachable_engine_from_config_json_is_forgotten_and_found_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": "http://127.0.0.1:1" })),
        )
        .mount(&server)
        .await;
    let client = client();
    let l = link(&server);
    let err = client.view(&l).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
    assert_eq!(err.params["host"], "127.0.0.1");
    // The site now names a live Engine.
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    assert_eq!(client.view(&l).await.unwrap().view.name, "survival");
}

#[tokio::test]
async fn a_slow_site_or_engine_times_out_as_offline() {
    let quick = || {
        client().with_timing(InviteTiming {
            request_timeout: Duration::from_millis(100),
            ..fast()
        })
    };
    // The Engine never finishes its answer in time.
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(envelope(modded_view(Some("c1"), "ready")))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let err = quick().view(&link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline, "view");
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the view slot is let go"
    );
    assert_token_free("slow view", &err);
    // Nor does the invite site.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "engineUrl": server.uri() }))
                .set_delay(Duration::from_secs(3)),
        )
        .mount(&server)
        .await;
    let started = std::time::Instant::now();
    let err = quick().view(&link(&server)).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline, "config.json");
    assert!(started.elapsed() < Duration::from_secs(2));
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
    let desired = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &noop_sink(),
        &Cancel::default(),
    )
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
    let desired = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &noop_sink(),
        &Cancel::default(),
    )
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
        let desired = perland::resolve(
            &sources(&server, tmp.path()),
            &link(&server),
            &noop_sink(),
            &Cancel::default(),
        )
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
        let err = perland::resolve(
            &sources(&server, tmp.path()),
            &link(&server),
            &noop_sink(),
            &Cancel::default(),
        )
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
    let err = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &noop_sink(),
        &Cancel::default(),
    )
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
    let err = perland::resolve(&src, &link(&server), &noop_sink(), &Cancel::default())
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
    let err = perland::resolve(&src, &link(&server), &noop_sink(), &Cancel::default())
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
    let desired = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &sink,
        &Cancel::default(),
    )
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
    let err = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &noop_sink(),
        &Cancel::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerPackPending);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the poll limit is injectable"
    );
    assert_eq!(hits(&server, &route("/pack")).await, 0);
}

/// Installs nothing: these tests are about which pack lands, not about Minecraft.
struct NoInstall;

impl GameInstall for NoInstall {
    fn install_game<'a>(
        &'a self,
        _game: &'a GameSpec,
        _sink: &'a EventSink,
    ) -> BoxFuture<'a, launcher_core::Result<()>> {
        Box::pin(async { Ok(()) })
    }
}

/// The wiremock sources with no view sharing, so a redeploy is seen at once.
fn unshared(server: &MockServer, root: &Path) -> Sources {
    let base = sources(server, root);
    Sources {
        invites: InviteClient::new(base.http.clone()).with_timing(InviteTiming {
            view_ttl: Duration::ZERO,
            ..fast()
        }),
        ..base
    }
}

#[tokio::test]
async fn a_server_instance_installs_on_first_play_and_follows_a_redeploy() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    // The first view says the pack is still building; then it is ready.
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

    let syncer = Syncer::new(
        Arc::new(NoInstall),
        unshared(&server, tmp.path()),
        tmp.path(),
    );
    let store = InstanceStore::new(tmp.path().join("instances"));
    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (instance, _) = syncer.add_pack(&store, &input).await.unwrap();
    assert_eq!(instance.update_policy, UpdatePolicy::Auto);
    assert!(matches!(instance.source, SourceRef::Perland { .. }));
    assert_eq!(
        hits(&server, &route("/pack")).await,
        0,
        "Add downloads nothing"
    );

    // First Play: the never-synced branch resolves with the run's sink.
    let (sink, seen) = collect();
    let ensured = syncer.ensure(&store, &instance.id, &sink).await.unwrap();
    assert!(matches!(ensured.outcome, SyncOutcome::Done(_)));
    assert!(store
        .game_dir(&instance.id)
        .join("config/server.txt")
        .is_file());
    assert_eq!(
        syncer.installed(&store, &instance.id).await.unwrap().stamp,
        "c1"
    );
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Progress { task, .. } if task.key == "server_pack_wait")));

    // The owner redeploys: a new commit and a new pack.
    server.reset().await;
    mount_config(&server).await;
    // The view's pack version wins over the archive's, so the redeployed view says 1.0.5 too.
    let mut redeployed = modded_view(Some("c2"), "ready");
    redeployed["pack"]["version"] = json!("1.0.5");
    mount_view(&server, redeployed).await;
    let dir = tempfile::tempdir().unwrap();
    mount_pack(&server, mrpack(dir.path(), "1.0.5"), "c2").await;
    let fresh = store.load(&instance.id).await.unwrap();
    let update = syncer
        .find_update_with(&store, &fresh, &noop_sink())
        .await
        .unwrap()
        .expect("the new commit is an update");
    assert_eq!(update.desired.stamp, "c2");
    assert_eq!(update.desired.pack_version, "1.0.5");
    syncer
        .ensure(&store, &instance.id, &noop_sink())
        .await
        .unwrap();
    assert_eq!(
        syncer.installed(&store, &instance.id).await.unwrap().stamp,
        "c2"
    );
}

fn assert_token_free(label: &str, e: &launcher_core::LauncherError) {
    for text in [
        serde_json::to_string(e).unwrap(),
        format!("{e:?}"),
        e.to_string(),
    ] {
        assert!(!text.contains(TOKEN), "{label} leaks the token: {text}");
    }
}

/// A fresh server with config.json, plus whatever `mount` adds.
async fn engine_with<F, Fut>(mount: F) -> MockServer
where
    F: FnOnce(MockServer) -> Fut,
    Fut: std::future::Future<Output = MockServer>,
{
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount(server).await
}

/// `at` answers GET and HEAD with `status`, so a HEAD never hides what the GET reports.
async fn answer(server: MockServer, at: String, status: u16, body: Value) -> MockServer {
    for verb in ["GET", "HEAD"] {
        Mock::given(method(verb))
            .and(path(at.clone()))
            .respond_with(ResponseTemplate::new(status).set_body_json(body.clone()))
            .mount(&server)
            .await;
    }
    server
}

#[tokio::test]
async fn no_failure_path_leaks_the_token() {
    let tmp = tempfile::tempdir().unwrap();
    let mut errors: Vec<(String, launcher_core::LauncherError)> = Vec::new();

    // config.json
    for body in ["nope", r#"{"engineUrl":"http://engine.example.com"}"#] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/config.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let e = client().view(&link(&server)).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::InviteSiteUnreachable, "config {body}");
        errors.push((format!("config {body}"), e));
    }
    let empty = MockServer::start().await;
    let e = client().view(&link(&empty)).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::InviteSiteUnreachable, "config 404");
    errors.push(("config 404".into(), e));

    // The view.
    for (status, code) in [
        (404, ErrorCode::InviteGone),
        (429, ErrorCode::InviteRateLimited),
        (500, ErrorCode::HostTrouble),
    ] {
        let server = engine_with(|s| answer(s, route(""), status, failure(None))).await;
        let e = client().view(&link(&server)).await.unwrap_err();
        assert_eq!(e.code, code, "view {status}");
        errors.push((format!("view {status}"), e));
    }
    let offline = InviteLink::parse(&format!("http://127.0.0.1:1/{TOKEN}")).unwrap();
    let e = client().view(&offline).await.unwrap_err();
    errors.push(("offline".into(), e));

    // A page that is not the Engine's view.
    let server = engine_with(|s| async move {
        Mock::given(method("GET"))
            .and(path(route("")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(format!("<html>Sign in to reach /{TOKEN}</html>")),
            )
            .mount(&s)
            .await;
        s
    })
    .await;
    let e = client().view(&link(&server)).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::MetaUnexpected, "captive portal");
    errors.push(("captive portal".into(), e));

    // Views the launcher can't turn into an instance: a network, and a modded stack without a
    // modpack (owner ruling R1), through both `open` and `resolve`.
    let unsupported: [(&str, Value); 2] = [
        ("network", network_view()),
        (
            "modded without a pack",
            json!({
                "kind": "stack", "name": "modded", "motd": "Mods",
                "address": { "host": "play.example.com", "port": 25566, "display": "play.example.com:25566" },
                "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric" },
                "status": "online", "whitelisted": false, "pack": null,
                "team": { "displayName": "", "accent": "" }
            }),
        ),
    ];
    for (label, view) in unsupported {
        let server = engine_with(|s| async move {
            mount_view(&s, view).await;
            s
        })
        .await;
        let src = sources(&server, tmp.path());
        let e = perland::open(&src, &link(&server)).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::InviteUnsupported, "open {label}");
        errors.push((format!("open {label}"), e));
        let e = perland::resolve(&src, &link(&server), &noop_sink(), &Cancel::default())
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InviteUnsupported, "resolve {label}");
        errors.push((format!("resolve {label}"), e));
    }

    // The pack, with a commit: a failed HEAD is ignored and the GET reports the real error.
    // The HEAD is answered with the same failure, so only the GET can decide the code.
    for (status, engine_code, want) in [
        (404, None, ErrorCode::InviteGone),
        (
            400,
            Some("invalid_modpack"),
            ErrorCode::ServerPackUnavailable,
        ),
        (
            413,
            Some("export_too_large"),
            ErrorCode::ServerPackUnavailable,
        ),
        (429, None, ErrorCode::InviteRateLimited),
        (
            502,
            Some("mod_download_failed"),
            ErrorCode::ServerPackUnavailable,
        ),
        (503, None, ErrorCode::HostTrouble),
    ] {
        let server = engine_with(|s| async move {
            mount_view(&s, modded_view(Some("c1"), "ready")).await;
            answer(s, route("/pack"), status, failure(engine_code)).await
        })
        .await;
        let e = perland::resolve(
            &sources(&server, tmp.path()),
            &link(&server),
            &noop_sink(),
            &Cancel::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code, want, "pack {status}");
        errors.push((format!("pack {status}"), e));
    }
    // The pack, without a commit: the HEAD is the only way to name the stamp, so it reports.
    for (status, want) in [
        (404, ErrorCode::InviteGone),
        (429, ErrorCode::InviteRateLimited),
        (502, ErrorCode::ServerPackUnavailable),
        (503, ErrorCode::HostTrouble),
    ] {
        let server = engine_with(|s| async move {
            mount_view(&s, modded_view(None, "ready")).await;
            answer(s, route("/pack"), status, failure(None)).await
        })
        .await;
        let e = perland::resolve(
            &sources(&server, tmp.path()),
            &link(&server),
            &noop_sink(),
            &Cancel::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code, want, "pack without commit {status}");
        errors.push((format!("pack without commit {status}"), e));
    }
    // A pack that is still being built, and one the Engine gave up on.
    for (state, want) in [
        ("pending", ErrorCode::ServerPackPending),
        ("unavailable", ErrorCode::ServerPackUnavailable),
    ] {
        let server = engine_with(|s| async move {
            mount_view(&s, modded_view(Some("c1"), state)).await;
            s
        })
        .await;
        let e = perland::resolve(
            &sources(&server, tmp.path()),
            &link(&server),
            &noop_sink(),
            &Cancel::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(e.code, want, "pack {state}");
        errors.push((format!("pack {state}"), e));
    }

    // A downloader failure `pack_download_error` passes on as it is (the fall-through): a 416
    // the download never gets past. `scrub` is the only guard on that path.
    let server = engine_with(|s| async move {
        mount_view(&s, modded_view(Some("c1"), "ready")).await;
        answer(s, route("/pack"), 416, failure(None)).await
    })
    .await;
    let e = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &noop_sink(),
        &Cancel::default(),
    )
    .await
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::DownloadRestarted, "pack 416");
    errors.push(("pack 416".into(), e));

    // A refused server whose address has no usable port and whose display text is the invite
    // link itself: the refusal quotes the display, so only `scrub` keeps the token out.
    let server = MockServer::start().await;
    let mut echo = network_view();
    echo["address"] =
        json!({ "host": "play.example.com", "port": 0, "display": invite_url(&server) });
    mount_config(&server).await;
    mount_view(&server, echo).await;
    let src = sources(&server, tmp.path());
    let e = perland::open(&src, &link(&server)).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::InviteUnsupported, "open echo");
    assert!(e.message.contains("<invite>"), "{}", e.message);
    errors.push(("open echo".into(), e));
    let e = perland::resolve(&src, &link(&server), &noop_sink(), &Cancel::default())
        .await
        .unwrap_err();
    errors.push(("resolve echo".into(), e));
    match src.check(&source(&server), &installed("c0")).await {
        UpdateCheck::Unavailable(e) => errors.push(("check echo".into(), e)),
        other => panic!("{other:?}"),
    }
    // The app calls `supported` itself (Join server, Use a new link).
    let ev = src.invites.view(&link(&server)).await.unwrap();
    let e = perland::supported(&link(&server), &ev).unwrap_err();
    errors.push(("supported echo".into(), e));

    // Checks.
    for status in [404, 500] {
        let server = engine_with(|s| answer(s, route(""), status, failure(None))).await;
        match sources(&server, tmp.path())
            .check(&source(&server), &installed("c0"))
            .await
        {
            UpdateCheck::Unavailable(e) => errors.push((format!("check {status}"), e)),
            other => panic!("{other:?}"),
        }
    }
    // A check with no commit asks HEAD /pack, which can fail too.
    for status in [404, 500] {
        let server = engine_with(|s| async move {
            mount_view(&s, modded_view(None, "ready")).await;
            answer(s, route("/pack"), status, failure(None)).await
        })
        .await;
        match sources(&server, tmp.path())
            .check(&source(&server), &installed("c0"))
            .await
        {
            UpdateCheck::Unavailable(e) => errors.push((format!("check head {status}"), e)),
            other => panic!("{other:?}"),
        }
    }

    assert!(errors.len() >= 28, "{}", errors.len());
    for (label, e) in &errors {
        assert_token_free(label, e);
    }

    // Debug of every type that holds the invite.
    let l = InviteLink::parse(&format!("https://play.example.com/{TOKEN}")).unwrap();
    let source = SourceRef::Perland {
        invite: l.canonical(),
        engine: "https://api.example.com/".into(),
    };
    for text in [
        format!("{l:?}"),
        l.to_string(),
        format!("{source:?}"),
        format!("{:?}", PackInput::Perland { link: l.clone() }),
        format!("{:?}", client()),
        format!(
            "{:?}",
            UpdateCheck::Available {
                source: source.clone(),
                stamp: "c1".into(),
                version: "1".into()
            }
        ),
    ] {
        assert!(!text.contains(TOKEN), "{text}");
    }
}

fn is_pack_wait(e: &Event) -> bool {
    matches!(e, Event::Progress { task, .. } if task.key == "server_pack_wait")
}

/// The sources with the real 5 s poll and 3 min limit: only a wait that hears Stop ends soon.
fn real_wait(server: &MockServer, root: &Path) -> Sources {
    let base = sources(server, root);
    Sources {
        invites: InviteClient::new(base.http.clone()).with_timing(InviteTiming {
            pack_poll_every: Duration::from_secs(5),
            pack_poll_for: Duration::from_secs(180),
            ..fast()
        }),
        ..base
    }
}

#[tokio::test]
async fn stop_while_the_server_prepares_the_pack_ends_the_first_play() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "pending")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;
    let syncer = Syncer::new(
        Arc::new(NoInstall),
        real_wait(&server, tmp.path()),
        tmp.path(),
    );
    let store = InstanceStore::new(tmp.path().join("instances"));
    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (instance, _) = syncer.add_pack(&store, &input).await.unwrap();
    let lock = InstanceLock::try_acquire(&store.perland_dir(&instance.id)).unwrap();
    let cancel = Cancel::default();
    let (sink, seen) = collect();
    // The run's never-synced branch, as the app calls it, with the run's Stop.
    let play = syncer.ensure_locked(&lock, &store, &instance.id, None, &sink, &cancel);
    let stop = async {
        while !seen.lock().unwrap().iter().any(is_pack_wait) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        cancel.cancel();
    };
    let (result, ()) =
        tokio::time::timeout(Duration::from_secs(3), async { tokio::join!(play, stop) })
            .await
            .expect("Stop ends the wait well within one poll interval");
    assert_eq!(result.unwrap_err().code, ErrorCode::SyncCancelled);
    assert_eq!(hits(&server, &route("/pack")).await, 0, "nothing downloads");
}

#[tokio::test]
async fn a_stopped_resolve_never_starts_the_pack_download() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "ready")).await;
    mount_pack(&server, mrpack(tmp.path(), "1.0.4"), "c1").await;
    let cancel = Cancel::default();
    cancel.cancel();
    let err = perland::resolve(
        &sources(&server, tmp.path()),
        &link(&server),
        &noop_sink(),
        &cancel,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::SyncCancelled);
    assert_eq!(hits(&server, &route("/pack")).await, 0);
}

#[tokio::test]
async fn a_stop_during_the_wait_is_heard_before_the_next_poll() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modded_view(Some("c1"), "pending")).await;
    let src = real_wait(&server, tmp.path());
    let cancel = Cancel::default();
    let (sink, seen) = collect();
    let l = link(&server);
    let resolve = perland::resolve(&src, &l, &sink, &cancel);
    let stop = async {
        while !seen.lock().unwrap().iter().any(is_pack_wait) {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        cancel.cancel();
    };
    let started = std::time::Instant::now();
    let (result, ()) = tokio::time::timeout(Duration::from_secs(3), async {
        tokio::join!(resolve, stop)
    })
    .await
    .expect("the 5 s sleep hears Stop");
    assert_eq!(result.unwrap_err().code, ErrorCode::SyncCancelled);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(hits(&server, &route("")).await, 1, "no poll after the Stop");
}

/// A modpack free server: no Engine pack, the Modrinth version it runs instead.
fn modrinth_view(version_id: &str) -> Value {
    json!({
        "kind": "stack", "name": "survival", "motd": "Mods",
        "address": { "host": "play.example.com", "port": 25566, "display": "play.example.com:25566" },
        "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric" },
        "status": "online", "whitelisted": false, "note": "Be nice.", "language": "en",
        "pack": null, "modrinth": { "projectId": "P1", "versionId": version_id },
        "team": { "displayName": "", "accent": "" }
    })
}

/// Modrinth's project P1 (a modpack) and its version `id`, whose `.mrpack` is served here.
async fn mount_modrinth(server: &MockServer, dir: &Path, id: &str) {
    let bytes = mrpack(dir, id);
    let sha1 = launcher_core::download::hash::sha1_hex(&bytes);
    Mock::given(method("GET"))
        .and(path(format!("/files/{id}.mrpack")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
        .mount(server)
        .await;
    let project = json!({
        "id": "P1", "slug": "sky", "title": "Sky Islands", "description": "Float",
        "project_type": "modpack", "icon_url": "https://cdn.example.com/icon.png", "gallery": []
    });
    let version = json!({
        "id": id, "project_id": "P1", "name": id, "version_number": format!("{id}-num"),
        "version_type": "release", "game_versions": ["1.20.1"],
        "date_published": "2026-10-01T00:00:00Z",
        "files": [{
            "url": format!("{}/files/{id}.mrpack", server.uri()), "filename": format!("{id}.mrpack"),
            "primary": true, "size": bytes.len(), "hashes": { "sha1": sha1, "sha512": null }
        }]
    });
    for (at, body) in [
        ("/v2/project/P1".to_string(), project),
        (format!("/v2/version/{id}"), version),
    ] {
        Mock::given(method("GET"))
            .and(path(at))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }
}

#[tokio::test]
async fn modrinth_stamp_matches_after_resolve() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modrinth_view("R1")).await;
    mount_modrinth(&server, tmp.path(), "R1").await;
    let src = sources(&server, tmp.path());

    // Add takes it (a modded stack with a Modrinth version is joinable), downloading nothing.
    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (source, shell) = src.open(&input).await.unwrap();
    assert!(matches!(source, SourceRef::Perland { .. }));
    assert_eq!(shell.minecraft, "1.20.1");
    assert_eq!(hits(&server, "/files/R1.mrpack").await, 0);

    let desired = src.resolve(&source).await.unwrap();
    let view: launcher_core::invite::InviteView =
        serde_json::from_value(modrinth_view("R1")).unwrap();
    assert_eq!(perland::stamp_of(&view).as_deref(), Some("modrinth:R1"));
    assert_eq!(desired.stamp, "modrinth:R1");
    // The server's name and about, as for an Engine pack; the version is Modrinth's.
    assert_eq!(desired.name, "survival");
    assert_eq!(desired.pack_version, "R1-num");
    assert_eq!(desired.about.description.as_deref(), Some("Be nice."));
    assert_eq!(desired.about.policy, Some(UpdatePolicy::Auto));
    assert!(desired
        .about
        .art
        .icon
        .as_deref()
        .unwrap()
        .ends_with(&route("/icon")));
    assert_eq!(desired.loader.as_ref().unwrap().kind, LoaderKind::Fabric);
    let paths: Vec<_> = desired.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["config/server.txt"]);
    assert_eq!(hits(&server, &route("/pack")).await, 0, "no Engine pack");

    assert_eq!(src.check(&source, &desired).await, UpdateCheck::UpToDate);
}

#[tokio::test]
async fn modrinth_version_change_is_an_update() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, modrinth_view("R2")).await;
    match sources(&server, tmp.path())
        .check(&source(&server), &installed("modrinth:R1"))
        .await
    {
        UpdateCheck::Available {
            stamp, source: s, ..
        } => {
            assert_eq!(stamp, "modrinth:R2");
            assert!(matches!(s, SourceRef::Perland { .. }));
        }
        other => panic!("{other:?}"),
    }
}

/// Mojang's version manifest, as the mock serves it: only `latest` matters here.
async fn mount_manifest(server: &MockServer, release: &str) {
    Mock::given(method("GET"))
        .and(path("/mojang/version_manifest_v2.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "latest": { "release": release, "snapshot": "26.9-snapshot-1" },
            "versions": []
        })))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_latest_server_joins_mojangs_latest_release_and_follows_it() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    // itzg's `VERSION: LATEST`, passed through by the Engine; any case.
    mount_view(&server, vanilla_view("latest")).await;
    mount_manifest(&server, "1.21.4").await;
    let src = sources(&server, tmp.path());

    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let (source, shell) = src.open(&input).await.unwrap();
    assert_eq!(shell.minecraft, "1.21.4");
    assert_eq!(shell.pack_version, "1.21.4");

    let desired = src.resolve(&source).await.unwrap();
    assert_eq!(desired.stamp, "vanilla:1.21.4");
    assert_eq!(
        (desired.minecraft.as_str(), desired.pack_version.as_str()),
        ("1.21.4", "1.21.4")
    );
    assert_eq!(src.check(&source, &desired).await, UpdateCheck::UpToDate);

    // Mojang ships 1.21.5 and itzg moves the server to it: the player moves too.
    server.reset().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("LATEST")).await;
    mount_manifest(&server, "1.21.5").await;
    match sources(&server, tmp.path()).check(&source, &desired).await {
        UpdateCheck::Available { stamp, version, .. } => assert_eq!(
            (stamp.as_str(), version.as_str()),
            ("vanilla:1.21.5", "1.21.5")
        ),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn a_latest_server_without_the_manifest_fails_as_the_manifest_does() {
    let tmp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_view(&server, vanilla_view("LATEST")).await;
    Mock::given(method("GET"))
        .and(path("/mojang/version_manifest_v2.json"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let src = sources(&server, tmp.path());
    let input = PackInput::parse(&invite_url(&server)).unwrap();
    let e = src.open(&input).await.unwrap_err();
    let meta = launcher_core::meta::MetaClient::new(
        launcher_core::http::client("test").unwrap(),
        tmp.path().join("other-meta"),
    );
    let direct = meta
        .get_json::<serde_json::Value>(&format!("{}/mojang/version_manifest_v2.json", server.uri()))
        .await
        .unwrap_err();
    assert_eq!(e.code, direct.code);
    assert_ne!(e.code, ErrorCode::MinecraftVersionNotFound);
    assert_token_free("latest manifest", &e);
}
