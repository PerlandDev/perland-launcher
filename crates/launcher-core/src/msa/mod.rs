//! Microsoft sign-in: the credential store for refresh tokens and the Microsoft/Xbox/Minecraft
//! service chain. `MsaClient` holds the HTTP client and endpoints; each step is an `impl`
//! block in its own module (`oauth` here, `xbox` and `minecraft` after).
pub mod oauth;
pub mod secrets;

use std::fmt;

use crate::{http::HttpClient, invite::Url};

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
