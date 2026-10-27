//! Xbox Live user token and XSTS against wiremock endpoints. Fake tokens only.
use launcher_core::{
    invite::Url,
    msa::{
        xbox::{xerr_code, XboxUser, Xsts},
        MsaClient, MsaEndpoints,
    },
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

fn client(server: &MockServer) -> MsaClient {
    let base = Url::parse(&server.uri()).unwrap();
    let endpoints = MsaEndpoints {
        authorize: base.join("/authorize").unwrap(),
        token: base.join("/token").unwrap(),
        xbox_user: base.join("/xbox").unwrap(),
        xsts: base.join("/xsts").unwrap(),
        minecraft: base,
    };
    let http = launcher_core::http::client("test").unwrap();
    MsaClient::new(http, endpoints, "client-123".to_string())
}

fn user() -> XboxUser {
    XboxUser {
        token: "fake-xbox-token".to_string(),
        uhs: "fake-uhs".to_string(),
    }
}

#[tokio::test]
async fn user_token_sends_rps_ticket() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xbox"))
        .and(header("accept", "application/json"))
        .and(body_json(json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": "d=fake-ms-access"
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
    let got = client(&server).xbox_user("fake-ms-access").await.unwrap();
    assert_eq!(got.token, "fake-xbox-token");
    assert_eq!(got.uhs, "fake-uhs");
    assert_eq!(format!("{got:?}"), "XboxUser { .. }");
}

#[tokio::test]
async fn user_token_without_uhs_is_sign_in_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xbox"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Token": "fake-xbox-token",
            "DisplayClaims": { "xui": [] }
        })))
        .mount(&server)
        .await;
    let err = client(&server).xbox_user("x").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SignInFailed);
}

#[tokio::test]
async fn xsts_reads_uhs_and_xuid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .and(header("accept", "application/json"))
        .and(body_json(json!({
            "Properties": { "SandboxId": "RETAIL", "UserTokens": ["fake-xbox-token"] },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        })))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Token": "fake-xsts-token",
            "DisplayClaims": { "xui": [{ "uhs": "xsts-uhs", "xid": "2535000000000001" }] }
        })))
        .expect(1)
        .mount(&server)
        .await;
    let got: Xsts = client(&server).xsts(&user()).await.unwrap();
    assert_eq!(got.token, "fake-xsts-token");
    assert_eq!(got.uhs, "xsts-uhs");
    assert_eq!(got.xuid.as_deref(), Some("2535000000000001"));
    assert_eq!(format!("{got:?}"), "Xsts { .. }");
}

#[tokio::test]
async fn xsts_without_xid_has_no_xuid() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Token": "t", "DisplayClaims": { "xui": [{ "uhs": "u" }] }
        })))
        .mount(&server)
        .await;
    let got = client(&server).xsts(&user()).await.unwrap();
    assert_eq!(got.xuid, None);
}

#[tokio::test]
async fn xsts_401_maps_every_xerr() {
    let cases = [
        (2148916233u64, ErrorCode::XboxNoProfile),
        (2148916235, ErrorCode::XboxRegionBlocked),
        (2148916236, ErrorCode::XboxAgeVerification),
        (2148916237, ErrorCode::XboxAgeVerification),
        (2148916238, ErrorCode::XboxChildAccount),
        (1, ErrorCode::SignInFailed),
    ];
    for (xerr, code) in cases {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/xsts"))
            .respond_with(ResponseTemplate::new(401).set_body_json(json!({
                "Identity": "0", "XErr": xerr, "Message": "", "Redirect": "https://x"
            })))
            .mount(&server)
            .await;
        let err = client(&server).xsts(&user()).await.unwrap_err();
        assert_eq!(err.code, code, "XErr {xerr}");
    }
}

#[tokio::test]
async fn xsts_401_without_readable_body_is_sign_in_failed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .respond_with(ResponseTemplate::new(401).set_body_string("nope"))
        .mount(&server)
        .await;
    let err = client(&server).xsts(&user()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SignInFailed);
}

#[tokio::test]
async fn other_statuses_map_to_codes() {
    for (status, code) in [
        (403u16, ErrorCode::SignInFailed),
        (400, ErrorCode::SignInFailed),
        (429, ErrorCode::MicrosoftRateLimited),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let c = client(&server);
        assert_eq!(c.xbox_user("x").await.unwrap_err().code, code, "{status}");
        assert_eq!(c.xsts(&user()).await.unwrap_err().code, code, "{status}");
    }
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let err = client(&server).xsts(&user()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostTrouble);
}

#[test]
fn xerr_code_unknown_is_sign_in_failed() {
    assert_eq!(xerr_code(0), ErrorCode::SignInFailed);
    assert_eq!(xerr_code(2148916234), ErrorCode::SignInFailed);
    assert_eq!(xerr_code(u64::MAX), ErrorCode::SignInFailed);
}
