//! Xbox Live user token and XSTS token (steps 2 and 3 of the sign-in chain). Nothing here
//! puts a token into an error or a log line.

use std::fmt;

use serde::Deserialize;
use serde_json::json;

use super::{rate_limited, sign_in_failed, MsaClient, BODY_CAP};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_trouble, request_failed, unexpected},
        Url,
    },
};

/// The Xbox Live user token and its user hash.
#[derive(Clone, PartialEq, Eq)]
pub struct XboxUser {
    pub token: String,
    pub uhs: String,
}

impl fmt::Debug for XboxUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("XboxUser { .. }")
    }
}

/// The XSTS token for the Minecraft services, its user hash and the Xbox user id if given.
#[derive(Clone, PartialEq, Eq)]
pub struct Xsts {
    pub token: String,
    pub uhs: String,
    pub xuid: Option<String>,
}

impl fmt::Debug for Xsts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Xsts { .. }")
    }
}

#[derive(Deserialize)]
struct Answer {
    #[serde(rename = "Token")]
    token: Option<String>,
    #[serde(rename = "DisplayClaims")]
    claims: Option<Claims>,
}

#[derive(Deserialize)]
struct Claims {
    #[serde(default)]
    xui: Vec<Xui>,
}

#[derive(Deserialize)]
struct Xui {
    uhs: Option<String>,
    xid: Option<String>,
}

#[derive(Deserialize)]
struct XErrBody {
    #[serde(rename = "XErr")]
    xerr: Option<u64>,
}

/// What XSTS's `XErr` number means for the player.
pub fn xerr_code(xerr: u64) -> ErrorCode {
    match xerr {
        2148916233 => ErrorCode::XboxNoProfile,
        2148916235 => ErrorCode::XboxRegionBlocked,
        2148916236 | 2148916237 => ErrorCode::XboxAgeVerification,
        2148916238 => ErrorCode::XboxChildAccount,
        _ => ErrorCode::SignInFailed,
    }
}

fn xerr_error(code: ErrorCode) -> LauncherError {
    let message = match code {
        ErrorCode::XboxNoProfile => "This Microsoft account has no Xbox profile yet.",
        ErrorCode::XboxRegionBlocked => {
            "Xbox Live isn't available in your country, so this account can't sign in."
        }
        ErrorCode::XboxAgeVerification => {
            "This account needs adult verification on xbox.com before it can sign in."
        }
        ErrorCode::XboxChildAccount => {
            "This is a child account. An adult needs to add it to a Microsoft family first."
        }
        _ => return sign_in_failed(None),
    };
    LauncherError::auth(code, message)
}

impl MsaClient {
    /// Trades the Microsoft access token for an Xbox Live user token.
    pub async fn xbox_user(&self, ms_access: &str) -> Result<XboxUser> {
        let body = json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access}"),
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT",
        });
        let (token, uhs, _) = self.xbox_call(&self.endpoints.xbox_user, &body).await?;
        Ok(XboxUser { token, uhs })
    }

    /// Trades the Xbox user token for an XSTS token for the Minecraft services.
    pub async fn xsts(&self, user: &XboxUser) -> Result<Xsts> {
        let body = json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [user.token],
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT",
        });
        let (token, uhs, xuid) = self.xbox_call(&self.endpoints.xsts, &body).await?;
        Ok(Xsts { token, uhs, xuid })
    }

    /// One POST to an Xbox endpoint: `(Token, uhs, xid)` on success, a mapped error otherwise.
    async fn xbox_call(
        &self,
        url: &Url,
        body: &serde_json::Value,
    ) -> Result<(String, String, Option<String>)> {
        let host = url.host_str().unwrap_or("xboxlive.com").to_string();
        let response = self
            .http
            .post(url.clone())
            .header("Accept", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        let status = response.status();
        let bytes = read_capped(response, BODY_CAP)
            .await
            .map_err(|e| request_failed(&host, e))?;
        if status.is_success() {
            let answer: Answer = bytes
                .as_deref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .ok_or_else(|| unexpected(&host, "the Xbox answer wasn't readable"))?;
            let token = answer.token.filter(|t| !t.is_empty());
            let xui = answer.claims.and_then(|c| c.xui.into_iter().next());
            let uhs = xui
                .as_ref()
                .and_then(|x| x.uhs.clone())
                .filter(|u| !u.is_empty());
            return match (token, uhs) {
                (Some(token), Some(uhs)) => Ok((token, uhs, xui.and_then(|x| x.xid))),
                _ => Err(sign_in_failed(None)),
            };
        }
        match status.as_u16() {
            429 => Err(rate_limited()),
            401 => {
                let xerr = bytes
                    .as_deref()
                    .and_then(|b| serde_json::from_slice::<XErrBody>(b).ok())
                    .and_then(|e| e.xerr);
                Err(xerr.map_or_else(|| sign_in_failed(None), |x| xerr_error(xerr_code(x))))
            }
            400..=499 => Err(sign_in_failed(None)),
            s => Err(host_trouble(&host, s)),
        }
    }
}
