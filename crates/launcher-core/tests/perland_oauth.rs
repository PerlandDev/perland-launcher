//! Perland's OAuth sign-in (discovery, token, revoke) against a wiremock Engine. Fake codes
//! and tokens only.
use launcher_core::{
    invite::Url,
    perland::{OAuthClient, CLIENT_ID},
    ErrorCategory, ErrorCode, LauncherError,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_string, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const DISCOVERY: &str = "/.well-known/oauth-authorization-server";
const TOKEN: &str = "/api/v1/oauth/token";
const REVOKE: &str = "/api/v1/oauth/revoke";
const REDIRECT: &str = "http://127.0.0.1:5000/callback";

fn client(server: &MockServer) -> OAuthClient {
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    OAuthClient::new(http, Url::parse(&server.uri()).unwrap())
}

fn access_token() -> String {
    format!("plt_{}", "x".repeat(43))
}

fn token_answer() -> Value {
    json!({
        "access_token": access_token(),
        "token_type": "Bearer",
        "expires_in": 7776000,
        "scope": "launcher",
        "perland_player": {
            "id": "p1", "displayName": "Moi", "email": "moi@example.com",
            "createdAt": "2026-10-16T20:00:00Z"
        },
        "perland_new_player": true
    })
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

/// The error leaks none of the request's credentials. A failure names which marker leaked
/// (by position) and never prints the error, so a leak is not repeated in the test output.
fn clean(e: &LauncherError) {
    let shown = format!("{e:?}");
    let markers = ["the-code", "the-verifier", "plt_"];
    let leaked: Vec<usize> = (0..markers.len())
        .filter(|&i| shown.contains(markers[i]))
        .collect();
    assert!(
        leaked.is_empty(),
        "credential markers {leaked:?} appear in the error"
    );
}

#[tokio::test]
async fn discover_reads_the_authorize_page() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        DISCOVERY,
        200,
        json!({
            "issuer": server.uri(),
            "authorization_endpoint": "https://panel.example.com/oauth/authorize",
            "token_endpoint": "http://elsewhere.example.com/api/v1/oauth/token",
            "revocation_endpoint": "http://elsewhere.example.com/api/v1/oauth/revoke",
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code"],
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": ["launcher"]
        }),
    )
    .await;
    let found = client(&server).discover().await.unwrap().unwrap();
    assert_eq!(
        found.authorization_endpoint.as_str(),
        "https://panel.example.com/oauth/authorize"
    );
}

#[tokio::test]
async fn discover_404_is_no_oauth() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        DISCOVERY,
        404,
        json!({ "error": "not_found" }),
    )
    .await;
    assert_eq!(client(&server).discover().await.unwrap(), None);
}

#[tokio::test]
async fn discover_without_the_route_at_all_is_no_oauth() {
    // An older Engine: nothing mounted, wiremock answers 404 with no body.
    let server = MockServer::start().await;
    assert_eq!(client(&server).discover().await.unwrap(), None);
}

#[tokio::test]
async fn discover_unusable_metadata_is_no_oauth() {
    let server = MockServer::start().await;
    answer(
        &server,
        "GET",
        DISCOVERY,
        200,
        json!({ "authorization_endpoint": "http://panel.example.com/oauth/authorize" }),
    )
    .await;
    assert_eq!(client(&server).discover().await.unwrap(), None);
}

#[tokio::test]
async fn discover_trouble_is_an_error() {
    let server = MockServer::start().await;
    answer(&server, "GET", DISCOVERY, 503, json!({})).await;
    let e = client(&server).discover().await.unwrap_err();
    assert_eq!(e.code, ErrorCode::HostTrouble);

    let server = MockServer::start().await;
    answer(&server, "GET", DISCOVERY, 429, json!({})).await;
    let e = client(&server).discover().await.unwrap_err();
    assert_eq!(e.code, ErrorCode::InviteRateLimited);
}

#[tokio::test]
async fn token_posts_the_form_and_reads_the_rfc_answer() {
    let server = MockServer::start().await;
    let form = format!(
        "grant_type=authorization_code&code=the-code&redirect_uri=http%3A%2F%2F127.0.0.1%3A5000%2Fcallback&client_id={CLIENT_ID}&code_verifier=the-verifier"
    );
    Mock::given(method("POST"))
        .and(path(TOKEN))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(form))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_answer()))
        .expect(1)
        .mount(&server)
        .await;
    let answer = client(&server)
        .token("the-code", "the-verifier", REDIRECT)
        .await
        .unwrap();
    assert_eq!(answer.token, access_token());
    assert!(answer.new_player);
    assert_eq!(answer.player.display_name, "Moi");
    assert_eq!(answer.player.email.as_deref(), Some("moi@example.com"));
    // Debug never shows the token.
    assert!(!format!("{answer:?}").contains("plt_"));
}

#[tokio::test]
async fn token_errors_map_to_launcher_codes() {
    let cases = [
        (400, "invalid_grant", ErrorCode::SignInExpired),
        (401, "invalid_client", ErrorCode::SignInClientRefused),
        (400, "unauthorized_client", ErrorCode::SignInClientRefused),
        (400, "invalid_request", ErrorCode::SignInIncompatible),
        (400, "unsupported_grant_type", ErrorCode::SignInIncompatible),
        (400, "invalid_scope", ErrorCode::SignInIncompatible),
        (400, "something_new", ErrorCode::SignInFailed),
        (500, "server_error", ErrorCode::HostTrouble),
    ];
    for (status, error, want) in cases {
        let server = MockServer::start().await;
        answer(
            &server,
            "POST",
            TOKEN,
            status,
            json!({ "error": error, "error_description": "plt_ the-code nope" }),
        )
        .await;
        let e = client(&server)
            .token("the-code", "the-verifier", REDIRECT)
            .await
            .unwrap_err();
        assert_eq!(e.code, want, "{error}");
        assert!(param(&e, "host").is_some(), "{error}");
        clean(&e);
    }
}

#[tokio::test]
async fn token_incompatible_says_to_update() {
    let server = MockServer::start().await;
    answer(
        &server,
        "POST",
        TOKEN,
        400,
        json!({ "error": "invalid_scope" }),
    )
    .await;
    let e = client(&server)
        .token("the-code", "the-verifier", REDIRECT)
        .await
        .unwrap_err();
    assert_eq!(e.category, ErrorCategory::Auth);
    assert!(e.fix.is_some());
    assert_eq!(e.detail.as_deref(), Some("invalid_scope"));
}

#[tokio::test]
async fn token_rate_limit_and_odd_answers() {
    let server = MockServer::start().await;
    answer(&server, "POST", TOKEN, 429, json!({ "error": "slow_down" })).await;
    let e = client(&server).token("c", "v", REDIRECT).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::InviteRateLimited);

    // A refusal that isn't an RFC error object.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN))
        .respond_with(ResponseTemplate::new(403).set_body_string("<html>no</html>"))
        .mount(&server)
        .await;
    let e = client(&server).token("c", "v", REDIRECT).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::SignInFailed);

    // A success with no usable token.
    for body in [
        json!({ "token_type": "Bearer" }),
        json!({ "access_token": "", "token_type": "Bearer", "perland_player": {"id": "p", "displayName": "", "createdAt": "2026-10-16T20:00:00Z"} }),
        json!({ "access_token": access_token(), "token_type": "mac", "perland_player": {"id": "p", "displayName": "", "createdAt": "2026-10-16T20:00:00Z"} }),
    ] {
        let server = MockServer::start().await;
        answer(&server, "POST", TOKEN, 200, body).await;
        let e = client(&server).token("c", "v", REDIRECT).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::MetaUnexpected);
        clean(&e);
    }
}

#[tokio::test]
async fn token_does_not_follow_a_redirect() {
    let target = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(token_answer()))
        .expect(0)
        .mount(&target)
        .await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(TOKEN))
        .respond_with(
            ResponseTemplate::new(307).insert_header("location", format!("{}/steal", target.uri())),
        )
        .mount(&server)
        .await;
    assert!(client(&server)
        .token("the-code", "the-verifier", REDIRECT)
        .await
        .is_err());
}

#[tokio::test]
async fn revoke_posts_the_token_and_client() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(REVOKE))
        .and(header("content-type", "application/x-www-form-urlencoded"))
        .and(body_string(format!(
            "token={}&client_id={CLIENT_ID}",
            access_token()
        )))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;
    client(&server).revoke(&access_token()).await.unwrap();
}

#[tokio::test]
async fn revoke_trouble_is_an_error_without_the_token() {
    let server = MockServer::start().await;
    answer(&server, "POST", REVOKE, 502, json!({})).await;
    let e = client(&server).revoke(&access_token()).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::HostTrouble);
    clean(&e);
}
