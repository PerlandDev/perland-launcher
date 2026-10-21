//! Sharing an instance as a `.mrpack` against a wiremock Modrinth, read back with the
//! launcher's own `.mrpack` reader.
use std::{io::Read, path::Path};

use launcher_core::{
    browse::api::Api,
    download::{
        hash::{sha1_hex, sha512_hex},
        Hashes,
    },
    instances::LoaderSpec,
    meta::loaders::LoaderKind,
    share::{self, Category, Env, PackMeta, ShareChoice, SharePlan},
    sources::{
        mrpack::{self, Limits},
        FileSource, Side,
    },
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

const CDN: &str = "https://cdn.modrinth.com/data/AAAA/versions/v1";

fn api(server: &MockServer) -> Api {
    Api::new(
        launcher_core::http::client("test").unwrap(),
        format!("{}/v2", server.uri()),
    )
}

fn put(root: &Path, rel: &str, body: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn version(project: &str, name: &str, body: &[u8], url: &str, sha512: Option<String>) -> Value {
    json!({
        "id": "v1",
        "project_id": project,
        "version_number": "1.0",
        "version_type": "release",
        "date_published": "2026-01-01T00:00:00Z",
        "files": [{
            "url": url,
            "filename": name,
            "primary": true,
            "size": body.len(),
            "hashes": { "sha1": sha1_hex(body), "sha512": sha512 }
        }]
    })
}

fn project(id: &str, client: &str, server: &str) -> Value {
    json!({
        "id": id, "slug": id, "title": id, "project_type": "mod",
        "client_side": client, "server_side": server
    })
}

async fn serve_versions(server: &MockServer, answer: Value) {
    Mock::given(method("POST"))
        .and(path("/v2/version_files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

async fn serve_projects(server: &MockServer, answer: ResponseTemplate) {
    Mock::given(method("GET"))
        .and(path("/v2/projects"))
        .respond_with(answer)
        .mount(server)
        .await;
}

fn all() -> ShareChoice {
    ShareChoice {
        mods: true,
        settings: true,
        resource_packs: true,
        shader_packs: true,
        worlds: false,
    }
}

fn meta() -> PackMeta {
    PackMeta {
        name: "Sky Friends".into(),
        version_id: "2026-10-21".into(),
        minecraft: "1.20.1".into(),
        loader: Some(LoaderSpec {
            kind: LoaderKind::Fabric,
            version: "0.16.14".into(),
        }),
    }
}

/// A game folder with one mod Modrinth knows, one it doesn't, settings, a resource pack and a
/// world.
fn game(root: &Path) {
    put(root, "mods/known.jar", b"known");
    put(root, "mods/own.jar", b"own");
    put(root, "config/a.toml", b"a = 1\n");
    put(
        root,
        "options.txt",
        b"version:3465\nlastServer:secret.example.com\n",
    );
    put(root, "resourcepacks/r.zip", b"rp");
    put(root, "saves/W/level.dat", b"world");
    put(root, "logs/latest.log", b"log");
}

async fn plan(server: &MockServer, root: &Path) -> SharePlan {
    let scan = share::scan(root).await.unwrap();
    let sha1s: Vec<String> = scan.files.iter().filter_map(|f| f.sha1.clone()).collect();
    let linked = share::lookup(&api(server), &sha1s).await.unwrap();
    SharePlan { scan, linked }
}

fn entry(archive: &Path, name: &str) -> Option<String> {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(archive).unwrap()).unwrap();
    let mut file = zip.by_name(name).ok()?;
    let mut text = String::new();
    file.read_to_string(&mut text).unwrap();
    Some(text)
}

#[tokio::test]
async fn round_trip_through_the_reader() {
    let server = MockServer::start().await;
    let known_url = format!("{CDN}/known.jar");
    serve_versions(
        &server,
        json!({ sha1_hex(b"known"): version("proj1", "known.jar", b"known", &known_url, Some(sha512_hex(b"known"))) }),
    )
    .await;
    serve_projects(
        &server,
        ResponseTemplate::new(200).set_body_json(json!([project(
            "proj1",
            "required",
            "unsupported"
        )])),
    )
    .await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;

    let counts = share::counts(&plan);
    let mods = counts
        .iter()
        .find(|c| c.category == Category::Mods)
        .unwrap();
    assert_eq!((mods.files, mods.linked, mods.embedded_bytes), (2, 1, 3));
    let worlds = counts
        .iter()
        .find(|c| c.category == Category::Worlds)
        .unwrap();
    assert_eq!(worlds.files, 1);
    assert!(counts.iter().all(|c| c.category != Category::ShaderPacks));

    let out = tmp.path().join("Sky Friends.mrpack");
    share::write(&root, &plan, all(), meta(), &out)
        .await
        .unwrap();
    assert!(!tmp.path().join("Sky Friends.mrpack.part").exists());

    let desired = mrpack::read_archive(
        &out,
        "s".into(),
        Hashes::sha1("x".repeat(40)),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(desired.name, "Sky Friends");
    assert_eq!(desired.pack_version, "2026-10-21");
    assert_eq!(desired.minecraft, "1.20.1");
    let loader = desired.loader.unwrap();
    assert_eq!(
        (loader.kind, loader.version.as_str()),
        (LoaderKind::Fabric, "0.16.14")
    );
    let paths: Vec<&str> = desired.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "config/a.toml",
            "mods/known.jar",
            "mods/own.jar",
            "options.txt",
            "resourcepacks/r.zip"
        ]
    );
    let known = desired
        .files
        .iter()
        .find(|f| f.path == "mods/known.jar")
        .unwrap();
    assert_eq!(
        known.source,
        FileSource::Remote {
            urls: vec![known_url]
        }
    );
    assert_eq!(
        known.hashes.sha1.as_deref(),
        Some(sha1_hex(b"known").as_str())
    );
    assert_eq!(
        known.hashes.sha512.as_deref(),
        Some(sha512_hex(b"known").as_str())
    );
    assert_eq!(known.side, Side::Client);
    let own = desired
        .files
        .iter()
        .find(|f| f.path == "mods/own.jar")
        .unwrap();
    assert_eq!(
        own.source,
        FileSource::Archive {
            entry: "overrides/mods/own.jar".into()
        }
    );
    assert_eq!(own.hashes.sha1.as_deref(), Some(sha1_hex(b"own").as_str()));
    let rp = desired
        .files
        .iter()
        .find(|f| f.path == "resourcepacks/r.zip")
        .unwrap();
    assert_eq!(
        rp.source,
        FileSource::Archive {
            entry: "client-overrides/resourcepacks/r.zip".into()
        }
    );
    assert_eq!(
        entry(&out, "client-overrides/options.txt").unwrap(),
        "version:3465\n"
    );
    assert!(entry(&out, "overrides/logs/latest.log").is_none());
}

#[tokio::test]
async fn worlds_go_in_when_chosen() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;
    let out = tmp.path().join("p.mrpack");
    let choice = ShareChoice {
        worlds: true,
        ..all()
    };
    share::write(&root, &plan, choice, meta(), &out)
        .await
        .unwrap();
    assert_eq!(entry(&out, "overrides/saves/W/level.dat").unwrap(), "world");
}

#[tokio::test]
async fn choice_leaves_categories_out() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;
    let out = tmp.path().join("p.mrpack");
    let choice = ShareChoice {
        mods: false,
        settings: true,
        resource_packs: false,
        shader_packs: false,
        worlds: false,
    };
    share::write(&root, &plan, choice, meta(), &out)
        .await
        .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&out).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().unwrap().into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        vec![
            "client-overrides/options.txt",
            "modrinth.index.json",
            "overrides/config/a.toml"
        ]
    );
    // Resource packs stay out, so options.txt no longer names any.
    let index: Value = serde_json::from_str(&entry(&out, "modrinth.index.json").unwrap()).unwrap();
    assert_eq!(index["files"], json!([]));
    assert_eq!(index["formatVersion"], 1);
    assert_eq!(index["game"], "minecraft");
    assert_eq!(index["summary"], "Shared from Perland Launcher");
    assert_eq!(
        index["dependencies"],
        json!({ "minecraft": "1.20.1", "fabric-loader": "0.16.14" })
    );
}

#[tokio::test]
async fn too_large_refuses_before_writing() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;
    let out = tmp.path().join("p.mrpack");
    for limits in [
        share::Limits {
            entry: 4,
            total: 1000,
        },
        share::Limits {
            entry: 1000,
            total: 10,
        },
    ] {
        let err = share::write_with(&root, &plan, all(), meta(), &out, limits)
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ShareTooLarge);
        assert!(!out.exists());
    }
}

#[tokio::test]
async fn failed_write_leaves_no_file() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;
    std::fs::remove_file(root.join("config/a.toml")).unwrap();
    std::fs::create_dir(root.join("config/a.toml")).unwrap();
    let out = tmp.path().join("p.mrpack");
    share::write(&root, &plan, all(), meta(), &out)
        .await
        .unwrap_err();
    assert!(!out.exists());
    assert!(!tmp.path().join("p.mrpack.part").exists());
}

#[tokio::test]
async fn a_file_gone_since_the_scan_is_left_out() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;
    std::fs::remove_file(root.join("config/a.toml")).unwrap();
    let out = tmp.path().join("p.mrpack");
    share::write(&root, &plan, all(), meta(), &out)
        .await
        .unwrap();
    assert!(entry(&out, "overrides/config/a.toml").is_none());
}

#[tokio::test]
async fn off_host_or_no_sha512_is_embedded() {
    let server = MockServer::start().await;
    serve_versions(
        &server,
        json!({
            sha1_hex(b"a"): version("p1", "a.jar", b"a", "https://evil.example.com/a.jar", Some(sha512_hex(b"a"))),
            sha1_hex(b"b"): version("p2", "b.jar", b"b", &format!("{CDN}/b.jar"), None),
            sha1_hex(b"c"): version("p3", "c.jar", b"c", "http://cdn.modrinth.com/c.jar", Some(sha512_hex(b"c"))),
        }),
    )
    .await;
    serve_projects(&server, ResponseTemplate::new(200).set_body_json(json!([]))).await;
    let asked = vec![sha1_hex(b"a"), sha1_hex(b"b"), sha1_hex(b"c")];
    let linked = share::lookup(&api(&server), &asked).await.unwrap();
    assert!(linked.is_empty());
}

#[tokio::test]
async fn known_files_link_to_modrinth() {
    let server = MockServer::start().await;
    let url = format!("{CDN}/a.jar");
    serve_versions(
        &server,
        json!({ sha1_hex(b"a"): version("p1", "a.jar", b"a", &url, Some(sha512_hex(b"a"))) }),
    )
    .await;
    serve_projects(
        &server,
        ResponseTemplate::new(200).set_body_json(json!([project("p1", "unsupported", "required")])),
    )
    .await;
    let linked = share::lookup(&api(&server), &[sha1_hex(b"a")])
        .await
        .unwrap();
    let a = &linked[&sha1_hex(b"a")];
    assert_eq!(a.url, url);
    assert_eq!(a.size, 1);
    assert_eq!(a.sha512, sha512_hex(b"a"));
    assert_eq!(
        a.env,
        Some(Env {
            client: "unsupported".into(),
            server: "required".into()
        })
    );
}

#[tokio::test]
async fn projects_failure_drops_env_only() {
    let server = MockServer::start().await;
    serve_versions(
        &server,
        json!({ sha1_hex(b"a"): version("p1", "a.jar", b"a", &format!("{CDN}/a.jar"), Some(sha512_hex(b"a"))) }),
    )
    .await;
    serve_projects(&server, ResponseTemplate::new(500)).await;
    let linked = share::lookup(&api(&server), &[sha1_hex(b"a")])
        .await
        .unwrap();
    assert_eq!(linked[&sha1_hex(b"a")].env, None);
}

#[tokio::test]
async fn version_files_failure_is_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v2/version_files"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    assert!(share::lookup(&api(&server), &[sha1_hex(b"a")])
        .await
        .is_err());
}

#[tokio::test]
async fn duplicate_bytes_share_a_lookup() {
    let server = MockServer::start().await;
    let hash = sha1_hex(b"a");
    Mock::given(method("POST"))
        .and(path("/v2/version_files"))
        .and(body_json(
            json!({ "hashes": [hash.clone()], "algorithm": "sha1" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .expect(1)
        .mount(&server)
        .await;
    let linked = share::lookup(&api(&server), &[hash.clone(), hash.to_uppercase()])
        .await
        .unwrap();
    assert!(linked.is_empty());
}

#[tokio::test]
async fn nothing_to_look_up_asks_nothing() {
    let server = MockServer::start().await;
    let linked = share::lookup(&api(&server), &[]).await.unwrap();
    assert!(linked.is_empty());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_settings_file_that_gained_a_secret_since_the_scan_is_left_out() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    let plan = plan(&server, &root).await;
    std::fs::write(
        root.join("config/a.toml"),
        b"key = \"plt_0123456789abcdef0123\"",
    )
    .unwrap();
    let out = tmp.path().join("p.mrpack");
    share::write(&root, &plan, all(), meta(), &out)
        .await
        .unwrap();
    assert!(entry(&out, "overrides/config/a.toml").is_none());
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn names_differing_by_case_still_import() {
    let server = MockServer::start().await;
    serve_versions(&server, json!({})).await;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("minecraft");
    game(&root);
    put(&root, "config/A.toml", b"upper");
    let plan = plan(&server, &root).await;
    let out = tmp.path().join("p.mrpack");
    share::write(&root, &plan, all(), meta(), &out)
        .await
        .unwrap();
    mrpack::read_archive(
        &out,
        "s".into(),
        Hashes::sha1("x".repeat(40)),
        Limits::default(),
    )
    .unwrap();
}
