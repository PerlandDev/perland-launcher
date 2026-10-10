use std::time::Duration;

use launcher_core::{
    meta::{
        catalog::{
            forge_versions_at, loader_versions_at, minecraft_versions_at, LoaderVersion, McKind,
        },
        forge::MavenRepos,
        loaders::LoaderKind,
        MetaClient,
    },
    ErrorCode,
};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn meta(dir: &std::path::Path) -> MetaClient {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    MetaClient::new(http, dir)
}

async fn serve(server: &MockServer, at: &str, status: u16, body: &str) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn unknown_version_kind_is_skipped() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/manifest.json",
        200,
        r#"{"latest":{"release":"26.3","snapshot":"26.4-snapshot-3"},"versions":[
          {"id":"26.4-snapshot-3","type":"snapshot","url":"u","sha1":"s","releaseTime":"2026-10-01T00:00:00+00:00"},
          {"id":"26.3","type":"release","url":"u","sha1":"s","releaseTime":"2026-09-01T00:00:00+00:00"},
          {"id":"x-experiment","type":"experiment","url":"u","sha1":"s","releaseTime":"2026-08-01T00:00:00+00:00"},
          {"id":"b1.7.3","type":"old_beta","url":"u","sha1":"s","releaseTime":"2011-07-08T00:00:00+00:00"}
        ]}"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let list = minecraft_versions_at(
        &meta(tmp.path()),
        &format!("{}/manifest.json", server.uri()),
    )
    .await
    .unwrap();
    let ids: Vec<&str> = list.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, vec!["26.4-snapshot-3", "26.3", "b1.7.3"]);
    assert_eq!(list[1].kind, McKind::Release);
    assert_eq!(list[2].kind, McKind::OldBeta);
    assert_eq!(list[1].release_time, "2026-09-01T00:00:00+00:00");
}

#[tokio::test]
async fn quilt_versions_sorted_newest_first() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/quilt/versions/loader/1.20.1",
        200,
        r#"[{"loader":{"version":"0.20.0-beta.9"}},{"loader":{"version":"0.24.0"}},
            {"loader":{"version":"0.20.2-beta.1"}},{"loader":{"version":"0.19.0"}}]"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Quilt,
        "1.20.1",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    let got: Vec<(&str, bool)> = list
        .iter()
        .map(|v| (v.version.as_str(), v.stable))
        .collect();
    assert_eq!(
        got,
        vec![
            ("0.24.0", true),
            ("0.20.2-beta.1", false),
            ("0.20.0-beta.9", false),
            ("0.19.0", true)
        ]
    );
}

#[tokio::test]
async fn fabric_uses_its_stable_flag() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/fabric/versions/loader/1.20.1",
        200,
        r#"[{"loader":{"version":"0.19.5","stable":true}},{"loader":{"version":"0.19.4","stable":false}}]"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "1.20.1",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    assert_eq!(list.len(), 2);
    assert!(list[0].stable && !list[1].stable);
}

#[tokio::test]
async fn unknown_minecraft_version_has_no_loaders() {
    let server = MockServer::start().await;
    serve(&server, "/fabric/versions/loader/9.9.9", 400, "[]").await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "9.9.9",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    assert!(list.is_empty());
}

const FORGE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata><groupId>net.minecraftforge</groupId><artifactId>forge</artifactId>
<versioning><latest>1.21.1-52.1.0</latest><versions>
<version>1.20.1-47.4.9</version><version>1.20.1-47.4.22</version>
<version>1.20.10-99.0.0</version><version>1.21.1-52.1.0</version>
<version>1.20.1-47.4.10</version>
</versions></versioning></metadata>"#;

const NEO_XML: &str = r#"<metadata><versioning><versions>
<version>21.0.167</version><version>21.1.256</version><version>21.1.257</version>
<version>21.1.258-beta</version><version>21.10.5</version><version>26.3.0.64-beta</version>
<version>0.25w14craftmine.3-beta</version>
</versions></versioning></metadata>"#;

const NEO_FORGE_XML: &str = r#"<metadata><versioning><versions>
<version>1.20.1-47.1.105</version><version>1.20.1-47.1.106</version><version>47.1.82</version>
</versions></versioning></metadata>"#;

fn repos(uri: &str) -> MavenRepos<'_> {
    MavenRepos {
        forge: uri,
        neoforge: uri,
    }
}

fn versions(list: &[LoaderVersion]) -> Vec<(&str, bool)> {
    list.iter()
        .map(|v| (v.version.as_str(), v.stable))
        .collect()
}

#[tokio::test]
async fn forge_versions_for_one_minecraft_newest_first() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/net/minecraftforge/forge/maven-metadata.xml",
        200,
        FORGE_XML,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let list = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap();
    assert_eq!(
        versions(&list),
        vec![("47.4.22", true), ("47.4.10", true), ("47.4.9", true)]
    );
    let none = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.19.2", repos(&uri))
        .await
        .unwrap();
    assert!(none.is_empty());
}

#[tokio::test]
async fn neoforge_versions_follow_the_minecraft_version() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/net/neoforged/neoforge/maven-metadata.xml",
        200,
        NEO_XML,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let (uri, m) = (server.uri(), meta(tmp.path()));
    assert_eq!(
        versions(&neo(&m, &uri, "1.21.1").await),
        vec![
            ("21.1.258-beta", false),
            ("21.1.257", true),
            ("21.1.256", true)
        ]
    );
    assert_eq!(
        versions(&neo(&m, &uri, "1.21").await),
        vec![("21.0.167", true)]
    );
    assert_eq!(
        versions(&neo(&m, &uri, "26.3").await),
        vec![("26.3.0.64-beta", false)]
    );
    assert!(neo(&m, &uri, "26.4-snapshot-3").await.is_empty());
}

async fn neo(m: &MetaClient, uri: &str, mc: &str) -> Vec<LoaderVersion> {
    forge_versions_at(m, LoaderKind::NeoForge, mc, repos(uri))
        .await
        .unwrap()
}

#[tokio::test]
async fn neoforge_for_1_20_1_comes_from_its_forge_artifact() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/net/neoforged/forge/maven-metadata.xml",
        200,
        NEO_FORGE_XML,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let list = forge_versions_at(
        &meta(tmp.path()),
        LoaderKind::NeoForge,
        "1.20.1",
        repos(&uri),
    )
    .await
    .unwrap();
    assert_eq!(
        versions(&list),
        vec![("47.1.106", true), ("47.1.105", true)]
    );
}

#[tokio::test]
async fn missing_forge_metadata_is_an_error() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let err = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaNotFound);
}

#[tokio::test]
async fn being_offline_is_reported_not_hidden() {
    let tmp = tempfile::tempdir().unwrap();
    // Port 9 (discard) refuses connections immediately.
    let err = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "1.20.1",
        "http://127.0.0.1:9/fabric",
        "http://127.0.0.1:9/quilt",
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
}

#[tokio::test]
async fn quilt_404_for_unknown_game_version_has_no_loaders() {
    let server = MockServer::start().await;
    serve(&server, "/quilt/versions/loader/9.9.9", 404, "[]").await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let list = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Quilt,
        "9.9.9",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn malformed_loader_list_is_an_error_not_empty() {
    let server = MockServer::start().await;
    serve(&server, "/fabric/versions/loader/1.20.1", 200, "{}").await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let err = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "1.20.1",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
}

#[tokio::test]
async fn rate_limit_is_an_error_not_empty() {
    let server = MockServer::start().await;
    serve(&server, "/fabric/versions/loader/1.20.1", 429, "slow down").await;
    let tmp = tempfile::tempdir().unwrap();
    let base = server.uri();
    let err = loader_versions_at(
        &meta(tmp.path()),
        LoaderKind::Fabric,
        "1.20.1",
        &format!("{base}/fabric"),
        &format!("{base}/quilt"),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaNotFound);
    assert_eq!(err.params.get("status").map(String::as_str), Some("429"));
}

#[tokio::test]
async fn captive_portal_page_does_not_replace_cached_forge_metadata() {
    let server = MockServer::start().await;
    let at = "/net/minecraftforge/forge/maven-metadata.xml";
    serve(&server, at, 200, FORGE_XML).await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let first = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap();

    server.reset().await;
    serve(&server, at, 200, "<html>login</html>").await;
    let again = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap();
    assert_eq!(versions(&again), versions(&first));
}

#[tokio::test]
async fn non_metadata_page_without_cache_is_an_error_not_empty() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/net/minecraftforge/forge/maven-metadata.xml",
        200,
        "<html>login</html>",
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let err = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", repos(&uri))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);
}

#[tokio::test]
async fn forge_before_1_12_2_is_not_offered() {
    // No mock is mounted: a request would 404 and fail the call.
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let list = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.8.9", repos(&uri))
        .await
        .unwrap();
    assert!(list.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn forge_for_1_12_2_starts_at_the_first_supported_installer() {
    let server = MockServer::start().await;
    serve(
        &server,
        "/net/minecraftforge/forge/maven-metadata.xml",
        200,
        r#"<metadata><versioning><versions>
<version>1.12.2-14.23.5.2847</version><version>1.12.2-14.23.5.2851</version>
<version>1.12.2-14.23.5.2860</version>
</versions></versioning></metadata>"#,
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let uri = server.uri();
    let list = forge_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.12.2", repos(&uri))
        .await
        .unwrap();
    assert_eq!(
        versions(&list),
        vec![("14.23.5.2860", true), ("14.23.5.2851", true)]
    );
}
