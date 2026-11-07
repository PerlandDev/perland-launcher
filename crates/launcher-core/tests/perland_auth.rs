//! The Perland auth routes (`/api/v1/auth`) against a wiremock Engine. Fake tokens only.
use launcher_core::{
    invite::Url,
    perland::{AuthClient, Handoff, SessionJwt},
    ErrorCode,
};
use serde_json::{json, Value};
use wiremock::{
    matchers::{body_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

const DEVICE: &str = "plt_abc";

fn envelope(data: Value) -> Value {
    json!({ "success": true, "data": data, "error": null, "meta": { "requestId": "r" } })
}

fn client(server: &MockServer) -> AuthClient {
    let http = launcher_core::http::client_without_redirects("test").unwrap();
    AuthClient::new(http, Url::parse(&server.uri()).unwrap())
}

/// `/player-session` is deprecated: nothing may call it. Wiremock checks on drop.
async fn never_player_session(server: &MockServer) {
    Mock::given(path("/api/v1/auth/player-session"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(server)
        .await;
}

/// Every request carried the device token as a bearer, and no `X-User-Token`.
async fn only_bearer_device(server: &MockServer) {
    let requests = server.received_requests().await.unwrap();
    assert!(!requests.is_empty());
    for r in requests {
        assert_eq!(
            r.headers.get("authorization").and_then(|v| v.to_str().ok()),
            Some("Bearer plt_abc"),
            "{}",
            r.url
        );
        assert!(r.headers.get("x-user-token").is_none(), "{}", r.url);
    }
}

#[tokio::test]
async fn memberships_send_the_device_token_as_bearer_and_read_plan() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/auth/memberships"))
        .and(header("authorization", "Bearer plt_abc"))
        .respond_with(ResponseTemplate::new(200).set_body_json(envelope(json!({
            "memberships": [
                {
                    "tenantId": "t-1", "tenantName": "perland-t1", "role": "owner",
                    "branding": { "displayName": "Perland" }, "allowedScopes": ["*"],
                    "plan": { "name": "player-free", "title": "Free" }
                },
                {
                    "tenantId": "t-2", "tenantName": "acme", "role": "viewer",
                    "branding": {}, "allowedScopes": []
                }
            ]
        }))))
        .expect(1)
        .mount(&server)
        .await;
    let list = client(&server).memberships(DEVICE).await.unwrap();
    only_bearer_device(&server).await;
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].tenant_id, "t-1");
    assert_eq!(list[0].tenant_name, "perland-t1");
    assert_eq!(list[0].display_name, "Perland");
    assert_eq!(list[0].role, "owner");
    assert_eq!(list[0].plan.as_deref(), Some("player-free"));
    assert_eq!(list[1].display_name, "acme");
    assert_eq!(list[1].plan, None);
}

#[tokio::test]
async fn memberships_401_is_signed_out() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/auth/memberships"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let e = client(&server).memberships(DEVICE).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn select_tenant_posts_the_tenant_id_with_the_device_token() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/select-tenant"))
        .and(header("authorization", "Bearer plt_abc"))
        .and(body_json(json!({ "tenantId": "t-1" })))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(envelope(json!({ "token": "eyJ.c.d" }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let scoped = client(&server).select_tenant(DEVICE, "t-1").await.unwrap();
    only_bearer_device(&server).await;
    assert_eq!(scoped.as_str(), "eyJ.c.d");
}

#[tokio::test]
async fn handoff_404_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/handoff"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    assert!(client(&server)
        .handoff(DEVICE, "t-1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn handoff_405_is_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/handoff"))
        .respond_with(ResponseTemplate::new(405))
        .mount(&server)
        .await;
    assert!(client(&server)
        .handoff(DEVICE, "t-1")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn handoff_answers_the_code() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/handoff"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(envelope(json!({ "code": "hc_1", "expiresIn": 120 }))),
        )
        .mount(&server)
        .await;
    let handoff = client(&server)
        .handoff(DEVICE, "t-1")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(handoff.code, "hc_1");
    assert_eq!(handoff.expires_in, 120);
}

#[tokio::test]
async fn handoff_without_a_team_names_none() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/handoff"))
        .and(body_json(json!({})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(envelope(json!({ "code": "hc_2", "expiresIn": 120 }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    let handoff = client(&server).handoff(DEVICE, "").await.unwrap().unwrap();
    assert_eq!(handoff.code, "hc_2");
}

#[tokio::test]
async fn handoff_sends_bearer_device_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/handoff"))
        .and(header("authorization", "Bearer plt_abc"))
        .and(body_json(json!({ "tenantId": "t-1" })))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(envelope(json!({ "code": "hc_1", "expiresIn": 120 }))),
        )
        .expect(1)
        .mount(&server)
        .await;
    client(&server).handoff(DEVICE, "t-1").await.unwrap();
}

#[tokio::test]
async fn handoff_401_is_signed_out() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/handoff"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let e = client(&server).handoff(DEVICE, "t-1").await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn debug_never_prints_tokens() {
    let server = MockServer::start().await;
    let jwt = SessionJwt::new("eyJ.a.b");
    let shown = format!("{jwt:?}");
    assert!(!shown.contains("eyJ"), "{shown}");
    let handoff = Handoff {
        code: "hc_secret".into(),
        expires_in: 60,
    };
    let shown = format!("{handoff:?}");
    assert!(!shown.contains("hc_secret"), "{shown}");
    let shown = format!("{:?}", client(&server));
    assert!(!shown.contains("plt_"), "{shown}");
}

#[tokio::test]
async fn select_tenant_401_is_signed_out() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/select-tenant"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    let e = client(&server)
        .select_tenant(DEVICE, "t-1")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn a_200_without_data_is_unexpected() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/select-tenant"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "success": true })))
        .mount(&server)
        .await;
    let e = client(&server)
        .select_tenant(DEVICE, "t-1")
        .await
        .unwrap_err();
    assert_ne!(e.code, ErrorCode::PerlandSignedOut);
    assert!(!e.message.contains("plt_"));
}

#[tokio::test]
async fn select_tenant_403_is_forbidden_not_host_trouble() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/select-tenant"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(
                json!({ "success": false, "data": {}, "error": "user not in tenant" }),
            ),
        )
        .mount(&server)
        .await;
    let e = client(&server)
        .select_tenant(DEVICE, "t-1")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ManageForbidden);
}

#[tokio::test]
async fn select_tenant_disabled_account_stays_signed_out() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/v1/auth/select-tenant"))
        .respond_with(ResponseTemplate::new(403).set_body_json(
            json!({ "success": false, "data": { "code": "account_disabled" }, "error": "no" }),
        ))
        .mount(&server)
        .await;
    let e = client(&server)
        .select_tenant(DEVICE, "t-1")
        .await
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn memberships_disabled_account_is_signed_out() {
    let server = MockServer::start().await;
    never_player_session(&server).await;
    Mock::given(method("GET"))
        .and(path("/api/v1/auth/memberships"))
        .respond_with(ResponseTemplate::new(403).set_body_json(
            json!({ "success": false, "data": { "code": "account_disabled" }, "error": "no" }),
        ))
        .mount(&server)
        .await;
    let e = client(&server).memberships(DEVICE).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PerlandSignedOut);
}

#[tokio::test]
async fn memberships_accounts_disabled_is_accounts_closed() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/v1/auth/memberships"))
        .respond_with(ResponseTemplate::new(503).set_body_json(
            json!({ "success": false, "data": { "code": "player_accounts_disabled" }, "error": "no" }),
        ))
        .mount(&server)
        .await;
    let e = client(&server).memberships(DEVICE).await.unwrap_err();
    assert_eq!(e.code, ErrorCode::PlayerAccountsClosed);
}
