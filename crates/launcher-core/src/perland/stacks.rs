//! The Engine's server routes (`/api/v1/stacks`): what a player sees and does with one of
//! their servers in Manage. Every call carries the session JWT as `X-User-Token`; the JWT is a
//! credential and nothing here prints one.

use std::{fmt, time::Duration};

use reqwest::{Method, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use super::{
    api::{free_server_over_disk, host_full, signed_out, CREATE_TIMEOUT},
    auth::SessionJwt,
    wire::{failure, status_fallback, Credential, Reply, Wire},
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_of, request_failed},
        InviteTiming,
    },
};

/// The longest answer read, in bytes.
const BODY_CAP: usize = 512 << 10;
/// The server list can hold many servers.
const LIST_CAP: usize = 1 << 20;

/// One server in the list.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackItem {
    pub stack_name: String,
    pub status: String,
    pub health: String,
    /// What a player types to join; absent without a host or a published port.
    pub address: Option<String>,
    pub motd: String,
    pub version: String,
    pub kind: String,
    pub r#type: String,
}

/// The idle shutdown state of a server.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackIdle {
    /// `active`, `stopping`, `sleeping` or `waking`.
    pub state: String,
    pub paused: bool,
    pub stops_at: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackDetail {
    pub stack_name: String,
    pub status: String,
    pub address: Option<String>,
    pub motd: String,
    pub version: String,
    pub r#type: String,
    /// Saved changes have not reached the running server yet.
    pub pending_restart: bool,
    pub idle: Option<StackIdle>,
}

/// Who is on the server now. `players` is `None` when the Engine didn't understand the
/// server's reply.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Online {
    pub running: bool,
    pub paused: bool,
    pub online: u32,
    pub max: u32,
    pub players: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct IssueSubject {
    /// `plugin` or `mod`.
    pub kind: String,
    pub name: String,
    pub version: String,
}

/// One problem found in the server's last run. The UI words it by `code`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Issue {
    pub code: String,
    pub known: bool,
    /// `fatal`, `error` or `warning`.
    pub severity: String,
    pub params: Map<String, Value>,
    pub subject: Option<IssueSubject>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Diagnosis {
    /// `not_started`, `running`, `stopped` or `failed`.
    pub outcome: String,
    pub oom_killed: bool,
    pub primary: Option<Issue>,
}

/// The whitelist or the ops: just the names.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NameList {
    /// Changes go through the running server now; while it is off the Engine edits the file.
    pub running: bool,
    pub offline_mode: bool,
    /// The file exists but could not be read.
    pub unreadable: bool,
    pub names: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ban {
    pub name: String,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BanList {
    pub running: bool,
    pub unreadable: bool,
    pub bans: Vec<Ban>,
}

/// What a change did to one name (`added`, `already_listed`, `failed`, `skipped`, ...).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NameResult {
    pub name: String,
    pub status: String,
    /// The server's reply, or why the command did not run.
    pub message: Option<String>,
}

/// The server's settings. `settings` is kept raw and whole so a save sends back what the
/// Engine sent, fields the launcher doesn't know included.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsDoc {
    pub revision: String,
    /// `modpack` when a modpack sets type, version and loader version.
    pub managed_by_source: Option<String>,
    pub pending_restart: bool,
    pub settings: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SettingsSaved {
    pub revision: String,
    /// The changes wait for the next start or restart.
    pub pending: bool,
    /// The running server was recreated with them.
    pub restarted: bool,
    pub settings: Value,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ConsoleReply {
    /// `ok` or `error`.
    pub outcome: String,
    pub output: String,
    pub code: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Start,
    Stop,
    Restart,
}

impl Lifecycle {
    fn verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Whitelist,
    Ops,
    Bans,
}

/// When a saved change reaches a running server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Apply {
    /// The Engine recreates the server inside the request.
    Now,
    NextRestart,
}

#[derive(Deserialize)]
struct Servers {
    #[serde(default)]
    servers: Vec<StackItem>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Named {
    name: String,
}

/// The whitelist and ops answers: each carries its entries under its own key.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawNames {
    running: bool,
    offline_mode: bool,
    unreadable: bool,
    players: Vec<Named>,
    ops: Vec<Named>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Results {
    results: Vec<NameResult>,
}

#[derive(Clone)]
pub struct StackClient {
    wire: Wire,
    timeout: Duration,
}

// Manual Debug: keep to the origin.
impl fmt::Debug for StackClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StackClient")
            .field("engine", &host_of(self.wire.engine()))
            .finish_non_exhaustive()
    }
}

impl StackClient {
    /// `engine` should come from `check_engine_url`. `http` must not follow redirects
    /// (`http::client_without_redirects`): a redirect would forward the session JWT.
    pub fn new(http: reqwest::Client, engine: Url) -> Self {
        Self {
            wire: Wire::new(http, engine),
            timeout: InviteTiming::default().request_timeout,
        }
    }

    fn host(&self) -> String {
        self.wire.host()
    }

    /// `/api/v1/stacks/<segments...>` with each segment escaped, so a stack name is always
    /// one segment and can never reach another route.
    fn route(&self, segments: &[&str]) -> Result<String> {
        if segments.iter().any(|s| matches!(*s, "" | "." | "..")) {
            return Err(not_found(&self.host()));
        }
        let mut url = Url::parse("http://x/").expect("a fixed URL");
        {
            let mut path = url
                .path_segments_mut()
                .expect("a URL with a host has a path");
            path.extend(["api", "v1", "stacks"]);
            path.extend(segments);
        }
        Ok(url.path().to_string())
    }

    async fn call(
        &self,
        jwt: &SessionJwt,
        method: Method,
        path: String,
        body: Option<&Value>,
        timeout: Duration,
        cap: usize,
    ) -> Result<Reply> {
        let reply = self
            .wire
            .send(
                timeout,
                method,
                &path,
                Credential::UserToken(jwt.as_str()),
                body,
                cap,
            )
            .await?;
        if (200..300).contains(&reply.status) {
            Ok(reply)
        } else {
            Err(self.refusal(&reply))
        }
    }

    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        jwt: &SessionJwt,
        path: String,
        cap: usize,
    ) -> Result<T> {
        let reply = self
            .call(jwt, Method::GET, path, None, self.timeout, cap)
            .await?;
        self.wire.data(&reply)
    }

    async fn post<T: serde::de::DeserializeOwned>(
        &self,
        jwt: &SessionJwt,
        path: String,
        body: &Value,
    ) -> Result<T> {
        let reply = self
            .call(jwt, Method::POST, path, Some(body), self.timeout, BODY_CAP)
            .await?;
        self.wire.data(&reply)
    }

    /// `GET /get-all?kind=single`: the player's servers (networks are left out).
    pub async fn get_all(&self, jwt: &SessionJwt) -> Result<Vec<StackItem>> {
        let path = format!("{}?kind=single", self.route(&["get-all"])?);
        Ok(self.get::<Servers>(jwt, path, LIST_CAP).await?.servers)
    }

    /// `GET /details/{stack}?stats=false`: the live CPU and memory sample is skipped.
    pub async fn details(&self, jwt: &SessionJwt, stack: &str) -> Result<StackDetail> {
        let path = format!("{}?stats=false", self.route(&["details", stack])?);
        self.get(jwt, path, BODY_CAP).await
    }

    /// `POST /{start|stop|restart}/{stack}`, with an empty JSON body: no broadcast, no wait.
    pub async fn lifecycle(&self, jwt: &SessionJwt, stack: &str, action: Lifecycle) -> Result<()> {
        let path = self.route(&[action.verb(), stack])?;
        self.call(
            jwt,
            Method::POST,
            path,
            Some(&json!({})),
            self.timeout,
            BODY_CAP,
        )
        .await
        .map(|_| ())
    }

    /// `GET /{stack}/diagnosis`: what went wrong in the server's last run.
    pub async fn diagnosis(&self, jwt: &SessionJwt, stack: &str) -> Result<Diagnosis> {
        self.get(jwt, self.route(&[stack, "diagnosis"])?, BODY_CAP)
            .await
    }

    /// `GET /{stack}/players`: who is on now.
    pub async fn online(&self, jwt: &SessionJwt, stack: &str) -> Result<Online> {
        self.get(jwt, self.route(&[stack, "players"])?, BODY_CAP)
            .await
    }

    /// `POST /{stack}/players/kick`.
    pub async fn kick(
        &self,
        jwt: &SessionJwt,
        stack: &str,
        names: &[String],
    ) -> Result<Vec<NameResult>> {
        let path = self.route(&[stack, "players", "kick"])?;
        let answer: Results = self.post(jwt, path, &json!({ "names": names })).await?;
        Ok(answer.results)
    }

    /// `GET /{stack}/whitelist` or `/ops`. Bans have their own shape (`bans`).
    pub async fn list(&self, jwt: &SessionJwt, stack: &str, kind: ListKind) -> Result<NameList> {
        let route = match kind {
            ListKind::Whitelist => "whitelist",
            ListKind::Ops => "ops",
            ListKind::Bans => "bans",
        };
        if kind == ListKind::Bans {
            // A ban list as names: the reasons are dropped.
            let bans = self.bans(jwt, stack).await?;
            return Ok(NameList {
                running: bans.running,
                offline_mode: false,
                unreadable: bans.unreadable,
                names: bans.bans.into_iter().map(|b| b.name).collect(),
            });
        }
        let raw: RawNames = self
            .get(jwt, self.route(&[stack, route])?, BODY_CAP)
            .await?;
        Ok(NameList {
            running: raw.running,
            offline_mode: raw.offline_mode,
            unreadable: raw.unreadable,
            names: raw
                .players
                .into_iter()
                .chain(raw.ops)
                .map(|n| n.name)
                .collect(),
        })
    }

    /// `GET /{stack}/bans`.
    pub async fn bans(&self, jwt: &SessionJwt, stack: &str) -> Result<BanList> {
        self.get(jwt, self.route(&[stack, "bans"])?, BODY_CAP).await
    }

    /// Adds names to a list or takes them off it (whitelist, ops, or bans and pardons), one
    /// result per name.
    pub async fn change(
        &self,
        jwt: &SessionJwt,
        stack: &str,
        kind: ListKind,
        add: bool,
        names: &[String],
    ) -> Result<Vec<NameResult>> {
        let segments: &[&str] = match (kind, add) {
            (ListKind::Whitelist, true) => &["whitelist"],
            (ListKind::Whitelist, false) => &["whitelist", "remove"],
            (ListKind::Ops, true) => &["ops"],
            (ListKind::Ops, false) => &["ops", "remove"],
            (ListKind::Bans, true) => &["bans"],
            (ListKind::Bans, false) => &["bans", "pardon"],
        };
        let mut route = vec![stack];
        route.extend(segments);
        let answer: Results = self
            .post(jwt, self.route(&route)?, &json!({ "names": names }))
            .await?;
        Ok(answer.results)
    }

    /// `GET /{stack}/settings`.
    pub async fn settings(&self, jwt: &SessionJwt, stack: &str) -> Result<SettingsDoc> {
        self.get(jwt, self.route(&[stack, "settings"])?, BODY_CAP)
            .await
    }

    /// `PUT /{stack}/settings`. `body` is the whole request (revision, settings and any
    /// confirmations). A running server is recreated inside the request, so this waits as long
    /// as a server creation does.
    pub async fn put_settings(
        &self,
        jwt: &SessionJwt,
        stack: &str,
        body: &Value,
        apply: Apply,
    ) -> Result<SettingsSaved> {
        let mut path = self.route(&[stack, "settings"])?;
        if apply == Apply::NextRestart {
            path.push_str("?apply=next-restart");
        }
        let reply = self
            .call(jwt, Method::PUT, path, Some(body), CREATE_TIMEOUT, BODY_CAP)
            .await?;
        self.wire.data(&reply)
    }

    /// `POST /{stack}/console`: one command and the server's answer. `confirm` lets `stop`,
    /// `restart`, `end` and `shutdown` through.
    pub async fn console(
        &self,
        jwt: &SessionJwt,
        stack: &str,
        command: &str,
        confirm: bool,
    ) -> Result<ConsoleReply> {
        let body = json!({ "command": command, "confirm": confirm });
        self.post(jwt, self.route(&[stack, "console"])?, &body)
            .await
    }

    /// `GET /{stack}/logs/stream`: the open response of the log stream on success, else what
    /// the Engine's refusal means. `since` (a timestamp the stream already gave) asks only for
    /// lines after it and skips the backlog.
    pub(crate) async fn stream_logs_request(
        &self,
        jwt: &SessionJwt,
        stack: &str,
        since: Option<&str>,
    ) -> std::result::Result<reqwest::Response, StreamRefusal> {
        let path = self
            .route(&[stack, "logs", "stream"])
            .map_err(StreamRefusal::Refused)?;
        let mut url = Url::parse("http://x/").expect("a fixed URL");
        url.set_path(&path);
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("tail", if since.is_some() { "0" } else { "200" });
            query.append_pair("timestamps", "true");
            if let Some(since) = since {
                query.append_pair("since", since);
            }
        }
        let target = format!("{}?{}", url.path(), url.query().unwrap_or_default());
        let response = self
            .wire
            .open_get(&target, jwt.as_str())
            .send()
            .await
            .map_err(|e| StreamRefusal::Unreachable(request_failed(&self.host(), e)))?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status().as_u16();
        let body = read_capped(response, 4 << 10)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let refusal = self.refusal(&Reply { status, body });
        // An Engine restarting (5xx) or busy (a plain 429) is worth another try; a full set
        // of streams (also 429, with its own code), a missing server and a signed-out player
        // are not.
        let passing = status >= 500 || (status == 429 && refusal.code != ErrorCode::TooManyStreams);
        Err(if passing {
            StreamRefusal::Unreachable(refusal)
        } else {
            StreamRefusal::Refused(refusal)
        })
    }

    /// What a refused request means: the Engine's `data.code` first, then the status.
    fn refusal(&self, reply: &Reply) -> LauncherError {
        let host = self.host();
        let failure = failure(reply);
        let text = |name: &str| {
            failure
                .data
                .get(name)
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let number = |name: &str| match failure.data.get(name) {
            Some(Value::Number(n)) => Some(n.to_string()),
            _ => None,
        };
        let net =
            |code, message: &str| LauncherError::network(code, message).with_param("host", &host);
        match failure.code.as_deref() {
            Some("server_not_running") => {
                return net(ErrorCode::ServerNotRunning, "The server isn't running.")
            }
            Some("rcon_unavailable") => {
                return net(
                    ErrorCode::ServerConsoleUnavailable,
                    "The server can't take commands right now. Try again in a minute.",
                )
            }
            Some("too_many_commands") => {
                return net(
                    ErrorCode::TooManyCommands,
                    "Too many commands at once. Try again in a few seconds.",
                )
            }
            Some("rcon_timeout") => {
                return net(
                    ErrorCode::ServerConsoleTimeout,
                    "The server took too long to answer.",
                )
            }
            Some("lifecycle_command_requires_confirmation") => {
                return net(
                    ErrorCode::CommandNeedsConfirmation,
                    "This command stops the server.",
                )
            }
            // The team's running servers leave no room (the player routes read it as a free
            // server being full; here it is the plan).
            Some("quota_exceeded") => {
                return net(
                    ErrorCode::TeamQuotaFull,
                    "Your plan has no room to run another server right now.",
                )
            }
            Some("plan_feature_locked") => {
                let e = net(
                    ErrorCode::PlanFeatureLocked,
                    "Your plan doesn't include this.",
                );
                return match text("field").or_else(|| text("scope")) {
                    Some(field) => e.with_param("field", field),
                    None => e,
                };
            }
            Some("settings_changed") => {
                return net(
                    ErrorCode::SettingsChanged,
                    "The settings changed since you opened them.",
                )
            }
            Some("version_change_requires_confirmation") => {
                return net(
                    ErrorCode::VersionChangeNeedsConfirmation,
                    "Changing the version needs your confirmation.",
                )
            }
            Some("data_loss_requires_acknowledgement") => {
                return net(
                    ErrorCode::DataLossNeedsConfirmation,
                    "This change can erase parts of the world. Confirm to go ahead.",
                )
            }
            Some("mojang_unavailable") => {
                return net(
                    ErrorCode::MojangUnavailable,
                    "Minecraft's account service isn't answering. Try again in a minute.",
                )
            }
            Some("too_many_streams") => {
                return net(ErrorCode::TooManyStreams, "Too many log views are open.")
            }
            Some("too_many_diagnoses") => {
                return net(
                    ErrorCode::TooManyDiagnoses,
                    "Two checks are already running.",
                )
            }
            Some("managed_by_source") => {
                return net(
                    ErrorCode::ManagedBySource,
                    "The modpack sets this, so it can't be changed here.",
                )
            }
            Some("host_full") => return host_full(&host),
            Some("free_server_over_disk") => {
                return free_server_over_disk(&host, number("usedBytes"), number("limitBytes"))
            }
            _ => {}
        }
        match reply.status {
            401 => signed_out(&host),
            403 => LauncherError::auth(
                ErrorCode::ManageForbidden,
                "Your role or plan doesn't allow this.",
            )
            .with_param("host", &host),
            404 => not_found(&host),
            status => status_fallback(&host, status),
        }
    }
}

/// Why the log stream did not open.
pub(crate) enum StreamRefusal {
    /// The Engine answered no.
    Refused(LauncherError),
    /// No usable answer: the network, a dropped connection, or an Engine restarting or
    /// busy (5xx, a plain 429). Worth retrying once the stream has been open before.
    Unreachable(LauncherError),
}

fn not_found(host: &str) -> LauncherError {
    LauncherError::network(ErrorCode::ServerNotFound, "That server wasn't found.")
        .with_param("host", host)
}
