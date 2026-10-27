//! Microsoft OAuth (authorize URL, code and refresh grants) against a wiremock token endpoint.
//! Fake tokens only.
use launcher_core::{
    invite::Url,
    msa::{MsaClient, MsaEndpoints},
    perland::Pkce,
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{body_string_contains, method, path},
    Mock, MockServer, ResponseTemplate,
};

const CLIENT_ID: &str = "client-123";
const REDIRECT: &str = "http://127.0.0.1:5555/callback";

fn endpoints(server: &MockServer) -> MsaEndpoints {
    let base = Url::parse(&server.uri()).unwrap();
    MsaEndpoints {
        authorize: base.join("/authorize").unwrap(),
        token: base.join("/token").unwrap(),
        xbox_user: base.join("/xbox").unwrap(),
        xsts: base.join("/xsts").unwrap(),
        minecraft: base,
    }
}

fn client(server: &MockServer) -> MsaClient {
    let http = launcher_core::http::client("test").unwrap();
    MsaClient::new(http, endpoints(server), CLIENT_ID.to_string())
}

fn tokens_body() -> serde_json::Value {
    json!({ "access_token": "fake-access", "refresh_token": "fake-refresh", "expires_in": 3600 })
}

#[test]
fn production_endpoints_are_the_documented_ones() {
    let e = MsaEndpoints::production();
    assert_eq!(
        e.authorize.as_str(),
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize"
    );
    assert_eq!(
        e.token.as_str(),
        "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
    );
    assert_eq!(
        e.xbox_user.as_str(),
        "https://user.auth.xboxlive.com/user/authenticate"
    );
    assert_eq!(
        e.xsts.as_str(),
        "https://xsts.auth.xboxlive.com/xsts/authorize"
    );
    assert_eq!(e.minecraft.as_str(), "https://api.minecraftservices.com/");
}

#[test]
fn authorize_url_carries_pkce_and_scope() {
    let server_less = MsaEndpoints::production();
    let http = launcher_core::http::client("test").unwrap();
    let c = MsaClient::new(http, server_less, CLIENT_ID.to_string());
    let pkce = Pkce::from_verifier("verifier", "state-1");
    let url = c.authorize_url(REDIRECT, &pkce);
    assert_eq!(url.path(), "/consumers/oauth2/v2.0/authorize");
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["client_id"], CLIENT_ID);
    assert_eq!(q["response_type"], "code");
    assert_eq!(q["redirect_uri"], REDIRECT);
    assert_eq!(q["scope"], "XboxLive.signin offline_access");
    assert_eq!(q["code_challenge"], pkce.challenge);
    assert_eq!(q["code_challenge_method"], "S256");
    assert_eq!(q["state"], "state-1");
    assert_eq!(q["prompt"], "select_account");
    assert!(!url.as_str().contains("verifier"));
}

#[tokio::test]
async fn redeem_code_posts_the_verifier() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("client_id=client-123"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains("code_verifier=the-verifier"))
        .and(body_string_contains(
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A5555%2Fcallback",
        ))
        .and(body_string_contains("scope=XboxLive.signin+offline_access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tokens_body()))
        .expect(1)
        .mount(&server)
        .await;
    let tokens = client(&server)
        .redeem_code("the-code", "the-verifier", REDIRECT)
        .await
        .unwrap();
    assert_eq!(tokens.access_token, "fake-access");
    assert_eq!(tokens.refresh_token, "fake-refresh");
}

#[tokio::test]
async fn refresh_posts_the_refresh_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("client_id=client-123"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=old-refresh"))
        .and(body_string_contains("scope=XboxLive.signin+offline_access"))
        .respond_with(ResponseTemplate::new(200).set_body_json(tokens_body()))
        .expect(1)
        .mount(&server)
        .await;
    let tokens = client(&server).refresh("old-refresh").await.unwrap();
    assert_eq!(tokens.refresh_token, "fake-refresh");
}

#[tokio::test]
async fn invalid_grant_is_sign_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({ "error": "invalid_grant", "error_description": "AADSTS70008 expired" }),
        ))
        .mount(&server)
        .await;
    let err = client(&server).refresh("old-refresh").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
    assert_eq!(err.message, "Sign in with Microsoft again to keep playing.");
}

#[tokio::test]
async fn other_client_errors_are_sign_in_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({ "error": "invalid_client", "error_description": "secret detail" }),
        ))
        .mount(&server)
        .await;
    let err = client(&server)
        .redeem_code("c", "v", REDIRECT)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::SignInFailed);
    assert_eq!(
        err.message,
        "Couldn't sign you in with Microsoft. Try again."
    );
    assert_eq!(err.detail.as_deref(), Some("invalid_client"));
    assert!(!format!("{err:?}").contains("secret detail"));
}

#[tokio::test]
async fn unreadable_client_error_is_sign_in_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(403).set_body_string("<html>nope</html>"))
        .mount(&server)
        .await;
    let err = client(&server).refresh("r").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SignInFailed);
    assert!(err.detail.is_none());
}

#[tokio::test]
async fn rate_limited_is_its_own_code() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let err = client(&server).refresh("r").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftRateLimited);
    assert_eq!(
        err.message,
        "Microsoft is getting too many sign-ins right now. Try again in a minute."
    );
}

#[tokio::test]
async fn server_trouble_is_host_trouble() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let err = client(&server).refresh("r").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostTrouble);
}

#[tokio::test]
async fn a_dead_host_is_offline() {
    let server = MockServer::start().await;
    let mut e = endpoints(&server);
    e.token = Url::parse("http://127.0.0.1:1/token").unwrap();
    let http = launcher_core::http::client("test").unwrap();
    let err = MsaClient::new(http, e, CLIENT_ID.to_string())
        .refresh("r")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
}

#[tokio::test]
async fn a_200_without_a_refresh_token_is_unexpected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "access_token": "a" })))
        .mount(&server)
        .await;
    let err = client(&server).refresh("r").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
}

#[tokio::test]
async fn errors_never_carry_tokens() {
    let server = MockServer::start().await;
    for status in [400u16, 429, 500] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({
                "error": "invalid_grant",
                "access_token": "fake-access",
                "refresh_token": "fake-refresh"
            })))
            .mount(&server)
            .await;
        let err = client(&server)
            .redeem_code("code-secret", "verifier-secret", REDIRECT)
            .await
            .unwrap_err();
        let shown = format!("{err:?} {}", serde_json::to_string(&err).unwrap());
        for secret in [
            "fake-access",
            "fake-refresh",
            "code-secret",
            "verifier-secret",
        ] {
            assert!(!shown.contains(secret), "{status}: leaked {secret}");
        }
    }
}

#[test]
fn tokens_debug_is_redacted() {
    let t = launcher_core::msa::MsTokens {
        access_token: "fake-access".into(),
        refresh_token: "fake-refresh".into(),
    };
    let shown = format!("{t:?}");
    assert!(!shown.contains("fake-access") && !shown.contains("fake-refresh"));
}
