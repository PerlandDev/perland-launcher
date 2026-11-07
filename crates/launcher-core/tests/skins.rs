//! Fetching a skin texture from the session and texture servers, against wiremock.
use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    invite::Url,
    skins::{SkinClient, SkinEndpoints, TEXTURE_CAP},
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const UUID: &str = "069a79f4-44e9-4726-a5be-fca90e38aaf5";
const ID: &str = "069a79f444e94726a5befca90e38aaf5";
const HASH: &str = "292009a4925b58f02c77dadc3ecef07ea4c7472f64e0fdc32ce5522489362680";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\nfake skin";

fn client(server: &MockServer) -> SkinClient {
    let base = Url::parse(&server.uri()).unwrap();
    let endpoints = SkinEndpoints {
        session: base.clone(),
        textures: base,
    };
    SkinClient::new(
        launcher_core::http::client_without_redirects("test").unwrap(),
        endpoints,
    )
}

fn profile(textures: serde_json::Value) -> serde_json::Value {
    json!({
        "id": ID,
        "name": "Notch",
        "properties": [{ "name": "textures", "value": STANDARD.encode(textures.to_string()) }],
    })
}

async fn serve_profile(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(format!("/session/minecraft/profile/{ID}")))
        .respond_with(answer)
        .mount(server)
        .await;
}

fn wearing_a_skin() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(profile(json!({
        "textures": { "SKIN": { "url": format!("http://textures.minecraft.net/texture/{HASH}") } }
    })))
}

#[tokio::test]
async fn the_skin_comes_from_the_texture_server_by_its_hash() {
    let server = MockServer::start().await;
    serve_profile(&server, wearing_a_skin()).await;
    Mock::given(method("GET"))
        .and(path(format!("/texture/{HASH}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG))
        .expect(1)
        .mount(&server)
        .await;
    let texture = client(&server).texture(UUID).await.unwrap();
    assert_eq!(texture.as_deref(), Some(PNG));
}

#[tokio::test]
async fn no_skin_of_its_own_is_none() {
    let server = MockServer::start().await;
    serve_profile(
        &server,
        ResponseTemplate::new(200).set_body_json(profile(json!({ "textures": {} }))),
    )
    .await;
    assert_eq!(client(&server).texture(UUID).await.unwrap(), None);
}

#[tokio::test]
async fn an_unknown_profile_is_none() {
    for status in [204, 404] {
        let server = MockServer::start().await;
        serve_profile(&server, ResponseTemplate::new(status)).await;
        assert_eq!(
            client(&server).texture(UUID).await.unwrap(),
            None,
            "{status}"
        );
    }
}

#[tokio::test]
async fn a_texture_that_is_gone_is_none() {
    let server = MockServer::start().await;
    serve_profile(&server, wearing_a_skin()).await;
    Mock::given(method("GET"))
        .and(path(format!("/texture/{HASH}")))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert_eq!(client(&server).texture(UUID).await.unwrap(), None);
}

#[tokio::test]
async fn trouble_is_an_error_so_the_last_skin_stays() {
    let server = MockServer::start().await;
    serve_profile(&server, ResponseTemplate::new(429)).await;
    let error = client(&server).texture(UUID).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::HostTrouble);

    let server = MockServer::start().await;
    serve_profile(
        &server,
        ResponseTemplate::new(200).set_body_string("not json"),
    )
    .await;
    let error = client(&server).texture(UUID).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::HostTrouble);

    let server = MockServer::start().await;
    serve_profile(&server, wearing_a_skin()).await;
    Mock::given(method("GET"))
        .and(path(format!("/texture/{HASH}")))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let error = client(&server).texture(UUID).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::HostTrouble);
}

#[tokio::test]
async fn a_texture_that_isnt_a_png_or_is_huge_is_an_error() {
    for body in [b"<html>".to_vec(), [PNG, &vec![0u8; TEXTURE_CAP]].concat()] {
        let server = MockServer::start().await;
        serve_profile(&server, wearing_a_skin()).await;
        Mock::given(method("GET"))
            .and(path(format!("/texture/{HASH}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&server)
            .await;
        let error = client(&server).texture(UUID).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::HostTrouble);
    }
}

#[tokio::test]
async fn a_strange_uuid_never_reaches_the_network() {
    let server = MockServer::start().await;
    assert_eq!(client(&server).texture("../steve").await.unwrap(), None);
    assert!(server.received_requests().await.unwrap().is_empty());
}
