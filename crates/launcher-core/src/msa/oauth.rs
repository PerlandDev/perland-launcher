//! Microsoft's OAuth 2.0 authorization-code flow with PKCE: the authorize URL, redeeming the
//! code and refreshing. Nothing here puts a token, code or verifier into an error.

use serde::Deserialize;

use super::{rate_limited, sign_in_failed, MsTokens, MsaClient, BODY_CAP};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_trouble, request_failed, unexpected},
        Url,
    },
    perland::Pkce,
};

/// What the launcher asks Microsoft for: Xbox sign-in, and a refresh token.
const SCOPE: &str = "XboxLive.signin offline_access";

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
            .append_pair("client_id", &self.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", redirect)
            .append_pair("scope", SCOPE)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &pkce.state)
            .append_pair("prompt", "select_account");
        url
    }

    /// Trades the code the browser returned for tokens.
    pub async fn redeem_code(
        &self,
        code: &str,
        verifier: &str,
        redirect: &str,
    ) -> Result<MsTokens> {
        self.token_grant(&[
            ("client_id", self.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect),
            ("scope", SCOPE),
        ])
        .await
    }

    /// Trades a refresh token for a new pair (Microsoft rotates the refresh token).
    pub async fn refresh(&self, refresh_token: &str) -> Result<MsTokens> {
        self.token_grant(&[
            ("client_id", self.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .await
    }

    async fn token_grant(&self, form: &[(&str, &str)]) -> Result<MsTokens> {
        let host = self
            .endpoints
            .token
            .host_str()
            .unwrap_or("login.microsoftonline.com")
            .to_string();
        let response = self
            .http
            .post(self.endpoints.token.clone())
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
