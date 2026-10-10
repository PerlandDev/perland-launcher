//! Getting a Forge/NeoForge installer: download, checksum, read, merge. Served by wiremock.
use std::{io::Write, path::Path, time::Duration};

use launcher_core::{
    download::{hash::sha1_hex, Downloader},
    install::forge::{fetch_installer, resolve_forge},
    meta::{forge::MavenRepos, loaders::LoaderKind, mojang::VersionJson, MetaClient},
    paths::LauncherDirs,
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

const FORGE_PROFILE: &str = include_str!("fixtures/forge-1.20.1-47.4.22-install_profile.json");
const FORGE_VERSION: &str = include_str!("fixtures/forge-1.20.1-47.4.22-version.json");
const NEO_PROFILE: &str = include_str!("fixtures/neoforge-21.1.257-install_profile.json");
const NEO_VERSION: &str = include_str!("fixtures/neoforge-21.1.257-version.json");
const FORGE_JAR: &str =
    "/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar";

fn installer_zip(profile: &str, version: &str) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in [("install_profile.json", profile), ("version.json", version)] {
        w.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        w.write_all(body.as_bytes()).unwrap();
    }
    w.finish().unwrap().into_inner()
}

async fn serve_installer(server: &MockServer, at: &str, jar: &[u8]) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(jar.to_vec()))
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{at}.sha1")))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha1_hex(jar)))
        .mount(server)
        .await;
}

struct Env {
    _tmp: tempfile::TempDir,
    dirs: LauncherDirs,
    meta: MetaClient,
    downloader: Downloader,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let dirs = LauncherDirs::new(tmp.path());
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    Env {
        meta: MetaClient::new(http.clone(), dirs.meta()),
        downloader: Downloader::new(http).with_backoff(Duration::from_millis(1)),
        dirs,
        _tmp: tmp,
    }
}

fn repos(uri: &str) -> MavenRepos<'_> {
    MavenRepos {
        forge: uri,
        neoforge: uri,
    }
}

fn vanilla() -> VersionJson {
    serde_json::from_str(include_str!("fixtures/1.20.1.json")).unwrap()
}

#[tokio::test]
async fn installer_is_downloaded_verified_and_merged() {
    let server = MockServer::start().await;
    let jar = installer_zip(FORGE_PROFILE, FORGE_VERSION);
    serve_installer(&server, FORGE_JAR, &jar).await;
    let e = env();
    let uri = server.uri();
    let (path, sha1) = fetch_installer(
        &e.meta,
        &e.downloader,
        &e.dirs,
        LoaderKind::Forge,
        "1.20.1",
        "47.4.22",
        repos(&uri),
    )
    .await
    .unwrap();
    assert_eq!(path, e.dirs.libraries().join(&FORGE_JAR[1..]));
    assert_eq!(sha1, sha1_hex(&jar));
    let (json, install) = resolve_forge(
        vanilla(),
        LoaderKind::Forge,
        "1.20.1",
        "47.4.22",
        path,
        sha1,
    )
    .await
    .unwrap();
    assert_eq!(json.id, "1.20.1-forge-47.4.22");
    assert!(json.inherits_from.is_none());
    assert_eq!(install.loader_version, "47.4.22");
    assert_eq!(install.profile.processors.len(), 10);
}

#[tokio::test]
async fn unknown_loader_version_is_a_loader_error() {
    let server = MockServer::start().await; // answers 404 to everything
    let e = env();
    let uri = server.uri();
    let err = fetch_installer(
        &e.meta,
        &e.downloader,
        &e.dirs,
        LoaderKind::Forge,
        "1.20.1",
        "99.9.9",
        repos(&uri),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderVersionNotFound);
    let invalid = fetch_installer(
        &e.meta,
        &e.downloader,
        &e.dirs,
        LoaderKind::Forge,
        "1.20.1",
        "../x",
        repos(&uri),
    )
    .await
    .unwrap_err();
    assert_eq!(invalid.code, ErrorCode::VersionInvalid);
}

#[tokio::test]
async fn installer_for_another_minecraft_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let jar = tmp.path().join("neoforge-21.1.257-installer.jar");
    std::fs::write(&jar, installer_zip(NEO_PROFILE, NEO_VERSION)).unwrap();
    // 21.1.x is for 1.21.1; this instance is 1.21.
    let err = resolve_forge(
        vanilla(),
        LoaderKind::NeoForge,
        "1.21",
        "21.1.257",
        jar,
        "s".into(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderVersionNotFound);
    assert_eq!(err.params["version"], "1.21");
}

#[tokio::test]
async fn installer_resolves_offline_after_the_first_download() {
    let e = env();
    let jar = installer_zip(FORGE_PROFILE, FORGE_VERSION);
    let uri = {
        let server = MockServer::start().await;
        serve_installer(&server, FORGE_JAR, &jar).await;
        let uri = server.uri();
        fetch_installer(
            &e.meta,
            &e.downloader,
            &e.dirs,
            LoaderKind::Forge,
            "1.20.1",
            "47.4.22",
            repos(&uri),
        )
        .await
        .unwrap();
        uri
    }; // the server is gone now
    let (path, sha1) = fetch_installer(
        &e.meta,
        &e.downloader,
        &e.dirs,
        LoaderKind::Forge,
        "1.20.1",
        "47.4.22",
        repos(&uri),
    )
    .await
    .unwrap();
    assert!(Path::new(&path).is_file());
    assert_eq!(sha1, sha1_hex(&jar));
}
