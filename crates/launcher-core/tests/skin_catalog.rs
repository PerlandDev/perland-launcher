//! The skin catalog client against a wiremock Engine, which is also the Perland site.
use image::{ImageFormat, RgbaImage};
use launcher_core::{
    invite::{InviteClient, Url},
    public_list::site_url,
    skin_catalog::{CatalogClient, Fetched, FORMAT_VERSION},
    ErrorCode,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const ROUTE: &str = "/api/v1/skin-catalog";

fn site(server: &MockServer) -> Url {
    site_url(&server.uri()).unwrap()
}

fn client(server: &MockServer) -> CatalogClient {
    let http = launcher_core::http::client("test").unwrap();
    CatalogClient::new(InviteClient::new(http), site(server))
}

async fn mount_config(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })),
        )
        .mount(server)
        .await;
}

fn manifest_json() -> Value {
    serde_json::from_str(include_str!("fixtures/skin-catalog-manifest.json")).unwrap()
}

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    RgbaImage::new(w, h)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

async fn mount_file(server: &MockServer, name: &str, body: Vec<u8>) {
    Mock::given(method("GET"))
        .and(path(format!("{ROUTE}/files/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn manifest_is_read_from_the_envelope() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(manifest_json())))
        .mount(&server)
        .await;
    match client(&server).manifest(None).await.unwrap() {
        Fetched::Manifest(m) => {
            assert_eq!(m.format_version, FORMAT_VERSION);
            assert_eq!(m.pieces.len(), 22);
            assert_eq!(m.looks.len(), 3);
            assert!(m.hashes().len() > 40);
            assert_eq!(m.looks[2].painting, None);
        }
        other => panic!("expected a manifest, got {other:?}"),
    }
}

#[tokio::test]
async fn manifest_sends_if_none_match_and_reads_304() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;
    assert_eq!(
        client(&server).manifest(Some("v1")).await.unwrap(),
        Fetched::NotModified
    );
}

#[tokio::test]
async fn manifest_over_one_mib_is_refused() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![b' '; (1 << 20) + 1]))
        .mount(&server)
        .await;
    let err = client(&server).manifest(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SkinCatalogUnavailable);
}

#[tokio::test]
async fn file_must_hash_to_its_name() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let name = sha(&png(64, 64));
    mount_file(&server, &name, png(48, 96)).await;
    let err = client(&server).file(&name).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SkinCatalogUnavailable);
    assert_eq!(err.params.len(), 1);
    assert!(err.params.contains_key("host"));
}

#[tokio::test]
async fn file_must_be_a_png_within_64_kib() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let c = client(&server);
    // The two sizes the Engine serves.
    for (w, h) in [(64, 64), (48, 96)] {
        let bytes = png(w, h);
        mount_file(&server, &sha(&bytes), bytes.clone()).await;
        assert_eq!(c.file(&sha(&bytes)).await.unwrap(), bytes);
    }
    // A PNG of another size.
    let odd = png(32, 32);
    mount_file(&server, &sha(&odd), odd.clone()).await;
    assert_eq!(
        c.file(&sha(&odd)).await.unwrap_err().code,
        ErrorCode::SkinCatalogUnavailable
    );
    // Not a PNG, though it hashes to its name.
    let text = b"hello".to_vec();
    mount_file(&server, &sha(&text), text.clone()).await;
    assert_eq!(
        c.file(&sha(&text)).await.unwrap_err().code,
        ErrorCode::SkinCatalogUnavailable
    );
    // Over 64 KiB.
    let mut big = png(64, 64);
    big.extend(vec![0u8; 65 << 10]);
    mount_file(&server, &sha(&big), big.clone()).await;
    assert_eq!(
        c.file(&sha(&big)).await.unwrap_err().code,
        ErrorCode::SkinCatalogUnavailable
    );
}

#[tokio::test]
async fn bad_hash_is_refused_before_any_request() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let c = client(&server);
    let good = sha(b"x");
    for bad in [
        "../x".to_string(),
        good.to_uppercase(),
        good[..63].to_string(),
        String::new(),
    ] {
        let err = c.file(&bad).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::SkinCatalogUnavailable, "{bad}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn engine_down_is_unavailable_naming_the_host() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let err = client(&server).manifest(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SkinCatalogUnavailable);
    let host = Url::parse(&server.uri()).unwrap();
    assert_eq!(err.params.get("host").map(String::as_str), host.host_str());
    assert_eq!(err.params.len(), 1);
}

async fn mount_manifest(server: &MockServer, data: Value) {
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(data)))
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_newer_format_is_too_new_even_when_its_shape_changed() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    // A future manifest that no longer parses as this launcher's format.
    mount_manifest(
        &server,
        json!({ "formatVersion": FORMAT_VERSION + 1, "version": "v9", "everything": "else" }),
    )
    .await;
    assert_eq!(
        client(&server).manifest(None).await.unwrap(),
        Fetched::TooNew
    );
}

#[tokio::test]
async fn a_newer_format_that_still_parses_is_too_new() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let mut data = manifest_json();
    data["formatVersion"] = json!(FORMAT_VERSION + 1);
    mount_manifest(&server, data).await;
    assert_eq!(
        client(&server).manifest(None).await.unwrap(),
        Fetched::TooNew
    );
}

#[tokio::test]
async fn a_manifest_naming_more_than_4000_files_is_refused() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    let mut data = manifest_json();
    // Small looks, so the manifest stays under the size cap: only the file count is over.
    let looks: Vec<Value> = (0..4001)
        .map(|i| {
            json!({ "id": format!("l{i}"), "name": { "en": "", "es": "" }, "tags": [],
                "model": "classic", "pieces": [], "thumb": format!("{i:064x}"), "retired": false })
        })
        .collect();
    data["looks"] = Value::Array(looks);
    assert!(serde_json::to_vec(&envelope(data.clone())).unwrap().len() < 1 << 20);
    mount_manifest(&server, data).await;
    let err = client(&server).manifest(None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SkinCatalogUnavailable);
}

#[tokio::test]
async fn a_manifest_naming_4000_files_or_fewer_is_read() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_manifest(&server, manifest_json()).await;
    assert!(matches!(
        client(&server).manifest(None).await.unwrap(),
        Fetched::Manifest(_)
    ));
}
