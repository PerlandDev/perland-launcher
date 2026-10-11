//! Browse (Plan 3c) against a wiremock Modrinth: the API client, search, projects, identify,
//! the content writer, images and the `Browse` service.
use launcher_core::{
    browse::api::{Api, DependencyType},
    download::hash::{sha1_hex, sha512_hex},
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

fn api(server: &MockServer) -> Api {
    Api::new(
        launcher_core::http::client("test").unwrap(),
        format!("{}/v2", server.uri()),
    )
}

async fn serve_json(server: &MockServer, at: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

/// How many requests the server saw for `at`.
async fn hits(server: &MockServer, at: &str) -> usize {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .filter(|r| r.url.path() == at)
        .count()
}

/// A Modrinth version whose one file is `body`, served at `<server>/files/<file_name>`.
fn version_json(
    server: &MockServer,
    id: &str,
    project: &str,
    game_versions: &[&str],
    loaders: &[&str],
    file_name: &str,
    body: &[u8],
) -> Value {
    json!({
        "id": id,
        "project_id": project,
        "name": format!("{project} {id}"),
        "version_number": id,
        "version_type": "release",
        "game_versions": game_versions,
        "loaders": loaders,
        "date_published": "2026-01-01T00:00:00Z",
        "files": [{
            "url": format!("{}/files/{file_name}", server.uri()),
            "filename": file_name,
            "primary": true,
            "size": body.len(),
            "hashes": { "sha1": sha1_hex(body), "sha512": sha512_hex(body) }
        }],
        "dependencies": []
    })
}

fn project_json(id: &str, slug: &str, title: &str, project_type: &str) -> Value {
    json!({
        "id": id,
        "slug": slug,
        "title": title,
        "description": format!("{title} summary"),
        "body": format!("# {title}\n\nLong text."),
        "project_type": project_type,
        "categories": ["optimization", "fabric"],
        "client_side": "required",
        "server_side": "optional",
        "downloads": 1234,
        "followers": 56,
        "icon_url": "https://cdn.modrinth.com/data/x/icon.png",
        "license": { "id": "LGPL-3.0-only", "name": "GNU LGPL 3.0" },
        "source_url": "https://github.com/example/x",
        "issues_url": null,
        "wiki_url": null,
        "gallery": []
    })
}

#[tokio::test]
async fn rate_limits_say_how_long_to_wait() {
    let server = MockServer::start().await;
    for (id, header, expected) in [
        ("aaa", Some("12"), "12"),
        ("bbb", Some("999"), "60"),
        ("ccc", None, "10"),
    ] {
        let mut response = ResponseTemplate::new(429);
        if let Some(h) = header {
            response = response.insert_header("x-ratelimit-reset", h);
        }
        Mock::given(method("GET"))
            .and(path(format!("/v2/project/{id}")))
            .respond_with(response)
            .mount(&server)
            .await;
        let error = api(&server).project(id).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::BrowseRateLimited, "{id}");
        assert_eq!(error.params["seconds"], expected, "{id}");
    }
}

#[tokio::test]
async fn missing_projects_and_server_trouble_have_their_own_codes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/project/gone"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v2/project/broken"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert_eq!(
        api(&server).project("gone").await.unwrap_err().code,
        ErrorCode::ContentNotFound
    );
    let trouble = api(&server).project("broken").await.unwrap_err();
    assert_eq!(trouble.code, ErrorCode::HostTrouble);
    assert_eq!(trouble.params["status"], "503");
    assert!(
        !trouble.message.contains("/v2/"),
        "messages name hosts only"
    );
    assert_eq!(
        api(&server).project("../x").await.unwrap_err().code,
        ErrorCode::ContentNotFound
    );
    assert_eq!(hits(&server, "/v2/project/../x").await, 0);
}

#[tokio::test]
async fn an_unreachable_host_is_host_unreachable() {
    let closed = Api::new(
        launcher_core::http::client("test").unwrap(),
        "http://127.0.0.1:9/v2",
    );
    let error = closed.project("abc").await.unwrap_err();
    assert_eq!(error.code, ErrorCode::HostUnreachable);
    assert_eq!(error.params["host"], "127.0.0.1");
}

#[tokio::test]
async fn junk_or_oversized_answers_are_unexpected_data() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/project/junk"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>not json</html>"))
        .mount(&server)
        .await;
    serve_json(
        &server,
        "/v2/project/big",
        project_json("big", "big", "Big", "mod"),
    )
    .await;
    assert_eq!(
        api(&server).project("junk").await.unwrap_err().code,
        ErrorCode::MetaUnexpected
    );
    assert_eq!(
        api(&server)
            .with_limit(16)
            .project("big")
            .await
            .unwrap_err()
            .code,
        ErrorCode::MetaUnexpected
    );
    assert_eq!(api(&server).project("big").await.unwrap().title, "Big");
}

#[tokio::test]
async fn versions_send_loader_and_game_filters_as_json_arrays() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/project/sodium/version"))
        .and(query_param("loaders", r#"["quilt","fabric"]"#))
        .and(query_param("game_versions", r#"["1.20.1"]"#))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!([version_json(
                &server,
                "v1",
                "sodium",
                &["1.20.1"],
                &["fabric"],
                "sodium.jar",
                b"jar"
            )])),
        )
        .mount(&server)
        .await;
    let list = api(&server)
        .versions("sodium", &["quilt", "fabric"], Some("1.20.1"))
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].files[0].filename, "sodium.jar");
    assert!(list[0].dependencies.is_empty());
}

#[tokio::test]
async fn version_files_posts_sha1_hashes() {
    let server = MockServer::start().await;
    let hash = sha1_hex(b"jar");
    let mut version = version_json(
        &server,
        "v1",
        "sodium",
        &["1.20.1"],
        &["fabric"],
        "sodium.jar",
        b"jar",
    );
    version["dependencies"] =
        json!([{ "project_id": "other", "version_id": null, "dependency_type": "incompatible" }]);
    Mock::given(method("POST"))
        .and(path("/v2/version_files"))
        .and(body_json(
            json!({ "hashes": [hash.clone()], "algorithm": "sha1" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ hash.clone(): version })))
        .mount(&server)
        .await;
    let found = api(&server)
        .version_files(std::slice::from_ref(&hash))
        .await
        .unwrap();
    assert_eq!(found[&hash].project_id, "sodium");
    assert_eq!(
        found[&hash].dependencies[0].dependency_type,
        DependencyType::Incompatible
    );
}

use launcher_core::browse::{
    compat::ContentKind,
    search::{self, SearchQuery, SortIndex},
};

fn hit(id: &str, project_type: &str, client_side: &str) -> Value {
    json!({
        "project_id": id,
        "project_type": project_type,
        "slug": id,
        "author": "someone",
        "title": format!("Title {id}"),
        "description": "Summary",
        "categories": ["optimization", "fabric"],
        "display_categories": ["optimization", "fabric", "iris"],
        "downloads": 10,
        "follows": 2,
        "icon_url": null,
        "client_side": client_side
    })
}

#[tokio::test]
async fn search_drops_server_only_hits_and_loader_tags() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v2/search"))
        .and(query_param("index", "downloads"))
        .and(query_param("limit", "20"))
        .and(query_param("query", "sodium"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "hits": [hit("a", "mod", "required"), hit("b", "mod", "unsupported"), hit("c", "plugin", "required")],
            "offset": 0,
            "limit": 20,
            "total_hits": 3
        })))
        .mount(&server)
        .await;
    let page = search::search(
        &api(&server),
        &SearchQuery {
            query: "sodium".into(),
            kind: ContentKind::Mod,
            minecraft: None,
            loader: None,
            categories: vec![],
            sort: SortIndex::Downloads,
            offset: 0,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.total, 3);
    assert_eq!(page.hits.len(), 1);
    assert_eq!(page.hits[0].categories, vec!["optimization".to_string()]);
    assert_eq!(page.hits[0].kind, ContentKind::Mod);
    let json = serde_json::to_value(&page).unwrap();
    assert_eq!(json["hits"][0]["projectId"], "a");
    assert_eq!(json["hits"][0]["iconUrl"], Value::Null);
}

#[tokio::test]
async fn categories_are_filtered_to_the_kind() {
    let server = MockServer::start().await;
    serve_json(
        &server,
        "/v2/tag/category",
        json!([
            { "icon": "", "name": "technology", "project_type": "mod", "header": "categories" },
            { "icon": "", "name": "adventure", "project_type": "mod", "header": "categories" },
            { "icon": "", "name": "16x", "project_type": "resourcepack", "header": "resolutions" },
            { "icon": "", "name": "adventure", "project_type": "mod", "header": "categories" }
        ]),
    )
    .await;
    assert_eq!(
        search::categories(&api(&server), ContentKind::Mod)
            .await
            .unwrap(),
        vec!["adventure".to_string(), "technology".into()]
    );
    assert_eq!(
        search::categories(&api(&server), ContentKind::ResourcePack)
            .await
            .unwrap(),
        vec!["16x".to_string()]
    );
}

use launcher_core::{
    browse::{compat::Target, project},
    meta::loaders::LoaderKind,
};

#[tokio::test]
async fn a_project_page_has_its_owner_and_featured_gallery_first() {
    let server = MockServer::start().await;
    let mut p = project_json("AANobbMI", "sodium", "Sodium", "mod");
    p["gallery"] = json!([
        { "url": "https://cdn.modrinth.com/a.png", "featured": false, "title": "A" },
        { "url": "https://cdn.modrinth.com/b.png", "featured": true, "title": null }
    ]);
    serve_json(&server, "/v2/project/sodium", p).await;
    serve_json(
        &server,
        "/v2/project/AANobbMI/members",
        json!([
            { "role": "Contributor", "user": { "username": "helper" }, "ordering": 1 },
            { "role": "Owner", "user": { "username": "jellysquid" }, "ordering": 0 }
        ]),
    )
    .await;
    let detail = project::project(&api(&server), "sodium").await.unwrap();
    assert_eq!(detail.author.as_deref(), Some("jellysquid"));
    assert_eq!(detail.gallery[0].url, "https://cdn.modrinth.com/b.png");
    assert_eq!(detail.categories, vec!["optimization".to_string()]);
    assert_eq!(detail.license.as_deref(), Some("GNU LGPL 3.0"));
    let json = serde_json::to_value(&detail).unwrap();
    assert_eq!(json["clientSide"], "required");
    assert_eq!(json["kind"], "mod");
}

#[tokio::test]
async fn a_project_without_members_or_of_another_type() {
    let server = MockServer::start().await;
    serve_json(
        &server,
        "/v2/project/lonely",
        project_json("lonely", "lonely", "Lonely", "resourcepack"),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/v2/project/lonely/members"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    serve_json(
        &server,
        "/v2/project/plug",
        project_json("plug", "plug", "Plug", "plugin"),
    )
    .await;
    assert_eq!(
        project::project(&api(&server), "lonely")
            .await
            .unwrap()
            .author,
        None
    );
    assert_eq!(
        project::project(&api(&server), "plug")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ContentNotFound
    );
}

#[tokio::test]
async fn versions_for_an_instance_fit_it_and_come_newest_first() {
    let server = MockServer::start().await;
    serve_json(
        &server,
        "/v2/project/sodium",
        project_json("sodium", "sodium", "Sodium", "mod"),
    )
    .await;
    let mut old = version_json(
        &server,
        "old",
        "sodium",
        &["1.20.1"],
        &["fabric"],
        "s-old.jar",
        b"old",
    );
    old["date_published"] = json!("2025-01-01T00:00:00Z");
    let new = version_json(
        &server,
        "new",
        "sodium",
        &["1.20.1"],
        &["fabric"],
        "s-new.jar",
        b"newer",
    );
    let forge_only = version_json(
        &server,
        "forge",
        "sodium",
        &["1.20.1"],
        &["forge"],
        "s-forge.jar",
        b"f",
    );
    Mock::given(method("GET"))
        .and(path("/v2/project/sodium/version"))
        .and(query_param("loaders", r#"["fabric"]"#))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([old, forge_only, new])))
        .mount(&server)
        .await;
    let target = Target {
        minecraft: "1.20.1".into(),
        loader: Some(LoaderKind::Fabric),
    };
    let list = project::versions(&api(&server), "sodium", Some(&target))
        .await
        .unwrap();
    assert_eq!(
        list.iter().map(|v| v.id.as_str()).collect::<Vec<_>>(),
        vec!["new", "old"]
    );
    assert_eq!(list[0].size, Some(5));
    assert_eq!(
        serde_json::to_value(&list[0]).unwrap()["versionType"],
        "release"
    );
}

use std::{collections::BTreeMap, path::Path};

use launcher_core::{
    browse::identify::{identify, FileHashes},
    download::Hashes,
    sources::FileMode,
    sync::lockfile::{Managed, ManagedFile},
};

fn write(dir: &Path, rel: &str, body: &[u8]) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn managed(paths: &[(&str, &[u8])]) -> Managed {
    Managed {
        version: Managed::VERSION,
        stamp: "s".into(),
        pack_version: "1".into(),
        choices: BTreeMap::new(),
        files: paths
            .iter()
            .map(|(p, body)| {
                (
                    p.to_string(),
                    ManagedFile {
                        name: p.to_string(),
                        sha1: sha1_hex(body),
                        size: body.len() as u64,
                        mode: FileMode::Replace,
                        declared: Hashes::default(),
                    },
                )
            })
            .collect(),
    }
}

#[tokio::test]
async fn identify_names_projects_by_hash_and_remembers_hashes() {
    let server = MockServer::start().await;
    let game = tempfile::tempdir().unwrap();
    write(game.path(), "mods/added.jar", b"added");
    write(game.path(), "mods/off.jar.disabled", b"off");
    write(game.path(), "mods/pack.jar", b"pack");
    write(game.path(), "mods/readme.txt", b"ignored");
    write(game.path(), "resourcepacks/faithful.zip", b"rp");
    let lock = managed(&[("mods/pack.jar", b"pack")]);
    let answer = json!({
        sha1_hex(b"added"): version_json(&server, "v1", "lithium", &["1.20.1"], &["fabric"], "added.jar", b"added"),
        sha1_hex(b"pack"): version_json(&server, "v2", "sodium", &["1.20.1"], &["fabric"], "pack.jar", b"pack"),
        sha1_hex(b"rp"): version_json(&server, "v3", "faithful", &["1.20.1"], &["minecraft"], "faithful.zip", b"rp")
    });
    Mock::given(method("POST"))
        .and(path("/v2/version_files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(&server)
        .await;
    let mut hashes = FileHashes::default();
    let installed = identify(&api(&server), game.path(), Some(&lock), &mut hashes)
        .await
        .unwrap();
    assert_eq!(
        installed.project_ids(),
        vec!["faithful".to_string(), "lithium".into(), "sodium".into()]
    );
    assert!(installed.projects["sodium"].managed);
    assert!(!installed.projects["lithium"].managed);
    assert_eq!(
        installed.projects["faithful"].path,
        "resourcepacks/faithful.zip"
    );
    // The managed jar's sha1 came from the lockfile; the other three were hashed once.
    assert_eq!(hashes.hashed(), 3);
    identify(&api(&server), game.path(), Some(&lock), &mut hashes)
        .await
        .unwrap();
    assert_eq!(hashes.hashed(), 3, "unchanged files aren't hashed again");
    let sent: Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(sent["hashes"].as_array().unwrap().len(), 4);

    let empty = tempfile::tempdir().unwrap();
    let before = hits(&server, "/v2/version_files").await;
    assert!(identify(&api(&server), empty.path(), None, &mut hashes)
        .await
        .unwrap()
        .projects
        .is_empty());
    assert_eq!(
        hits(&server, "/v2/version_files").await,
        before,
        "nothing to ask about"
    );
}

async fn serve_version_files(server: &MockServer, answer: Value) {
    Mock::given(method("POST"))
        .and(path("/v2/version_files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
}

#[tokio::test]
async fn identify_prefers_the_managed_file_then_the_smallest_path() {
    // Two versions of one project: the pack's jar (managed) sorts after the player's old one.
    let server = MockServer::start().await;
    let game = tempfile::tempdir().unwrap();
    write(game.path(), "mods/a-old.jar", b"old");
    write(game.path(), "mods/z-pack.jar", b"pack");
    let lock = managed(&[("mods/z-pack.jar", b"pack")]);
    serve_version_files(
        &server,
        json!({
            sha1_hex(b"old"): version_json(&server, "v1", "sodium", &["1.20.1"], &["fabric"], "a-old.jar", b"old"),
            sha1_hex(b"pack"): version_json(&server, "v2", "sodium", &["1.20.1"], &["fabric"], "z-pack.jar", b"pack")
        }),
    )
    .await;
    let mut hashes = FileHashes::default();
    for _ in 0..3 {
        let installed = identify(&api(&server), game.path(), Some(&lock), &mut hashes)
            .await
            .unwrap();
        assert_eq!(installed.project_ids(), vec!["sodium".to_string()]);
        assert!(installed.projects["sodium"].managed);
        assert_eq!(installed.projects["sodium"].path, "mods/z-pack.jar");
    }

    // A player's copy of the same bytes, earlier in path order, doesn't hide the managed one.
    let copy = tempfile::tempdir().unwrap();
    write(copy.path(), "mods/a-copy.jar", b"pack");
    write(copy.path(), "mods/z-pack.jar", b"pack");
    let installed = identify(&api(&server), copy.path(), Some(&lock), &mut hashes)
        .await
        .unwrap();
    assert!(installed.projects["sodium"].managed);
    assert_eq!(installed.projects["sodium"].path, "mods/z-pack.jar");

    // Two unmanaged files of one project: the smaller path wins.
    let plain = tempfile::tempdir().unwrap();
    write(plain.path(), "mods/b.jar", b"old");
    write(plain.path(), "mods/a.jar", b"pack");
    for _ in 0..3 {
        let installed = identify(&api(&server), plain.path(), None, &mut hashes)
            .await
            .unwrap();
        assert!(!installed.projects["sodium"].managed);
        assert_eq!(installed.projects["sodium"].path, "mods/a.jar");
    }
}

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use launcher_core::{
    browse::{
        compat::ContentKind as Kind,
        install::install,
        plan::{ItemReason, PlanItem, PlannedFile},
    },
    download::{cache::ObjectCache, Downloader},
    events::{Event, EventSink, TaskId},
};

fn downloader() -> Downloader {
    Downloader::new(launcher_core::http::client("test").unwrap())
        .with_backoff(Duration::from_millis(1))
}

async fn host_file(server: &MockServer, name: &str, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(format!("/files/{name}")))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

fn planned(server: &MockServer, title: &str, kind: Kind, name: &str, body: &[u8]) -> PlanItem {
    PlanItem {
        project_id: title.to_lowercase(),
        title: title.into(),
        kind,
        version_id: "v1".into(),
        version_number: "1.0".into(),
        reason: ItemReason::Requested,
        file: PlannedFile {
            url: format!("{}/files/{name}", server.uri()),
            file_name: name.into(),
            size: body.len() as u64,
            hashes: Hashes {
                sha1: Some(sha1_hex(body)),
                sha512: Some(sha512_hex(body)),
                ..Hashes::default()
            },
        },
    }
}

fn recorder() -> (EventSink, Arc<Mutex<Vec<Event>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let seen = events.clone();
    (Arc::new(move |e| seen.lock().unwrap().push(e)), events)
}

#[tokio::test]
async fn content_goes_into_its_folder_without_overwriting() {
    let server = MockServer::start().await;
    host_file(&server, "sodium.jar", b"sodium").await;
    host_file(&server, "faithful.zip", b"faithful").await;
    let root = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(root.path().join("objects"));
    let game = root.path().join("game");
    let items = vec![
        planned(&server, "Sodium", Kind::Mod, "sodium.jar", b"sodium"),
        planned(
            &server,
            "Faithful",
            Kind::ResourcePack,
            "faithful.zip",
            b"faithful",
        ),
    ];
    let (sink, events) = recorder();
    let task = TaskId::new("install_content").with("name", "Sodium");
    let added = install(&cache, &downloader(), &game, &items, None, &task, &sink)
        .await
        .unwrap();
    assert_eq!(added, vec!["sodium.jar".to_string(), "faithful.zip".into()]);
    assert_eq!(
        std::fs::read(game.join("mods/sodium.jar")).unwrap(),
        b"sodium"
    );
    assert_eq!(
        std::fs::read(game.join("resourcepacks/faithful.zip")).unwrap(),
        b"faithful"
    );
    let last = events.lock().unwrap().last().cloned().unwrap();
    assert_eq!(
        last,
        Event::Progress {
            task: task.clone(),
            done: 2,
            total: 2,
            bytes: 14
        }
    );
    assert!(
        std::fs::read_dir(game.join("mods")).unwrap().count() == 1,
        "no temp files left"
    );

    // The same file again is skipped; a different file with that name is refused.
    assert!(install(
        &cache,
        &downloader(),
        &game,
        &items[..1],
        None,
        &task,
        &sink
    )
    .await
    .unwrap()
    .is_empty());
    std::fs::write(game.join("mods/sodium.jar"), b"player's own").unwrap();
    let exists = install(
        &cache,
        &downloader(),
        &game,
        &items[..1],
        None,
        &task,
        &sink,
    )
    .await
    .unwrap_err();
    assert_eq!(exists.code, ErrorCode::ModExists);
    assert_eq!(
        std::fs::read(game.join("mods/sodium.jar")).unwrap(),
        b"player's own"
    );

    // A disabled twin counts as present.
    host_file(&server, "lithium.jar", b"lithium").await;
    std::fs::write(game.join("mods/lithium.jar.disabled"), b"old").unwrap();
    let twin = planned(&server, "Lithium", Kind::Mod, "lithium.jar", b"lithium");
    assert_eq!(
        install(&cache, &downloader(), &game, &[twin], None, &task, &sink)
            .await
            .unwrap_err()
            .code,
        ErrorCode::ModExists
    );
}

#[tokio::test]
async fn pack_paths_and_corrupt_downloads_are_refused() {
    let server = MockServer::start().await;
    host_file(&server, "jei.jar", b"tampered").await;
    let root = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(root.path().join("objects"));
    let game = root.path().join("game");
    let (sink, _) = recorder();
    let task = TaskId::new("install_content");
    let lock = managed(&[("mods/JEI.jar", b"jei")]);
    let jei = planned(&server, "JEI", Kind::Mod, "jei.jar", b"jei");
    let refused = install(
        &cache,
        &downloader(),
        &game,
        std::slice::from_ref(&jei),
        Some(&lock),
        &task,
        &sink,
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code, ErrorCode::ModManaged);
    assert_eq!(
        hits(&server, "/files/jei.jar").await,
        0,
        "refused before downloading"
    );
    let corrupt = install(&cache, &downloader(), &game, &[jei], None, &task, &sink)
        .await
        .unwrap_err();
    assert_eq!(corrupt.code, ErrorCode::DownloadCorrupt);
    assert!(!game.join("mods/jei.jar").exists());
}

#[tokio::test]
async fn hostile_file_names_are_refused_before_downloading() {
    let server = MockServer::start().await;
    let root = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(root.path().join("objects"));
    let game = root.path().join("game");
    let (sink, _) = recorder();
    let task = TaskId::new("install_content");
    for name in [
        "../mods/x.jar",
        "CON.jar",
        "a/b.jar",
        r"a\b.jar",
        "x.jar.",
        "x.zip",
        ".jar",
        "",
    ] {
        let item = planned(&server, "X", Kind::Mod, name, b"x");
        let error = install(&cache, &downloader(), &game, &[item], None, &task, &sink)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ModNameInvalid, "{name:?}");
    }
    let shader = planned(&server, "S", Kind::Shader, "s.jar", b"s");
    assert_eq!(
        install(&cache, &downloader(), &game, &[shader], None, &task, &sink)
            .await
            .unwrap_err()
            .code,
        ErrorCode::ModNameInvalid
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(!game.exists() || std::fs::read_dir(&game).unwrap().next().is_none());
}

#[tokio::test]
async fn conflicts_are_found_before_anything_is_written() {
    let server = MockServer::start().await;
    let root = tempfile::tempdir().unwrap();
    let cache = ObjectCache::new(root.path().join("objects"));
    let game = root.path().join("game");
    write(&game, "mods/own.jar", b"player's own");
    write(&game, "mods/twin.jar.disabled", b"old");
    let (sink, _) = recorder();
    let task = TaskId::new("install_content");
    for clash in ["own.jar", "twin.jar"] {
        let items = vec![
            planned(&server, "Good", Kind::Mod, "good.jar", b"good"),
            planned(&server, "Clash", Kind::Mod, clash, b"clash"),
        ];
        let error = install(&cache, &downloader(), &game, &items, None, &task, &sink)
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ModExists, "{clash}");
        assert!(!game.join("mods/good.jar").exists(), "{clash}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

use launcher_core::browse::images::{ImageCache, ImageKind};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[tokio::test]
async fn images_come_from_the_cdn_once_and_only_when_real() {
    let server = MockServer::start().await;
    host_file(&server, "icon.png", PNG).await;
    host_file(&server, "page.html", b"<html>login</html>").await;
    host_file(&server, "anim.gif", b"GIF89a\x01\x00\x01\x00").await;
    host_file(&server, "big.png", &[PNG, &[0u8; 64]].concat()).await;
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(
        launcher_core::http::client("test").unwrap(),
        dir.path(),
        &server.uri(),
    )
    .with_limits(32, 1 << 20, 1 << 20);
    let url = |name: &str| format!("{}/files/{name}", server.uri());

    let icon = cache.get(&url("icon.png"), ImageKind::Icon).await.unwrap();
    assert_eq!((icon.bytes.as_slice(), icon.mime), (PNG, "image/png"));
    assert_eq!(
        cache
            .get(&url("anim.gif"), ImageKind::Icon)
            .await
            .unwrap()
            .mime,
        "image/gif"
    );
    for refused in ["page.html", "big.png"] {
        assert_eq!(
            cache
                .get(&url(refused), ImageKind::Icon)
                .await
                .unwrap_err()
                .code,
            ErrorCode::ImageRefused,
            "{refused}"
        );
    }
    assert_eq!(
        cache
            .get(&url("big.png"), ImageKind::Gallery)
            .await
            .unwrap()
            .bytes
            .len(),
        80
    );
    assert_eq!(
        cache
            .get("https://i.imgur.com/x.png", ImageKind::Icon)
            .await
            .unwrap_err()
            .code,
        ErrorCode::ImageRefused
    );

    // Offline, a cached image still comes back.
    server.reset().await;
    assert_eq!(
        cache
            .get(&url("icon.png"), ImageKind::Icon)
            .await
            .unwrap()
            .bytes,
        PNG
    );
}

#[tokio::test]
async fn the_image_cache_prunes_the_oldest_first() {
    let server = MockServer::start().await;
    for name in ["a.png", "b.png", "c.png"] {
        host_file(&server, name, &[PNG, &[0u8; 40]].concat()).await;
    }
    let dir = tempfile::tempdir().unwrap();
    let cache = ImageCache::new(
        launcher_core::http::client("test").unwrap(),
        dir.path(),
        &server.uri(),
    )
    .with_limits(1 << 20, 1 << 20, 120);
    let url = |name: &str| format!("{}/files/{name}", server.uri());
    cache.get(&url("a.png"), ImageKind::Icon).await.unwrap();
    // Make "a" clearly the oldest, whatever the file system's timestamp resolution.
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let file = std::fs::File::options()
            .write(true)
            .open(entry.unwrap().path())
            .unwrap();
        file.set_modified(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
    }
    cache.get(&url("b.png"), ImageKind::Icon).await.unwrap();
    cache.get(&url("c.png"), ImageKind::Icon).await.unwrap();
    let total: u64 = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().metadata().unwrap().len())
        .sum();
    assert!(total <= 120, "{total}");
    server.reset().await;
    assert!(
        cache.get(&url("a.png"), ImageKind::Icon).await.is_err(),
        "a was pruned"
    );
    assert!(
        cache.get(&url("c.png"), ImageKind::Icon).await.is_ok(),
        "c is the newest"
    );
}
