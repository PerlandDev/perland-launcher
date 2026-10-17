//! The Perland player API against a wiremock Engine. Fake tokens only.
use launcher_core::{
    invite::Url,
    perland::{PlayerClient, PlayerConfig, Relation, SignInMethod},
    ErrorCode, LauncherError,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const PLAYER: &str = "/api/v1/player";

fn token() -> String {
    format!("plt_{}", "x".repeat(43))
}

fn invite_token() -> String {
    "A".repeat(43)
}

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

/// The Engine's failure: `error` is a string, `data` carries the code and any extras.
fn failure(code: &str, extra: Value) -> Value {
    let mut data = json!({ "code": code });
    if let (Some(data), Some(extra)) = (data.as_object_mut(), extra.as_object()) {
        data.extend(extra.clone());
    }
    json!({ "success": false, "data": data, "error": "nope", "meta": { "requestId": "r" } })
}

fn client(server: &MockServer) -> PlayerClient {
    let http = launcher_core::http::client("test").unwrap();
    PlayerClient::new(http, Url::parse(&server.uri()).unwrap())
}

fn route(suffix: &str) -> String {
    format!("{PLAYER}{suffix}")
}

fn player() -> Value {
    json!({ "id": "p1", "displayName": "Moi", "email": "moi@example.com", "createdAt": "2026-10-16T20:00:00Z" })
}

async fn answer(server: &MockServer, verb: &str, suffix: &str, status: u16, body: Value) {
    Mock::given(method(verb))
        .and(path(route(suffix)))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

fn param<'a>(e: &'a LauncherError, key: &str) -> Option<&'a str> {
    e.params.get(key).map(String::as_str)
}

#[tokio::test]
async fn config_reads_methods() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        "/config",
        200,
        envelope(json!({ "enabled": true, "methods": ["discord", "email"] })),
    )
    .await;
    let config = client(&server).config().await.unwrap();
    assert!(config.enabled);
    assert_eq!(config.methods, [SignInMethod::Discord, SignInMethod::Email]);
}

#[tokio::test]
async fn config_404_is_off() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        "/config",
        404,
        json!({ "error": "not found" }),
    )
    .await;
    assert_eq!(client(&server).config().await.unwrap(), PlayerConfig::off());
}

#[tokio::test]
async fn config_503_is_off() {
    let server = MockServer::start().await;
    answer(&server, "GET", "/config", 503, json!({})).await;
    assert_eq!(client(&server).config().await.unwrap(), PlayerConfig::off());
}

#[tokio::test]
async fn token_posts_code_verifier_redirect() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route("/token")))
        .and(body_json(json!({
            "code": "the-code",
            "codeVerifier": "the-verifier",
            "redirectUri": "http://127.0.0.1:40000/callback"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(
            json!({ "token": token(), "player": player(), "newPlayer": true }),
        )))
        .expect(1)
        .mount(&server)
        .await;
    let answer = client(&server)
        .token(
            "the-code",
            "the-verifier",
            "http://127.0.0.1:40000/callback",
        )
        .await
        .unwrap();
    assert_eq!(answer.token, token());
    assert!(answer.new_player);
    assert_eq!(answer.player.display_name, "Moi");
    assert_eq!(answer.player.email.as_deref(), Some("moi@example.com"));
}

#[tokio::test]
async fn token_refused_is_sign_in_failed() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/token",
        400,
        failure("invalid_code", json!({})),
    )
    .await;
    let e = client(&server).token("c", "v", "r").await.unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInFailed);
}

#[tokio::test]
async fn email_start_parses_timings() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route("/email/start")))
        .and(body_json(
            json!({ "email": "moi@example.com", "language": "es" }),
        ))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(envelope(json!({ "expiresIn": 600, "resendIn": 60 }))),
        )
        .mount(&server)
        .await;
    let started = client(&server)
        .email_start("moi@example.com", "es")
        .await
        .unwrap();
    assert_eq!((started.expires_in, started.resend_in), (600, 60));
}

#[tokio::test]
async fn invalid_code_carries_attempts_left() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route("/email/verify")))
        .and(body_json(
            json!({ "email": "moi@example.com", "code": "123456" }),
        ))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(failure("invalid_code", json!({ "attemptsLeft": 2 }))),
        )
        .mount(&server)
        .await;
    let e = client(&server)
        .email_verify("moi@example.com", "123456")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidCode);
    assert_eq!(param(&e, "attemptsLeft"), Some("2"));
}

#[tokio::test]
async fn email_verify_parses_the_answer() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/email/verify",
        200,
        envelope(json!({ "token": token(), "player": player(), "newPlayer": false })),
    )
    .await;
    let answer = client(&server)
        .email_verify("moi@example.com", "123456")
        .await
        .unwrap();
    assert!(!answer.new_player);
}

#[tokio::test]
async fn too_many_codes_carries_retry_after() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/email/start",
        429,
        failure("too_many_codes", json!({ "retryAfter": 42 })),
    )
    .await;
    let e = client(&server)
        .email_start("a@b.co", "en")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::TooManyCodes);
    assert_eq!(param(&e, "retryAfter"), Some("42"));
    assert!(e.message.contains("42"));
}

#[tokio::test]
async fn plain_429_is_rate_limited() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/email/start",
        429,
        json!({ "error": "slow" }),
    )
    .await;
    let e = client(&server)
        .email_start("a@b.co", "en")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::InviteRateLimited);
}

#[tokio::test]
async fn email_unavailable_maps() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/email/start",
        502,
        failure("email_unavailable", json!({})),
    )
    .await;
    let e = client(&server)
        .email_start("a@b.co", "en")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::EmailUnavailable);
}

#[tokio::test]
async fn invalid_email_maps() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/email/start",
        400,
        failure("invalid_email", json!({})),
    )
    .await;
    let e = client(&server).email_start("nope", "en").await.unwrap_err();
    assert_eq!(e.code, ErrorCode::InvalidEmail);
}

#[tokio::test]
async fn sign_in_method_unavailable_maps_with_method() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/email/start",
        503,
        failure("sign_in_method_unavailable", json!({ "method": "email" })),
    )
    .await;
    let e = client(&server)
        .email_start("a@b.co", "en")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInUnavailable);
    assert_eq!(param(&e, "method"), Some("email"));
}

#[tokio::test]
async fn player_accounts_disabled_is_closed() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        "/me",
        503,
        failure("player_accounts_disabled", json!({})),
    )
    .await;
    let e = client(&server).me(&token()).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PlayerAccountsClosed);
}

#[tokio::test]
async fn me_sends_bearer_and_parses() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(route("/me")))
        .and(header("authorization", format!("Bearer {}", token()).as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "id": "p1",
            "displayName": "Moi",
            "discord": { "id": "80", "username": "moi", "globalName": "Moisés", "avatarUrl": "https://cdn.example/a.png" },
            "createdAt": "2026-10-16T20:00:00Z"
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let me = client(&server).me(&token()).await.unwrap();
    assert_eq!(me.id, "p1");
    assert_eq!(me.email, None);
    let discord = me.discord.unwrap();
    assert_eq!(discord.username, "moi");
    assert_eq!(discord.global_name.as_deref(), Some("Moisés"));
    assert_eq!(me.created_at, "2026-10-16T20:00:00Z");
}

#[tokio::test]
async fn me_401_is_signed_out() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        "/me",
        401,
        json!({ "error": "unauthorized" }),
    )
    .await;
    let e = client(&server).me(&token()).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn servers_parses_relations() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        "/servers",
        200,
        envelope(json!({ "servers": [
            { "token": invite_token(), "name": "mine", "status": "offline", "relation": "owner",
              "following": false, "news": { "unseen": true } },
            { "token": "B".repeat(43), "name": "theirs", "status": "online", "relation": "followed",
              "following": true }
        ] })),
    )
    .await;
    let servers = client(&server).servers(&token()).await.unwrap();
    assert_eq!(servers.len(), 2);
    assert_eq!(servers[0].relation, Relation::Owner);
    assert!(!servers[0].following);
    assert_eq!(servers[0].server.name, "mine");
    assert_eq!(servers[0].server.token, invite_token());
    assert_eq!(servers[1].relation, Relation::Followed);
    assert!(servers[1].following);
}

#[tokio::test]
async fn servers_503_unavailable_is_closed() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        "/servers",
        503,
        failure("player_servers_unavailable", json!({})),
    )
    .await;
    let e = client(&server).servers(&token()).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PlayerServersClosed);
}

#[tokio::test]
async fn follow_posts_token_in_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/player/servers/follow"))
        .and(header(
            "authorization",
            format!("Bearer {}", token()).as_str(),
        ))
        .and(body_json(json!({ "token": invite_token() })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope(json!({ "following": true }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .follow(&token(), &invite_token())
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].url.query(), None);
}

#[tokio::test]
async fn unfollow_and_joined_post_their_routes() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/servers/unfollow",
        200,
        envelope(json!({ "following": false })),
    )
    .await;
    answer(
        &server,
        "POST",
        "/servers/joined",
        200,
        envelope(json!({ "recorded": true })),
    )
    .await;
    let c = client(&server);
    c.unfollow(&token(), &invite_token()).await.unwrap();
    c.joined(&token(), &invite_token()).await.unwrap();
}

#[tokio::test]
async fn follow_404_is_ok() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/servers/follow",
        404,
        json!({ "error": "not found" }),
    )
    .await;
    client(&server)
        .follow(&token(), &invite_token())
        .await
        .unwrap();
}

#[tokio::test]
async fn joined_404_is_ok() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/servers/joined",
        404,
        json!({ "error": "not found" }),
    )
    .await;
    client(&server)
        .joined(&token(), &invite_token())
        .await
        .unwrap();
}

#[tokio::test]
async fn follow_409_too_many() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        "/servers/follow",
        409,
        failure("too_many_player_servers", json!({})),
    )
    .await;
    let e = client(&server)
        .follow(&token(), &invite_token())
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::TooManyPlayerServers);
}

#[tokio::test]
async fn follow_401_is_signed_out() {
    let server = MockServer::start().await;
    answer(&server, "POST", "/servers/follow", 401, json!({})).await;
    let e = client(&server)
        .follow(&token(), &invite_token())
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn sign_out_posts() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route("/sign-out")))
        .and(header(
            "authorization",
            format!("Bearer {}", token()).as_str(),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope(json!({ "signedOut": true }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    client(&server).sign_out(&token()).await.unwrap();
}

#[tokio::test]
async fn errors_name_host_only() {
    let server = MockServer::start().await;
    answer(&server, "GET", "/me", 500, json!({})).await;
    answer(&server, "GET", "/servers", 200, json!("not an envelope")).await;
    let host = Url::parse(&server.uri())
        .unwrap()
        .host_str()
        .unwrap()
        .to_string();
    let c = client(&server);
    let mut errors = vec![
        c.me(&token()).await.unwrap_err(),
        c.servers(&token()).await.unwrap_err(),
    ];
    // An Engine that isn't there at all.
    let dead = MockServer::start().await;
    let dead_client = client(&dead);
    drop(dead);
    errors.push(dead_client.me(&token()).await.unwrap_err());
    for e in errors {
        let everything = format!("{} {:?} {:?}", e.message, e.detail, e.params);
        assert!(!everything.contains("/api/v1"), "{everything}");
        assert!(!everything.contains(&token()), "{everything}");
    }
    let e = c.me(&token()).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::HostTrouble);
    assert!(e.message.contains(&host), "{}", e.message);
}
