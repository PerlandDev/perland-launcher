use std::time::Duration;

use launcher_core::{download::hash::sha1_hex, meta::MetaClient, ErrorCategory};
use serde::Deserialize;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[derive(Debug, Deserialize, PartialEq)]
struct A {
    a: u32,
}

const BODY: &str = "{\"a\":1}";

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap()
}

async fn serve_with_etag(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/m.json"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("ETag", "\"v1\"")
                .set_body_string(BODY),
        )
        .up_to_n_times(1)
        .expect(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn caches_and_revalidates_with_etag() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    Mock::given(method("GET"))
        .and(path("/m.json"))
        .and(header("if-none-match", "\"v1\""))
        .respond_with(ResponseTemplate::new(304))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn offline_uses_cached_copy() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    meta.get_bytes(&url).await.unwrap();
    drop(server);
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn offline_without_cache_is_network_error() {
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta
        .get_bytes("http://127.0.0.1:9/nothing.json")
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Network);
    assert!(err.fix.is_some());
}

#[tokio::test]
async fn verified_cache_hit_needs_no_network() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    let sha = sha1_hex(BODY.as_bytes());
    assert_eq!(
        meta.get_verified_json::<A>(&url, &sha).await.unwrap(),
        A { a: 1 }
    );
    // The server stays up: the second call must be answered by the sha1-verified cache alone.
    assert_eq!(
        meta.get_verified_json::<A>(&url, &sha).await.unwrap(),
        A { a: 1 }
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_200_does_not_overwrite_good_cache() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    Mock::given(method("GET"))
        .and(path("/m.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>captive portal</html>"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let url = format!("{}/m.json", server.uri());
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
    drop(server);
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn verified_mismatch_is_integrity_error() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta
        .get_verified_json::<A>(
            &format!("{}/m.json", server.uri()),
            "0000000000000000000000000000000000000000",
        )
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Integrity);
}

#[tokio::test]
async fn bad_json_is_loader_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/bad.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta
        .get_json::<A>(&format!("{}/bad.json", server.uri()))
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Loader);
}

#[tokio::test]
async fn unwritable_cache_does_not_fail_a_good_fetch() {
    let server = MockServer::start().await;
    serve_with_etag(&server).await;
    let dir = tempfile::tempdir().unwrap();
    // The cache "folder" is a regular file, so every cache write fails.
    let not_a_dir = dir.path().join("meta");
    std::fs::write(&not_a_dir, "x").unwrap();
    let meta = MetaClient::new(http(), &not_a_dir);
    let url = format!("{}/m.json", server.uri());
    assert_eq!(meta.get_json::<A>(&url).await.unwrap(), A { a: 1 });
}

#[tokio::test]
async fn client_error_without_cache_is_loader_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/missing.json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta
        .get_bytes(&format!("{}/missing.json", server.uri()))
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Loader);
    assert!(err.message.contains("404"), "{}", err.message);
    assert_eq!(err.fix.as_deref(), Some("Check that this version exists"));
}

#[tokio::test]
async fn server_error_without_cache_is_network_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/down.json"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let meta = MetaClient::new(http(), dir.path());
    let err = meta
        .get_bytes(&format!("{}/down.json", server.uri()))
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Network);
    assert_eq!(err.fix.as_deref(), Some("Try again in a few minutes"));
}
