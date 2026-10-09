use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use launcher_core::{
    download::{DownloadRequest, Downloader, ExistingCheck, Hashes},
    events::{Event, EventSink},
    ErrorCategory,
};
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const HELLO_SHA1: &str = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d";
const HELLO_WORLD_SHA1: &str = "2aae6c35c94fcfb415dbe95f408b9ce91ee846ed";

fn downloader() -> Downloader {
    Downloader::new(reqwest::Client::new()).with_backoff(Duration::from_millis(1))
}

fn part_of(dest: &Path) -> PathBuf {
    PathBuf::from(format!("{}.part", dest.display()))
}

async fn serve(server: &MockServer, at: &str, status: u16, body: &str, times: u64) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn downloads_and_verifies() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("a/b/hello.txt");
    let req = DownloadRequest::new(
        format!("{}/hello.txt", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_SHA1),
        Some(5),
    );
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
    assert!(!part_of(&dest).exists());
}

#[tokio::test]
async fn skips_existing_file_with_matching_size() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 0).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    std::fs::write(&dest, "hello").unwrap();
    let req = DownloadRequest::new(
        format!("{}/hello.txt", server.uri()),
        dest,
        Hashes::sha1(HELLO_SHA1),
        Some(5),
    );
    downloader().fetch(&req).await.unwrap();
}

#[tokio::test]
async fn hash_mode_redownloads_same_size_wrong_content() {
    let server = MockServer::start().await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    std::fs::write(&dest, "jello").unwrap();
    let req = DownloadRequest::new(
        format!("{}/hello.txt", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_SHA1),
        Some(5),
    );
    downloader()
        .with_existing_check(ExistingCheck::Hash)
        .fetch(&req)
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn falls_back_to_next_url_on_404() {
    let server = MockServer::start().await;
    serve(&server, "/missing", 404, "", 1).await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let mut req = DownloadRequest::new(
        format!("{}/missing", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_SHA1),
        None,
    );
    req.urls.push(format!("{}/hello.txt", server.uri()));
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn retries_server_errors() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    serve(&server, "/flaky", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let req = DownloadRequest::new(
        format!("{}/flaky", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_SHA1),
        None,
    );
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn persistent_hash_mismatch_is_integrity_error() {
    let server = MockServer::start().await;
    serve(&server, "/bad", 200, "jello", 2).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    let req = DownloadRequest::new(
        format!("{}/bad", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_SHA1),
        None,
    );
    let err = downloader().fetch(&req).await.unwrap_err();
    assert_eq!(err.category, ErrorCategory::Integrity);
    assert!(err.fix.is_some());
    assert!(!dest.exists());
    assert!(!part_of(&dest).exists());
}

#[tokio::test]
async fn resumes_partial_download_with_range() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hw.txt"))
        .and(header("range", "bytes=5-"))
        .respond_with(ResponseTemplate::new(206).set_body_string(" world"))
        .expect(1)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hw.txt");
    std::fs::write(part_of(&dest), "hello").unwrap();
    let req = DownloadRequest::new(
        format!("{}/hw.txt", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_WORLD_SHA1),
        Some(11),
    );
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello world");
}

#[tokio::test]
async fn stale_part_file_self_heals() {
    // A .part left over from an older version of this file: resuming it gives garbage, so it must be discarded.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello.txt"))
        .and(header("range", "bytes=3-"))
        .respond_with(ResponseTemplate::new(206).set_body_string("lo"))
        .expect(1)
        .mount(&server)
        .await;
    serve(&server, "/hello.txt", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("hello.txt");
    std::fs::write(part_of(&dest), "OLD").unwrap();
    let req = DownloadRequest::new(
        format!("{}/hello.txt", server.uri()),
        dest.clone(),
        Hashes::sha1(HELLO_SHA1),
        None,
    );
    downloader().fetch(&req).await.unwrap();
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "hello");
}

#[tokio::test]
async fn fetch_all_dedupes_destinations() {
    let server = MockServer::start().await;
    serve(&server, "/x", 200, "hello", 1).await;
    serve(&server, "/y", 200, "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.txt");
    let b = dir.path().join("b.txt");
    let reqs = vec![
        DownloadRequest::new(
            format!("{}/x", server.uri()),
            a.clone(),
            Hashes::sha1(HELLO_SHA1),
            Some(5),
        ),
        DownloadRequest::new(
            format!("{}/x", server.uri()),
            a.clone(),
            Hashes::sha1(HELLO_SHA1),
            Some(5),
        ),
        DownloadRequest::new(
            format!("{}/y", server.uri()),
            b.clone(),
            Hashes::sha1(HELLO_SHA1),
            Some(5),
        ),
    ];
    let events = Arc::new(Mutex::new(Vec::new()));
    let e2 = events.clone();
    let sink: EventSink = Arc::new(move |e| e2.lock().unwrap().push(e));
    downloader().fetch_all(reqs, "Test", &sink).await.unwrap();
    assert!(a.exists() && b.exists());
    let last = events.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        last,
        Event::Progress {
            task: "Test".into(),
            done: 2,
            total: 2,
            bytes: 10
        }
    );
}
