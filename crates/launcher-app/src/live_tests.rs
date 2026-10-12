//! The app on the real core (`CoreRunner`), with a wiremock invite site and Engine: what the
//! fakes can't show, because it happens inside the core's invite client and pack wait. Fake
//! tokens only.
use std::{path::Path, sync::Arc, time::Duration};

use launcher_core::{
    accounts::BuildKind,
    browse::{Browse, BrowseConfig},
    download::{cache::ObjectCache, Downloader},
    install::Installer,
    instances::{ServerStatus, SourceRef},
    invite::{InviteClient, InviteTiming},
    meta::MetaClient,
    paths::LauncherDirs,
    platform::Platform,
    sources::{SourceConfig, Sources},
    sync::Syncer,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use crate::{
    app::{AppConfig, AppCore},
    events::RunStateKind::{Idle, Syncing},
    runner::CoreRunner,
    testing::RecordingHost,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

type LiveCore = AppCore<CoreRunner, RecordingHost>;

/// A fresh launcher process on `root`: nothing in memory, only what is on disk.
fn launcher(root: &Path, timing: InviteTiming) -> (Arc<LiveCore>, Arc<RecordingHost>) {
    let dirs = LauncherDirs::new(root);
    let http = launcher_core::http::client("test").unwrap();
    let meta = MetaClient::new(http.clone(), dirs.meta());
    let downloader = Downloader::new(http.clone()).with_backoff(Duration::from_millis(1));
    let installer = Arc::new(Installer {
        dirs: dirs.clone(),
        meta: meta.clone(),
        downloader: downloader.clone(),
        platform: Platform::current(),
    });
    // The invite flows never browse, so Browse keeps its defaults and is never called.
    let browse = Browse::new(
        http.clone(),
        launcher_core::http::client_without_redirects("test").unwrap(),
        downloader.clone(),
        &dirs,
        BrowseConfig::default(),
    );
    let sources = Sources {
        invites: InviteClient::new(http.clone()).with_timing(timing),
        http,
        meta: meta.clone(),
        downloader,
        cache: ObjectCache::new(dirs.objects()),
        config: SourceConfig::default(),
    };
    let syncer = Syncer::new(installer.clone(), sources, dirs.root());
    let runner = Arc::new(CoreRunner::new(installer, syncer, "0.1.0".into(), browse));
    let host = RecordingHost::new();
    let core = AppCore::new(
        AppConfig {
            dirs,
            build: BuildKind::Dev,
            launcher_version: "0.1.0".into(),
            system_memory_mb: 16384,
            downloads_dir: None,
        },
        meta,
        runner,
        host.clone(),
    );
    (core, host)
}

/// No view sharing, so every status poll reads the Engine; a short pack poll.
fn unshared() -> InviteTiming {
    InviteTiming {
        view_ttl: Duration::ZERO,
        pack_poll_every: Duration::from_millis(10),
        pack_poll_for: Duration::from_millis(300),
        ..InviteTiming::default()
    }
}

fn route(suffix: &str) -> String {
    format!("/api/v1/invites/{TOKEN}{suffix}")
}

fn view(state: &str) -> Value {
    json!({ "success": true, "data": {
        "kind": "stack", "name": "survival", "motd": "Survival",
        "address": { "host": "play.example.com", "port": 25566, "display": "play.example.com:25566" },
        "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric", "loaderVersion": "0.16.14" },
        "status": "online", "players": { "online": 3, "max": 20 }, "whitelisted": false,
        "pack": {
            "name": "Survival Pack", "version": "1.0.4", "minecraft": "1.20.1",
            "loader": "fabric", "loaderVersion": "0.16.14", "modCount": 2,
            "state": state, "commit": "c1"
        },
        "team": { "displayName": "Acme", "accent": "violet" }
    }})
}

/// The invite site and its (live) Engine on one mock server.
async fn site(state: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(200).set_body_json(view(state)))
        .mount(&server)
        .await;
    server
}

async fn hits(server: &MockServer, at: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == at)
        .count()
}

/// A server instance whose saved Engine is `stale`, as a fresh launcher sees it.
async fn moved_instance(root: &Path, site: &MockServer, stale: &str) -> String {
    let (first, _) = launcher(root, unshared());
    let invite = format!("{}/{TOKEN}", site.uri());
    let mut instance = first.join_add(&invite).await.unwrap();
    let SourceRef::Perland { invite, .. } = instance.source.clone() else {
        panic!("a server instance");
    };
    instance.source = SourceRef::Perland {
        invite,
        engine: stale.to_string(),
    };
    first.instances.save(&instance).await.unwrap();
    instance.id
}

#[tokio::test]
async fn a_status_poll_finds_an_engine_that_moved() {
    let tmp = tempfile::tempdir().unwrap();
    let site = site("ready").await;
    // Nothing listens on port 1: the Engine the instance saved is gone.
    let id = moved_instance(tmp.path(), &site, "http://127.0.0.1:1/").await;
    let (core, _host) = launcher(tmp.path(), unshared());
    for poll in 0..2 {
        let status = core.server_status(&id).await.unwrap();
        assert_eq!(status.status, Some(ServerStatus::Online), "poll {poll}");
        assert!(!status.gone);
    }
    assert_eq!(
        hits(&site, "/config.json").await,
        2,
        "join_add asked once, the new process once"
    );
}

#[tokio::test]
async fn a_status_poll_does_not_call_a_moved_link_dead() {
    let tmp = tempfile::tempdir().unwrap();
    let site = site("ready").await;
    // The old Engine still answers, but no longer knows the invite.
    let old = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(route("")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&old)
        .await;
    let id = moved_instance(tmp.path(), &site, &format!("{}/", old.uri())).await;
    let (core, _host) = launcher(tmp.path(), unshared());
    for poll in 0..2 {
        let status = core.server_status(&id).await.unwrap();
        assert!(!status.gone, "poll {poll}: no dead-link plate");
        assert_eq!(status.status, Some(ServerStatus::Online));
    }
    let saved = core.instances.load(&id).await.unwrap();
    assert!(!saved.server.unwrap().gone);
    assert_eq!(hits(&old, &route("")).await, 1, "asked once, then trusted");
}

#[tokio::test]
async fn stop_while_the_server_prepares_the_modpack_ends_the_run_at_once() {
    let tmp = tempfile::tempdir().unwrap();
    let site = site("pending").await;
    // The real poll and limit: only a wait that hears Stop ends within the test.
    let (core, host) = launcher(
        tmp.path(),
        InviteTiming {
            pack_poll_every: Duration::from_secs(5),
            pack_poll_for: Duration::from_secs(180),
            ..InviteTiming::default()
        },
    );
    core.account_add_offline("Steve").await.unwrap();
    let id = core
        .join_add(&format!("{}/{TOKEN}", site.uri()))
        .await
        .unwrap()
        .id;
    core.play(&id).await.unwrap();
    host.wait_for(&id, Syncing).await;
    // "Server is preparing the modpack…" is on screen.
    while host.progress_count(&id) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    core.stop(&id).unwrap();
    tokio::time::timeout(Duration::from_secs(2), host.wait_for(&id, Idle))
        .await
        .expect("the run ends at once, not after the 3 minute wait");
    assert!(host.last_exit(&id).unwrap().cancelled);
    assert_eq!(hits(&site, &route("/pack")).await, 0);
    assert_eq!(
        core.install_slot.available_permits(),
        1,
        "the install slot is free"
    );
}
