//! Reading the active skin and uploading one, against wiremock. Fake tokens only.
use launcher_core::{
    invite::Url,
    msa::{minecraft::McToken, MsaApp, MsaClient, MsaEndpoints},
    skins_library::SkinModel,
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{header, method, path},
    Match, Mock, MockServer, Request, ResponseTemplate,
};

/// The body holds `needle` as bytes (a multipart body with a PNG isn't valid UTF-8).
struct BodyHas(&'static str);

impl Match for BodyHas {
    fn matches(&self, request: &Request) -> bool {
        request
            .body
            .windows(self.0.len())
            .any(|w| w == self.0.as_bytes())
    }
}

fn client(server: &MockServer) -> MsaClient {
    let base = Url::parse(&server.uri()).unwrap();
    let endpoints = MsaEndpoints {
        authorize: base.join("/authorize").unwrap(),
        token: base.join("/token").unwrap(),
        xbox_user: base.join("/xbox").unwrap(),
        xsts: base.join("/xsts").unwrap(),
        live_authorize: base.join("/live/authorize").unwrap(),
        live_token: base.join("/live/token").unwrap(),
        minecraft: base.clone(),
        textures: base,
    };
    let http = launcher_core::http::client("test").unwrap();
    MsaClient::new(http, endpoints, MsaApp::Live)
}

fn mc() -> McToken {
    McToken {
        access_token: "fake-mc-token".to_string(),
        expires_at: 5000,
    }
}

fn profile(server: &MockServer, skins: serde_json::Value) -> serde_json::Value {
    let _ = server;
    json!({ "id": "069a79f444e94726a5befca90e38aaf5", "name": "Builder", "skins": skins, "capes": [] })
}

fn skin_png() -> Vec<u8> {
    let image = image::RgbaImage::new(64, 64);
    launcher_core::skins_png::encode(&image).unwrap()
}

#[tokio::test]
async fn active_skin_reads_slim_variant() {
    let server = MockServer::start().await;
    let texture = format!("{}/texture/abc", server.uri());
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .and(header("authorization", "Bearer fake-mc-token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(profile(
            &server,
            json!([
                { "id": "1", "state": "INACTIVE", "url": "http://nope.invalid/x", "variant": "CLASSIC" },
                { "id": "2", "state": "ACTIVE", "url": texture, "variant": "SLIM" }
            ]),
        )))
        .expect(1)
        .mount(&server)
        .await;
    let active = client(&server).active_skin(&mc()).await.unwrap().unwrap();
    assert_eq!(active.model, SkinModel::Slim);
    assert_eq!(active.url, texture);
}

#[tokio::test]
async fn no_skins_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/minecraft/profile"))
        .respond_with(ResponseTemplate::new(200).set_body_json(profile(&server, json!([]))))
        .mount(&server)
        .await;
    assert!(client(&server).active_skin(&mc()).await.unwrap().is_none());
}

#[tokio::test]
async fn upload_sends_multipart_with_bearer() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/minecraft/profile/skins"))
        .and(header("authorization", "Bearer fake-mc-token"))
        .and(BodyHas("name=\"variant\""))
        .and(BodyHas("slim"))
        .and(BodyHas("name=\"file\"; filename=\"skin.png\""))
        .and(BodyHas("image/png"))
        .respond_with(ResponseTemplate::new(200).set_body_json(profile(&server, json!([]))))
        .expect(1)
        .mount(&server)
        .await;
    client(&server)
        .upload_skin(&mc(), skin_png(), SkinModel::Slim)
        .await
        .unwrap();
}

async fn upload_status(status: u16) -> ErrorCode {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/minecraft/profile/skins"))
        .respond_with(ResponseTemplate::new(status).set_body_json(json!({
            "errorMessage": "Could not validate image data."
        })))
        .mount(&server)
        .await;
    let e = client(&server)
        .upload_skin(&mc(), skin_png(), SkinModel::Classic)
        .await
        .unwrap_err();
    assert!(!format!("{e:?}").contains("fake-mc-token"));
    e.code
}

#[tokio::test]
async fn upload_400_is_skin_rejected() {
    assert_eq!(upload_status(400).await, ErrorCode::SkinRejected);
}

#[tokio::test]
async fn upload_401_is_sign_in_again() {
    assert_eq!(upload_status(401).await, ErrorCode::MicrosoftSignInAgain);
}

#[tokio::test]
async fn upload_429_is_rate_limited() {
    assert_eq!(upload_status(429).await, ErrorCode::MicrosoftRateLimited);
}

#[tokio::test]
async fn upload_500_is_host_trouble() {
    assert_eq!(upload_status(500).await, ErrorCode::HostTrouble);
}
