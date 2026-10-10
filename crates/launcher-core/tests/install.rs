use std::{path::Path, time::Duration};

use launcher_core::{
    download::{DownloadRequest, Hashes},
    install::fill_missing_sha1,
    meta::MetaClient,
    ErrorCategory,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const SHA_A: &str = "dc19ecb3f7889b7860697215cae99c0f9b6f6b4b";
const SHA_B: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";

fn meta(dir: &Path) -> MetaClient {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    MetaClient::new(http, dir)
}

fn unhashed(url: String, dir: &Path) -> DownloadRequest {
    DownloadRequest::new(url, dir.join("lib.jar"), Hashes::default(), None)
}

async fn serve(server: &MockServer, at: &str, status: u16, body: &str) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn missing_sha1_is_filled_from_sha1_file() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar.sha1", 200, &format!("{SHA_A}  a.jar\n")).await;
    serve(&server, "/b.jar.sha1", 200, &SHA_B.to_ascii_uppercase()).await;
    let dir = tempfile::tempdir().unwrap();
    let mut reqs = vec![
        unhashed(format!("{}/a.jar", server.uri()), dir.path()),
        unhashed(format!("{}/b.jar", server.uri()), dir.path()),
    ];
    fill_missing_sha1(&meta(&dir.path().join("meta")), &mut reqs)
        .await
        .unwrap();
    assert_eq!(reqs[0].hashes.sha1.as_deref(), Some(SHA_A));
    assert_eq!(reqs[1].hashes.sha1.as_deref(), Some(SHA_B));
}

#[tokio::test]
async fn malformed_sha1_file_is_integrity_error() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar.sha1", 200, "<html>not a hash</html>").await;
    let dir = tempfile::tempdir().unwrap();
    let url = format!("{}/a.jar", server.uri());
    let mut reqs = vec![unhashed(url.clone(), dir.path())];
    let err = fill_missing_sha1(&meta(&dir.path().join("meta")), &mut reqs)
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Integrity);
    assert_eq!(err.message, "Couldn't verify a library download");
    assert_eq!(err.fix.as_deref(), Some("Try again later"));
    assert_eq!(err.detail.as_deref(), Some(url.as_str()));
}

#[tokio::test]
async fn absent_sha1_file_is_integrity_error() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar.sha1", 404, "").await;
    let dir = tempfile::tempdir().unwrap();
    let mut reqs = vec![unhashed(format!("{}/a.jar", server.uri()), dir.path())];
    let err = fill_missing_sha1(&meta(&dir.path().join("meta")), &mut reqs)
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Integrity);
}

#[tokio::test]
async fn offline_without_cache_stays_a_network_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut reqs = vec![unhashed("http://127.0.0.1:9/a.jar".into(), dir.path())];
    let err = fill_missing_sha1(&meta(&dir.path().join("meta")), &mut reqs)
        .await
        .unwrap_err();
    assert_eq!(err.category, ErrorCategory::Network);
}

#[tokio::test]
async fn published_sha1_needs_no_lookup() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().unwrap();
    let mut reqs = vec![DownloadRequest::new(
        format!("{}/a.jar", server.uri()),
        dir.path().join("a.jar"),
        Hashes::sha1(SHA_A),
        Some(1),
    )];
    fill_missing_sha1(&meta(&dir.path().join("meta")), &mut reqs)
        .await
        .unwrap();
    assert_eq!(reqs[0].hashes.sha1.as_deref(), Some(SHA_A));
    assert!(server.received_requests().await.unwrap().is_empty());
}
