mod common;

use common::{sources, write_zip};
use launcher_core::{
    download::hash::sha1_hex,
    instances::SourceRef,
    sources::{modrinth, PackInput, UpdateCheck},
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

async fn mount_json(server: &MockServer, at: &str, body: serde_json::Value) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

fn version(
    server: &MockServer,
    id: &str,
    kind: &str,
    date: &str,
    sha1: &str,
    size: u64,
) -> serde_json::Value {
    json!({
        "id": id, "project_id": "P1", "name": id, "version_number": format!("{id}-num"),
        "version_type": kind, "game_versions": ["1.20.1"], "date_published": date,
        "files": [{
            "url": format!("{}/files/{id}.mrpack", server.uri()), "filename": format!("{id}.mrpack"),
            "primary": true, "size": size, "hashes": { "sha1": sha1, "sha512": null }
        }]
    })
}

/// A project "sky" (id P1) with a release R1 (served) and a newer alpha A1.
async fn sky(server: &MockServer, tmp: &std::path::Path) {
    let pack = tmp.join("r1.mrpack");
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": "1.0", "name": "Sky (index name)",
        "files": [], "dependencies": { "minecraft": "1.20.1", "fabric-loader": "0.16.14" }
    }))
    .unwrap();
    write_zip(
        &pack,
        &[
            ("modrinth.index.json", &index[..]),
            ("overrides/options.txt", &b"fov:1"[..]),
        ],
    );
    let bytes = std::fs::read(&pack).unwrap();
    let sha1 = sha1_hex(&bytes);
    Mock::given(method("GET"))
        .and(path("/files/R1.mrpack"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes.clone()))
        .mount(server)
        .await;
    let project = json!({
        "id": "P1", "slug": "sky", "title": "Sky Islands", "description": "Float",
        "project_type": "modpack", "icon_url": "https://cdn.example.com/icon.png",
        "gallery": [
            { "url": "https://cdn.example.com/a.png", "featured": false },
            { "url": "https://cdn.example.com/b.png", "featured": true }
        ]
    });
    mount_json(server, "/v2/project/sky", project.clone()).await;
    mount_json(server, "/v2/project/P1", project).await;
    let r1 = version(
        server,
        "R1",
        "release",
        "2024-01-01T00:00:00Z",
        &sha1,
        bytes.len() as u64,
    );
    let a1 = version(
        server,
        "A1",
        "alpha",
        "2024-02-01T00:00:00Z",
        &sha1,
        bytes.len() as u64,
    );
    mount_json(server, "/v2/version/R1", r1.clone()).await;
    mount_json(server, "/v2/project/P1/version", json!([a1, r1])).await;
}

#[tokio::test]
async fn opens_the_newest_release_of_a_modpack() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    sky(&server, tmp.path()).await;
    let src = sources(&server, tmp.path());
    let (source, desired) = src
        .open(&PackInput::Modrinth {
            project: "sky".into(),
            version: None,
        })
        .await
        .unwrap();
    assert_eq!(
        source,
        SourceRef::Modrinth {
            project_id: "P1".into(),
            version_id: "R1".into()
        }
    );
    assert_eq!(desired.name, "Sky Islands");
    assert_eq!(desired.pack_version, "R1-num");
    assert_eq!(desired.stamp, "R1");
    assert_eq!(desired.about.description.as_deref(), Some("Float"));
    assert_eq!(
        desired.about.art.icon.as_deref(),
        Some("https://cdn.example.com/icon.png")
    );
    assert_eq!(
        desired.about.art.banner.as_deref(),
        Some("https://cdn.example.com/b.png")
    );
    assert_eq!(desired.files[0].path, "options.txt");
    assert_eq!(src.resolve(&source).await.unwrap(), desired);
}

#[tokio::test]
async fn only_modpacks_open() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    mount_json(
        &server,
        "/v2/project/sodium",
        json!({
            "id": "AANobbMI", "slug": "sodium", "title": "Sodium", "project_type": "mod", "gallery": []
        }),
    )
    .await;
    let src = sources(&server, tmp.path());
    let err = modrinth::open(&src, "sodium", Some("X")).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackUnsupported);
    let missing = modrinth::open(&src, "nothing-here", None)
        .await
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::PackNotFound);
}

#[tokio::test]
async fn check_offers_only_as_stable_updates_on_the_same_minecraft() {
    // Exclusive server: it really shuts down on drop (pooled ones keep listening).
    let server = MockServer::builder().start().await;
    let tmp = tempfile::tempdir().unwrap();
    sky(&server, tmp.path()).await;
    let src = sources(&server, tmp.path());
    // Installed: the release R1. The only newer version is an alpha.
    assert_eq!(
        modrinth::check(&src, "P1", "R1", "1.20.1").await,
        UpdateCheck::UpToDate
    );

    let newer = version(
        &server,
        "R2",
        "release",
        "2024-03-01T00:00:00Z",
        &sha1_hex(b"x"),
        1,
    );
    Mock::given(method("GET"))
        .and(path("/v2/project/P1/version"))
        .and(query_param("game_versions", "[\"1.20.1\"]"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([newer])))
        .with_priority(1)
        .mount(&server)
        .await;
    assert_eq!(
        modrinth::check(&src, "P1", "R1", "1.20.1").await,
        UpdateCheck::Available {
            source: SourceRef::Modrinth {
                project_id: "P1".into(),
                version_id: "R2".into()
            },
            stamp: "R2".into(),
            version: "R2-num".into(),
        }
    );

    drop(server);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    match modrinth::check(&src, "P1", "R1", "1.20.1").await {
        UpdateCheck::Unavailable(e) => assert_eq!(e.code, ErrorCode::UpdateCheckFailed),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn sources_facade_routes_by_kind() {
    let server = MockServer::start().await;
    let tmp = tempfile::tempdir().unwrap();
    let src = sources(&server, tmp.path());
    assert_eq!(
        src.resolve(&SourceRef::None).await.unwrap_err().code,
        ErrorCode::NoPack
    );
    let pack = tmp.path().join("local.mrpack");
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": "1", "name": "Local",
        "files": [], "dependencies": { "minecraft": "1.20.1" }
    }))
    .unwrap();
    write_zip(&pack, &[("modrinth.index.json", &index[..])]);
    let (source, desired) = src
        .open(&PackInput::MrpackFile { path: pack })
        .await
        .unwrap();
    assert!(matches!(source, SourceRef::Mrpack { .. }));
    assert_eq!(src.check(&source, &desired).await, UpdateCheck::UpToDate);
    assert_eq!(src.resolve(&source).await.unwrap(), desired);
}
