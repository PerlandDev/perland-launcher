mod common;

use common::{sources, PackBuilder};
use launcher_core::{
    download::hash::sha256_hex,
    instances::{SourceRef, UpdatePolicy},
    meta::loaders::LoaderKind,
    sources::{packwiz, FileMode, FileSource, Side, UpdateCheck},
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn tiny(server: &MockServer) -> PackBuilder {
    PackBuilder::new(server, "Tiny", "1.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple", "both", None)
        .url_mod("zoom", "zoom.jar", b"zoom", "client", None)
        .url_mod("backups", "backups.jar", b"backup", "server", None)
        .url_mod("shaders", "shaders.jar", b"shade", "client", Some(false))
        .cf_mod("cozy", "cozy.jar", 1234567, 42, b"cozy", false)
        .file("config/a.toml", b"a = 1", true)
}

#[tokio::test]
async fn resolves_a_tiny_pack() {
    let server = MockServer::start().await;
    let pack = tiny(&server);
    let url = pack.mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let desired = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap();

    assert_eq!(desired.name, "Tiny");
    assert_eq!(desired.pack_version, "1.0.0");
    assert_eq!(desired.minecraft, "1.20.1");
    let loader = desired.loader.clone().unwrap();
    assert_eq!(
        (loader.kind, loader.version.as_str()),
        (LoaderKind::Forge, "47.4.22")
    );
    assert_eq!(desired.stamp, sha256_hex(pack.pack_toml().as_bytes()));

    let by_path = |p: &str| desired.files.iter().find(|f| f.path == p).unwrap().clone();
    let apple = by_path("mods/appleskin.jar");
    assert_eq!(
        apple.source,
        FileSource::Remote {
            urls: vec![format!("{}/files/appleskin.jar", server.uri())]
        }
    );
    assert!(apple.hashes.sha512.is_some());
    assert_eq!(by_path("mods/zoom.jar").side, Side::Client);
    assert_eq!(by_path("mods/backups.jar").side, Side::Server);
    assert!(!by_path("mods/shaders.jar").optional.unwrap().default);
    let cozy = by_path("mods/cozy.jar");
    assert_eq!(
        cozy.source,
        FileSource::Remote {
            urls: vec![format!("{}/files/1234/567/cozy.jar", server.uri())]
        }
    );
    assert_eq!(
        cozy.manual.unwrap().page_url,
        "https://www.curseforge.com/projects/42"
    );
    let config = by_path("config/a.toml");
    assert_eq!(config.mode, FileMode::Preserve);
    assert_eq!(
        config.source,
        FileSource::Remote {
            urls: vec![format!("{}/pack/config/a.toml", server.uri())]
        }
    );
    assert_eq!(desired.files.len(), 6);
}

#[tokio::test]
async fn reads_the_sidecar_when_there_is_one() {
    let server = MockServer::start().await;
    let url = tiny(&server)
        .sidecar("[art]\nicon = \"assets/icon.png\"\n[sync]\npolicy = \"auto\"\n")
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let desired = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap();
    assert_eq!(
        desired.about.art.icon,
        Some(format!("{}/pack/assets/icon.png", server.uri()))
    );
    assert_eq!(desired.about.policy, Some(UpdatePolicy::Auto));

    let plain = MockServer::start().await;
    let url = tiny(&plain).mount(&plain).await;
    let tmp = tempfile::tempdir().unwrap();
    let desired = packwiz::resolve(&sources(&plain, tmp.path()), &url)
        .await
        .unwrap();
    assert_eq!(desired.about, Default::default(), "404 means no sidecar");
}

#[tokio::test]
async fn a_wrong_index_hash_is_refused() {
    let server = MockServer::start().await;
    let url = tiny(&server).corrupt_index().mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap_err();
    // A pack error naming the file, not "a version list" from the metadata client.
    assert_eq!(err.code, ErrorCode::PackInvalid);
    assert_eq!(
        err.params.get("file").map(String::as_str),
        Some("index.toml")
    );
    assert!(err.detail.is_some(), "the checksum detail is kept");
}

/// `at` answers with `response`, ahead of what `PackBuilder::mount` serves there.
async fn override_path(server: &MockServer, at: &str, response: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(response)
        .with_priority(1)
        .mount(server)
        .await;
}

#[tokio::test]
async fn a_missing_metafile_is_a_missing_pack() {
    let server = MockServer::start().await;
    let url = tiny(&server).mount(&server).await;
    override_path(
        &server,
        "/pack/mods/zoom.pw.toml",
        ResponseTemplate::new(404),
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackNotFound);
}

#[tokio::test]
async fn a_refused_index_is_host_trouble() {
    let server = MockServer::start().await;
    let url = tiny(&server).mount(&server).await;
    override_path(&server, "/pack/index.toml", ResponseTemplate::new(403)).await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::HostTrouble);
    assert_eq!(err.params.get("status").map(String::as_str), Some("403"));
    let host = server.address().ip().to_string();
    assert_eq!(err.params.get("host"), Some(&host));
}

#[tokio::test]
async fn a_metafile_with_a_wrong_hash_names_its_path() {
    let server = MockServer::start().await;
    let url = tiny(&server).mount(&server).await;
    override_path(
        &server,
        "/pack/mods/zoom.pw.toml",
        ResponseTemplate::new(200).set_body_bytes(&b"name = \"tampered\""[..]),
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackInvalid);
    assert_eq!(
        err.params.get("file").map(String::as_str),
        Some("mods/zoom.pw.toml")
    );
}

#[tokio::test]
async fn an_oversized_pack_toml_is_an_invalid_pack() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let url = tiny(&server).mount(&server).await;
    let mut src = sources(&server, tmp.path());
    src.meta = src.meta.with_limit(16); // pack.toml is larger than this
    let err = packwiz::resolve(&src, &url).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackInvalid);
    assert_eq!(
        err.params.get("file").map(String::as_str),
        Some("pack.toml")
    );
}

#[tokio::test]
async fn packwiz_rejects_escaping_and_colliding_paths() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let url = PackBuilder::new(&server, "Evil", "1")
        .file("../escape.txt", b"x", false)
        .mount(&server)
        .await;
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackUnsafePath);

    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let url = PackBuilder::new(&server, "Clash", "1")
        .file("config/A.toml", b"x", false)
        .file("config/a.toml", b"y", false)
        .mount(&server)
        .await;
    let err = packwiz::resolve(&sources(&server, tmp.path()), &url)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackUnsafePath);
}

#[tokio::test]
async fn resolves_offline_from_the_cache() {
    // Not the pooled `start()`: a pooled server keeps listening after it is dropped.
    let server = MockServer::builder().start().await;
    let url = tiny(&server).mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let src = sources(&server, tmp.path());
    let online = packwiz::resolve(&src, &url).await.unwrap();
    drop(server); // connections are now refused
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let offline = packwiz::resolve(&src, &url).await.unwrap();
    assert_eq!(online, offline);
}

#[tokio::test]
async fn a_missing_pack_is_not_found() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let err = packwiz::resolve(
        &sources(&server, tmp.path()),
        &format!("{}/nothing/pack.toml", server.uri()),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackNotFound);
}

#[tokio::test]
async fn check_compares_the_stamp_and_fails_softly() {
    // Not the pooled `start()`: a pooled server keeps listening after it is dropped.
    let server = MockServer::builder().start().await;
    let v1 = tiny(&server);
    let url = v1.mount(&server).await;
    let tmp = tempfile::tempdir().unwrap();
    let src = sources(&server, tmp.path());
    let stamp = sha256_hex(v1.pack_toml().as_bytes());
    assert_eq!(
        packwiz::check(&src, &url, &stamp).await,
        UpdateCheck::UpToDate
    );

    server.reset().await;
    let v2 = tiny(&server).with_version("1.0.1");
    v2.mount(&server).await;
    assert_eq!(
        packwiz::check(&src, &url, &stamp).await,
        UpdateCheck::Available {
            source: SourceRef::Packwiz { url: url.clone() },
            stamp: sha256_hex(v2.pack_toml().as_bytes()),
            version: "1.0.1".into(),
        }
    );

    drop(server); // connections are now refused
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    match packwiz::check(&src, &url, &stamp).await {
        UpdateCheck::Unavailable(e) => assert_eq!(e.code, ErrorCode::UpdateCheckFailed),
        other => panic!("{other:?}"),
    }
}
