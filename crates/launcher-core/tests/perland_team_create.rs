//! A server made on a team (`POST /api/v1/stacks/create-server` and `/create-server/pack`), the
//! team's room (`/tenants/current/usage`) and the Engine's version lists (`/versions`), against a
//! wiremock Engine. Fake tokens only.
use launcher_core::{
    invite::Url,
    perland::{CreateServer, FreeServerSettings, ServerCustomization, SessionJwt, StackClient},
    ErrorCode, LauncherError,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

const JWT: &str = "eyJ.a.b";
const BEARER: &str = "Bearer eyJ.a.b";

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn failed(error: &str, data: Value) -> Value {
    json!({ "success": false, "data": data, "error": error, "meta": { "requestId": "r" } })
}

fn client(server: &MockServer) -> StackClient {
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    StackClient::new(http, Url::parse(&server.uri()).unwrap())
}

fn jwt() -> SessionJwt {
    SessionJwt::new(JWT)
}

fn created() -> Value {
    json!({
        "message": "created", "servername": "Amigos", "port": 25570, "status": "running",
        "started": true, "address": "play.example.com:25570"
    })
}

fn base(name: &str) -> CreateServer {
    CreateServer {
        server_name: name.into(),
        ram: "4G".into(),
        cpu: 2.0,
        ..CreateServer::default()
    }
}

async fn expect_create(server: &MockServer, body: Value) {
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/create-server"))
        .and(header("x-user-token", BEARER))
        .and(body_json(body))
        .respond_with(ResponseTemplate::new(201).set_body_json(envelope(created())))
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_vanilla_create_sends_type_and_version() {
    let server = MockServer::start().await;
    expect_create(
        &server,
        json!({
            "serverName": "Amigos", "type": "vanilla", "version": "1.21.1",
            "ram": "4G", "cpu": 2.0
        }),
    )
    .await;
    let made = client(&server)
        .create_server(
            &jwt(),
            &CreateServer {
                r#type: Some("vanilla".into()),
                version: Some("1.21.1".into()),
                ..base("Amigos")
            },
        )
        .await
        .unwrap();
    assert_eq!(made.server_name, "Amigos");
    assert!(made.started);
    assert_eq!(made.not_started_reason, None);
    assert_eq!(made.address.as_deref(), Some("play.example.com:25570"));
}

#[tokio::test]
async fn a_loader_create_sends_its_loader_version() {
    let server = MockServer::start().await;
    expect_create(
        &server,
        json!({
            "serverName": "Amigos", "type": "fabric", "version": "1.20.1",
            "loaderVersion": "0.16.14", "ram": "6G", "cpu": 1.5
        }),
    )
    .await;
    client(&server)
        .create_server(
            &jwt(),
            &CreateServer {
                r#type: Some("fabric".into()),
                version: Some("1.20.1".into()),
                loader_version: Some("0.16.14".into()),
                ram: "6G".into(),
                cpu: 1.5,
                ..base("Amigos")
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn a_modrinth_create_sends_the_version_id_and_no_type() {
    let server = MockServer::start().await;
    expect_create(
        &server,
        json!({ "serverName": "Amigos", "ram": "4G", "cpu": 2.0, "modrinthVersionId": "Yp8wLY1P" }),
    )
    .await;
    client(&server)
        .create_server(
            &jwt(),
            &CreateServer {
                modrinth_version_id: Some("Yp8wLY1P".into()),
                ..base("Amigos")
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn the_options_go_as_the_stacks_customization() {
    let server = MockServer::start().await;
    expect_create(
        &server,
        json!({
            "serverName": "Amigos", "type": "paper", "version": "1.21.1", "ram": "4G", "cpu": 2.0,
            "customization": {
                "mode": "survival", "hardcore": true, "difficulty": "hard", "maxPlayers": 8,
                "pvp": false, "enableWhitelist": true, "enforceWhitelist": true,
                "seed": "abc", "motd": "Hi"
            }
        }),
    )
    .await;
    let settings = FreeServerSettings {
        game_mode: Some("hardcore".into()),
        difficulty: None,
        max_players: Some(8),
        pvp: Some(false),
        whitelist: Some(true),
        seed: Some("abc".into()),
        motd: Some("Hi".into()),
    };
    client(&server)
        .create_server(
            &jwt(),
            &CreateServer {
                r#type: Some("paper".into()),
                version: Some("1.21.1".into()),
                customization: ServerCustomization::from_settings(&settings),
                ..base("Amigos")
            },
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn started_false_is_kept_with_its_reason() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/create-server"))
        .respond_with(ResponseTemplate::new(201).set_body_json(envelope(json!({
            "servername": "Amigos", "status": "stopped", "started": false,
            "notStartedReason": "quota_exceeded"
        }))))
        .mount(&server)
        .await;
    let made = client(&server)
        .create_server(&jwt(), &base("Amigos"))
        .await
        .unwrap();
    assert!(!made.started);
    assert_eq!(made.not_started_reason.as_deref(), Some("quota_exceeded"));
    assert_eq!(made.address, None);
}

#[tokio::test]
async fn an_answer_without_started_is_started() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/create-server"))
        .respond_with(ResponseTemplate::new(201).set_body_json(envelope(
            json!({ "servername": "Amigos", "status": "running" }),
        )))
        .mount(&server)
        .await;
    let made = client(&server)
        .create_server(&jwt(), &base("Amigos"))
        .await
        .unwrap();
    assert!(made.started);
}

/// Matches a multipart body holding every one of these byte strings.
struct MultipartWith(Vec<Vec<u8>>);

impl wiremock::Match for MultipartWith {
    fn matches(&self, request: &wiremock::Request) -> bool {
        let multipart = request
            .headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("multipart/form-data; boundary="));
        multipart
            && self.0.iter().all(|want| {
                request
                    .body
                    .windows(want.len())
                    .any(|w| w == want.as_slice())
            })
    }
}

#[tokio::test]
async fn a_pack_create_uploads_request_sha256_and_the_pack_file() {
    let server = MockServer::start().await;
    let pack = b"PK pack bytes".to_vec();
    let sha = launcher_core::download::hash::sha256_hex(&pack);
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/create-server/pack"))
        .and(header("x-user-token", BEARER))
        .and(MultipartWith(vec![
            b"name=\"request\"\r\n\r\n{\"serverName\":\"Amigos\",\"ram\":\"6G\",\"cpu\":2.0}"
                .to_vec(),
            format!("name=\"sha256\"\r\n\r\n{sha}").into_bytes(),
            // A file part: without a file name the Engine reads it as a plain field.
            b"name=\"pack\"; filename=\"pack.mrpack\"".to_vec(),
            pack.clone(),
        ]))
        .respond_with(ResponseTemplate::new(201).set_body_json(envelope(created())))
        .expect(1)
        .mount(&server)
        .await;
    let made = client(&server)
        .create_server_from_pack(
            &jwt(),
            &CreateServer {
                ram: "6G".into(),
                ..base("Amigos")
            },
            pack,
        )
        .await
        .unwrap();
    assert_eq!(made.server_name, "Amigos");
}

async fn create_refused(status: u16, body: Value) -> LauncherError {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/stacks/create-server"))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    client(&server)
        .create_server(&jwt(), &base("Amigos"))
        .await
        .unwrap_err()
}

#[tokio::test]
async fn a_locked_field_keeps_its_name() {
    let e = create_refused(
        402,
        failed(
            "not in your plan",
            json!({ "code": "plan_feature_locked", "field": "ram" }),
        ),
    )
    .await;
    assert_eq!(e.code, ErrorCode::PlanFeatureLocked);
    assert_eq!(e.params.get("field").map(String::as_str), Some("ram"));
}

#[tokio::test]
async fn create_refusals_are_worded_for_a_team() {
    let e = create_refused(
        402,
        failed(
            "over",
            json!({ "code": "quota_exceeded", "requested": { "memoryMb": 8192 } }),
        ),
    )
    .await;
    assert_eq!(e.code, ErrorCode::TeamServerTooBig);
    assert!(!e.message.to_lowercase().contains("free"), "{}", e.message);

    let e = create_refused(503, failed("full", json!({ "code": "host_full" }))).await;
    assert_eq!(e.code, ErrorCode::HostFull);

    let e = create_refused(409, failed("taken", json!({ "code": "name_taken" }))).await;
    assert_eq!(e.code, ErrorCode::ServerNameTaken);
    assert_eq!(e.params.get("field").map(String::as_str), Some("name"));

    let e = create_refused(
        400,
        failed(
            "with modrinthVersionId, leave out type",
            json!({ "field": "type" }),
        ),
    )
    .await;
    assert_eq!(e.code, ErrorCode::InvalidTeamServer);
    assert_eq!(e.params.get("field").map(String::as_str), Some("type"));

    let e = create_refused(
        400,
        failed(
            "too heavy",
            json!({ "code": "modpack_too_heavy", "requiredMemoryMb": 6144, "limitMemoryMb": 4096 }),
        ),
    )
    .await;
    assert_eq!(e.code, ErrorCode::ModpackTooHeavy);
    assert_eq!(
        e.params.get("requiredMemoryMb").map(String::as_str),
        Some("6144")
    );
    assert_eq!(
        e.params.get("limitMemoryMb").map(String::as_str),
        Some("4096")
    );
    assert!(!e.message.to_lowercase().contains("free"), "{}", e.message);

    let e = create_refused(
        400,
        failed(
            "no",
            json!({ "code": "modpack_unsupported", "reason": "loader" }),
        ),
    )
    .await;
    assert_eq!(e.code, ErrorCode::ModpackUnsupported);
    assert_eq!(e.params.get("reason").map(String::as_str), Some("loader"));
    assert!(!e.message.to_lowercase().contains("free"), "{}", e.message);

    let e = create_refused(
        502,
        failed("modrinth down", json!({ "code": "modrinth_unavailable" })),
    )
    .await;
    assert_eq!(e.code, ErrorCode::HostTrouble);

    let e = create_refused(
        502,
        failed("no list", json!({ "code": "versions_unavailable" })),
    )
    .await;
    assert_eq!(e.code, ErrorCode::VersionsUnavailable);

    // A proxy giving up on the long create: the Engine may finish it anyway.
    let e = create_refused(504, json!({})).await;
    assert_eq!(e.code, ErrorCode::ConnectionDropped);

    let e = create_refused(401, failed("no", json!({}))).await;
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn usage_reads_what_is_available() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tenants/current/usage"))
        .and(header("x-user-token", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "limits": { "maxCpu": 4, "maxMemoryMb": 8192, "maxServers": 3 },
            "allocated": { "cpu": 2, "memoryMb": 4096, "servers": 1 },
            "running": { "cpu": 2, "memoryMb": 4096, "servers": 1 },
            "available": { "cpu": 2, "memoryMb": 4096, "servers": 2 }
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let usage = client(&server).usage(&jwt()).await.unwrap();
    assert_eq!(usage.available.memory_mb, Some(4096));
    assert_eq!(usage.available.cpu, Some(2.0));
    assert_eq!(usage.available.servers, Some(2));
    assert_eq!(usage.limits.max_memory_mb, Some(8192));
    assert_eq!(usage.limits.max_cpu, Some(4.0));
}

#[tokio::test]
async fn usage_without_docker_has_no_available_memory() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/tenants/current/usage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "limits": { "maxCpu": 4, "maxMemoryMb": 8192, "maxServers": 3 },
            "available": { "servers": 2 },
            "liveError": "docker unavailable"
        }))))
        .mount(&server)
        .await;
    let usage = client(&server).usage(&jwt()).await.unwrap();
    assert_eq!(usage.available.memory_mb, None);
    assert_eq!(usage.available.cpu, None);
    assert_eq!(usage.limits.max_memory_mb, Some(8192));
}

#[tokio::test]
async fn versions_list_one_type_and_loader_versions_for_a_minecraft() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/versions"))
        .and(query_param("type", "paper"))
        .and(header("x-user-token", BEARER))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "type": "paper",
            "versions": [{ "id": "1.21.1", "stable": true }, { "id": "1.21.2-rc1", "stable": false }]
        }))))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/v1/versions"))
        .and(query_param("type", "forge"))
        .and(query_param("minecraft", "1.20.1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "type": "forge", "minecraft": "1.20.1",
            "versions": [{ "id": "47.3.0", "stable": true, "recommended": true }]
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let paper = c.versions(&jwt(), "paper", None).await.unwrap();
    assert_eq!(paper.len(), 2);
    assert_eq!(paper[0].id, "1.21.1");
    assert!(paper[0].stable && !paper[1].stable);
    let forge = c.versions(&jwt(), "forge", Some("1.20.1")).await.unwrap();
    assert_eq!(forge[0].id, "47.3.0");
    assert!(forge[0].recommended);
}

#[tokio::test]
async fn versions_unavailable_is_its_code() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/versions"))
        .respond_with(
            ResponseTemplate::new(502)
                .set_body_json(failed("no", json!({ "code": "versions_unavailable" }))),
        )
        .mount(&server)
        .await;
    let e = client(&server)
        .versions(&jwt(), "vanilla", None)
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::VersionsUnavailable);
}
