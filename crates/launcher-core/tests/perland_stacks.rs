//! The Engine's server routes (`/api/v1/stacks`) against a wiremock Engine. Fake tokens only.
use launcher_core::{
    invite::Url,
    perland::{Apply, Lifecycle, ListKind, SessionJwt, StackClient},
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path, query_param},
    Mock, MockServer, Request, ResponseTemplate,
};

const JWT: &str = "eyJ.a.b";
/// `X-User-Token` as the Engine reads it: the JWT after `Bearer `.
const BEARER: &str = "Bearer eyJ.a.b";

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn client(server: &MockServer) -> StackClient {
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    StackClient::new(http, Url::parse(&server.uri()).unwrap())
}

fn jwt() -> SessionJwt {
    SessionJwt::new(JWT)
}

async fn ok(server: &MockServer, verb: &str, route: &str, data: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .and(header("x-user-token", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(data)))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn get_all_lists_single_servers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stacks/get-all"))
        .and(query_param("kind", "single"))
        .and(header("x-user-token", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "servers": [{
                "stackName": "survival", "status": "running", "health": "healthy",
                "address": "play.example.com", "motd": "Hi", "version": "1.21",
                "kind": "single", "type": "paper", "ownerId": "x"
            }, { "stackName": "creative", "status": "exited" }]
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let items = client(&server).get_all(&jwt()).await.unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].stack_name, "survival");
    assert_eq!(items[0].address.as_deref(), Some("play.example.com"));
    assert_eq!(items[0].r#type, "paper");
    assert_eq!(items[1].address, None);
}

#[tokio::test]
async fn details_reads_without_stats() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stacks/details/survival"))
        .and(query_param("stats", "false"))
        .and(header("x-user-token", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "stackName": "survival", "status": "running", "version": "1.21", "type": "paper",
            "pendingRestart": true,
            "idle": { "state": "active", "paused": false, "stopsAt": "2026-10-21T10:00:00Z" }
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let d = client(&server).details(&jwt(), "survival").await.unwrap();
    assert!(d.pending_restart);
    let idle = d.idle.unwrap();
    assert_eq!(idle.state, "active");
    assert_eq!(idle.stops_at.as_deref(), Some("2026-10-21T10:00:00Z"));
}

#[tokio::test]
async fn details_read_the_minecraft_containers_health() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stacks/details/survival"))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "stackName": "survival", "status": "running",
            "containers": [
                { "containerName": "mysql_ab_cd_survival", "status": "running", "health": "healthy" },
                { "containerName": "/minecraft_ab_cd_survival", "status": "running", "health": "starting" }
            ]
        }))))
        .mount(&server)
        .await;
    let d = client(&server).details(&jwt(), "survival").await.unwrap();
    assert_eq!(d.containers.len(), 2);
    assert_eq!(d.minecraft_health(), "starting");
}

#[tokio::test]
async fn details_without_a_minecraft_container_take_the_first_health() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stacks/details/survival"))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "stackName": "survival", "status": "running",
            "containers": [
                { "containerName": "x", "status": "running" },
                "nonsense",
                { "containerName": "y", "status": "running", "health": "starting" }
            ]
        }))))
        .mount(&server)
        .await;
    let d = client(&server).details(&jwt(), "survival").await.unwrap();
    assert_eq!(
        d.minecraft_health(),
        "starting",
        "a bad item is skipped, not fatal"
    );
}

#[tokio::test]
async fn details_with_null_containers_have_no_health() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stacks/details/survival"))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "stackName": "survival", "status": "running", "containers": null
        }))))
        .mount(&server)
        .await;
    let d = client(&server).details(&jwt(), "survival").await.unwrap();
    assert!(d.containers.is_empty());
    assert_eq!(d.minecraft_health(), "");
}

#[tokio::test]
async fn stack_names_are_one_escaped_segment() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(|r: &Request| r.url.path() == "/api/v1/stacks/details/my%20world")
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(
            json!({ "stackName": "my world", "status": "exited" }),
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(|r: &Request| r.url.path() == "/api/v1/stacks/a%2Fb%3Fc/players")
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({}))))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(
        c.details(&jwt(), "my world").await.unwrap().stack_name,
        "my world"
    );
    c.online(&jwt(), "a/b?c").await.unwrap();
}

#[tokio::test]
async fn lifecycle_posts_to_each_verb() {
    let server = MockServer::start().await;
    for verb in ["start", "stop", "restart"] {
        Mock::given(method("POST"))
            .and(path(format!("/api/v1/stacks/{verb}/survival")))
            .and(header("x-user-token", BEARER))
            .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({}))))
            .expect(1)
            .mount(&server)
            .await;
    }
    let c = client(&server);
    c.lifecycle(&jwt(), "survival", Lifecycle::Start)
        .await
        .unwrap();
    c.lifecycle(&jwt(), "survival", Lifecycle::Stop)
        .await
        .unwrap();
    c.lifecycle(&jwt(), "survival", Lifecycle::Restart)
        .await
        .unwrap();
}

#[tokio::test]
async fn stop_sends_an_empty_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/stop/survival"))
        .and(body_json(json!({})))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({}))))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .lifecycle(&jwt(), "survival", Lifecycle::Stop)
        .await
        .unwrap();
}

#[tokio::test]
async fn diagnosis_reads_the_primary_issue() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        "/api/v1/stacks/survival/diagnosis",
        json!({
            "outcome": "failed", "oomKilled": true,
            "primary": {
                "code": "mod_missing_dependency", "known": true, "severity": "fatal",
                "params": { "mod": "fabric-api" },
                "subject": { "kind": "mod", "name": "Sodium", "version": "0.5" }
            }
        }),
    )
    .await;
    let d = client(&server).diagnosis(&jwt(), "survival").await.unwrap();
    assert_eq!(d.outcome, "failed");
    assert!(d.oom_killed);
    let p = d.primary.unwrap();
    assert_eq!(p.code, "mod_missing_dependency");
    assert!(p.known);
    assert_eq!(p.params["mod"], "fabric-api");
    assert_eq!(p.subject.unwrap().name, "Sodium");
}

#[tokio::test]
async fn online_reads_the_players() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        "/api/v1/stacks/survival/players",
        json!({ "running": true, "paused": false, "online": 2, "max": 20, "players": ["Ana", "Bo"] }),
    )
    .await;
    let o = client(&server).online(&jwt(), "survival").await.unwrap();
    assert!(o.running);
    assert_eq!((o.online, o.max), (2, 20));
    assert_eq!(o.players, Some(vec!["Ana".to_string(), "Bo".to_string()]));
}

#[tokio::test]
async fn kick_sends_the_names() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/survival/players/kick"))
        .and(header("x-user-token", BEARER))
        .and(body_json(json!({ "names": ["Ana"] })))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "results": [{ "name": "Ana", "status": "failed", "message": "No player" }]
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let r = client(&server)
        .kick(&jwt(), "survival", &["Ana".to_string()])
        .await
        .unwrap();
    assert_eq!(r[0].name, "Ana");
    assert_eq!(r[0].status, "failed");
    assert_eq!(r[0].message.as_deref(), Some("No player"));
}

#[tokio::test]
async fn the_whitelist_names_come_from_players() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        "/api/v1/stacks/survival/whitelist",
        json!({
            "running": true, "offlineMode": true, "unreadable": false,
            "players": [{ "name": "Ana", "uuid": "u" }, { "name": "Bo", "uuid": "v" }]
        }),
    )
    .await;
    let l = client(&server)
        .list(&jwt(), "survival", ListKind::Whitelist)
        .await
        .unwrap();
    assert!(l.running && l.offline_mode && !l.unreadable);
    assert_eq!(l.names, ["Ana", "Bo"]);
}

#[tokio::test]
async fn the_ops_names_come_from_ops() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        "/api/v1/stacks/survival/ops",
        json!({ "running": false, "unreadable": true, "ops": [{ "name": "Ana", "level": 4 }] }),
    )
    .await;
    let l = client(&server)
        .list(&jwt(), "survival", ListKind::Ops)
        .await
        .unwrap();
    assert!(!l.running && l.unreadable);
    assert_eq!(l.names, ["Ana"]);
}

#[tokio::test]
async fn bans_read_name_and_reason() {
    let server = MockServer::start().await;
    ok(
        &server,
        "GET",
        "/api/v1/stacks/survival/bans",
        json!({
            "running": true, "unreadable": false,
            "bans": [{ "name": "Eve", "reason": "Griefing", "source": "Rcon" }, { "name": "Mal" }]
        }),
    )
    .await;
    let b = client(&server).bans(&jwt(), "survival").await.unwrap();
    assert_eq!(b.bans.len(), 2);
    assert_eq!(b.bans[0].reason.as_deref(), Some("Griefing"));
    assert_eq!(b.bans[1].reason, None);
}

#[tokio::test]
async fn change_uses_each_lists_add_and_remove_routes() {
    let server = MockServer::start().await;
    let routes = [
        "/api/v1/stacks/survival/whitelist",
        "/api/v1/stacks/survival/whitelist/remove",
        "/api/v1/stacks/survival/ops",
        "/api/v1/stacks/survival/ops/remove",
        "/api/v1/stacks/survival/bans",
        "/api/v1/stacks/survival/bans/pardon",
    ];
    for route in routes {
        Mock::given(method("POST"))
            .and(path(route))
            .and(header("x-user-token", BEARER))
            .and(body_json(json!({ "names": ["Ana"] })))
            .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
                "results": [{ "name": "Ana", "status": "ok" }]
            }))))
            .expect(1)
            .mount(&server)
            .await;
    }
    let c = client(&server);
    let names = ["Ana".to_string()];
    for kind in [ListKind::Whitelist, ListKind::Ops, ListKind::Bans] {
        for add in [true, false] {
            let r = c
                .change(&jwt(), "survival", kind, add, &names)
                .await
                .unwrap();
            assert_eq!(r[0].status, "ok");
        }
    }
}

#[tokio::test]
async fn settings_keep_the_whole_document() {
    let server = MockServer::start().await;
    let settings = json!({
        "type": "paper", "version": "1.21", "javaVersion": 21,
        "customization": { "maxPlayers": 10, "env": { "MODRINTH_MODPACK": "abc" } },
        "futureField": [1, 2]
    });
    ok(
        &server,
        "GET",
        "/api/v1/stacks/survival/settings",
        json!({
            "revision": "r1", "managedBySource": "modpack", "pendingRestart": true,
            "settings": settings, "world": { "exists": true }
        }),
    )
    .await;
    let d = client(&server).settings(&jwt(), "survival").await.unwrap();
    assert_eq!(d.revision, "r1");
    assert_eq!(d.managed_by_source.as_deref(), Some("modpack"));
    assert!(d.pending_restart);
    assert_eq!(d.settings, settings);
}

fn saved() -> Value {
    envelope(json!({
        "revision": "r2", "pending": true, "restarted": false, "changed": true,
        "settings": { "type": "paper" }
    }))
}

#[tokio::test]
async fn put_settings_next_restart_sets_the_query() {
    let server = MockServer::start().await;
    let body = json!({ "revision": "r1", "settings": { "type": "paper" } });
    Mock::given(method("PUT"))
        .and(path("/api/v1/stacks/survival/settings"))
        .and(query_param("apply", "next-restart"))
        .and(header("x-user-token", BEARER))
        .and(body_json(body.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(saved()))
        .expect(1)
        .mount(&server)
        .await;
    let s = client(&server)
        .put_settings(&jwt(), "survival", &body, Apply::NextRestart)
        .await
        .unwrap();
    assert_eq!(s.revision, "r2");
    assert!(s.pending && !s.restarted);
    assert_eq!(s.settings, json!({ "type": "paper" }));
}

#[tokio::test]
async fn put_settings_now_has_no_query() {
    let server = MockServer::start().await;
    Mock::given(method("PUT"))
        .and(path("/api/v1/stacks/survival/settings"))
        .and(|r: &Request| r.url.query().is_none())
        .respond_with(ResponseTemplate::new(200).set_body_json(saved()))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .put_settings(&jwt(), "survival", &json!({}), Apply::Now)
        .await
        .unwrap();
}

#[tokio::test]
async fn console_sends_the_command_and_confirmation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/survival/console"))
        .and(header("x-user-token", BEARER))
        .and(body_json(json!({ "command": "say hi", "confirm": false })))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "outcome": "ok", "output": "done"
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let r = client(&server)
        .console(&jwt(), "survival", "say hi", false)
        .await
        .unwrap();
    assert_eq!(r.outcome, "ok");
    assert_eq!(r.output, "done");
    assert_eq!(r.code, None);
}

/// Every call, each refused the same way, so a table can check them through one of them.
async fn refused(status: u16, body: Value) -> launcher_core::LauncherError {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/stacks/survival/players"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    client(&server)
        .online(&jwt(), "survival")
        .await
        .unwrap_err()
}

#[tokio::test]
async fn refusals_map_to_codes() {
    let cases: [(u16, &str, Value, ErrorCode); 19] = [
        (
            409,
            "server_not_running",
            json!({}),
            ErrorCode::ServerNotRunning,
        ),
        (
            409,
            "rcon_unavailable",
            json!({}),
            ErrorCode::ServerConsoleUnavailable,
        ),
        (
            429,
            "too_many_commands",
            json!({}),
            ErrorCode::TooManyCommands,
        ),
        (
            504,
            "rcon_timeout",
            json!({}),
            ErrorCode::ServerConsoleTimeout,
        ),
        (
            409,
            "lifecycle_command_requires_confirmation",
            json!({}),
            ErrorCode::CommandNeedsConfirmation,
        ),
        (
            402,
            "quota_exceeded",
            json!({ "limit": 1 }),
            ErrorCode::TeamQuotaFull,
        ),
        (
            403,
            "plan_feature_locked",
            json!({ "field": "maxPlayers" }),
            ErrorCode::PlanFeatureLocked,
        ),
        (
            409,
            "settings_changed",
            json!({}),
            ErrorCode::SettingsChanged,
        ),
        (
            409,
            "version_change_requires_confirmation",
            json!({}),
            ErrorCode::VersionChangeNeedsConfirmation,
        ),
        (
            409,
            "data_loss_requires_acknowledgement",
            json!({}),
            ErrorCode::DataLossNeedsConfirmation,
        ),
        (
            502,
            "mojang_unavailable",
            json!({}),
            ErrorCode::MojangUnavailable,
        ),
        (
            429,
            "too_many_streams",
            json!({}),
            ErrorCode::TooManyStreams,
        ),
        (
            429,
            "too_many_diagnoses",
            json!({}),
            ErrorCode::TooManyDiagnoses,
        ),
        (
            409,
            "managed_by_source",
            json!({}),
            ErrorCode::ManagedBySource,
        ),
        (
            503,
            "host_full",
            json!({ "dimension": "ram" }),
            ErrorCode::HostFull,
        ),
        (
            409,
            "safety_backup_failed",
            json!({}),
            ErrorCode::SafetyBackupFailed,
        ),
        (500, "apply_failed", json!({}), ErrorCode::ApplyFailed),
        (
            409,
            "compose_not_editable",
            json!({}),
            ErrorCode::ComposeNotEditable,
        ),
        (
            403,
            "free_server_over_disk",
            json!({ "usedBytes": 10, "limitBytes": 5 }),
            ErrorCode::FreeServerOverDisk,
        ),
    ];
    for (status, code, data, want) in cases {
        let mut data = data;
        data["code"] = json!(code);
        let e = refused(status, json!({ "success": false, "data": data })).await;
        assert_eq!(e.code, want, "{code}");
        match code {
            "free_server_over_disk" => {
                assert_eq!(e.params["usedBytes"], "10");
                assert_eq!(e.params["limitBytes"], "5");
            }
            "plan_feature_locked" => assert_eq!(e.params["field"], "maxPlayers"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn a_locked_feature_falls_back_to_its_scope() {
    let e = refused(
        403,
        json!({ "data": { "code": "plan_feature_locked", "scope": "customDomain" } }),
    )
    .await;
    assert_eq!(e.code, ErrorCode::PlanFeatureLocked);
    assert_eq!(e.params["field"], "customDomain");
}

#[tokio::test]
async fn a_401_is_signed_out() {
    assert_eq!(
        refused(401, json!({})).await.code,
        ErrorCode::PerlandSignedOut
    );
}

#[tokio::test]
async fn a_403_is_forbidden() {
    assert_eq!(
        refused(403, json!({})).await.code,
        ErrorCode::ManageForbidden
    );
}

#[tokio::test]
async fn a_404_is_not_found() {
    assert_eq!(
        refused(404, json!({})).await.code,
        ErrorCode::ServerNotFound
    );
}

#[tokio::test]
async fn other_failures_fall_back_by_status() {
    assert_eq!(refused(500, json!({})).await.code, ErrorCode::HostTrouble);
}

#[test]
fn debug_holds_no_jwt() {
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    let c = StackClient::new(http, Url::parse("https://engine.example.com").unwrap());
    let shown = format!("{c:?} {:?}", jwt());
    assert!(!shown.contains(JWT));
    assert!(shown.contains("engine.example.com"));
}
