//! Microsoft sign-in: the credential store for refresh tokens and the Microsoft/Xbox/Minecraft
//! service chain. `MsaClient` holds the HTTP client and endpoints; each step is an `impl`
//! block in its own module (`oauth`, `xbox`, `minecraft`).
pub mod minecraft;
pub mod oauth;
pub mod secrets;
pub mod xbox;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{
    accounts::BuildKind,
    error::{ErrorCode, LauncherError},
    http::HttpClient,
    invite::Url,
};

/// The most of an answer or error body from Microsoft or Xbox that is read.
pub(super) const BODY_CAP: usize = 64 << 10;

pub(super) fn rate_limited() -> LauncherError {
    LauncherError::network(
        ErrorCode::MicrosoftRateLimited,
        "Microsoft is getting too many sign-ins right now. Try again in a minute.",
    )
}

/// The generic refusal; `detail` is a short plain code from the service, never a token.
pub(super) fn sign_in_failed(detail: Option<&str>) -> LauncherError {
    let e = LauncherError::auth(
        ErrorCode::SignInFailed,
        "Couldn't sign you in with Microsoft. Try again.",
    );
    match detail {
        Some(d) => e.with_detail(d),
        None => e,
    }
}

/// One call of the sign-in chain, reported as it starts: redeeming Microsoft's code, the Xbox Live
/// user token, the XSTS token for Minecraft, Minecraft's `login_with_xbox`, then the profile and
/// ownership check. A refusal stops the chain on the step it was reported for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MsaStep {
    Microsoft,
    Xbox,
    Xsts,
    Minecraft,
    Profile,
}

/// Where each step of the chain lives. Tests point these at a mock server.
#[derive(Debug, Clone)]
pub struct MsaEndpoints {
    pub authorize: Url,
    pub token: Url,
    pub xbox_user: Url,
    pub xsts: Url,
    /// The Minecraft services base, without a path.
    pub minecraft: Url,
}

impl MsaEndpoints {
    pub fn production() -> Self {
        let url = |s: &str| Url::parse(s).expect("a constant URL");
        Self {
            authorize: url("https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize"),
            token: url("https://login.microsoftonline.com/consumers/oauth2/v2.0/token"),
            xbox_user: url("https://user.auth.xboxlive.com/user/authenticate"),
            xsts: url("https://xsts.auth.xboxlive.com/xsts/authorize"),
            minecraft: url("https://api.minecraftservices.com"),
        }
    }
}

/// Talks to Microsoft, Xbox and Minecraft services.
#[derive(Clone)]
pub struct MsaClient {
    pub(crate) http: HttpClient,
    pub(crate) endpoints: MsaEndpoints,
    pub(crate) client_id: String,
}

impl MsaClient {
    pub fn new(http: HttpClient, endpoints: MsaEndpoints, client_id: String) -> Self {
        Self {
            http,
            endpoints,
            client_id,
        }
    }
}

impl fmt::Debug for MsaClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MsaClient").finish_non_exhaustive()
    }
}

/// The two tokens Microsoft's token endpoint hands back.
#[derive(Clone, PartialEq, Eq)]
pub struct MsTokens {
    pub access_token: String,
    pub refresh_token: String,
}

impl fmt::Debug for MsTokens {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("MsTokens { .. }")
    }
}

/// The Azure application id Microsoft sign-in uses: `run` (the `PERLAND_MS_CLIENT_ID` of the
/// running process) in a dev build only, else `built` (the value the release build was made
/// with). Anything but a GUID is ignored, and only that fact is logged: the value could be
/// anything.
pub fn client_id(run: Option<&str>, built: Option<&str>, build: BuildKind) -> Option<String> {
    let run = (build == BuildKind::Dev).then_some(run).flatten();
    let raw = run.or(built).map(str::trim).filter(|raw| !raw.is_empty())?;
    if is_guid(raw) {
        Some(raw.to_string())
    } else {
        tracing::warn!("PERLAND_MS_CLIENT_ID isn't a GUID; ignored");
        None
    }
}

/// `8-4-4-4-12` hex digits.
fn is_guid(s: &str) -> bool {
    s.len() == 36
        && s.split('-').map(str::len).eq([8, 4, 4, 4, 12])
        && s.chars().all(|c| c == '-' || c.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = "11111111-2222-3333-4444-555555555555";
    const BUILT: &str = "AAAAAAAA-bbbb-CCCC-dddd-eeeeeeeeeeee";

    #[test]
    fn client_id_runtime_only_in_dev() {
        assert_eq!(
            client_id(Some(RUN), Some(BUILT), BuildKind::Dev).as_deref(),
            Some(RUN)
        );
        assert_eq!(
            client_id(Some(RUN), None, BuildKind::Dev).as_deref(),
            Some(RUN)
        );
        for build in [BuildKind::Beta, BuildKind::Release] {
            assert_eq!(
                client_id(Some(RUN), Some(BUILT), build).as_deref(),
                Some(BUILT)
            );
            assert_eq!(client_id(Some(RUN), None, build), None);
        }
    }

    #[test]
    fn client_id_rejects_non_guid() {
        for bad in [
            "",
            "   ",
            "not-a-guid",
            "1111111122223333444455555555555555555",
            "11111111-2222-3333-4444-55555555555",
            "11111111-2222-3333-4444-5555555555555",
            "1111111-12222-3333-4444-555555555555",
            "g1111111-2222-3333-4444-555555555555",
            "{11111111-2222-3333-4444-555555555555}",
        ] {
            assert_eq!(client_id(Some(bad), None, BuildKind::Dev), None, "{bad:?}");
            assert_eq!(
                client_id(None, Some(bad), BuildKind::Release),
                None,
                "{bad:?}"
            );
        }
        // A bad runtime value doesn't fall back to the built one: it is the developer's mistake.
        assert_eq!(client_id(Some("x"), Some(BUILT), BuildKind::Dev), None);
    }

    #[test]
    fn client_id_built_value_used_in_release() {
        assert_eq!(
            client_id(None, Some(&format!("  {BUILT}\n")), BuildKind::Release).as_deref(),
            Some(BUILT)
        );
        assert_eq!(client_id(None, None, BuildKind::Release), None);
        assert_eq!(client_id(None, Some(""), BuildKind::Release), None);
    }
}
