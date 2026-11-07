//! Perland sign-in over standard OAuth 2.0 (RFC 6749 authorization code with S256 PKCE,
//! RFC 8252 loopback redirect, RFC 8414 discovery, RFC 7009 revocation): the flow every
//! launcher uses, with Perland Launcher as the Engine's seeded first-party client.
//!
//! The browser opens the panel's authorize page (from discovery), where the player picks how
//! to sign in; the loopback gets the code back; the code is redeemed at the Engine's token
//! endpoint. The code, the verifier and the access token are credentials: they travel in
//! request bodies only, and nothing here prints one.
//!
//! The token and revocation endpoints are always the Engine's own (`<engine>/api/v1/oauth/…`),
//! never the ones discovery names: the Engine builds those from the request's scheme and
//! host, which behind a proxy can say `http`, and the code and token must only ever go to the
//! Engine the launcher already trusts.

use std::fmt;

use reqwest::Url;
use serde::Deserialize;
use serde_json::Value;

use super::{
    api::{launcher_update_required, Player, TokenAnswer},
    panel::panel_url,
    pkce::Pkce,
    wire::{failure, Reply, Wire},
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    invite::{
        client::{host_of, host_trouble, rate_limited, unexpected},
        InviteTiming,
    },
};

/// Perland Launcher's public client ID, seeded on every Engine (no secret: a desktop app
/// can't keep one).
pub const CLIENT_ID: &str = "pcl_perlandlauncher0000000000";
/// The one scope the Engine has.
pub const SCOPE: &str = "launcher";

const DISCOVERY_PATH: &str = "/.well-known/oauth-authorization-server";
const TOKEN_PATH: &str = "/api/v1/oauth/token";
const REVOKE_PATH: &str = "/api/v1/oauth/revoke";
/// The longest answer read, in bytes.
const BODY_CAP: usize = 64 << 10;

/// What discovery says the launcher needs: where the browser signs in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Discovery {
    /// The panel's authorize page, checked like a panel URL (https, or http on this computer).
    pub authorization_endpoint: Url,
}

#[derive(Deserialize)]
struct RawDiscovery {
    authorization_endpoint: String,
    #[serde(default)]
    code_challenge_methods_supported: Option<Vec<String>>,
    #[serde(default)]
    response_types_supported: Option<Vec<String>>,
}

impl Discovery {
    /// Reads an RFC 8414 answer. `None` when it names no authorize page the launcher may open,
    /// or says it has no S256 PKCE or no `code` response.
    pub fn parse(body: &[u8]) -> Option<Self> {
        let raw: RawDiscovery = serde_json::from_slice(body).ok()?;
        let has = |list: &Option<Vec<String>>, want: &str| {
            list.as_ref().is_none_or(|l| l.iter().any(|v| v == want))
        };
        if !has(&raw.code_challenge_methods_supported, "S256")
            || !has(&raw.response_types_supported, "code")
        {
            return None;
        }
        Some(Self {
            authorization_endpoint: panel_url(&raw.authorization_endpoint)?,
        })
    }
}

/// `<authorize>?response_type=code&client_id=…&redirect_uri=…&code_challenge=…&code_challenge_method=S256&state=…&scope=launcher&ui_locales=…`.
/// Holds no secret: the verifier stays here. `language` is the launcher's (`en`, `es`, or a
/// tag such as `es-MX`, which becomes `es`).
pub fn oauth_authorize_url(
    discovery: &Discovery,
    redirect: &str,
    pkce: &Pkce,
    language: &str,
) -> Url {
    let mut url = discovery.authorization_endpoint.clone();
    url.set_query(None);
    url.set_fragment(None);
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", redirect)
        .append_pair("code_challenge", &pkce.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &pkce.state)
        .append_pair("scope", SCOPE)
        .append_pair("ui_locales", ui_locale(language));
    url
}

/// The page's language: `es` for any Spanish tag, else `en`.
fn ui_locale(language: &str) -> &'static str {
    match super::loopback::Language::parse(language) {
        super::loopback::Language::Es => "es",
        super::loopback::Language::En => "en",
    }
}

/// RFC 6749 §5.1, plus the Engine's player.
#[derive(Deserialize)]
struct RawToken {
    access_token: String,
    #[serde(default)]
    token_type: String,
    perland_player: Player,
    #[serde(default)]
    perland_new_player: bool,
}

/// RFC 6749 §5.2.
#[derive(Deserialize)]
struct RawTokenError {
    error: String,
}

/// The Engine's OAuth endpoints.
#[derive(Clone)]
pub struct OAuthClient {
    wire: Wire,
    timeout: std::time::Duration,
}

// Manual Debug: keep to the origin.
impl fmt::Debug for OAuthClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OAuthClient")
            .field("engine", &host_of(self.wire.engine()))
            .finish_non_exhaustive()
    }
}

impl OAuthClient {
    /// `engine` should come from `check_engine_url`. `http` must not follow redirects
    /// (`http::client_without_redirects`): a redirect would replay the code or the token to
    /// another host.
    pub fn new(http: reqwest::Client, engine: Url) -> Self {
        Self {
            wire: Wire::new(http, engine),
            timeout: InviteTiming::default().request_timeout,
        }
    }

    fn host(&self) -> String {
        self.wire.host()
    }

    /// `GET /.well-known/oauth-authorization-server`. `Ok(None)` when the Engine has no OAuth
    /// sign-in (404: no panel configured, or an Engine from before it) or answers metadata the
    /// launcher can't use; then the launcher offers no Perland sign-in.
    pub async fn discover(&self) -> Result<Option<Discovery>> {
        let reply = self
            .wire
            .send(
                self.timeout,
                reqwest::Method::GET,
                DISCOVERY_PATH,
                super::wire::Credential::None,
                None,
                BODY_CAP,
            )
            .await?;
        match reply.status {
            200..=299 => Ok(Discovery::parse(&reply.body)),
            404 | 405 | 410 | 501 => Ok(None),
            429 => Err(rate_limited(&self.host())),
            status => Err(host_trouble(&self.host(), status)),
        }
    }

    /// `POST /api/v1/oauth/token`: redeem the code the loopback got. `redirect` is exactly the
    /// one the authorize URL carried.
    pub async fn token(&self, code: &str, verifier: &str, redirect: &str) -> Result<TokenAnswer> {
        let form = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect),
            ("client_id", CLIENT_ID),
            ("code_verifier", verifier),
        ];
        let reply = self
            .wire
            .send_urlencoded(self.timeout, TOKEN_PATH, &form, BODY_CAP)
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(self.token_refusal(&reply));
        }
        let raw: RawToken = serde_json::from_slice(&reply.body)
            .map_err(|_| unexpected(&self.host(), "the token answer isn't valid"))?;
        if raw.access_token.is_empty()
            || !(raw.token_type.is_empty() || raw.token_type.eq_ignore_ascii_case("bearer"))
        {
            return Err(unexpected(
                &self.host(),
                "the token answer isn't a bearer token",
            ));
        }
        Ok(TokenAnswer {
            token: raw.access_token,
            player: raw.perland_player,
            new_player: raw.perland_new_player,
        })
    }

    /// `POST /api/v1/oauth/revoke`: end the session on the Engine. RFC 7009 answers 200
    /// whatever happened, so only a request that got no answer, or an Engine in trouble, is an
    /// error; the caller forgets the token either way.
    pub async fn revoke(&self, token: &str) -> Result<()> {
        let form = [("token", token), ("client_id", CLIENT_ID)];
        let reply = self
            .wire
            .send_urlencoded(self.timeout, REVOKE_PATH, &form, BODY_CAP)
            .await?;
        match reply.status {
            200..=299 => Ok(()),
            429 => Err(rate_limited(&self.host())),
            status => Err(host_trouble(&self.host(), status)),
        }
    }

    /// What a refused token request means: the RFC 6749 `error` first, then the status.
    fn token_refusal(&self, reply: &Reply) -> LauncherError {
        let host = self.host();
        if reply.status == 429 {
            return rate_limited(&host);
        }
        // Not RFC 6749: the Engine's own envelope, for a sign-in that would make an account
        // an older launcher can't finish.
        if failure(reply).code.as_deref() == Some("launcher_update_required") {
            return launcher_update_required(&host);
        }
        let error = serde_json::from_slice::<Value>(&reply.body)
            .ok()
            .and_then(|v| serde_json::from_value::<RawTokenError>(v).ok())
            .map(|e| e.error);
        match error.as_deref() {
            Some("invalid_grant") => sign_in_expired(&host),
            Some("invalid_client" | "unauthorized_client") => sign_in_client_refused(&host),
            Some(e @ ("invalid_request" | "unsupported_grant_type" | "invalid_scope")) => {
                sign_in_incompatible(&host, e)
            }
            _ if reply.status >= 500 => host_trouble(&host, reply.status),
            Some(other) => sign_in_failed(&host, other),
            None => sign_in_failed(&host, &format!("status {}", reply.status)),
        }
    }
}

/// The code was unknown, expired or used, or the player's account can't sign in.
pub fn sign_in_expired(host: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::SignInExpired,
        "That sign-in expired before it finished. Try again.",
    )
    .with_param("host", host)
}

/// The Engine doesn't know this launcher as a client, or no longer allows it.
pub fn sign_in_client_refused(host: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::SignInClientRefused,
        "This Perland doesn't accept sign-ins from this launcher.",
    )
    .with_param("host", host)
}

/// The Engine wants a request this launcher doesn't make: an update is the fix.
pub fn sign_in_incompatible(host: &str, error: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::SignInIncompatible,
        "This Perland signs in a way this launcher doesn't support yet.",
    )
    .with_param("host", host)
    .with_fix("Update the launcher")
    .with_detail(error)
}

/// The player canceled on the Perland page (`error=access_denied`).
pub fn sign_in_canceled(host: &str) -> LauncherError {
    LauncherError::auth(ErrorCode::SignInCanceled, "Sign-in was canceled.").with_param("host", host)
}

/// Any other failure; `why` is a short identifier for the detail, never a credential.
pub fn sign_in_failed(host: &str, why: &str) -> LauncherError {
    LauncherError::auth(ErrorCode::SignInFailed, "Couldn't sign you in. Try again.")
        .with_param("host", host)
        .with_detail(why)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn discovery(authorize: &str) -> Discovery {
        Discovery {
            authorization_endpoint: Url::parse(authorize).unwrap(),
        }
    }

    #[test]
    fn parse_reads_the_engine_metadata() {
        let body = br#"{
            "issuer": "https://engine.example.com",
            "authorization_endpoint": "https://panel.example.com/oauth/authorize",
            "token_endpoint": "http://engine.example.com/api/v1/oauth/token",
            "revocation_endpoint": "http://engine.example.com/api/v1/oauth/revoke",
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none", "client_secret_post"],
            "scopes_supported": ["launcher"]
        }"#;
        assert_eq!(
            Discovery::parse(body),
            Some(discovery("https://panel.example.com/oauth/authorize"))
        );
    }

    #[test]
    fn parse_takes_minimal_metadata() {
        let body = br#"{"authorization_endpoint": "http://127.0.0.1:5173/oauth/authorize"}"#;
        assert!(Discovery::parse(body).is_some());
    }

    #[test]
    fn parse_refuses_what_the_launcher_cant_use() {
        for body in [
            &br#"{}"#[..],
            br#"not json"#,
            br#"{"authorization_endpoint": ""}"#,
            br#"{"authorization_endpoint": "http://panel.example.com/oauth/authorize"}"#,
            br#"{"authorization_endpoint": "javascript:alert(1)"}"#,
            br#"{"authorization_endpoint": "https://user@panel.example.com/oauth/authorize"}"#,
            br#"{"authorization_endpoint": "https://panel.example.com/oauth/authorize?x=1"}"#,
            br#"{"authorization_endpoint": "https://panel.example.com/oauth/authorize",
                 "code_challenge_methods_supported": ["plain"]}"#,
            br#"{"authorization_endpoint": "https://panel.example.com/oauth/authorize",
                 "response_types_supported": ["token"]}"#,
        ] {
            assert_eq!(
                Discovery::parse(body),
                None,
                "{}",
                String::from_utf8_lossy(body)
            );
        }
    }

    #[test]
    fn authorize_url_has_every_param_in_order() {
        let pkce = Pkce::from_verifier("v", "st ate");
        let d = discovery("https://example.com/panel/oauth/authorize");
        let url = oauth_authorize_url(&d, "http://127.0.0.1:5000/callback", &pkce, "es-MX");
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(url.path(), "/panel/oauth/authorize");
        assert_eq!(url.fragment(), None);
        let pairs: Vec<(String, String)> = url.query_pairs().into_owned().collect();
        let want: Vec<(String, String)> = [
            ("response_type", "code"),
            ("client_id", CLIENT_ID),
            ("redirect_uri", "http://127.0.0.1:5000/callback"),
            ("code_challenge", pkce.challenge.as_str()),
            ("code_challenge_method", "S256"),
            ("state", "st ate"),
            ("scope", "launcher"),
            ("ui_locales", "es"),
        ]
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        assert_eq!(pairs, want);
        assert!(!url.as_str().contains("verifier"));
    }

    #[test]
    fn authorize_url_language_falls_back_to_english() {
        let pkce = Pkce::from_verifier("v", "s");
        let d = discovery("https://panel.example.com/oauth/authorize");
        for (tag, want) in [("en", "en"), ("es", "es"), ("ca", "en"), ("", "en")] {
            let url = oauth_authorize_url(&d, "r", &pkce, tag);
            let got = url
                .query_pairs()
                .find(|(k, _)| k == "ui_locales")
                .map(|(_, v)| v.into_owned());
            assert_eq!(got.as_deref(), Some(want), "{tag:?}");
        }
    }

    #[test]
    fn client_id_is_the_seeded_one() {
        assert_eq!(CLIENT_ID, "pcl_perlandlauncher0000000000");
        assert_eq!(CLIENT_ID.len(), 4 + 25);
    }
}
