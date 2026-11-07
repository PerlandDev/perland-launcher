//! The Minecraft Launcher's public client on login.live.com: its authorize URL, reading the
//! code off the desktop redirect, redeeming and refreshing it, and the Xbox ticket it sends.
//! Fake tokens only.
use launcher_core::{
    invite::Url,
    msa::{
        oauth::{is_live_redirect, live_callback, LIVE_REDIRECT},
        MsaApp, MsaClient, MsaEndpoints, LIVE_CLIENT_ID,
    },
    perland::loopback::Callback,
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{body_json, body_string_contains, method, path},
    Mock, MockServer, ResponseTemplate,
};

const SCOPE_FORM: &str = "scope=service%3A%3Auser.auth.xboxlive.com%3A%3AMBI_SSL";
const REDIRECT_FORM: &str = "redirect_uri=https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf";

fn endpoints(server: &MockServer) -> MsaEndpoints {
    let base = Url::parse(&server.uri()).unwrap();
    MsaEndpoints {
        authorize: base.join("/authorize").unwrap(),
        token: base.join("/token").unwrap(),
        xbox_user: base.join("/xbox").unwrap(),
        xsts: base.join("/xsts").unwrap(),
        live_authorize: base.join("/live/authorize").unwrap(),
        live_token: base.join("/live/token").unwrap(),
        minecraft: base,
    }
}

fn live(server: &MockServer) -> MsaClient {
    let http = launcher_core::http::client("test").unwrap();
    MsaClient::new(http, endpoints(server), MsaApp::Live)
}

fn tokens_body() -> serde_json::Value {
    json!({ "access_token": "fake-access", "refresh_token": "fake-refresh", "expires_in": 86400 })
}

#[test]
fn the_live_client_is_the_minecraft_launchers() {
    assert_eq!(LIVE_CLIENT_ID, "00000000402b5328");
    assert_eq!(MsaApp::Live.client_id(), LIVE_CLIENT_ID);
    assert_eq!(MsaApp::Azure("abc".into()).client_id(), "abc");
    assert_eq!(LIVE_REDIRECT, "https://login.live.com/oauth20_desktop.srf");
}

#[test]
fn production_live_endpoints_are_login_live_com() {
    let e = MsaEndpoints::production();
    assert_eq!(
        e.live_authorize.as_str(),
        "https://login.live.com/oauth20_authorize.srf"
    );
    assert_eq!(
        e.live_token.as_str(),
        "https://login.live.com/oauth20_token.srf"
    );
}

#[test]
fn live_authorize_url_is_the_documented_one() {
    let http = launcher_core::http::client("test").unwrap();
    let c = MsaClient::new(http, MsaEndpoints::production(), MsaApp::Live);
    assert_eq!(
        c.live_authorize_url().as_str(),
        "https://login.live.com/oauth20_authorize.srf?client_id=00000000402b5328\
         &response_type=code&scope=service%3A%3Auser.auth.xboxlive.com%3A%3AMBI_SSL\
         &redirect_uri=https%3A%2F%2Flogin.live.com%2Foauth20_desktop.srf"
    );
}

#[test]
fn only_the_desktop_redirect_counts() {
    for yes in [
        "https://login.live.com/oauth20_desktop.srf?code=M.C1_abc&lc=1033",
        "https://login.live.com/oauth20_desktop.srf?error=access_denied",
        "https://LOGIN.live.com/oauth20_desktop.srf?code=x",
        "https://login.live.com/oauth20_desktop.srf",
    ] {
        assert!(is_live_redirect(yes), "{yes}");
    }
    for no in [
        "https://login.live.com/oauth20_authorize.srf?client_id=00000000402b5328",
        "http://login.live.com/oauth20_desktop.srf?code=x",
        "https://login.live.com.evil.example/oauth20_desktop.srf?code=x",
        "https://evil.example/oauth20_desktop.srf?code=x",
        "https://login.live.com/oauth20_desktop.srf.evil?code=x",
        "https://login.live.com:8443/oauth20_desktop.srf?code=x",
        "https://user@login.live.com/oauth20_desktop.srf?code=x",
        "not a url",
        "",
    ] {
        assert!(!is_live_redirect(no), "{no}");
        assert!(live_callback(no).is_none(), "{no}");
    }
}

#[test]
fn the_code_is_read_off_the_redirect() {
    let got = live_callback("https://login.live.com/oauth20_desktop.srf?code=M.C1_abc-1&lc=1033");
    assert!(matches!(got, Some(Callback::Code(c)) if c == "M.C1_abc-1"));
}

#[test]
fn a_refusal_on_the_redirect_is_read() {
    let denied = live_callback(
        "https://login.live.com/oauth20_desktop.srf?error=access_denied&error_description=The+user+declined",
    );
    assert!(matches!(denied, Some(Callback::Denied)));
    let failed = live_callback(
        "https://login.live.com/oauth20_desktop.srf?error=server_error&error_description=secret",
    );
    assert!(matches!(failed, Some(Callback::Failed(e)) if e == "server_error"));
    // An odd error value isn't passed on.
    let odd = live_callback("https://login.live.com/oauth20_desktop.srf?error=%3Cscript%3E");
    assert!(matches!(odd, Some(Callback::Failed(e)) if e == "unknown"));
    // Neither a code nor an error: Microsoft's page broke.
    let empty = live_callback("https://login.live.com/oauth20_desktop.srf?lc=1033");
    assert!(matches!(empty, Some(Callback::Failed(e)) if e == "no_code"));
    let blank = live_callback("https://login.live.com/oauth20_desktop.srf?code=");
    assert!(matches!(blank, Some(Callback::Failed(e)) if e == "no_code"));
}

#[tokio::test]
async fn redeeming_posts_no_secret_and_no_verifier() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/live/token"))
        .and(body_string_contains("client_id=00000000402b5328"))
        .and(body_string_contains("grant_type=authorization_code"))
        .and(body_string_contains("code=the-code"))
        .and(body_string_contains(REDIRECT_FORM))
        .and(body_string_contains(SCOPE_FORM))
        .respond_with(ResponseTemplate::new(200).set_body_json(tokens_body()))
        .expect(1)
        .mount(&server)
        .await;
    let tokens = live(&server).redeem_live_code("the-code").await.unwrap();
    assert_eq!(tokens.access_token, "fake-access");
    assert_eq!(tokens.refresh_token, "fake-refresh");
    let body =
        String::from_utf8(server.received_requests().await.unwrap()[0].body.clone()).unwrap();
    assert!(
        !body.contains("code_verifier") && !body.contains("client_secret"),
        "{body}"
    );
}

#[tokio::test]
async fn a_live_refresh_goes_to_login_live_com() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/live/token"))
        .and(body_string_contains("client_id=00000000402b5328"))
        .and(body_string_contains("grant_type=refresh_token"))
        .and(body_string_contains("refresh_token=old-refresh"))
        .and(body_string_contains(REDIRECT_FORM))
        .and(body_string_contains(SCOPE_FORM))
        .respond_with(ResponseTemplate::new(200).set_body_json(tokens_body()))
        .expect(1)
        .mount(&server)
        .await;
    let tokens = live(&server).refresh("old-refresh").await.unwrap();
    assert_eq!(tokens.refresh_token, "fake-refresh");
}

#[tokio::test]
async fn an_expired_live_refresh_is_sign_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/live/token"))
        .respond_with(ResponseTemplate::new(400).set_body_json(
            json!({ "error": "invalid_grant", "error_description": "The token has expired." }),
        ))
        .mount(&server)
        .await;
    let err = live(&server).refresh("old-refresh").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
}

#[tokio::test]
async fn live_errors_never_carry_tokens() {
    let server = MockServer::start().await;
    for status in [400u16, 429, 500] {
        server.reset().await;
        Mock::given(method("POST"))
            .and(path("/live/token"))
            .respond_with(ResponseTemplate::new(status).set_body_json(json!({
                "error": "invalid_request",
                "access_token": "fake-access",
                "refresh_token": "fake-refresh"
            })))
            .mount(&server)
            .await;
        let err = live(&server)
            .redeem_live_code("code-secret")
            .await
            .unwrap_err();
        let shown = format!("{err:?} {}", serde_json::to_string(&err).unwrap());
        for secret in ["fake-access", "fake-refresh", "code-secret"] {
            assert!(!shown.contains(secret), "{status}: leaked {secret}");
        }
    }
}

#[tokio::test]
async fn the_live_ticket_has_no_d_prefix() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xbox"))
        .and(body_json(json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": "fake-ms-access"
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Token": "fake-xbox-token",
            "DisplayClaims": { "xui": [{ "uhs": "fake-uhs" }] }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let user = live(&server).xbox_user("fake-ms-access").await.unwrap();
    assert_eq!(user.token, "fake-xbox-token");
}

#[test]
fn the_app_is_azure_only_with_a_client_id() {
    use launcher_core::{accounts::BuildKind, msa::app};
    const ID: &str = "11111111-2222-3333-4444-555555555555";
    assert_eq!(
        app(None, Some(ID), BuildKind::Release),
        MsaApp::Azure(ID.into())
    );
    assert_eq!(
        app(Some(ID), None, BuildKind::Dev),
        MsaApp::Azure(ID.into())
    );
    for build in [BuildKind::Dev, BuildKind::Beta, BuildKind::Release] {
        assert_eq!(app(None, None, build), MsaApp::Live);
        assert_eq!(app(None, Some(""), build), MsaApp::Live);
    }
    // A runtime value is a dev build's only.
    assert_eq!(app(Some(ID), None, BuildKind::Release), MsaApp::Live);
}
