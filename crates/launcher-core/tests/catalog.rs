use std::time::Duration;

use launcher_core::{
    meta::{
        catalog::{loader_versions_at, minecraft_versions_at, McKind},
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

#[tokio::test]
async fn forge_is_not_offered_yet() {
    let tmp = tempfile::tempdir().unwrap();
    let err = loader_versions_at(&meta(tmp.path()), LoaderKind::Forge, "1.20.1", "x", "y")
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderUnsupported);
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
