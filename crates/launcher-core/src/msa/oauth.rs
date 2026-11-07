//! Microsoft's OAuth 2.0 authorization-code flow: for the Azure app (`MsaApp::Azure`) with PKCE
//! and a loopback redirect, for the Minecraft Launcher's client (`MsaApp::Live`) on login.live.com
//! with its desktop redirect. The authorize URL, reading the redirect, redeeming the code and
//! refreshing. Nothing here puts a token, code or verifier into an error.

use serde::Deserialize;

use super::{rate_limited, sign_in_failed, MsTokens, MsaApp, MsaClient, BODY_CAP};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_trouble, request_failed, unexpected},
        Url,
    },
    perland::{Callback, Pkce},
};

/// What the launcher asks Microsoft for: Xbox sign-in, and a refresh token.
const SCOPE: &str = "XboxLive.signin offline_access";

/// The only redirect login.live.com takes for the Minecraft Launcher's client. Nothing listens
/// there: the sign-in window watches for a navigation to it and reads the code off the URL.
pub const LIVE_REDIRECT: &str = "https://login.live.com/oauth20_desktop.srf";

/// What the Minecraft Launcher's client asks for: a ticket for Xbox Live user sign-in.
const LIVE_SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";

/// `url` is the desktop redirect: https, login.live.com on the default port, no user info, the
/// exact path. Anything else (Microsoft's own pages on the way, a look-alike host) isn't.
pub fn is_live_redirect(url: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    url.scheme() == "https"
        && url.host_str() == Some("login.live.com")
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/oauth20_desktop.srf"
}

/// What the desktop redirect says, or `None` when `url` isn't it. A code wins; `access_denied`
/// is the player saying no; any other error keeps only its plain code (`unknown` if it isn't
/// one), and a redirect with neither is `no_code`. The error description is never kept.
pub fn live_callback(url: &str) -> Option<Callback> {
    if !is_live_redirect(url) {
        return None;
    }
    let url = Url::parse(url).ok()?;
    let (mut code, mut error) = (None, None);
    for (key, value) in url.query_pairs() {
        match &*key {
            "code" if code.is_none() => code = Some(value.into_owned()),
            "error" if error.is_none() => error = Some(value.into_owned()),
            _ => {}
        }
    }
    Some(match (code.filter(|c| !c.is_empty()), error) {
        (Some(code), _) => Callback::Code(code),
        (None, Some(e)) if e == "access_denied" => Callback::Denied,
        (None, Some(e)) => Callback::Failed(plain_error_code(&e).unwrap_or("unknown").to_string()),
        (None, None) => Callback::Failed("no_code".to_string()),
    })
}

#[derive(Deserialize)]
struct TokenAnswer {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct OAuthError {
    error: Option<String>,
}

fn sign_in_again() -> LauncherError {
    LauncherError::auth(
        ErrorCode::MicrosoftSignInAgain,
        "Sign in with Microsoft again to keep playing.",
    )
}

/// An OAuth `error` value is a short token like `invalid_client`; anything else isn't kept.
fn plain_error_code(raw: &str) -> Option<&str> {
    (!raw.is_empty() && raw.len() <= 40 && raw.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'))
        .then_some(raw)
}

impl MsaClient {
    /// The page the browser opens to sign in. The PKCE verifier never appears in it.
    pub fn authorize_url(&self, redirect: &str, pkce: &Pkce) -> Url {
        let mut url = self.endpoints.authorize.clone();
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("client_id", self.app.client_id())
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", redirect)
            .append_pair("scope", SCOPE)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &pkce.state)
            .append_pair("prompt", "select_account");
        url
    }

    /// The Minecraft Launcher's sign-in page on login.live.com. It ends on [`LIVE_REDIRECT`].
    pub fn live_authorize_url(&self) -> Url {
        let mut url = self.endpoints.live_authorize.clone();
        url.set_query(None);
        url.query_pairs_mut()
            .append_pair("client_id", MsaApp::Live.client_id())
            .append_pair("response_type", "code")
            .append_pair("scope", LIVE_SCOPE)
            .append_pair("redirect_uri", LIVE_REDIRECT);
        url
    }

    /// Trades the code the browser returned for tokens (the Azure app).
    pub async fn redeem_code(
        &self,
        code: &str,
        verifier: &str,
        redirect: &str,
    ) -> Result<MsTokens> {
        self.token_grant(
            &self.endpoints.token,
            &[
                ("client_id", self.app.client_id()),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("code_verifier", verifier),
                ("redirect_uri", redirect),
                ("scope", SCOPE),
            ],
        )
        .await
    }

    /// Trades the code read off the desktop redirect for tokens (the Minecraft Launcher's
    /// client). There is no secret and no verifier.
    pub async fn redeem_live_code(&self, code: &str) -> Result<MsTokens> {
        self.token_grant(
            &self.endpoints.live_token,
            &[
                ("client_id", MsaApp::Live.client_id()),
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", LIVE_REDIRECT),
                ("scope", LIVE_SCOPE),
            ],
        )
        .await
    }

    /// Trades a refresh token for a new pair (Microsoft rotates the refresh token), with the
    /// app it was issued to.
    pub async fn refresh(&self, refresh_token: &str) -> Result<MsTokens> {
        match &self.app {
            MsaApp::Azure(id) => {
                self.token_grant(
                    &self.endpoints.token,
                    &[
                        ("client_id", id.as_str()),
                        ("grant_type", "refresh_token"),
                        ("refresh_token", refresh_token),
                        ("scope", SCOPE),
                    ],
                )
                .await
            }
            MsaApp::Live => {
                self.token_grant(
                    &self.endpoints.live_token,
                    &[
                        ("client_id", MsaApp::Live.client_id()),
                        ("grant_type", "refresh_token"),
                        ("refresh_token", refresh_token),
                        ("redirect_uri", LIVE_REDIRECT),
                        ("scope", LIVE_SCOPE),
                    ],
                )
                .await
            }
        }
    }

    async fn token_grant(&self, endpoint: &Url, form: &[(&str, &str)]) -> Result<MsTokens> {
        let host = endpoint
            .host_str()
            .unwrap_or("login.microsoftonline.com")
            .to_string();
        let response = self
            .http
            .post(endpoint.clone())
            .form(form)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        let status = response.status();
        let body = read_capped(response, BODY_CAP)
            .await
            .map_err(|e| request_failed(&host, e))?;
        if status.is_success() {
            let answer: TokenAnswer = body
                .as_deref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .ok_or_else(|| unexpected(&host, "the token answer wasn't readable"))?;
            return match answer {
                TokenAnswer {
                    access_token: Some(access_token),
                    refresh_token: Some(refresh_token),
                } if !access_token.is_empty() && !refresh_token.is_empty() => Ok(MsTokens {
                    access_token,
                    refresh_token,
                }),
                _ => Err(unexpected(&host, "the token answer had no tokens")),
            };
        }
        match status.as_u16() {
            429 => Err(rate_limited()),
            400..=499 => {
                let error = body
                    .as_deref()
                    .and_then(|b| serde_json::from_slice::<OAuthError>(b).ok())
                    .and_then(|e| e.error);
                if status.as_u16() == 400 && error.as_deref() == Some("invalid_grant") {
                    Err(sign_in_again())
                } else {
                    Err(sign_in_failed(error.as_deref().and_then(plain_error_code)))
                }
            }
            s => Err(host_trouble(&host, s)),
        }
    }
}
