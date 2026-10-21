//! The Engine's player routes (`/api/v1/player`): sign-in, the profile, and Your servers. The
//! bearer token goes in a header, never in a URL; request bodies hold the PKCE verifier, the
//! authorization code and the email code, so none of them is ever printed.

use std::{fmt, time::Duration};

use reqwest::{Method, Url};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    invite::{
        client::{host_of, host_trouble},
        view::modrinth_ref,
        InviteGame, InviteModrinth, InvitePlayers, InviteTiming,
    },
    public_list::PublicServer,
};

use super::wire::{failure, status_fallback, Credential, Reply, Wire};

/// The longest answer read, in bytes.
const BODY_CAP: usize = 64 << 10;
/// Your servers: up to 200 cards.
const SERVERS_CAP: usize = 1 << 20;
/// Per-server stats: 200 servers with up to 2000 advancement IDs each can pass 1 MiB.
const STATS_CAP: usize = 4 << 20;
/// Creating a free server: the Engine builds the stack while the request is open.
pub(super) const CREATE_TIMEOUT: Duration = Duration::from_secs(150);
/// The Engine's own name for Modrinth, for a refusal that blames it.
const MODRINTH_HOST: &str = "modrinth.com";

/// A way to sign in. Unknown methods are dropped, so an Engine ahead of the launcher doesn't
/// offer a button the launcher can't run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignInMethod {
    Discord,
    Email,
    /// The panel's own sign-in page, for operators who manage servers. The Engine lists it
    /// only when that path works for the launcher.
    GitHub,
}

impl SignInMethod {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "discord" => Some(Self::Discord),
            "email" => Some(Self::Email),
            "github" => Some(Self::GitHub),
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
    pub minecraft: Option<MinecraftAccount>,
    pub created_at: String,
}

/// The Minecraft account a player linked with a code.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MinecraftAccount {
    /// The Mojang UUID, dashed.
    pub uuid: String,
    /// The name the account had when it was linked.
    pub name: String,
    pub linked_at: String,
}

/// A Minecraft link code, typed in the game as `/perland link <code>`. Shown once.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCode {
    pub code: String,
    /// The code's lifetime in seconds.
    #[serde(default)]
    pub expires_in: u64,
}

// Manual Debug: the code links an account to whoever types it first.
impl fmt::Debug for LinkCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkCode")
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

/// A server's invite token, which is what joining by invite needs.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
pub struct StatsInvite {
    pub token: String,
}

// Manual Debug: keep invite tokens out of logs.
impl fmt::Debug for StatsInvite {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StatsInvite").finish_non_exhaustive()
    }
}

/// A player's totals on one server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ServerStats {
    pub name: String,
    /// Absent while the server's invite is off.
    pub invite: Option<StatsInvite>,
    pub playtime_seconds: u64,
    pub advancements: u32,
    /// The completed advancements that have a display (no recipes), sorted. At most
    /// [`MAX_ADVANCEMENT_IDS`] are kept.
    pub advancement_ids: Vec<String>,
}

/// The most advancement IDs kept per server: the Engine sends at most this many.
pub const MAX_ADVANCEMENT_IDS: usize = 2000;

/// The signed-in player's stats on every server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlayerStats {
    /// `None` while no Minecraft account is linked.
    pub minecraft: Option<MinecraftAccount>,
    pub servers: Vec<ServerStats>,
}

impl PlayerStats {
    /// Keep at most [`MAX_ADVANCEMENT_IDS`] per server, whatever an Engine sends.
    fn cap_ids(&mut self) {
        for server in &mut self.servers {
            server.advancement_ids.truncate(MAX_ADVANCEMENT_IDS);
        }
    }
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

/// Where an owned server lives on the Engine: its team and its name there.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManageRefCore {
    pub tenant_id: String,
    pub stack: String,
}

/// `manage`, or `None` when it is absent or not the shape the launcher knows: a newer or odd
/// Engine must never drop the card.
fn lenient_manage<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<ManageRefCore>, D::Error> {
    let value = Option::<Value>::deserialize(d)?;
    Ok(value
        .and_then(|v| serde_json::from_value::<ManageRefCore>(v).ok())
        .filter(|m| !m.tenant_id.trim().is_empty() && !m.stack.trim().is_empty()))
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
    /// An owner entry of a server the player can manage (absent on older Engines).
    #[serde(default, deserialize_with = "lenient_manage")]
    pub manage: Option<ManageRefCore>,
}

/// The memory of each kind of free server, in MB.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct MemoryLimits {
    pub vanilla: u32,
    pub modded: u32,
}

/// What every free server has.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FreeLimits {
    pub max_players: u32,
    pub memory_mb: MemoryLimits,
    pub disk_bytes: u64,
    /// 0 when the Engine's plan never stops idle servers.
    pub sleep_minutes: u32,
    pub unused_days: u32,
    pub warning_days: u32,
    pub creates_per_week: u32,
}

/// Whether the player may create a free server now. `reason` is `email_required`,
/// `free_server_exists`, `too_many_free_servers` (with `retry_after` seconds),
/// `free_servers_full`, `host_full`, `free_servers_paused` or `free_server_blocked`; the launcher
/// keeps it as the Engine says it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CreateAllowance {
    pub allowed: bool,
    pub reason: Option<String>,
    pub retry_after: Option<u64>,
}

/// When an unused server is deleted. `warned`: the owner has been told.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Deletion {
    pub deletes_at: String,
    pub warned: bool,
}

/// The last measure of the server's data folder.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Disk {
    pub used_bytes: u64,
    pub limit_bytes: u64,
    pub checked_at: Option<String>,
    pub over: bool,
}

/// The player's free server. `status` is `online`, `starting`, `sleeping`, `offline`,
/// `stopped` (over the disk limit only), `creating` or `deleting`; an unknown one is kept.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FreeServer {
    pub name: String,
    /// `vanilla` or `modrinth`.
    pub kind: String,
    pub game: InviteGame,
    #[serde(deserialize_with = "modrinth_ref")]
    pub modrinth: Option<InviteModrinth>,
    pub memory_mb: u32,
    pub max_players: u32,
    pub status: String,
    pub players: Option<InvitePlayers>,
    /// What a player types to connect; absent without the Engine's `PUBLIC_GAME_HOST`.
    pub address: Option<String>,
    /// Absent without the Engine's `INVITE_BASE_URL`.
    pub invite_url: Option<String>,
    pub join_url: Option<String>,
    pub created_at: String,
    pub last_played_at: Option<String>,
    pub deletion: Option<Deletion>,
    pub disk: Disk,
}

// Manual Debug: the invite and join URLs hold the invite token.
impl fmt::Debug for FreeServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FreeServer")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

/// `GET /free-server`.
#[derive(Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FreeServerState {
    pub limits: FreeLimits,
    pub create: CreateAllowance,
    pub server: Option<FreeServer>,
}

// Manual Debug: the server's links hold the invite token.
impl fmt::Debug for FreeServerState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FreeServerState")
            .field("server", &self.server)
            .finish_non_exhaustive()
    }
}

/// `POST /free-server`. `modrinth_version_id` is for `kind: "modrinth"` only; `language` is
/// `es`, else `en`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFreeServer {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modrinth_version_id: Option<String>,
    pub language: String,
}

#[derive(Deserialize)]
struct DeletedData {
    #[serde(default)]
    deleted: bool,
}

#[derive(Deserialize)]
struct ServersData {
    #[serde(default)]
    servers: Vec<YourServer>,
}

/// Where a refusal comes from, for the few codes that mean different things by route.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Plain,
    /// `POST /token`: the player typed no code, so a refused one is a failed sign-in.
    Token,
    /// The email sign-in routes: no session yet, so a disabled account is said as such.
    SignIn,
    /// Your servers: a 404 is an Engine that doesn't have them.
    Servers,
}

#[derive(Clone)]
pub struct PlayerClient {
    wire: Wire,
    timeout: std::time::Duration,
}

// Manual Debug: keep to the origin.
impl fmt::Debug for PlayerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PlayerClient")
            .field("engine", &host_of(self.wire.engine()))
            .finish_non_exhaustive()
    }
}

impl PlayerClient {
    /// `engine` should come from `check_engine_url` (as `engine_for_site` gives it).
    pub fn new(http: reqwest::Client, engine: Url) -> Self {
        Self {
            wire: Wire::new(http, engine),
            timeout: InviteTiming::default().request_timeout,
        }
    }

    pub fn engine(&self) -> &Url {
        self.wire.engine()
    }

    fn host(&self) -> String {
        self.wire.host()
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
        self.ok(&reply, Route::SignIn)?;
        self.data(&reply)
    }

    /// `POST /email/verify`: trade the emailed code for a token.
    pub async fn email_verify(&self, email: &str, code: &str) -> Result<TokenAnswer> {
        let body = json!({ "email": email, "code": code });
        let reply = self
            .send(Method::POST, "/email/verify", None, Some(&body), BODY_CAP)
            .await?;
        self.ok(&reply, Route::SignIn)?;
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

    /// `GET /free-server`. An Engine without free servers (503 `free_servers_disabled`, or a
    /// 404) is `None`, so nothing of them shows.
    pub async fn free_server(&self, token: &str) -> Result<Option<FreeServerState>> {
        let reply = self
            .send(Method::GET, "/free-server", Some(token), None, BODY_CAP)
            .await?;
        if reply.status == 404 || failure(&reply).code.as_deref() == Some("free_servers_disabled") {
            return Ok(None);
        }
        self.ok(&reply, Route::Plain)?;
        self.data(&reply).map(Some)
    }

    /// `POST /free-server`. The Engine builds the server while the request is open, so this
    /// one waits up to 150 s.
    pub async fn create_free_server(
        &self,
        token: &str,
        server: &CreateFreeServer,
    ) -> Result<FreeServer> {
        let body = serde_json::to_value(server).expect("a plain struct is JSON");
        let reply = self
            .send_within(
                CREATE_TIMEOUT,
                Method::POST,
                "/free-server",
                Some(token),
                Some(&body),
                BODY_CAP,
            )
            .await?;
        // A reverse proxy that gives up on the long create answers its own 502 or 504, with
        // no Engine code. The Engine finishes the create anyway, so the answer is lost: the
        // dialog then waits for the server to show (spec §5 "Lost create answer").
        if matches!(reply.status, 502 | 504) && failure(&reply).code.is_none() {
            return Err(answer_lost(&self.host(), reply.status));
        }
        self.ok(&reply, Route::Plain)?;
        self.data(&reply)
    }

    /// `POST /free-server/start`: start or wake the server. A server over its disk limit stays
    /// stopped (`free_server_over_disk`).
    pub async fn start_free_server(&self, token: &str) -> Result<FreeServer> {
        let reply = self
            .send(
                Method::POST,
                "/free-server/start",
                Some(token),
                None,
                BODY_CAP,
            )
            .await?;
        self.ok(&reply, Route::Plain)?;
        self.data(&reply)
    }

    /// `DELETE /free-server`: `false` when there was nothing to delete.
    pub async fn delete_free_server(&self, token: &str) -> Result<bool> {
        let reply = self
            .send(Method::DELETE, "/free-server", Some(token), None, BODY_CAP)
            .await?;
        self.ok(&reply, Route::Plain)?;
        Ok(self.data::<DeletedData>(&reply)?.deleted)
    }

    /// `POST /minecraft/link-code`: a fresh code to type in the game.
    pub async fn link_code(&self, token: &str) -> Result<LinkCode> {
        let reply = self
            .send(
                Method::POST,
                "/minecraft/link-code",
                Some(token),
                None,
                BODY_CAP,
            )
            .await?;
        self.ok(&reply, Route::Plain)?;
        self.data(&reply)
    }

    /// `DELETE /minecraft`: unlink the Minecraft account. The answer says whether there was
    /// one, and either way it is unlinked now.
    pub async fn unlink(&self, token: &str) -> Result<()> {
        let reply = self
            .send(Method::DELETE, "/minecraft", Some(token), None, BODY_CAP)
            .await?;
        self.ok(&reply, Route::Plain)
    }

    /// `GET /stats`: the player's totals per server. `None` when the Engine doesn't have them
    /// (404), or has accounts or the server add-on off (503).
    pub async fn stats(&self, token: &str) -> Result<Option<PlayerStats>> {
        let reply = self
            .send(Method::GET, "/stats", Some(token), None, STATS_CAP)
            .await?;
        match reply.status {
            404 => return Ok(None),
            503 => {
                let failure = failure(&reply);
                if matches!(
                    failure.code.as_deref(),
                    Some("server_addon_disabled" | "player_accounts_disabled")
                ) {
                    return Ok(None);
                }
            }
            _ => {}
        }
        self.ok(&reply, Route::Plain)?;
        let mut stats: PlayerStats = self.data(&reply)?;
        stats.cap_ids();
        Ok(Some(stats))
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
        self.send_within(self.timeout, method, route, token, body, cap)
            .await
    }

    /// `send` with its own timeout.
    async fn send_within(
        &self,
        timeout: Duration,
        method: Method,
        route: &str,
        token: Option<&str>,
        body: Option<&Value>,
        cap: usize,
    ) -> Result<Reply> {
        let credential = token.map_or(Credential::None, Credential::Bearer);
        self.wire
            .send(
                timeout,
                method,
                &format!("/api/v1/player{route}"),
                credential,
                body,
                cap,
            )
            .await
    }

    /// `Ok` for a success status, else the refusal.
    fn ok(&self, reply: &Reply, route: Route) -> Result<()> {
        if (200..300).contains(&reply.status) {
            Ok(())
        } else {
            Err(self.refusal(reply, route))
        }
    }

    fn data<T: DeserializeOwned>(&self, reply: &Reply) -> Result<T> {
        self.wire.data(reply)
    }

    /// What a refused request means: the Engine's `data.code` first, then the status.
    fn refusal(&self, reply: &Reply, route: Route) -> LauncherError {
        let host = self.host();
        let failure = failure(reply);
        let param = |name: &str| match failure.data.get(name) {
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        match failure.code.as_deref() {
            Some("player_accounts_disabled") => return accounts_closed(&host),
            // Signing in to a disabled account: say so, there is no session to end.
            Some("account_disabled") if matches!(route, Route::Token | Route::SignIn) => {
                return account_disabled(&host)
            }
            // A disabled player user: the session is no use any more.
            Some("account_disabled") => return signed_out(&host),
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
            Some("email_required") => return email_required(&host),
            Some("free_server_exists") => return free_server_exists(&host),
            Some("too_many_free_servers") => {
                return too_many_free_servers(&host, param("retryAfter"))
            }
            Some("free_servers_full" | "quota_exceeded") => return free_servers_full(&host),
            Some("invalid_free_server") => return invalid_free_server(&host),
            Some("modpack_unsupported") => return modpack_unsupported(&host, param("reason")),
            Some("modpack_too_heavy") => {
                return modpack_too_heavy(&host, param("requiredMemoryMb"), param("limitMemoryMb"))
            }
            Some("host_full") => return host_full(&host),
            Some("free_servers_paused") => return free_servers_paused(&host),
            Some("free_server_blocked") => return free_server_blocked(&host),
            Some("free_server_over_disk") => {
                return free_server_over_disk(&host, param("usedBytes"), param("limitBytes"))
            }
            Some("modrinth_unavailable") => return host_trouble(MODRINTH_HOST, reply.status),
            Some("server_addon_disabled") => return minecraft_link_closed(&host),
            _ => {}
        }
        match reply.status {
            404 if route == Route::Servers => servers_closed(&host),
            status => status_fallback(&host, status),
        }
    }
}

fn answer_lost(host: &str, status: u16) -> LauncherError {
    LauncherError::network(
        ErrorCode::ConnectionDropped,
        format!("The answer from {host} was lost ({status}). Your server may still be on its way."),
    )
    .with_param("host", host)
    .with_param("status", status.to_string())
}

pub(super) fn accounts_closed(host: &str) -> LauncherError {
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

fn account_disabled(host: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::AccountDisabled,
        "This Perland account is turned off.",
    )
    .with_fix("Contact Perland support.")
    .with_param("host", host)
}

pub(super) fn signed_out(host: &str) -> LauncherError {
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

fn email_required(host: &str) -> LauncherError {
    LauncherError::auth(
        ErrorCode::EmailRequired,
        "Free servers need a verified email on your account.",
    )
    .with_param("host", host)
}

fn free_server_exists(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::FreeServerExists,
        "You already have a free server.",
    )
    .with_param("host", host)
}

fn too_many_free_servers(host: &str, retry_after: Option<String>) -> LauncherError {
    let e = LauncherError::network(
        ErrorCode::TooManyFreeServers,
        "You can't create another server yet.",
    )
    .with_param("host", host);
    match retry_after {
        Some(seconds) => e.with_param("retryAfter", seconds),
        None => e,
    }
}

fn free_servers_full(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::FreeServersFull,
        "Free servers are full right now. Try again later.",
    )
    .with_param("host", host)
}

/// Perland's machine has no room for another server now. The Engine's `dimension` and `scope`
/// are for the platform, not for players, so they are not kept.
pub(super) fn host_full(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::HostFull,
        "Perland is full right now. Try again in a few minutes.",
    )
    .with_param("host", host)
}

fn free_servers_paused(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::FreeServersPaused,
        "New free servers are paused. Try again later.",
    )
    .with_param("host", host)
}

fn free_server_blocked(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::FreeServerBlocked,
        "You can't create a free server.",
    )
    .with_param("host", host)
}

fn invalid_free_server(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "Check the server name: letters, numbers, - and _ only, up to 32.",
    )
    .with_param("host", host)
}

fn modpack_unsupported(host: &str, reason: Option<String>) -> LauncherError {
    let e = LauncherError::pack(
        ErrorCode::ModpackUnsupported,
        "This modpack can't run on a free server.",
    )
    .with_param("host", host);
    match reason {
        Some(reason) => e.with_param("reason", reason),
        None => e,
    }
}

fn modpack_too_heavy(host: &str, required: Option<String>, limit: Option<String>) -> LauncherError {
    let mut e = LauncherError::pack(
        ErrorCode::ModpackTooHeavy,
        "This modpack needs more memory than a free server has.",
    )
    .with_param("host", host);
    if let Some(required) = required {
        e = e.with_param("requiredMemoryMb", required);
    }
    if let Some(limit) = limit {
        e = e.with_param("limitMemoryMb", limit);
    }
    e
}

pub(super) fn free_server_over_disk(
    host: &str,
    used: Option<String>,
    limit: Option<String>,
) -> LauncherError {
    let mut e = LauncherError::network(
        ErrorCode::FreeServerOverDisk,
        "Your server is out of space, so it stays off.",
    )
    .with_param("host", host);
    if let Some(used) = used {
        e = e.with_param("usedBytes", used);
    }
    if let Some(limit) = limit {
        e = e.with_param("limitBytes", limit);
    }
    e
}

fn minecraft_link_closed(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::MinecraftLinkClosed,
        "Linking isn't available right now.",
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
    fn create_waits_longer_than_other_calls() {
        assert_eq!(CREATE_TIMEOUT, Duration::from_secs(150));
        assert!(CREATE_TIMEOUT > InviteTiming::default().request_timeout);
    }

    #[test]
    fn create_body_omits_missing_version_id() {
        let body = serde_json::to_value(CreateFreeServer {
            name: "a".into(),
            kind: "vanilla".into(),
            modrinth_version_id: None,
            language: "en".into(),
        })
        .unwrap();
        assert_eq!(
            body,
            json!({ "name": "a", "kind": "vanilla", "language": "en" })
        );
    }

    #[test]
    fn free_server_debug_holds_no_invite_token() {
        let token = "b4sE64url_-b4sE64url_-b4sE64url_-b4sE64url_";
        let server: FreeServer = serde_json::from_value(json!({
            "name": "Aventura", "kind": "vanilla", "status": "sleeping",
            "inviteUrl": format!("https://play.example.com/{token}"),
            "joinUrl": format!("perland://join?invite=https%3A%2F%2Fplay.example.com%2F{token}")
        }))
        .unwrap();
        let state = FreeServerState {
            server: Some(server.clone()),
            ..FreeServerState::default()
        };
        for text in [
            format!("{server:?}"),
            format!("{state:?}"),
            format!("{state:#?}"),
        ] {
            assert!(!text.contains(token), "{text}");
            assert!(
                text.contains("Aventura") && text.contains("sleeping"),
                "{text}"
            );
        }
    }

    #[test]
    fn free_server_drops_an_invalid_modrinth_pair() {
        let s: FreeServer =
            serde_json::from_value(json!({ "modrinth": { "projectId": "a/b", "versionId": "x" } }))
                .unwrap();
        assert!(s.modrinth.is_none());
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

    #[test]
    fn player_minecraft_parses_and_is_optional() {
        let player: Player = serde_json::from_value(json!({ "id": "p" })).unwrap();
        assert_eq!(player.minecraft, None);
        let player: Player = serde_json::from_value(json!({
            "id": "p",
            "minecraft": { "name": "Moi", "uuid": "u", "linkedAt": "t" }
        }))
        .unwrap();
        let minecraft = player.minecraft.unwrap();
        assert_eq!(minecraft.name, "Moi");
        assert_eq!(minecraft.uuid, "u");
        assert_eq!(minecraft.linked_at, "t");
    }

    #[test]
    fn link_code_debug_hides_code() {
        let code: LinkCode =
            serde_json::from_value(json!({ "code": "ABCD-EFGH", "expiresIn": 600 })).unwrap();
        let shown = format!("{code:?}");
        assert!(!shown.contains("ABCD"), "{shown}");
        assert!(shown.contains("600"), "{shown}");
    }

    #[test]
    fn server_stats_ids_default_to_empty() {
        let stats: ServerStats =
            serde_json::from_value(json!({ "name": "a", "advancements": 0 })).unwrap();
        assert!(stats.advancement_ids.is_empty());
    }

    #[test]
    fn server_stats_ids_are_capped() {
        let ids: Vec<String> = (0..MAX_ADVANCEMENT_IDS + 3)
            .map(|i| format!("m:{i}"))
            .collect();
        let mut stats = PlayerStats {
            minecraft: None,
            servers: vec![ServerStats {
                advancement_ids: ids,
                ..ServerStats::default()
            }],
        };
        stats.cap_ids();
        assert_eq!(stats.servers[0].advancement_ids.len(), MAX_ADVANCEMENT_IDS);
        assert_eq!(stats.servers[0].advancement_ids[0], "m:0");
    }

    #[test]
    fn stats_invite_debug_hides_token() {
        let token = "A".repeat(43);
        let invite: StatsInvite = serde_json::from_value(json!({ "token": token })).unwrap();
        let shown = format!("{invite:?}");
        assert!(!shown.contains("AAAA"), "{shown}");
    }
}
