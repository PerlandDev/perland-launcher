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
