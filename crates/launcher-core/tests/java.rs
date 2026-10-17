use std::time::Duration;

use launcher_core::{download::hash::sha1_hex, java::required_for_at, meta::MetaClient, ErrorCode};
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

async fn serve(server: &MockServer, at: &str, body: String) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(server)
        .await;
}

#[tokio::test]
async fn required_java_comes_from_the_version_json() {
    let server = MockServer::start().await;
    let modern = r#"{"id":"1.20.5","mainClass":"m","javaVersion":{"component":"java-runtime-delta","majorVersion":21}}"#;
    let old = r#"{"id":"1.12.2","mainClass":"m"}"#;
    let entry = |id: &str, body: &str| {
        format!(
            r#"{{"id":"{id}","type":"release","url":"{}/{id}.json","sha1":"{}","releaseTime":"2024-04-23T00:00:00+00:00"}}"#,
            server.uri(),
            sha1_hex(body.as_bytes())
        )
    };
    let manifest = format!(
        r#"{{"latest":{{"release":"1.20.5","snapshot":"1.20.5"}},"versions":[{},{}]}}"#,
        entry("1.20.5", modern),
        entry("1.12.2", old)
    );
    serve(&server, "/manifest.json", manifest).await;
    serve(&server, "/1.20.5.json", modern.into()).await;
    serve(&server, "/1.12.2.json", old.into()).await;
    let tmp = tempfile::tempdir().unwrap();
    let meta = meta(tmp.path());
    let url = format!("{}/manifest.json", server.uri());
    assert_eq!(required_for_at(&meta, &url, "1.20.5").await.unwrap(), 21);
    assert_eq!(required_for_at(&meta, &url, "1.12.2").await.unwrap(), 8);
    assert_eq!(
        required_for_at(&meta, &url, "9.9").await.unwrap_err().code,
        ErrorCode::MinecraftVersionNotFound
    );
}
