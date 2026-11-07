//! Adding an email to the signed-in Perland account (`/api/v1/auth/me/email/*`), and the
//! Engine's `launcher_update_required`, against a wiremock Engine. Fake tokens only.
use launcher_core::{
    invite::Url,
    perland::{OAuthClient, PlayerClient},
    ErrorCategory, ErrorCode, LauncherError,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const START: &str = "/api/v1/auth/me/email/start";
const VERIFY: &str = "/api/v1/auth/me/email/verify";

fn token() -> String {
    format!("plt_{}", "x".repeat(43))
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

async fn answer(server: &MockServer, verb: &str, route: &str, status: u16, body: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

fn param<'a>(e: &'a LauncherError, key: &str) -> Option<&'a str> {
    e.params.get(key).map(String::as_str)
}

/// The error leaks no token.
fn clean(e: &LauncherError) {
    assert!(
        !format!("{e:?}").contains("plt_"),
        "the error holds a token"
    );
}

async fn start_fails(status: u16, body: Value) -> LauncherError {
    let server = MockServer::start().await;
    answer(&server, "POST", START, status, body).await;
    let e = client(&server)
        .account_email_start(&token(), "a@b.co", "en")
        .await
        .unwrap_err();
    clean(&e);
    e
}

async fn verify_fails(status: u16, body: Value) -> LauncherError {
    let server = MockServer::start().await;
    answer(&server, "POST", VERIFY, status, body).await;
    let e = client(&server)
        .account_email_verify(&token(), "a@b.co", "123456")
        .await
        .unwrap_err();
    clean(&e);
    e
}

#[tokio::test]
async fn account_email_start_sends_bearer_and_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(START))
        .and(header(
            "authorization",
            format!("Bearer {}", token()).as_str(),
        ))
        .and(body_json(json!({ "email": "a@b.co", "language": "es" })))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(envelope(json!({ "expiresIn": 600, "resendIn": 60 }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let started = client(&server)
        .account_email_start(&token(), "a@b.co", "es")
        .await
        .unwrap();
    assert_eq!(started.resend_in, 60);
    assert_eq!(started.expires_in, 600);
}

#[tokio::test]
async fn account_email_verify_returns_the_email() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(VERIFY))
        .and(header(
            "authorization",
            format!("Bearer {}", token()).as_str(),
        ))
        .and(body_json(json!({ "email": "a@b.co", "code": "123456" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope(json!({ "email": "a@b.co" }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let email = client(&server)
        .account_email_verify(&token(), "a@b.co", "123456")
        .await
        .unwrap();
    assert_eq!(email, "a@b.co");
}

#[tokio::test]
async fn account_email_verify_wrong_code() {
    let e = verify_fails(400, failure("invalid_code", json!({ "attemptsLeft": 2 }))).await;
    assert_eq!(e.code, ErrorCode::InvalidCode);
    assert_eq!(param(&e, "attemptsLeft"), Some("2"));
}

#[tokio::test]
async fn account_email_in_use() {
    let e = verify_fails(409, failure("email_in_use", json!({}))).await;
    assert_eq!(e.code, ErrorCode::EmailInUse);
    assert_eq!(e.category, ErrorCategory::Auth);
    assert_eq!(
        e.message,
        "That email is already on another Perland account."
    );
    assert_eq!(
        e.fix.as_deref(),
        Some("Use another email, or sign in with that account.")
    );
    assert!(param(&e, "host").is_some());
}

#[tokio::test]
async fn account_email_401_signs_out() {
    let start = start_fails(401, json!({ "error": "unauthorized" })).await;
    assert_eq!(start.code, ErrorCode::PerlandSignedOut);
    let verify = verify_fails(401, json!({ "error": "unauthorized" })).await;
    assert_eq!(verify.code, ErrorCode::PerlandSignedOut);
    let disabled = verify_fails(403, failure("account_disabled", json!({}))).await;
    assert_eq!(disabled.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn account_email_start_refusals_map() {
    let e = start_fails(400, failure("invalid_email", json!({}))).await;
    assert_eq!(e.code, ErrorCode::InvalidEmail);
    let e = start_fails(429, failure("too_many_codes", json!({ "retryAfter": 42 }))).await;
    assert_eq!(e.code, ErrorCode::TooManyCodes);
    assert_eq!(param(&e, "retryAfter"), Some("42"));
    assert!(e.message.contains("42"));
    let e = start_fails(502, failure("email_unavailable", json!({}))).await;
    assert_eq!(e.code, ErrorCode::EmailUnavailable);
    let e = start_fails(
        503,
        failure("sign_in_method_unavailable", json!({ "method": "email" })),
    )
    .await;
    assert_eq!(e.code, ErrorCode::SignInUnavailable);
    assert_eq!(param(&e, "method"), Some("email"));
    let e = start_fails(429, json!({ "error": "slow" })).await;
    assert_eq!(e.code, ErrorCode::InviteRateLimited);
}

fn update_required() -> Value {
    json!({
        "success": false,
        "data": { "code": "launcher_update_required" },
        "error": "Update Perland Launcher to sign up."
    })
}

fn is_update_required(e: &LauncherError) {
    assert_eq!(e.code, ErrorCode::LauncherUpdateRequired);
    assert_eq!(e.category, ErrorCategory::Auth);
    assert_eq!(e.message, "Update Perland Launcher to sign up.");
    assert_eq!(
        e.fix.as_deref(),
        Some("Update Perland Launcher from Settings.")
    );
    clean(e);
}

#[tokio::test]
async fn launcher_update_required_is_its_own_code() {
    let server = MockServer::start().await;
    answer(&server, "GET", "/api/v1/player/me", 403, update_required()).await;
    answer(
        &server,
        "POST",
        "/api/v1/oauth/token",
        403,
        update_required(),
    )
    .await;
    answer(&server, "POST", START, 403, update_required()).await;
    answer(
        &server,
        "POST",
        "/api/v1/auth/player-session",
        403,
        update_required(),
    )
    .await;
    let engine = Url::parse(&server.uri()).unwrap();
    is_update_required(&client(&server).me(&token()).await.unwrap_err());
    is_update_required(
        &client(&server)
            .account_email_start(&token(), "a@b.co", "en")
            .await
            .unwrap_err(),
    );
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    let oauth = OAuthClient::new(http.clone(), engine.clone());
    is_update_required(
        &oauth
            .token("c", "v", "http://127.0.0.1:5000/callback")
            .await
            .unwrap_err(),
    );
    let auth = launcher_core::perland::AuthClient::new(http, engine);
    is_update_required(&auth.player_session(&token()).await.unwrap_err());
}
