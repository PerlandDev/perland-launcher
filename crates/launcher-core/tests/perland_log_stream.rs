//! The log stream against a wiremock Engine. Fake tokens only. The reconnect wait is short:
//! a paused clock would fire the HTTP client's connect timeout while a socket is pending.
use std::sync::{Arc, Mutex};

use launcher_core::{
    invite::Url,
    perland::{follow_logs, follow_logs_after, follow_logs_with, LogLine, SessionJwt, StackClient},
    ErrorCode,
};
use serde_json::json;
use tokio::sync::watch;
use wiremock::{
    matchers::{header, method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

const JWT: &str = "eyJ.a.b";
/// `X-User-Token` as the Engine reads it: the JWT after `Bearer `.
const BEARER: &str = "Bearer eyJ.a.b";
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
        .and(header("x-user-token", BEARER))
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
        .and(header("x-user-token", "Bearer old"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(header("x-user-token", "Bearer fresh"))
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

const T1: &str = "2026-10-21T10:00:01.000000000Z";
const T2: &str = "2026-10-21T10:00:02.000000000Z";

fn texts(sink: &Arc<Mutex<Vec<String>>>) -> impl FnMut(Vec<LogLine>) + Send {
    let sink = sink.clone();
    move |lines| {
        sink.lock()
            .unwrap()
            .extend(lines.into_iter().map(|l| l.text))
    }
}

#[tokio::test]
async fn follow_reconnects_after_a_drop_without_end() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("since", T1))
        .and(query_param("tail", "0"))
        .respond_with(sse(log(T2, "two")))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("tail", "200"))
        // No `end` event: the connection just closes.
        .respond_with(sse(log(T1, "one")))
        .mount(&server)
        .await;
    let (tx, rx) = watch::channel(false);
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let mut push = texts(&seen);
    let probe = seen.clone();
    follow_logs_after(
        &client(&server),
        jwt_source(),
        "survival",
        move |lines| {
            push(lines);
            if probe.lock().unwrap().len() >= 2 {
                tx.send(true).unwrap();
            }
        },
        rx,
        std::time::Duration::from_millis(20),
    )
    .await
    .unwrap();
    assert_eq!(*seen.lock().unwrap(), ["one", "two"]);
}

#[tokio::test]
async fn follow_retries_an_engine_restart_after_connecting() {
    let server = MockServer::start().await;
    // After the first connect: a 503, then a plain 429, then the stream.
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("since", T1))
        .respond_with(sse(log(T2, "two")))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("since", T1))
        .respond_with(ResponseTemplate::new(429))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("since", T1))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .with_priority(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("tail", "200"))
        .respond_with(sse(log(T1, "one")))
        .mount(&server)
        .await;
    let (tx, rx) = watch::channel(false);
    let seen: Arc<Mutex<Vec<String>>> = Arc::default();
    let mut push = texts(&seen);
    let probe = seen.clone();
    follow_logs_after(
        &client(&server),
        jwt_source(),
        "survival",
        move |lines| {
            push(lines);
            if probe.lock().unwrap().len() >= 2 {
                tx.send(true).unwrap();
            }
        },
        rx,
        std::time::Duration::from_millis(20),
    )
    .await
    .unwrap();
    assert_eq!(*seen.lock().unwrap(), ["one", "two"]);
}

#[tokio::test]
async fn follow_keeps_failing_first_connect_terminal() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let (_tx, rx) = watch::channel(false);
    let err = follow_logs(&client(&server), jwt_source(), "survival", |_| {}, rx)
        .await
        .unwrap_err();
    assert_ne!(err.code, ErrorCode::TooManyStreams);
}

#[tokio::test]
async fn follow_stops_during_the_reconnect_wait() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(sse(log(T1, "one")))
        .expect(1)
        .mount(&server)
        .await;
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        tx.send(true).unwrap();
    });
    let done = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        follow_logs_after(
            &client(&server),
            jwt_source(),
            "survival",
            |_| {},
            rx,
            std::time::Duration::from_secs(60),
        ),
    )
    .await
    .expect("stop is honored while waiting");
    done.unwrap();
}

#[tokio::test]
async fn follow_waits_between_attempts_when_the_server_closes_at_once() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(sse(String::new()))
        .mount(&server)
        .await;
    let (tx, rx) = watch::channel(false);
    tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(450)).await;
        tx.send(true).unwrap();
    });
    follow_logs_after(
        &client(&server),
        jwt_source(),
        "survival",
        |_| {},
        rx,
        std::time::Duration::from_millis(100),
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap().len();
    // About one per 100 ms: a hot loop would make hundreds.
    assert!((2..=7).contains(&requests), "{requests} requests");
}

/// A first connect that works, then a reconnect the Engine refuses for good.
async fn refused_after_connecting(refusal: ResponseTemplate) -> launcher_core::LauncherError {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("since", T1))
        .respond_with(refusal)
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(query_param("tail", "200"))
        .respond_with(sse(log(T1, "one")))
        .mount(&server)
        .await;
    let (_tx, rx) = watch::channel(false);
    follow_logs_after(
        &client(&server),
        jwt_source(),
        "survival",
        |_| {},
        rx,
        std::time::Duration::from_millis(20),
    )
    .await
    .unwrap_err()
}

#[tokio::test]
async fn follow_ends_on_not_found_after_connecting() {
    let err = refused_after_connecting(ResponseTemplate::new(404)).await;
    assert_eq!(err.code, ErrorCode::ServerNotFound);
}

#[tokio::test]
async fn follow_ends_on_too_many_streams_after_connecting() {
    let err = refused_after_connecting(ResponseTemplate::new(429).set_body_json(json!({
        "success": false, "data": { "code": "too_many_streams" }
    })))
    .await;
    assert_eq!(err.code, ErrorCode::TooManyStreams);
}

#[tokio::test]
async fn follow_with_reports_the_first_connect_and_asks_for_a_new_jwt_after_401() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(header("x-user-token", "Bearer old"))
        .respond_with(ResponseTemplate::new(401))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .and(header("x-user-token", "Bearer fresh"))
        .respond_with(sse(log(T1, "Hi")))
        .mount(&server)
        .await;
    let refused: Arc<Mutex<Vec<bool>>> = Arc::default();
    let r = refused.clone();
    let jwt = move |was_refused: bool| {
        r.lock().unwrap().push(was_refused);
        let token = if was_refused { "fresh" } else { "old" };
        Box::pin(async move { Ok(SessionJwt::new(token)) })
            as launcher_core::sync::BoxFuture<'static, launcher_core::Result<SessionJwt>>
    };
    let connects = Arc::new(Mutex::new(0));
    let c = connects.clone();
    let seen_before_lines = Arc::new(Mutex::new(None));
    let s = seen_before_lines.clone();
    let probe = connects.clone();
    let (tx, rx) = watch::channel(false);
    follow_logs_with(
        &client(&server),
        jwt,
        "survival",
        move |_| {
            *s.lock().unwrap() = Some(*probe.lock().unwrap());
            tx.send(true).unwrap();
        },
        move || *c.lock().unwrap() += 1,
        rx,
        std::time::Duration::from_millis(20),
    )
    .await
    .unwrap();
    assert_eq!(*connects.lock().unwrap(), 1);
    assert_eq!(
        *seen_before_lines.lock().unwrap(),
        Some(1),
        "connected before the lines"
    );
    assert_eq!(*refused.lock().unwrap(), [false, true]);
}

#[tokio::test]
async fn follow_with_never_reports_a_refused_first_connect() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path(ROUTE))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    let connected = Arc::new(Mutex::new(false));
    let c = connected.clone();
    let (_tx, rx) = watch::channel(false);
    let err = follow_logs_with(
        &client(&server),
        |_| {
            Box::pin(async { Ok(SessionJwt::new(JWT)) })
                as launcher_core::sync::BoxFuture<'static, launcher_core::Result<SessionJwt>>
        },
        "survival",
        |_| {},
        move || *c.lock().unwrap() = true,
        rx,
        std::time::Duration::from_millis(20),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::ServerNotFound);
    assert!(!*connected.lock().unwrap());
}
