//! The skin a Microsoft account wears: read the active one from the Minecraft profile, fetch its
//! texture from `textures.minecraft.net`, and upload a new one. The token goes only in the
//! `Authorization` header, which is marked sensitive, and never into an error or a log line.

use reqwest::{header::HeaderValue, multipart};
use serde::Deserialize;

use super::{minecraft::McToken, rate_limited, MsaClient};
use crate::{
    error::{ErrorCategory, ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_trouble, request_failed, unexpected},
        Url,
    },
    skins_library::SkinModel,
    skins_png::{self, MAX_SKIN_BYTES},
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

fn not_a_texture(url: &str) -> LauncherError {
    LauncherError::new(
        ErrorCategory::Integrity,
        ErrorCode::SkinInvalid,
        "Your current skin couldn't be read.",
    )
    .with_detail(format!("texture address refused: {url}"))
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

    /// The texture at `url`, normalised. Only the textures host is read (the profile gives
    /// `http://` addresses, so the scheme always follows the configured base).
    pub async fn skin_texture(&self, url: &str) -> Result<Vec<u8>> {
        let base = &self.endpoints.textures;
        let mut parsed = Url::parse(url).map_err(|_| not_a_texture(url))?;
        if parsed.host_str() != base.host_str()
            || !matches!(parsed.scheme(), "http" | "https")
            || parsed.username() != ""
            || parsed.password().is_some()
        {
            return Err(not_a_texture(url));
        }
        parsed
            .set_scheme(base.scheme())
            .map_err(|_| not_a_texture(url))?;
        parsed
            .set_port(base.port())
            .map_err(|_| not_a_texture(url))?;
        let host = base
            .host_str()
            .unwrap_or("textures.minecraft.net")
            .to_string();
        let response = self
            .http
            .get(parsed)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        let status = response.status();
        if !status.is_success() {
            return Err(host_trouble(&host, status.as_u16()));
        }
        let bytes = read_capped(response, MAX_SKIN_BYTES)
            .await
            .map_err(|e| request_failed(&host, e))?
            .ok_or_else(|| not_a_texture(url))?;
        skins_png::normalise(&bytes)
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
