use std::time::{Duration, SystemTime};

use launcher_core::{
    download::{
        cache::{CacheScope, ObjectCache},
        hash::{curseforge_murmur2, sha1_hex},
        Downloader, Hashes,
    },
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

#[tokio::test]
async fn concurrent_inserts_of_one_object_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path().join("objects"));
    let src = dir.path().join("download.jar");
    std::fs::write(&src, "hello").unwrap();
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let results =
        futures_util::future::join_all((0..8).map(|_| cache.insert_file(&src, &hashes))).await;
    for r in results {
        r.unwrap();
    }
    let stored = cache.get(&hashes).await.expect("object verifies");
    assert_eq!(std::fs::read_to_string(stored).unwrap(), "hello");
}

fn murmur2_only(body: &[u8]) -> Hashes {
    Hashes {
        murmur2: Some(curseforge_murmur2(body)),
        ..Default::default()
    }
}

#[tokio::test]
async fn murmur2_only_objects_are_kept_apart_per_instance() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path().join("objects"));
    let (a, b) = (
        cache.scoped(CacheScope::for_instance("a")),
        cache.scoped(CacheScope::for_instance("b")),
    );
    let src = dir.path().join("cozy.jar");
    std::fs::write(&src, "cozy").unwrap();
    let hashes = murmur2_only(b"cozy");
    let in_a = a.insert_file(&src, &hashes).await.unwrap();
    assert_eq!(a.get(&hashes).await, Some(in_a.clone()));
    assert_eq!(b.get(&hashes).await, None, "b never sees a's file");
    let in_b = b.insert_file(&src, &hashes).await.unwrap();
    assert_ne!(in_a, in_b);
    let name = in_a.file_name().unwrap().to_string_lossy().into_owned();
    let tag = &sha1_hex(b"a")[..12];
    assert_eq!(name, format!("m2-{}-{tag}", curseforge_murmur2(b"cozy")));
    // Without an instance there is no key at all.
    let err = cache.insert_file(&src, &hashes).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackInvalid);
    assert_eq!(
        err.params.get("file").map(String::as_str),
        Some("cozy.jar"),
        "the real file name, never a placeholder word"
    );
    assert_eq!(cache.path_for(&hashes), None);
}

#[test]
fn strong_hashes_are_shared_by_every_instance() {
    let cache = ObjectCache::new("objects");
    let hashes = Hashes {
        sha1: Some(sha1_hex(b"x")),
        murmur2: Some(7),
        ..Default::default()
    };
    let shared = cache.path_for(&hashes);
    assert!(shared.is_some());
    assert_eq!(
        cache
            .scoped(CacheScope::for_instance("a"))
            .path_for(&hashes),
        shared
    );
    assert_eq!(
        cache
            .scoped(CacheScope::for_instance("b"))
            .path_for(&hashes),
        shared
    );
}

const OLD: SystemTime = SystemTime::UNIX_EPOCH;

fn age(path: &std::path::Path) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(OLD)
        .unwrap();
}

fn modified(path: &std::path::Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[tokio::test]
async fn reusing_an_object_marks_it_used() {
    let server = MockServer::start().await;
    serve(&server, "/a.jar", "hello", 1).await;
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path().join("objects"));
    let hashes = Hashes::sha1(sha1_hex(b"hello"));
    let urls = vec![format!("{}/a.jar", server.uri())];
    let object = cache
        .ensure(&downloader(), &urls, &hashes, Some(5))
        .await
        .unwrap();

    age(&object);
    cache.get(&hashes).await.unwrap();
    assert!(modified(&object) > OLD, "a get hit counts as a use");

    age(&object);
    cache
        .ensure(&downloader(), &urls, &hashes, Some(5))
        .await
        .unwrap();
    assert!(modified(&object) > OLD, "ensure reusing it counts as a use");

    age(&object);
    let src = dir.path().join("hello.jar");
    std::fs::write(&src, "hello").unwrap();
    cache.insert_file(&src, &hashes).await.unwrap();
    assert!(
        modified(&object) > OLD,
        "an insert finding it counts as a use"
    );
}

#[tokio::test]
async fn a_file_with_no_usable_hash_says_so_by_context() {
    let dir = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(dir.path());
    let err = cache
        .ensure(&downloader(), &[], &Hashes::default(), None)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackInvalid);
    // The UI's plain message names a file; with none to name it needs its own sentence.
    assert_eq!(
        err.params.get("context").map(String::as_str),
        Some("no_checksum")
    );
}
