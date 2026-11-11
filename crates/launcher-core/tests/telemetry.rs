//! The two telemetry requests against a fake Engine: exact routes and bodies, no credential,
//! and what each answer means for the body that was sent.
use std::{collections::BTreeMap, time::Duration};

use launcher_core::{
    http,
    invite::Url,
    telemetry::{Client, ErrorBatch, Outcome, Ping, PingChannel, Report, ReportKind, Sender},
};
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn sender() -> Sender {
    Sender::new(http::client("9.9.9").unwrap())
}

fn batch() -> ErrorBatch {
    ErrorBatch::new(
        &Client::new("9.9.9", "linux", "x86_64", "es"),
        vec![Report::new(ReportKind::AppError, "Couldn't save settings")
            .code("disk_write_failed")
            .context("category", "disk")],
    )
}

fn ping() -> Ping {
    Ping {
        version: "9.9.9".into(),
        channel: PingChannel::Beta,
        os: "linux",
        arch: "x86_64".into(),
        locale: "es".into(),
        signed_in: false,
        first_ever: true,
        first_this_week: true,
        first_this_month: true,
        install_week: "2026-W46".into(),
        counters: BTreeMap::from([("game_launches".to_string(), 2)]),
    }
}

async fn engine(route: &str, answer: ResponseTemplate) -> (MockServer, Url) {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(route))
        .respond_with(answer)
        .mount(&server)
        .await;
    let url = Url::parse(&server.uri()).unwrap();
    (server, url)
}

#[tokio::test]
async fn errors_go_to_their_route_as_the_contract_says_with_no_credential() {
    let accepted = ResponseTemplate::new(202)
        .set_body_json(json!({ "success": true, "data": { "accepted": 1 } }));
    let (server, url) = engine("/api/v1/telemetry/errors", accepted).await;
    assert_eq!(sender().errors(&url, &batch()).await, Outcome::Delivered);

    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(
        request.body_json::<serde_json::Value>().unwrap(),
        json!({
            "source": "launcher",
            "version": "9.9.9",
            "os": "linux",
            "arch": "x86_64",
            "locale": "es",
            "reports": [{
                "kind": "app_error",
                "message": "Couldn't save settings",
                "code": "disk_write_failed",
                "context": { "category": "disk" },
                "count": 1
            }]
        })
    );
    assert_eq!(
        request.headers["user-agent"],
        "PerlandLauncher/9.9.9 (+https://github.com/PerlandDev/perland-launcher)"
    );
    assert_eq!(request.headers["content-type"], "application/json");
    for header in ["authorization", "cookie", "x-user-token", "x-user-id"] {
        assert!(!request.headers.contains_key(header), "{header} was sent");
    }
}

#[tokio::test]
async fn the_ping_goes_to_its_route_with_exactly_the_contract_fields() {
    let (server, url) = engine(
        "/api/v1/telemetry/launcher/ping",
        ResponseTemplate::new(202),
    )
    .await;
    // A trailing slash on the Engine's address doesn't double up.
    let with_slash = Url::parse(&format!("{}/", url.as_str().trim_end_matches('/'))).unwrap();
    assert_eq!(
        sender().ping(&with_slash, &ping()).await,
        Outcome::Delivered
    );
    let requests = server.received_requests().await.unwrap();
    let body = requests[0].body_json::<serde_json::Value>().unwrap();
    let mut keys: Vec<&str> = body
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "arch",
            "channel",
            "counters",
            "firstEver",
            "firstThisMonth",
            "firstThisWeek",
            "installWeek",
            "locale",
            "os",
            "signedIn",
            "version"
        ]
    );
    assert_eq!(body["counters"], json!({ "game_launches": 2 }));
    assert_eq!(body["channel"], "beta");
}

#[tokio::test]
async fn each_answer_says_what_to_do_with_the_body() {
    for (status, outcome) in [
        (200, Outcome::Delivered),
        (202, Outcome::Delivered),
        (204, Outcome::Delivered),
        (400, Outcome::Rejected),
        (413, Outcome::Rejected),
        (401, Outcome::Retry),
        (404, Outcome::Retry),
        (429, Outcome::Retry),
        (500, Outcome::Retry),
        (503, Outcome::Retry),
    ] {
        let (_server, url) =
            engine("/api/v1/telemetry/errors", ResponseTemplate::new(status)).await;
        assert_eq!(sender().errors(&url, &batch()).await, outcome, "{status}");
        let (_server, url) = engine(
            "/api/v1/telemetry/launcher/ping",
            ResponseTemplate::new(status),
        )
        .await;
        assert_eq!(sender().ping(&url, &ping()).await, outcome, "{status}");
    }
}

#[tokio::test]
async fn no_network_and_a_slow_engine_keep_the_body() {
    // Nothing listens here.
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        format!("http://{}", listener.local_addr().unwrap())
    };
    let url = Url::parse(&closed).unwrap();
    assert_eq!(sender().errors(&url, &batch()).await, Outcome::Retry);

    let slow = ResponseTemplate::new(202).set_delay(Duration::from_secs(5));
    let (_server, url) = engine("/api/v1/telemetry/launcher/ping", slow).await;
    let started = std::time::Instant::now();
    let quick = sender().with_timeout(Duration::from_millis(200));
    assert_eq!(quick.ping(&url, &ping()).await, Outcome::Retry);
    assert!(started.elapsed() < Duration::from_secs(3));
}
