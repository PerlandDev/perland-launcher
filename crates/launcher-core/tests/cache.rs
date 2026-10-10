use std::time::Duration;

use launcher_core::{
    download::{cache::ObjectCache, hash::sha1_hex, Downloader, Hashes},
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn downloader() -> Downloader {
    Downloader::new(reqwest::Client::new()).with_backoff(Duration::from_millis(1))
}

async fn serve(server: &MockServer, at: &str, body: &str, times: u64) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .expect(times)
        .mount(server)
        .await;
}

#[tokio::test]
async fn ensure_downloads_once_then_serves_from_disk() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar", "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path());
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let urls = vec![format!("{}/a.jar", server.uri())];
    let first = cache
        .ensure(&downloader(), &urls, &hashes, Some(5))
        .await
        .unwrap();
    assert_eq!(
        first,
        dir.path()
            .join("aa")
            .join("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d")
    );
    let second = cache
        .ensure(&downloader(), &urls, &hashes, Some(5))
        .await
        .unwrap();
    assert_eq!(first, second);
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "hello");
    assert_eq!(cache.get(&hashes).await, Some(first));
}

#[tokio::test]
async fn a_damaged_object_is_downloaded_again() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar", "hello", 2).await;
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path());
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let urls = vec![format!("{}/a.jar", server.uri())];
    let path = cache
        .ensure(&downloader(), &urls, &hashes, Some(5))
        .await
        .unwrap();
    std::fs::write(&path, "jello").unwrap(); // same size, wrong bytes
    assert_eq!(
        cache.get(&hashes).await,
        None,
        "get never returns a damaged object"
    );
    cache
        .ensure(&downloader(), &urls, &hashes, Some(5))
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "hello");
}

#[tokio::test]
async fn insert_file_checks_before_copying() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path().join("objects"));
    let src = dir.path().join("download.jar");
    std::fs::write(&src, "jello").unwrap();
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let err = cache.insert_file(&src, &hashes).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::DownloadCorrupt);
    assert!(
        !dir.path().join("objects").exists(),
        "nothing written on a mismatch"
    );
    std::fs::write(&src, "hello").unwrap();
    let stored = cache.insert_file(&src, &hashes).await.unwrap();
    assert_eq!(cache.get(&hashes).await, Some(stored));
    assert!(src.exists(), "the player's file is copied, not moved");
}

#[test]
fn files_without_a_usable_hash_have_no_place() {
    let cache = ObjectCache::new("objects");
    assert_eq!(cache.path_for(&Hashes::default()), None);
    assert_eq!(cache.path_for(&Hashes::sha1("../../x")), None);
}
