//! The Engine's player routes (`/api/v1/player`): sign-in, the profile, and Your servers. The
//! bearer token goes in a header, never in a URL; request bodies hold the PKCE verifier, the
//! authorization code and the email code, so none of them is ever printed.

use std::fmt;

use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_of, host_trouble, rate_limited, request_failed, unexpected},
        InviteTiming,
    },
    public_list::PublicServer,
};

/// The longest answer read, in bytes.
const BODY_CAP: usize = 64 << 10;
/// Your servers: up to 200 cards.
const SERVERS_CAP: usize = 1 << 20;
/// The longest error body read, in bytes.
const ERROR_CAP: usize = 4 << 10;

/// A way to sign in. Unknown methods are dropped, so an Engine ahead of the launcher doesn't
/// offer a button the launcher can't run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInMethod {
    Discord,
    Email,
}

impl SignInMethod {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "discord" => Some(Self::Discord),
            "email" => Some(Self::Email),
            _ => None,
        }
    }
}

fn known_methods<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Vec<SignInMethod>, D::Error> {
    let names = Vec::<String>::deserialize(d)?;
    Ok(names
        .iter()
        .filter_map(|n| SignInMethod::parse(n))
        .collect())
}

/// What the Engine offers: `GET /config`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct PlayerConfig {
    pub enabled: bool,
    #[serde(deserialize_with = "known_methods")]
    pub methods: Vec<SignInMethod>,
}

impl PlayerConfig {
    /// Accounts are off (or the Engine predates them).
    pub fn off() -> Self {
        Self::default()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DiscordAccount {
    pub id: String,
    pub username: String,
    pub global_name: Option<String>,
}

/// The signed-in player. `display_name` is `""` for an email-only player.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Player {
    pub id: String,
    pub display_name: String,
    pub email: Option<String>,
    pub discord: Option<DiscordAccount>,
    pub created_at: String,
}

/// A sign-in's result. `token` is the credential.
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenAnswer {
    pub token: String,
    pub player: Player,
    #[serde(default)]
    pub new_player: bool,
}

// Manual Debug: the token is a credential.
impl fmt::Debug for TokenAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenAnswer")
            .field("new_player", &self.new_player)
            .finish_non_exhaustive()
    }
}

/// An email code was sent: seconds until it expires and until another may be asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EmailStarted {
    pub expires_in: u64,
    pub resend_in: u64,
}

/// How the player relates to a server in Your servers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Relation {
    Owner,
    Invited,
    /// Also what a value the launcher doesn't know reads as.
    #[default]
    Followed,
}

impl<'de> Deserialize<'de> for Relation {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        Ok(match String::deserialize(d)?.as_str() {
            "owner" => Self::Owner,
            "invited" => Self::Invited,
            _ => Self::Followed,
        })
    }
}

/// One entry of Your servers: the public list's card plus how the player relates to it.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct YourServer {
    #[serde(flatten)]
    pub server: PublicServer,
    #[serde(default)]
    pub relation: Relation,
    #[serde(default)]
    pub following: bool,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

#[derive(Deserialize)]
struct ServersData {
    #[serde(default)]
    servers: Vec<YourServer>,
}

/// What a failed request's error data says.
struct Failure {
    code: Option<String>,
    data: Value,
}

struct Reply {
    status: u16,
    body: Vec<u8>,
}

/// Where a refusal comes from, for the few codes that mean different things by route.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Plain,
    /// `POST /token`: the player typed no code, so a refused one is a failed sign-in.
    Token,
    /// Your servers: a 404 is an Engine that doesn't have them.
    Servers,
}

#[derive(Clone)]
pub struct PlayerClient {
    http: reqwest::Client,
    engine: Url,
    timeout: std::time::Duration,
}

// Manual Debug: keep to the origin.
impl fmt::Debug for PlayerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlayerClient")
            .field("engine", &host_of(&self.engine))
            .finish_non_exhaustive()
    }
}

impl PlayerClient {
    /// `engine` should come from `check_engine_url` (as `engine_for_site` gives it).
    pub fn new(http: reqwest::Client, engine: Url) -> Self {
        Self {
            http,
            engine,
            timeout: InviteTiming::default().request_timeout,
        }
    }

    pub fn engine(&self) -> &Url {
        &self.engine
    }

    fn host(&self) -> String {
        host_of(&self.engine)
    }

    fn route(&self, route: &str) -> Url {
        Url::parse(&format!(
            "{}/api/v1/player{route}",
            self.engine.as_str().trim_end_matches('/')
        ))
        .expect("a checked Engine URL plus a fixed path is a URL")
    }

    /// `GET /config`. An Engine without accounts (404) or with them off (503) is
    /// `PlayerConfig::off()`.
    pub async fn config(&self) -> Result<PlayerConfig> {
        let reply = self
            .send(Method::GET, "/config", None, None, BODY_CAP)
            .await?;
        match reply.status {
            404 | 503 => Ok(PlayerConfig::off()),
            200..=299 => self.data(&reply),
            _ => Err(self.refusal(&reply, Route::Plain)),
        }
    }

    /// `POST /token`: redeem the code the loopback redirect delivered.
    pub async fn token(&self, code: &str, verifier: &str, redirect: &str) -> Result<TokenAnswer> {
        let body = json!({ "code": code, "codeVerifier": verifier, "redirectUri": redirect });
        let reply = self
            .send(Method::POST, "/token", None, Some(&body), BODY_CAP)
            .await?;
        self.ok(&reply, Route::Token)?;
        self.data(&reply)
    }

    /// `POST /email/start`: email a one-time code.
    pub async fn email_start(&self, email: &str, language: &str) -> Result<EmailStarted> {
        let body = json!({ "email": email, "language": language });
        let reply = self
            .send(Method::POST, "/email/start", None, Some(&body), BODY_CAP)
            .await?;
        self.ok(&reply, Route::Plain)?;
        self.data(&reply)
    }

    /// `POST /email/verify`: trade the emailed code for a token.
    pub async fn email_verify(&self, email: &str, code: &str) -> Result<TokenAnswer> {
        let body = json!({ "email": email, "code": code });
        let reply = self
            .send(Method::POST, "/email/verify", None, Some(&body), BODY_CAP)
            .await?;
        self.ok(&reply, Route::Plain)?;
        self.data(&reply)
    }

    /// `GET /me`. A 401 is `perland_signed_out`.
    pub async fn me(&self, token: &str) -> Result<Player> {
        let reply = self
            .send(Method::GET, "/me", Some(token), None, BODY_CAP)
            .await?;
        self.ok(&reply, Route::Plain)?;
        self.data(&reply)
    }

    /// `POST /sign-out`: delete the token's session on the Engine.
    pub async fn sign_out(&self, token: &str) -> Result<()> {
        let reply = self
            .send(Method::POST, "/sign-out", Some(token), None, BODY_CAP)
            .await?;
        self.ok(&reply, Route::Plain)
    }

    /// `GET /servers`: Your servers, in the Engine's order.
    pub async fn servers(&self, token: &str) -> Result<Vec<YourServer>> {
        let reply = self
            .send(Method::GET, "/servers", Some(token), None, SERVERS_CAP)
            .await?;
        self.ok(&reply, Route::Servers)?;
        Ok(self.data::<ServersData>(&reply)?.servers)
    }

    /// `POST /servers/follow`. A server that isn't public now (404) is not an error.
    pub async fn follow(&self, token: &str, invite_token: &str) -> Result<()> {
        self.server_write("/servers/follow", token, invite_token)
            .await
    }

    /// `POST /servers/unfollow`. A server that is gone (404) is already unfollowed.
    pub async fn unfollow(&self, token: &str, invite_token: &str) -> Result<()> {
        self.server_write("/servers/unfollow", token, invite_token)
            .await
    }

    /// `POST /servers/joined`. A server that can't be recorded (404) is not an error.
    pub async fn joined(&self, token: &str, invite_token: &str) -> Result<()> {
        self.server_write("/servers/joined", token, invite_token)
            .await
    }

    async fn server_write(&self, route: &str, token: &str, invite_token: &str) -> Result<()> {
        let body = json!({ "token": invite_token });
        let reply = self
            .send(Method::POST, route, Some(token), Some(&body), BODY_CAP)
            .await?;
        if reply.status == 404 {
            return Ok(());
        }
        self.ok(&reply, Route::Servers)
    }

    /// One request. Any status comes back as a `Reply`; only a request that got no answer, or
    /// a success too large to read, is an error here. An error body is read up to 4 KiB (and
    /// dropped when longer).
    async fn send(
        &self,
        method: Method,
        route: &str,
        token: Option<&str>,
        body: Option<&Value>,
        cap: usize,
    ) -> Result<Reply> {
        let host = self.host();
        let mut request = self
            .http
            .request(method, self.route(route))
            .timeout(self.timeout);
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.map_err(|e| request_failed(&host, e))?;
        let status = response.status().as_u16();
        let ok = response.status().is_success();
        let read = read_capped(response, if ok { cap } else { ERROR_CAP }).await;
        let body = match read {
            Ok(Some(body)) => body,
            Ok(None) if ok => return Err(unexpected(&host, "the answer is too large")),
            Ok(None) => Vec::new(),
            Err(e) if ok => return Err(request_failed(&host, e)),
            Err(_) => Vec::new(),
        };
        Ok(Reply { status, body })
    }

    /// `Ok` for a success status, else the refusal.
    fn ok(&self, reply: &Reply, route: Route) -> Result<()> {
        if (200..300).contains(&reply.status) {
            Ok(())
        } else {
            Err(self.refusal(reply, route))
        }
    }

    /// The `data` of a success. The parser's own message can quote the body, so it is never
    /// shown.
    fn data<T: DeserializeOwned>(&self, reply: &Reply) -> Result<T> {
        serde_json::from_slice::<Envelope<T>>(&reply.body)
            .ok()
            .and_then(|e| e.data)
            .ok_or_else(|| unexpected(&self.host(), "the answer isn't valid"))
    }

    fn failure(reply: &Reply) -> Failure {
        let data = serde_json::from_slice::<Value>(&reply.body)
            .ok()
            .and_then(|mut v| v.get_mut("data").map(Value::take))
            .unwrap_or(Value::Null);
        let code = data.get("code").and_then(Value::as_str).map(str::to_string);
        Failure { code, data }
    }

    /// What a refused request means: the Engine's `data.code` first, then the status.
    fn refusal(&self, reply: &Reply, route: Route) -> LauncherError {
        let host = self.host();
        let failure = Self::failure(reply);
        let param = |name: &str| match failure.data.get(name) {
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        match failure.code.as_deref() {
            Some("player_accounts_disabled") => return accounts_closed(&host),
            Some("sign_in_method_unavailable") => {
                return sign_in_unavailable(&host, param("method").as_deref())
            }
            Some("invalid_email") => return invalid_email(&host),
            Some("invalid_code") if route == Route::Token => return sign_in_failed(&host),
            Some("invalid_code") => return invalid_code(&host, param("attemptsLeft")),
            Some("too_many_codes") => return too_many_codes(&host, param("retryAfter")),
            Some("email_unavailable") => return email_unavailable(&host),
            Some("player_servers_unavailable") => return servers_closed(&host),
            Some("too_many_player_servers") => return too_many_player_servers(&host),
            _ => {}
        }
        match reply.status {
            401 => signed_out(&host),
            404 if route == Route::Servers => servers_closed(&host),
            429 => rate_limited(&host),
            400 => unexpected(&host, "the request was refused"),
            s => host_trouble(&host, s),
        }
    }
}

fn accounts_closed(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::PlayerAccountsClosed,
        "Perland accounts aren't open yet.",
    )
    .with_param("host", host)
}

fn sign_in_unavailable(host: &str, method: Option<&str>) -> LauncherError {
    let e = LauncherError::network(
        ErrorCode::SignInUnavailable,
        "That way of signing in isn't available.",
    )
    .with_param("host", host);
    match method {
        Some(method) => e.with_param("method", method),
        None => e,
    }
}

fn sign_in_failed(host: &str) -> LauncherError {
    LauncherError::auth(ErrorCode::SignInFailed, "Couldn't sign you in. Try again.")
        .with_param("host", host)
}

fn invalid_email(host: &str) -> LauncherError {
    LauncherError::auth(ErrorCode::InvalidEmail, "Check the email address.")
        .with_param("host", host)
}

fn invalid_code(host: &str, attempts_left: Option<String>) -> LauncherError {
    let e = LauncherError::auth(ErrorCode::InvalidCode, "That code isn't right.")
        .with_param("host", host);
    match attempts_left {
        Some(n) => e.with_param("attemptsLeft", n),
        None => e,
    }
}

fn too_many_codes(host: &str, retry_after: Option<String>) -> LauncherError {
    // The Engine always says; a minute is its own resend wait.
    let seconds = retry_after.unwrap_or_else(|| "60".to_string());
    LauncherError::network(
        ErrorCode::TooManyCodes,
        format!("Wait {seconds} seconds before asking for another code."),
    )
    .with_param("host", host)
    .with_param("retryAfter", seconds)
}

fn email_unavailable(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::EmailUnavailable,
        "The email couldn't be sent. Try again in a moment.",
    )
    .with_param("host", host)
}

fn signed_out(host: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::PerlandSignedOut,
        "You were signed out of Perland.",
    )
    .with_param("host", host)
}

fn servers_closed(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::PlayerServersClosed,
        "Your servers aren't available right now.",
    )
    .with_param("host", host)
}

fn too_many_player_servers(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::TooManyPlayerServers,
        "You have too many servers saved. Unfollow some first.",
    )
    .with_param("host", host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_unknown_is_followed() {
        let relation: Relation = serde_json::from_value(json!("friend")).unwrap();
        assert_eq!(relation, Relation::Followed);
        let relation: Relation = serde_json::from_value(json!("owner")).unwrap();
        assert_eq!(relation, Relation::Owner);
    }

    #[test]
    fn methods_drop_unknown() {
        let config: PlayerConfig = serde_json::from_value(
            json!({ "enabled": true, "methods": ["discord", "passkey", "email"] }),
        )
        .unwrap();
        assert_eq!(config.methods, [SignInMethod::Discord, SignInMethod::Email]);
    }

    #[test]
    fn token_answer_debug_hides_token() {
        let token = format!("plt_{}", "x".repeat(43));
        let answer: TokenAnswer = serde_json::from_value(
            json!({ "token": token, "player": { "id": "p" }, "newPlayer": true }),
        )
        .unwrap();
        let shown = format!("{answer:?}");
        assert!(!shown.contains("plt_"), "{shown}");
    }
}
