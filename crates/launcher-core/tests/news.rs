//! A server's news against a wiremock Engine. The mock server is both the invite's site
//! (`/config.json`) and the Engine (`/api/v1/invites/<token>/…`). Fake tokens and ids only.
use std::time::Duration;

use launcher_core::{
    invite::{InviteClient, InviteLink},
    news::NewsClient,
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const NEWS: &str = "/api/v1/invites/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/news";
const POST_ID: &str = "6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5e";
const MINUTE: Duration = Duration::from_secs(60);

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n and some pixels";
const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0 and some pixels";

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn link(server: &MockServer) -> InviteLink {
    InviteLink::parse(&format!("{}/{TOKEN}", server.uri())).unwrap()
}

fn invites() -> InviteClient {
    InviteClient::new(launcher_core::http::client("test").unwrap())
}

fn client() -> NewsClient {
    NewsClient::new(invites())
}

async fn mount_config(server: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/config.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })),
        )
        .mount(server)
        .await;
}

async fn mount_news(server: &MockServer, data: Value) {
    Mock::given(method("GET"))
        .and(path(NEWS))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(data)))
        .mount(server)
        .await;
}

async fn mount_status(server: &MockServer, status: u16, body: &str) {
    Mock::given(method("GET"))
        .and(path(NEWS))
        .respond_with(ResponseTemplate::new(status).set_body_string(body))
        .mount(server)
        .await;
}

async fn requests_to(server: &MockServer, route: &str) -> Vec<wiremock::Request> {
    server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.url.path() == route)
        .collect()
}

fn minimal_page() -> Value {
    json!({ "posts": [{
        "id": POST_ID, "type": "update", "title": "t", "createdAt": "2026-10-16T18:00:00Z"
    }] })
}

#[tokio::test]
async fn parses_a_full_and_a_minimal_page() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_news(
        &server,
        json!({
            "posts": [
                {
                    "id": POST_ID,
                    "type": "event",
                    "title": "Build contest",
                    "body": "Theme: castles.\nPrizes for the top 3. <b>not bold</b>",
                    "startsAt": "2026-10-24T20:00:00Z",
                    "endsAt": "2026-10-24T22:00:00Z",
                    "imageVersion": "a1b2c3d4e5f60718",
                    "createdAt": "2026-10-16T18:00:00Z",
                    "updatedAt": "2026-10-16T18:05:00Z",
                    "somethingNew": true
                },
                {
                    "id": "7a2d3e4f-5c6b-4d7e-9f80-a12b3c4d5e6f",
                    "type": "modpack",
                    "title": "Modpack updated",
                    "body": "3 mods added.",
                    "modpack": {
                        "fromCommit": "a".repeat(40),
                        "toCommit": "b".repeat(40),
                        "changes": {
                            "added": { "count": 3, "names": ["Create", "Jade", "Sodium"] },
                            "removed": { "count": 1, "names": ["Optifine"] },
                            "updated": { "count": 0, "names": [] }
                        }
                    },
                    "createdAt": "2026-10-15T12:00:00Z",
                    "updatedAt": "2026-10-15T12:00:00Z"
                },
                {
                    "id": "8b3e4f50-6d7c-4e8f-a091-b23c4d5e6f70",
                    "type": "modpack",
                    "title": "Modpack updated",
                    "body": "The modpack changed.",
                    "modpack": { "fromCommit": "a", "toCommit": "b" },
                    "createdAt": "2026-10-14T12:00:00Z"
                }
            ],
            "nextBefore": "41"
        }),
    )
    .await;
    let page = client()
        .page(&link(&server), 20, None)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(page.next_before.as_deref(), Some("41"));
    assert_eq!(page.posts.len(), 3);
    let event = &page.posts[0];
    assert_eq!(event.id, POST_ID);
    assert_eq!(event.kind, "event");
    assert_eq!(event.title, "Build contest");
    assert!(event.body.contains("<b>not bold</b>"));
    assert_eq!(event.starts_at.as_deref(), Some("2026-10-24T20:00:00Z"));
    assert_eq!(event.ends_at.as_deref(), Some("2026-10-24T22:00:00Z"));
    assert_eq!(event.image_version.as_deref(), Some("a1b2c3d4e5f60718"));
    assert_eq!(event.created_at, "2026-10-16T18:00:00Z");
    assert!(event.modpack.is_none());

    let changes = page.posts[1]
        .modpack
        .as_ref()
        .and_then(|m| m.changes.as_ref())
        .unwrap();
    assert_eq!(changes.added.count, 3);
    assert_eq!(changes.added.names, ["Create", "Jade", "Sodium"]);
    assert_eq!(changes.removed.names, ["Optifine"]);
    assert_eq!(changes.updated.count, 0);
    assert!(changes.updated.names.is_empty());

    // A modpack post whose pack could not be read has no `changes`.
    let bare = page.posts[2].modpack.as_ref().unwrap();
    assert!(bare.changes.is_none());
}

#[tokio::test]
async fn parses_a_minimal_page() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_news(&server, minimal_page()).await;
    let page = client()
        .page(&link(&server), 20, None)
        .await
        .unwrap()
        .unwrap();
    assert!(page.next_before.is_none());
    let post = &page.posts[0];
    assert_eq!((post.kind.as_str(), post.title.as_str()), ("update", "t"));
    assert_eq!(post.body, "");
    assert!(post.starts_at.is_none() && post.ends_at.is_none());
    assert!(post.image_version.is_none() && post.modpack.is_none());
    assert_eq!(post.created_at, "2026-10-16T18:00:00Z");
    // A page of no posts is a page, not "no news" (the app decides what that means).
    let empty = MockServer::start().await;
    mount_config(&empty).await;
    mount_news(&empty, json!({ "posts": [] })).await;
    let page = client()
        .page(&link(&empty), 20, None)
        .await
        .unwrap()
        .unwrap();
    assert!(page.posts.is_empty());
}

#[tokio::test]
async fn limit_and_before_pass_through() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_news(&server, minimal_page()).await;
    let client = client();
    let link = link(&server);
    client.page(&link, 20, Some("41")).await.unwrap();
    client.page(&link, 0, Some("42")).await.unwrap();
    client.page(&link, 99, Some("43")).await.unwrap();
    // Not digits: no later page to ask for (asking without it would hand back page one again).
    assert!(client.page(&link, 20, Some("4x")).await.unwrap().is_none());
    assert!(client.page(&link, 20, Some("")).await.unwrap().is_none());
    let hits = requests_to(&server, NEWS).await;
    let queries: Vec<_> = hits.iter().map(|r| r.url.query().unwrap_or("")).collect();
    assert_eq!(
        queries,
        [
            "limit=20&before=41",
            "limit=1&before=42",
            "limit=50&before=43",
        ]
    );
}

#[tokio::test]
async fn no_news_is_none() {
    for (status, body) in [
        (404, "not found".to_string()),
        (
            503,
            json!({ "success": false, "data": { "code": "news_disabled" } }).to_string(),
        ),
        (503, String::new()),
    ] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_status(&server, status, &body).await;
        let page = client().page(&link(&server), 20, None).await.unwrap();
        assert!(page.is_none(), "{status} {body}");
        // A later page of a server with no news is the same answer.
        let page = client().page(&link(&server), 20, Some("5")).await.unwrap();
        assert!(page.is_none(), "{status} {body} (before)");
    }
}

#[tokio::test]
async fn troubles_map_to_codes() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_status(&server, 500, "boom").await;
    let err = client().page(&link(&server), 20, None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::HostTrouble);

    let big = MockServer::start().await;
    mount_config(&big).await;
    mount_status(&big, 200, &"x".repeat(256 * 1024 + 1)).await;
    let err = client().page(&link(&big), 20, None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);

    let junk = MockServer::start().await;
    mount_config(&junk).await;
    mount_status(&junk, 200, "<html>nope</html>").await;
    let err = client().page(&link(&junk), 20, None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);

    let empty = MockServer::start().await;
    mount_config(&empty).await;
    mount_status(&empty, 200, r#"{"success":true,"data":null}"#).await;
    let err = client().page(&link(&empty), 20, None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::MetaUnexpected);

    // An Engine nobody answers for.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let nowhere = InviteLink::parse(&format!("http://127.0.0.1:{port}/{TOKEN}")).unwrap();
    let err = client().page(&nowhere, 20, None).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Offline);
}

#[tokio::test]
async fn a_page_is_shared_for_a_minute() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_news(&server, minimal_page()).await;
    let link = link(&server);

    let shared = client();
    let (a, b) = tokio::join!(shared.page(&link, 20, None), shared.page(&link, 20, None));
    assert_eq!(a.unwrap().unwrap().posts.len(), 1);
    assert_eq!(b.unwrap().unwrap().posts.len(), 1);
    shared.page(&link, 20, None).await.unwrap();
    assert_eq!(requests_to(&server, NEWS).await.len(), 1);

    // A page with `before` is never remembered.
    shared.page(&link, 20, Some("41")).await.unwrap();
    shared.page(&link, 20, Some("41")).await.unwrap();
    assert_eq!(requests_to(&server, NEWS).await.len(), 3);

    // After the memo time the Engine is asked again.
    // Wide windows: a loaded runner may pause the task between calls.
    let quick = NewsClient::new(invites()).with_timing(Duration::from_millis(500), MINUTE);
    let other = MockServer::start().await;
    mount_config(&other).await;
    mount_news(&other, minimal_page()).await;
    let other_link = self::link(&other);
    quick.page(&other_link, 20, None).await.unwrap();
    quick.page(&other_link, 20, None).await.unwrap();
    assert_eq!(requests_to(&other, NEWS).await.len(), 1);
    tokio::time::sleep(Duration::from_millis(650)).await;
    quick.page(&other_link, 20, None).await.unwrap();
    assert_eq!(requests_to(&other, NEWS).await.len(), 2);
}

#[tokio::test]
async fn no_news_is_remembered_too() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_status(&server, 404, "not found").await;
    let client = client();
    let link = link(&server);
    assert!(client.page(&link, 20, None).await.unwrap().is_none());
    assert!(client.page(&link, 20, None).await.unwrap().is_none());
    assert_eq!(requests_to(&server, NEWS).await.len(), 1);
}

#[tokio::test]
async fn a_rate_limit_backs_off_quietly() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_status(&server, 429, "slow down").await;
    // Ready before the 429, so the back-off window spans in-memory calls only.
    let elsewhere = MockServer::start().await;
    mount_config(&elsewhere).await;
    mount_news(&elsewhere, minimal_page()).await;
    let other_link = self::link(&elsewhere);
    let client = NewsClient::new(invites()).with_timing(MINUTE, Duration::from_millis(500));
    let link = link(&server);

    assert!(client.page(&link, 20, None).await.unwrap().is_none());
    assert_eq!(requests_to(&server, NEWS).await.len(), 1);

    // Inside the back-off: a first page asks nobody, a later page says why.
    assert!(client.page(&link, 20, None).await.unwrap().is_none());
    let err = client.page(&link, 20, Some("41")).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteRateLimited);
    assert_eq!(requests_to(&server, NEWS).await.len(), 1);

    // The back-off covers every server of the client, not just this one.
    assert!(client.page(&other_link, 20, None).await.unwrap().is_none());
    assert!(requests_to(&elsewhere, NEWS).await.is_empty());

    // After it, the Engine is asked again.
    tokio::time::sleep(Duration::from_millis(650)).await;
    assert!(client.page(&other_link, 20, None).await.unwrap().is_some());
    assert_eq!(requests_to(&elsewhere, NEWS).await.len(), 1);
}

#[tokio::test]
async fn a_rate_limit_on_a_later_page_is_an_error_and_a_back_off() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_status(&server, 429, "slow down").await;
    let client = client();
    let link = link(&server);
    let err = client.page(&link, 20, Some("41")).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InviteRateLimited);
    assert!(client.page(&link, 20, None).await.unwrap().is_none());
    assert_eq!(requests_to(&server, NEWS).await.len(), 1);
}

#[tokio::test]
async fn no_error_names_the_token() {
    let mut errors = Vec::new();
    for status in [429, 500, 502] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_status(&server, status, TOKEN).await;
        errors.push(
            client()
                .page(&link(&server), 20, Some("1"))
                .await
                .unwrap_err(),
        );
    }
    for body in [format!("{{\"posts\":\"{TOKEN}\""), TOKEN.repeat(8000)] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_status(&server, 200, &body).await;
        errors.push(client().page(&link(&server), 20, None).await.unwrap_err());
    }
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let nowhere = InviteLink::parse(&format!("http://127.0.0.1:{port}/{TOKEN}")).unwrap();
    errors.push(client().page(&nowhere, 20, None).await.unwrap_err());
    assert_eq!(errors.len(), 6);
    for e in errors {
        assert!(!format!("{e:?}").contains(TOKEN), "{e:?}");
        assert!(!e.message.contains(TOKEN));
        assert!(!e.to_string().contains(TOKEN));
    }

    // Neither does the client's own Debug, once it has remembered an invite.
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_news(&server, minimal_page()).await;
    mount_image(&server, 200, PNG).await;
    let news = client();
    let link = link(&server);
    let page = news.page(&link, 20, None).await.unwrap().unwrap();
    news.image(&link, POST_ID, "v1").await.unwrap();
    assert!(!format!("{news:?}").contains(TOKEN));
    assert!(!format!("{page:?}").contains(TOKEN));
}

fn image_route(post_id: &str) -> String {
    format!("{NEWS}/{post_id}/image")
}

async fn mount_image(server: &MockServer, status: u16, body: &[u8]) {
    Mock::given(method("GET"))
        .and(path(image_route(POST_ID)))
        .respond_with(ResponseTemplate::new(status).set_body_bytes(body.to_vec()))
        .mount(server)
        .await;
}

#[tokio::test]
async fn image_checks_signature_size_and_caches() {
    // A PNG, then the cache.
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_image(&server, 200, PNG).await;
    let client = client();
    let link = link(&server);
    assert_eq!(
        client.image(&link, POST_ID, "v1").await.as_deref(),
        Some(PNG)
    );
    assert_eq!(
        client.image(&link, POST_ID, "v1").await.as_deref(),
        Some(PNG)
    );
    let hits = requests_to(&server, &image_route(POST_ID)).await;
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].url.query(), Some("v=v1"));
    // Another version is another picture.
    client.image(&link, POST_ID, "v2").await.unwrap();
    assert_eq!(requests_to(&server, &image_route(POST_ID)).await.len(), 2);

    // A JPEG.
    let jpeg = MockServer::start().await;
    mount_config(&jpeg).await;
    mount_image(&jpeg, 200, JPEG).await;
    assert_eq!(
        self::client()
            .image(&self::link(&jpeg), POST_ID, "v1")
            .await
            .as_deref(),
        Some(JPEG)
    );

    // A GIF, a 404 and a body past 2 MiB are nothing.
    let mut huge = PNG.to_vec();
    huge.resize((2 << 20) + 1, 0);
    let mut limit = PNG.to_vec();
    limit.resize(2 << 20, 0);
    for (status, body, shown) in [
        (200, b"GIF89a....".to_vec(), false),
        (404, b"not found".to_vec(), false),
        (200, huge, false),
        (200, limit, true),
    ] {
        let server = MockServer::start().await;
        mount_config(&server).await;
        mount_image(&server, status, &body).await;
        let got = self::client()
            .image(&self::link(&server), POST_ID, "v1")
            .await;
        assert_eq!(got.is_some(), shown, "{status} {}", body.len());
    }
}

#[tokio::test]
async fn image_ids_that_are_not_uuids_ask_nobody() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path_regex(".*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG.to_vec()))
        .mount(&server)
        .await;
    let client = client();
    let link = link(&server);
    for id in [
        "../x",
        "",
        "x",
        "6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5",
        "6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5g",
        "6f1c2d3e/4b5a-4c6d-8e7f-901a2b3c4d5e",
    ] {
        assert!(client.image(&link, id, "v1").await.is_none(), "{id:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_image_cache_keeps_64_pictures() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    Mock::given(method("GET"))
        .and(path_regex(r"^/api/v1/invites/.*/news/.*/image$"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(PNG.to_vec()))
        .mount(&server)
        .await;
    let client = client();
    let link = link(&server);
    let id = |n: usize| format!("00000000-0000-4000-8000-{n:012}");
    for n in 0..65 {
        client.image(&link, &id(n), "v1").await.unwrap();
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 66);
    // The newest is kept; the oldest went.
    client.image(&link, &id(64), "v1").await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 66);
    client.image(&link, &id(0), "v1").await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 67);
}

#[tokio::test]
async fn an_upper_case_id_is_asked_for_in_lower_case() {
    let server = MockServer::start().await;
    mount_config(&server).await;
    mount_image(&server, 200, PNG).await;
    let got = client()
        .image(&link(&server), &POST_ID.to_ascii_uppercase(), "v1")
        .await;
    assert_eq!(got.as_deref(), Some(PNG));
}
