//! Minecraft services login, ownership and profile (steps 4 and 5 of the sign-in chain), and
//! `finish`, which runs the whole chain after Microsoft's tokens. Nothing here puts a token into
//! an error or a log line.

use std::fmt;

use reqwest::header::HeaderValue;
use serde::Deserialize;
use serde_json::json;

use super::{rate_limited, sign_in_failed, xbox::Xsts, MsTokens, MsaClient, MsaStep, BODY_CAP};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_trouble, request_failed, unexpected},
        Url,
    },
};

/// The Minecraft services access token and when it stops working (Unix seconds).
#[derive(Clone, PartialEq, Eq)]
pub struct McToken {
    pub access_token: String,
    pub expires_at: u64,
}

impl fmt::Debug for McToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("McToken { .. }")
    }
}

/// The player's Minecraft profile; `id` is the API's 32-hex form.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct McProfile {
    pub id: String,
    pub name: String,
}

/// Everything a finished sign-in yields.
#[derive(Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub profile: McProfile,
    pub refresh_token: String,
    pub minecraft: McToken,
    pub xuid: Option<String>,
}

impl fmt::Debug for SignedIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedIn")
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct LoginAnswer {
    access_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Deserialize)]
struct Entitlements {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Deserialize)]
struct Item {
    name: Option<String>,
}

/// 32 hex characters to the 8-4-4-4-12 form, lower case. Anything else comes back unchanged.
pub fn dashed(id: &str) -> String {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return id.to_string();
    }
    let id = id.to_ascii_lowercase();
    format!(
        "{}-{}-{}-{}-{}",
        &id[..8],
        &id[8..12],
        &id[12..16],
        &id[16..20],
        &id[20..]
    )
}

/// A profile settles it; without one, owning the game means the name isn't picked yet.
pub fn ownership(profile: Option<McProfile>, owns: bool) -> Result<McProfile> {
    match profile {
        Some(profile) => Ok(profile),
        None if owns => Err(LauncherError::auth(
            ErrorCode::MinecraftNoProfile,
            "This account owns Minecraft but hasn't picked a player name yet.",
        )
        .with_fix("Set one up at minecraft.net, then sign in again.")),
        None => Err(LauncherError::auth(
            ErrorCode::MinecraftNotOwned,
            "This Microsoft account doesn't own Minecraft: Java Edition.",
        )
        .with_fix("Sign in with the account that bought it, or get it at minecraft.net.")),
    }
}

fn sign_in_again() -> LauncherError {
    LauncherError::auth(
        ErrorCode::MicrosoftSignInAgain,
        "Sign in with Microsoft again to keep playing.",
    )
    .with_fix("Sign in")
}

impl MsaClient {
    fn mc_url(&self, path: &str) -> Result<Url> {
        let host = self
            .endpoints
            .minecraft
            .host_str()
            .unwrap_or("minecraftservices.com");
        self.endpoints
            .minecraft
            .join(path)
            .map_err(|_| unexpected(host, "the Minecraft services address isn't valid"))
    }

    fn mc_host(&self) -> String {
        self.endpoints
            .minecraft
            .host_str()
            .unwrap_or("minecraftservices.com")
            .to_string()
    }

    /// Trades the XSTS token for a Minecraft access token.
    pub async fn minecraft_login(&self, xsts: &Xsts, now: u64) -> Result<McToken> {
        let host = self.mc_host();
        let url = self.mc_url("authentication/login_with_xbox")?;
        let body = json!({ "identityToken": format!("XBL3.0 x={};{}", xsts.uhs, xsts.token) });
        let response = self
            .http
            .post(url)
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        let status = response.status();
        let bytes = read_capped(response, BODY_CAP)
            .await
            .map_err(|e| request_failed(&host, e))?;
        if status.is_success() {
            let answer: LoginAnswer = bytes
                .as_deref()
                .and_then(|b| serde_json::from_slice(b).ok())
                .ok_or_else(|| unexpected(&host, "the login answer wasn't readable"))?;
            return match (
                answer.access_token.filter(|t| !t.is_empty()),
                answer.expires_in,
            ) {
                (Some(access_token), Some(expires_in)) => Ok(McToken {
                    access_token,
                    expires_at: now.saturating_add(expires_in),
                }),
                _ => Err(unexpected(&host, "the login answer had no token")),
            };
        }
        match status.as_u16() {
            403 => Err(LauncherError::auth(
                ErrorCode::MicrosoftAwaitingApproval,
                "Microsoft sign-in is waiting on approval from Microsoft. You'll be able to play as soon as it's ready.",
            )),
            429 => Err(rate_limited()),
            400..=499 => Err(sign_in_failed(None)),
            s => Err(host_trouble(&host, s)),
        }
    }

    /// Whether the account has bought Minecraft: Java Edition.
    pub async fn owns_game(&self, mc: &McToken) -> Result<bool> {
        let host = self.mc_host();
        let url = self.mc_url("entitlements/mcstore")?;
        let bytes = self
            .mc_get(&host, url, mc)
            .await?
            .ok_or_else(|| unexpected(&host, "the entitlements answer wasn't readable"))?;
        let entitlements: Entitlements = serde_json::from_slice(&bytes)
            .map_err(|_| unexpected(&host, "the entitlements answer wasn't readable"))?;
        Ok(entitlements.items.iter().any(|i| {
            matches!(
                i.name.as_deref(),
                Some("game_minecraft" | "product_minecraft")
            )
        }))
    }

    /// The player's profile, or `None` when the account hasn't made one.
    pub async fn profile(&self, mc: &McToken) -> Result<Option<McProfile>> {
        let host = self.mc_host();
        let url = self.mc_url("minecraft/profile")?;
        let Some(bytes) = self.mc_get(&host, url, mc).await? else {
            return Ok(None);
        };
        let profile: McProfile = serde_json::from_slice(&bytes)
            .map_err(|_| unexpected(&host, "the profile answer wasn't readable"))?;
        if profile.id.is_empty() || profile.name.is_empty() {
            return Err(unexpected(&host, "the profile answer was incomplete"));
        }
        Ok(Some(profile))
    }

    /// Runs the chain after Microsoft's tokens: Xbox user, XSTS, Minecraft login, profile, and
    /// the entitlements only when there is no profile to tell why.
    pub async fn finish(&self, ms: MsTokens, now: u64) -> Result<SignedIn> {
        self.finish_with(ms, now, &|_| {}).await
    }

    /// [`finish`](Self::finish), reporting each step to `step` as it starts.
    pub async fn finish_with(
        &self,
        ms: MsTokens,
        now: u64,
        step: &(dyn Fn(MsaStep) + Send + Sync),
    ) -> Result<SignedIn> {
        step(MsaStep::Xbox);
        let user = self.xbox_user(&ms.access_token).await?;
        step(MsaStep::Xsts);
        let xsts = self.xsts(&user).await?;
        step(MsaStep::Minecraft);
        let minecraft = self.minecraft_login(&xsts, now).await?;
        step(MsaStep::Profile);
        let profile = self.profile(&minecraft).await?;
        let owns = match profile {
            Some(_) => false,
            None => self.owns_game(&minecraft).await?,
        };
        Ok(SignedIn {
            profile: ownership(profile, owns)?,
            refresh_token: ms.refresh_token,
            minecraft,
            xuid: xsts.xuid,
        })
    }

    /// One authenticated GET: the body on 200, `None` on 404 (only the profile route cares), 401
    /// as "sign in again".
    async fn mc_get(&self, host: &str, url: Url, mc: &McToken) -> Result<Option<Vec<u8>>> {
        let mut auth = HeaderValue::from_str(&format!("Bearer {}", mc.access_token))
            .map_err(|_| unexpected(host, "the credential isn't a valid header"))?;
        auth.set_sensitive(true);
        let response = self
            .http
            .get(url)
            .header("Accept", "application/json")
            .header(reqwest::header::AUTHORIZATION, auth)
            .send()
            .await
            .map_err(|e| request_failed(host, e))?;
        let status = response.status();
        let bytes = read_capped(response, BODY_CAP)
            .await
            .map_err(|e| request_failed(host, e))?;
        if status.is_success() {
            return bytes
                .map(Some)
                .ok_or_else(|| unexpected(host, "the answer was too large"));
        }
        match status.as_u16() {
            404 => Ok(None),
            401 => Err(sign_in_again()),
            429 => Err(rate_limited()),
            400..=499 => Err(sign_in_failed(None)),
            s => Err(host_trouble(host, s)),
        }
    }
}
