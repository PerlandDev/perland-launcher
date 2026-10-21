//! The log stream against a wiremock Engine. Fake tokens only. The reconnect wait is short:
//! a paused clock would fire the HTTP client's connect timeout while a socket is pending.
use std::sync::{Arc, Mutex};

use launcher_core::{
    invite::Url,
    perland::{follow_logs, follow_logs_after, LogLine, SessionJwt, StackClient},
    ErrorCode,
};
use serde_json::json;
use tokio::sync::watch;
use wiremock::{
    matchers::{header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

const JWT: &str = "eyJ.a.b";
const ROUTE: &str = "/api/v1/stacks/survival/logs/stream";

fn client(server: &MockServer) -> StackClient {
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    StackClient::new(http, Url::parse(&server.uri()).unwrap())
}

fn log(ts: &str, text: &str) -> String {
    let data = json!({ "stream": "stdout", "timestamp": ts, "text": text });
    format!("event: log\ndata: {data}\n\n")
}

fn sse(body: String) -> ResponseTemplate {
    ResponseTemplate::new(200)
        .insert_header("content-type", "text/event-stream")
        .set_body_string(body)
}

fn jwt_source(
) -> impl Fn() -> launcher_core::sync::BoxFuture<'static, launcher_core::Result<SessionJwt>> {
    || Box::pin(async { Ok(SessionJwt::new(JWT)) })
}

#[tokio::test]
async fn follow_reads_lines_then_stops() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("tail", "200"))
        .and(query_param("timestamps", "true"))
        .and(header("x-user-token", JWT))
        .respond_with(sse(format!(
            "{}{}event: end\ndata: {{\"reason\":\"eof\"}}\n\n",
            log("2026-10-21T10:00:00.000000001Z", "§aHello"),
            log("2026-10-21T10:00:01.000000000Z", "World"),
        )))
        .mount(&server)
        .await;
    let (tx, rx) = watch::channel(false);
    let seen: Arc<Mutex<Vec<LogLine>>> = Arc::default();
    let sink = seen.clone();
    follow_logs(
        &client(&server),
        jwt_source(),
        "survival",
        move |lines| {
            sink.lock().unwrap().extend(lines);
            if sink.lock().unwrap().len() >= 2 {
                tx.send(true).unwrap();
            }
        },
        rx,
    )
    .await
    .unwrap();
    let seen = seen.lock().unwrap();
    let texts: Vec<_> = seen.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(texts, ["Hello", "World"]);
    assert_eq!(
        seen[1].timestamp.as_deref(),
        Some("2026-10-21T10:00:01.000000000Z")
    );
}

#[tokio::test]
async fn follow_reconnects_with_since_after_end() {
    let server = MockServer::start().await;
    let last = "2026-10-21T10:00:01.000000000Z";
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("since", last))
        .and(query_param("tail", "0"))
        .respond_with(sse(format!(
            // The first line repeats one already seen: it must not show twice.
            "{}{}",
            log(last, "World"),
            log("2026-10-21T10:00:02.000000000Z", "Again"),
        )))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("tail", "200"))
        .respond_with(sse(format!(
            "{}{}event: end\ndata: {{\"reason\":\"max_duration\"}}\n\n",
            log("2026-10-21T10:00:00.000000001Z", "Hello"),
            log(last, "World"),
        )))
        .expect(1)
        .mount(&server)
        .await;
    let (tx, rx) = watch::channel(false);
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = seen.clone();
    follow_logs_after(
        &client(&server),
        jwt_source(),
        "survival",
        move |lines| {
            let mut all = sink.lock().unwrap();
            all.extend(lines.into_iter().map(|l| l.text));
            if all.len() >= 3 {
                tx.send(true).unwrap();
            }
        },
        rx,
        std::time::Duration::from_millis(20),
    )
    .await
    .unwrap();
    assert_eq!(*seen.lock().unwrap(), ["Hello", "World", "Again"]);
}

#[tokio::test]
async fn follow_returns_too_many_streams() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(429).set_body_json(json!({
            "success": false, "data": { "code": "too_many_streams" }
        })))
        .mount(&server)
        .await;
    let (_tx, rx) = watch::channel(false);
    let err = follow_logs(&client(&server), jwt_source(), "survival", |_| {}, rx)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::TooManyStreams);
}

#[tokio::test]
async fn follow_returns_not_found() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let (_tx, rx) = watch::channel(false);
    let err = follow_logs(&client(&server), jwt_source(), "survival", |_| {}, rx)
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerNotFound);
}

#[tokio::test]
async fn follow_asks_for_a_new_jwt_once_on_401() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(header("x-user-token", "old"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(header("x-user-token", "fresh"))
        .respond_with(sse(log("2026-10-21T10:00:00.000000001Z", "Hi")))
        .expect(1)
        .mount(&server)
        .await;
    let calls = Arc::new(Mutex::new(0));
    let c = calls.clone();
    let jwt = move || {
        let n = {
            let mut c = c.lock().unwrap();
            *c += 1;
            *c
        };
        Box::pin(async move { Ok(SessionJwt::new(if n == 1 { "old" } else { "fresh" })) })
            as launcher_core::sync::BoxFuture<'static, launcher_core::Result<SessionJwt>>
    };
    let (tx, rx) = watch::channel(false);
    follow_logs(
        &client(&server),
        jwt,
        "survival",
        move |_| tx.send(true).unwrap(),
        rx,
    )
    .await
    .unwrap();
    assert_eq!(*calls.lock().unwrap(), 2);
}
