//! The Engine's auth routes (`/api/v1/auth`): the launcher's device session token (`plt_…`)
//! becomes a short-lived session JWT, which lists the player's servers' tenants and scopes to
//! one. The device token, every JWT and every handoff code are credentials: they travel in
//! headers or bodies only, and nothing here prints one.

use std::fmt;

use reqwest::{Method, Url};
use serde::Deserialize;
use serde_json::{json, Value};

use super::{
    api::{accounts_closed, signed_out},
    wire::{failure, status_fallback, Credential, Reply, Wire},
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    invite::{client::host_of, InviteTiming},
};

/// The longest answer read, in bytes.
const BODY_CAP: usize = 64 << 10;

/// An Engine session JWT, sent as `X-User-Token`.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionJwt(String);

impl SessionJwt {
    pub fn new(token: impl Into<String>) -> Self {
        Self(token.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for SessionJwt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionJwt(<redacted>)")
    }
}

/// A server (tenant) the player belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Membership {
    pub tenant_id: String,
    pub tenant_name: String,
    /// `branding.displayName`, else the tenant's name.
    pub display_name: String,
    /// `owner`, `member` or `viewer`.
    pub role: String,
    /// The plan's name; absent on an Engine that doesn't say.
    pub plan: Option<String>,
}

/// A one-time code that signs the player in on the panel.
#[derive(Clone, PartialEq, Eq)]
pub struct Handoff {
    pub code: String,
    pub expires_in: u64,
}

impl fmt::Debug for Handoff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Handoff")
            .field("code", &"<redacted>")
            .field("expires_in", &self.expires_in)
            .finish()
    }
}

#[derive(Deserialize)]
struct TokenData {
    token: String,
}

#[derive(Deserialize)]
struct MembershipsData {
    #[serde(default)]
    memberships: Vec<RawMembership>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMembership {
    tenant_id: String,
    #[serde(default)]
    tenant_name: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    branding: Option<Branding>,
    #[serde(default)]
    plan: Option<RawPlan>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Branding {
    display_name: Option<String>,
}

#[derive(Deserialize)]
struct RawPlan {
    name: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HandoffData {
    code: String,
    #[serde(default)]
    expires_in: u64,
}

#[derive(Clone)]
pub struct AuthClient {
    wire: Wire,
    timeout: std::time::Duration,
}

// Manual Debug: keep to the origin.
impl fmt::Debug for AuthClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AuthClient")
            .field("engine", &host_of(self.wire.engine()))
            .finish_non_exhaustive()
    }
}

impl AuthClient {
    /// `engine` should come from `check_engine_url` (as `engine_for_site` gives it). `http`
    /// must not follow redirects (`http::client_without_redirects`): a redirect would forward
    /// the device token or the session JWT to another host.
    pub fn new(http: reqwest::Client, engine: Url) -> Self {
        Self {
            wire: Wire::new(http, engine),
            timeout: InviteTiming::default().request_timeout,
        }
    }

    fn host(&self) -> String {
        self.wire.host()
    }

    async fn send(
        &self,
        method: Method,
        route: &str,
        credential: Credential<'_>,
        body: Option<&Value>,
    ) -> Result<Reply> {
        self.wire
            .send(
                self.timeout,
                method,
                &format!("/api/v1/auth{route}"),
                credential,
                body,
                BODY_CAP,
            )
            .await
    }

    fn data<T: serde::de::DeserializeOwned>(&self, reply: &Reply) -> Result<T> {
        self.wire.data(reply)
    }

    /// `POST /player-session`: the device token for a session JWT. An Engine without the
    /// route (404) is one without player accounts.
    pub async fn player_session(&self, device: &str) -> Result<SessionJwt> {
        let reply = self
            .send(
                Method::POST,
                "/player-session",
                Credential::Bearer(device),
                None,
            )
            .await?;
        if reply.status == 404 {
            return Err(accounts_closed(&self.host()));
        }
        self.ok(&reply)?;
        self.data::<TokenData>(&reply).map(|d| SessionJwt(d.token))
    }

    /// `GET /memberships`: the servers the player belongs to.
    pub async fn memberships(&self, jwt: &SessionJwt) -> Result<Vec<Membership>> {
        let reply = self
            .send(
                Method::GET,
                "/memberships",
                Credential::UserToken(jwt.as_str()),
                None,
            )
            .await?;
        self.ok(&reply)?;
        let data: MembershipsData = self.data(&reply)?;
        Ok(data
            .memberships
            .into_iter()
            .map(|m| {
                let display_name = m
                    .branding
                    .and_then(|b| b.display_name)
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| m.tenant_name.clone());
                Membership {
                    tenant_id: m.tenant_id,
                    tenant_name: m.tenant_name,
                    display_name,
                    role: m.role,
                    plan: m.plan.and_then(|p| p.name),
                }
            })
            .collect())
    }

    /// `POST /select-tenant`: a JWT scoped to one server.
    pub async fn select_tenant(&self, jwt: &SessionJwt, tenant_id: &str) -> Result<SessionJwt> {
        let body = json!({ "tenantId": tenant_id });
        let reply = self
            .send(
                Method::POST,
                "/select-tenant",
                Credential::UserToken(jwt.as_str()),
                Some(&body),
            )
            .await?;
        self.ok(&reply)?;
        self.data::<TokenData>(&reply).map(|d| SessionJwt(d.token))
    }

    /// `POST /handoff`: a one-time panel sign-in code for a server's team, or for no team when
    /// `tenant_id` is `""`. `None` when the Engine doesn't have the route (404 or 405).
    pub async fn handoff(&self, device: &str, tenant_id: &str) -> Result<Option<Handoff>> {
        let body = if tenant_id.is_empty() {
            json!({})
        } else {
            json!({ "tenantId": tenant_id })
        };
        let reply = self
            .send(
                Method::POST,
                "/handoff",
                Credential::Bearer(device),
                Some(&body),
            )
            .await?;
        if matches!(reply.status, 404 | 405) {
            return Ok(None);
        }
        self.ok(&reply)?;
        let data: HandoffData = self.data(&reply)?;
        Ok(Some(Handoff {
            code: data.code,
            expires_in: data.expires_in,
        }))
    }

    fn ok(&self, reply: &Reply) -> Result<()> {
        if (200..300).contains(&reply.status) {
            Ok(())
        } else {
            Err(self.refusal(reply))
        }
    }

    /// What a refused request means: the Engine's `data.code` first, then the status.
    fn refusal(&self, reply: &Reply) -> LauncherError {
        let host = self.host();
        let code = failure(reply).code;
        match code.as_deref() {
            Some("player_accounts_disabled") => return accounts_closed(&host),
            // The session no longer works.
            Some("account_disabled") => return signed_out(&host),
            _ => {}
        }
        match reply.status {
            // Not (or no longer) a member of that server, or a role that can't use it: not
            // the Engine having trouble.
            403 => LauncherError::auth(
                ErrorCode::ManageForbidden,
                "Your role or plan doesn't allow this.",
            )
            .with_param("host", &host),
            status => status_fallback(&host, status),
        }
    }
}
