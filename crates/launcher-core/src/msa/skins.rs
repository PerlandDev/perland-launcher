//! The skin a Microsoft account wears: read the active one's arm model from the Minecraft profile,
//! and upload a new one. (The texture itself comes through `crate::skins`, cached.) The token goes only in the
//! `Authorization` header, which is marked sensitive, and never into an error or a log line.

use reqwest::{header::HeaderValue, multipart};
use serde::Deserialize;

use super::{minecraft::McToken, rate_limited, MsaClient};
use crate::{
    error::{ErrorCategory, ErrorCode, LauncherError, Result},
    invite::client::{host_trouble, request_failed, unexpected},
    skins_library::SkinModel,
};

/// The skin an account wears: where its texture is and its arm model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveSkin {
    pub url: String,
    pub model: SkinModel,
}

#[derive(Deserialize)]
struct ProfileSkins {
    #[serde(default)]
    skins: Vec<ProfileSkin>,
}

#[derive(Deserialize)]
struct ProfileSkin {
    #[serde(default)]
    state: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    variant: String,
}

fn rejected() -> LauncherError {
    LauncherError::new(
        ErrorCategory::Auth,
        ErrorCode::SkinRejected,
        "Minecraft didn't accept this skin.",
    )
    .with_fix("Check that it's a 64 × 64 skin and try again in a minute.")
}

fn bearer(host: &str, mc: &McToken) -> Result<HeaderValue> {
    let mut auth = HeaderValue::from_str(&format!("Bearer {}", mc.access_token))
        .map_err(|_| unexpected(host, "the credential isn't a valid header"))?;
    auth.set_sensitive(true);
    Ok(auth)
}

impl MsaClient {
    /// The skin the account wears now; `None` when it has none (or no profile).
    pub async fn active_skin(&self, mc: &McToken) -> Result<Option<ActiveSkin>> {
        let host = self.mc_host();
        let url = self.mc_url("minecraft/profile")?;
        let Some(bytes) = self.mc_get(&host, url, mc).await? else {
            return Ok(None);
        };
        let profile: ProfileSkins = serde_json::from_slice(&bytes)
            .map_err(|_| unexpected(&host, "the profile answer wasn't readable"))?;
        Ok(profile
            .skins
            .into_iter()
            .find(|s| s.state.eq_ignore_ascii_case("active") && !s.url.is_empty())
            .map(|s| ActiveSkin {
                url: s.url,
                model: if s.variant.eq_ignore_ascii_case("slim") {
                    SkinModel::Slim
                } else {
                    SkinModel::Classic
                },
            }))
    }

    /// Makes `png` the account's skin, drawn for `model`.
    pub async fn upload_skin(&self, mc: &McToken, png: Vec<u8>, model: SkinModel) -> Result<()> {
        let host = self.mc_host();
        let url = self.mc_url("minecraft/profile/skins")?;
        let file = multipart::Part::bytes(png)
            .file_name("skin.png")
            .mime_str("image/png")
            .map_err(|_| unexpected(&host, "the skin couldn't be attached"))?;
        let form = multipart::Form::new()
            .text("variant", model.variant())
            .part("file", file);
        let response = self
            .http
            .post(url)
            .header("Accept", "application/json")
            .header(reqwest::header::AUTHORIZATION, bearer(&host, mc)?)
            .multipart(form)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        let status = response.status();
        match status.as_u16() {
            200..=299 => Ok(()),
            401 => Err(LauncherError::auth(
                ErrorCode::MicrosoftSignInAgain,
                "Sign in with Microsoft again to change your skin.",
            )
            .with_fix("Sign in")),
            429 => Err(rate_limited()),
            400..=499 => Err(rejected().with_detail(format!("HTTP {status}"))),
            s => Err(host_trouble(&host, s)),
        }
    }
}
