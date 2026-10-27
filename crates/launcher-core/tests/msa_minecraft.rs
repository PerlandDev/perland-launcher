//! Minecraft login, ownership, profile and the whole chain against wiremock. Fake tokens only.
use launcher_core::{
    invite::Url,
    msa::{
        minecraft::{dashed, ownership, McProfile, McToken},
        MsTokens, MsaClient, MsaEndpoints,
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

fn xsts() -> launcher_core::msa::xbox::Xsts {
    launcher_core::msa::xbox::Xsts {
        token: "fake-xsts".to_string(),
        uhs: "fake-uhs".to_string(),
        xuid: Some("fake-xuid".to_string()),
    }
}

fn mc() -> McToken {
    McToken {
        access_token: "fake-mc-token".to_string(),
        expires_at: 5000,
    }
}

const ID: &str = "069a79f444e94726a5befca90e38aaf5";

#[tokio::test]
async fn login_sets_expiry_from_now() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/authentication/login_with_xbox"))
        .and(header("accept", "application/json"))
        .and(body_json(
            json!({ "identityToken": "XBL3.0 x=fake-uhs;fake-xsts" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fake-mc-token", "expires_in": 86400
        })))
        .expect(1)
        .mount(&server)
        .await;
    let got = client(&server)
        .minecraft_login(&xsts(), 1000)
        .await
        .unwrap();
    assert_eq!(got.access_token, "fake-mc-token");
    assert_eq!(got.expires_at, 87400);
    assert!(!format!("{got:?}").contains("fake-mc-token"));
}

#[tokio::test]
async fn login_403_is_awaiting_approval() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    let err = client(&server)
        .minecraft_login(&xsts(), 0)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftAwaitingApproval);
    assert!(err.fix.is_none());
}

#[tokio::test]
async fn login_other_statuses_map_to_codes() {
    for (status, code) in [
        (429u16, ErrorCode::MicrosoftRateLimited),
        (401, ErrorCode::SignInFailed),
        (400, ErrorCode::SignInFailed),
        (503, ErrorCode::HostTrouble),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status))
            .mount(&server)
            .await;
        let err = client(&server)
            .minecraft_login(&xsts(), 0)
            .await
            .unwrap_err();
        assert_eq!(err.code, code, "{status}");
    }
}

#[tokio::test]
async fn login_without_token_is_unexpected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "", "expires_in": 10
        })))
        .mount(&server)
        .await;
    let err = client(&server)
        .minecraft_login(&xsts(), 0)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
}

#[tokio::test]
async fn entitlements_recognise_both_names() {
    for (name, owns) in [
        ("game_minecraft", true),
        ("product_minecraft", true),
        ("other", false),
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/entitlements/mcstore"))
            .and(header("authorization", "Bearer fake-mc-token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "items": [{ "name": "x" }, { "name": name }] })),
            )
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            client(&server).owns_game(&mc()).await.unwrap(),
            owns,
            "{name}"
        );
    }
}

#[tokio::test]
async fn entitlements_401_is_sign_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = client(&server).owns_game(&mc()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
}

#[tokio::test]
async fn profile_reads_id_and_name() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .and(header("authorization", "Bearer fake-mc-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "id": ID, "name": "Steve" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    let got = client(&server).profile(&mc()).await.unwrap().unwrap();
    assert_eq!(got.id, ID);
    assert_eq!(got.name, "Steve");
}

#[tokio::test]
async fn profile_404_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert!(client(&server).profile(&mc()).await.unwrap().is_none());
}

#[tokio::test]
async fn profile_401_is_sign_in_again() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let err = client(&server).profile(&mc()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
    assert_eq!(err.message, "Sign in with Microsoft again to keep playing.");
}

#[test]
fn ownership_decides_three_ways() {
    let p = McProfile {
        id: ID.to_string(),
        name: "Steve".to_string(),
    };
    assert_eq!(ownership(Some(p.clone()), false).unwrap().name, "Steve");
    assert_eq!(ownership(Some(p), true).unwrap().name, "Steve");
    assert_eq!(
        ownership(None, true).unwrap_err().code,
        ErrorCode::MinecraftNoProfile
    );
    assert_eq!(
        ownership(None, false).unwrap_err().code,
        ErrorCode::MinecraftNotOwned
    );
}

#[test]
fn dashed_formats_uuid() {
    assert_eq!(dashed(ID), "069a79f4-44e9-4726-a5be-fca90e38aaf5");
    assert_eq!(
        dashed("069A79F444E94726A5BEFCA90E38AAF5"),
        "069a79f4-44e9-4726-a5be-fca90e38aaf5"
    );
    // Not 32 hex characters: returned unchanged, never a panic.
    assert_eq!(dashed("short"), "short");
    assert_eq!(dashed(""), "");
    assert_eq!(dashed(&"g".repeat(32)), "g".repeat(32));
    assert_eq!(
        dashed("069a79f4-44e9-4726-a5be-fca90e38aaf5"),
        "069a79f4-44e9-4726-a5be-fca90e38aaf5"
    );
    assert_eq!(
        dashed("ééééééééééééééééééééééééééééééé"),
        "ééééééééééééééééééééééééééééééé"
    );
}

async fn mount_front(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/xbox"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Token": "fake-xbox-token",
            "DisplayClaims": { "xui": [{ "uhs": "fake-uhs" }] }
        })))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/xsts"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "Token": "fake-xsts",
            "DisplayClaims": { "xui": [{ "uhs": "fake-uhs", "xid": "fake-xuid" }] }
        })))
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path("/authentication/login_with_xbox"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "access_token": "fake-mc-token", "expires_in": 100
        })))
        .expect(1)
        .mount(server)
        .await;
}

fn ms() -> MsTokens {
    MsTokens {
        access_token: "fake-ms-access".to_string(),
        refresh_token: "fake-ms-refresh".to_string(),
    }
}

#[tokio::test]
async fn finish_runs_the_whole_chain() {
    let server = MockServer::start().await;
    mount_front(&server).await;
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "id": ID, "name": "Steve" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/entitlements/mcstore"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": [] })))
        .expect(0)
        .mount(&server)
        .await;
    let got = client(&server).finish(ms(), 10).await.unwrap();
    assert_eq!(got.profile.name, "Steve");
    assert_eq!(got.refresh_token, "fake-ms-refresh");
    assert_eq!(got.minecraft.access_token, "fake-mc-token");
    assert_eq!(got.minecraft.expires_at, 110);
    assert_eq!(got.xuid.as_deref(), Some("fake-xuid"));
    let debug = format!("{got:?}");
    assert!(!debug.contains("fake-"), "{debug}");
}

#[tokio::test]
async fn finish_asks_entitlements_only_without_profile() {
    for (items, code) in [
        (
            json!([{ "name": "game_minecraft" }]),
            ErrorCode::MinecraftNoProfile,
        ),
        (json!([]), ErrorCode::MinecraftNotOwned),
    ] {
        let server = MockServer::start().await;
        mount_front(&server).await;
        Mock::given(method("GET"))
            .and(path("/minecraft/profile"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/entitlements/mcstore"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "items": items })))
            .expect(1)
            .mount(&server)
            .await;
        let err = client(&server).finish(ms(), 10).await.unwrap_err();
        assert_eq!(err.code, code);
    }
}

#[tokio::test]
async fn finish_propagates_step_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/xbox"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let err = client(&server).finish(ms(), 10).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MicrosoftRateLimited);
}
