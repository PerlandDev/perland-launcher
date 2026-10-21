//! Manage your server (Manage your server spec §5.2): the servers a signed-in player owns or
//! helps run, read and run from the launcher through the Engine's `/api/v1/stacks` routes.
//! Every call carries a session JWT scoped to the server's team, minted from the device session
//! token and kept in `AppCore::jwts`. The device token, every JWT and every handoff code are
//! credentials: nothing here logs one, and the panel URL that holds a code is only opened.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use launcher_core::{
    http::HttpClient,
    invite::Url,
    perland::{
        self, Apply, AuthClient, BanList, ConsoleReply, Diagnosis, Handoff, JwtSource, Lifecycle,
        ListKind, Membership, NameList, NameResult, Online, Session, SessionJwt, SettingsDoc,
        SettingsSaved, StackClient, StackDetail, StackIdle, StackItem,
    },
    ErrorCode, LauncherError, Result,
};
use serde_json::Value;

use crate::{
    app::AppCore,
    dto::{
        ManageBan, ManageCause, ManageChangeResult, ManageConsoleReply, ManageDetail,
        ManageDiagnosis, ManageListChange, ManagePlayers, ManageSaved, ManageSettings,
        ManageSettingsSave, ManagedServer,
    },
    events::Host,
    runner::{BoxFuture, Runner},
};

/// The plan of a player's free team.
const FREE_PLAN: &str = "player-free";
/// The name the Engine gives a player's free team.
const FREE_TEAM_PREFIX: &str = "player-";

/// The Engine's auth and server routes for Manage. Every call names the Engine it goes to (the
/// session's) and the credential it carries. The real one is `CoreStack`; tests use `FakeStack`.
pub trait StackApi: Send + Sync + 'static {
    /// The device token (`plt_…`) for a session JWT.
    fn player_session(&self, engine: Url, device: String) -> BoxFuture<Result<SessionJwt>>;
    /// The teams the player belongs to.
    fn memberships(&self, engine: Url, jwt: SessionJwt) -> BoxFuture<Result<Vec<Membership>>>;
    /// A JWT scoped to one team.
    fn select_tenant(
        &self,
        engine: Url,
        jwt: SessionJwt,
        tenant: String,
    ) -> BoxFuture<Result<SessionJwt>>;
    /// A one-time panel sign-in code; `None` on an Engine without the route.
    fn handoff(
        &self,
        engine: Url,
        device: String,
        tenant: String,
    ) -> BoxFuture<Result<Option<Handoff>>>;
    fn get_all(&self, engine: Url, jwt: SessionJwt) -> BoxFuture<Result<Vec<StackItem>>>;
    fn details(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
    ) -> BoxFuture<Result<StackDetail>>;
    fn lifecycle(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        action: Lifecycle,
    ) -> BoxFuture<Result<()>>;
    fn diagnosis(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
    ) -> BoxFuture<Result<Diagnosis>>;
    fn online(&self, engine: Url, jwt: SessionJwt, stack: String) -> BoxFuture<Result<Online>>;
    fn kick(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        names: Vec<String>,
    ) -> BoxFuture<Result<Vec<NameResult>>>;
    fn list(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        kind: ListKind,
    ) -> BoxFuture<Result<NameList>>;
    fn bans(&self, engine: Url, jwt: SessionJwt, stack: String) -> BoxFuture<Result<BanList>>;
    fn change(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        kind: ListKind,
        add: bool,
        names: Vec<String>,
    ) -> BoxFuture<Result<Vec<NameResult>>>;
    fn settings(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
    ) -> BoxFuture<Result<SettingsDoc>>;
    fn put_settings(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        body: Value,
        apply: Apply,
    ) -> BoxFuture<Result<SettingsSaved>>;
    fn console(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        command: String,
        confirm: bool,
    ) -> BoxFuture<Result<ConsoleReply>>;
}

/// `StackApi` over `launcher_core::perland::{AuthClient, StackClient}`.
pub struct CoreStack {
    /// Never follows a redirect: one would forward the device token or a JWT to another host.
    http: Result<HttpClient>,
}

impl CoreStack {
    /// `http` should come from `launcher_core::http::client_without_redirects`; an error is
    /// what every call then answers.
    pub fn new(http: Result<HttpClient>) -> Self {
        Self { http }
    }

    fn auth(&self, engine: Url) -> Result<AuthClient> {
        Ok(AuthClient::new(self.http.clone()?, engine))
    }

    fn stacks(&self, engine: Url) -> Result<StackClient> {
        Ok(StackClient::new(self.http.clone()?, engine))
    }
}

impl StackApi for CoreStack {
    fn player_session(&self, engine: Url, device: String) -> BoxFuture<Result<SessionJwt>> {
        let client = self.auth(engine);
        Box::pin(async move { client?.player_session(&device).await })
    }

    fn memberships(&self, engine: Url, jwt: SessionJwt) -> BoxFuture<Result<Vec<Membership>>> {
        let client = self.auth(engine);
        Box::pin(async move { client?.memberships(&jwt).await })
    }

    fn select_tenant(
        &self,
        engine: Url,
        jwt: SessionJwt,
        tenant: String,
    ) -> BoxFuture<Result<SessionJwt>> {
        let client = self.auth(engine);
        Box::pin(async move { client?.select_tenant(&jwt, &tenant).await })
    }

    fn handoff(
        &self,
        engine: Url,
        device: String,
        tenant: String,
    ) -> BoxFuture<Result<Option<Handoff>>> {
        let client = self.auth(engine);
        Box::pin(async move { client?.handoff(&device, &tenant).await })
    }

    fn get_all(&self, engine: Url, jwt: SessionJwt) -> BoxFuture<Result<Vec<StackItem>>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.get_all(&jwt).await })
    }

    fn details(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
    ) -> BoxFuture<Result<StackDetail>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.details(&jwt, &stack).await })
    }

    fn lifecycle(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        action: Lifecycle,
    ) -> BoxFuture<Result<()>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.lifecycle(&jwt, &stack, action).await })
    }

    fn diagnosis(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
    ) -> BoxFuture<Result<Diagnosis>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.diagnosis(&jwt, &stack).await })
    }

    fn online(&self, engine: Url, jwt: SessionJwt, stack: String) -> BoxFuture<Result<Online>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.online(&jwt, &stack).await })
    }

    fn kick(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        names: Vec<String>,
    ) -> BoxFuture<Result<Vec<NameResult>>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.kick(&jwt, &stack, &names).await })
    }

    fn list(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        kind: ListKind,
    ) -> BoxFuture<Result<NameList>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.list(&jwt, &stack, kind).await })
    }

    fn bans(&self, engine: Url, jwt: SessionJwt, stack: String) -> BoxFuture<Result<BanList>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.bans(&jwt, &stack).await })
    }

    fn change(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        kind: ListKind,
        add: bool,
        names: Vec<String>,
    ) -> BoxFuture<Result<Vec<NameResult>>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.change(&jwt, &stack, kind, add, &names).await })
    }

    fn settings(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
    ) -> BoxFuture<Result<SettingsDoc>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.settings(&jwt, &stack).await })
    }

    fn put_settings(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        body: Value,
        apply: Apply,
    ) -> BoxFuture<Result<SettingsSaved>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.put_settings(&jwt, &stack, &body, apply).await })
    }

    fn console(
        &self,
        engine: Url,
        jwt: SessionJwt,
        stack: String,
        command: String,
        confirm: bool,
    ) -> BoxFuture<Result<ConsoleReply>> {
        let client = self.stacks(engine);
        Box::pin(async move { client?.console(&jwt, &stack, &command, confirm).await })
    }
}

/// Fresh JWTs for `JwtCache`, from the session's Engine and device token. It notes whether
/// `player-session` refused the device token, so the caller can sign the player out: a refused
/// scoped call alone doesn't mean the device session ended.
struct SessionSource {
    api: Arc<dyn StackApi>,
    engine: Url,
    device: String,
    /// `player-session` refused the device token.
    signed_out: AtomicBool,
    /// `player-session` was asked: the base JWT is new, so replacing it again won't help.
    minted_base: AtomicBool,
}

impl SessionSource {
    fn new(api: Arc<dyn StackApi>, session: &Session) -> Self {
        Self {
            api,
            engine: session.engine.clone(),
            device: session.token.clone(),
            signed_out: AtomicBool::new(false),
            minted_base: AtomicBool::new(false),
        }
    }
}

impl JwtSource for SessionSource {
    fn player_session(&self) -> launcher_core::sync::BoxFuture<'_, Result<SessionJwt>> {
        // Asked only when awaited: the cache builds this future even when its JWT is fresh.
        Box::pin(async move {
            self.minted_base.store(true, Ordering::SeqCst);
            let answer = self
                .api
                .player_session(self.engine.clone(), self.device.clone())
                .await;
            if answer
                .as_ref()
                .is_err_and(|e| e.code == ErrorCode::PerlandSignedOut)
            {
                self.signed_out.store(true, Ordering::SeqCst);
            }
            answer
        })
    }

    fn select_tenant<'a>(
        &'a self,
        base: &'a SessionJwt,
        tenant: &'a str,
    ) -> launcher_core::sync::BoxFuture<'a, Result<SessionJwt>> {
        Box::pin(async move {
            self.api
                .select_tenant(self.engine.clone(), base.clone(), tenant.to_string())
                .await
        })
    }
}

/// Seconds since the Unix epoch, for JWT freshness.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn signed_out() -> LauncherError {
    LauncherError::auth(
        ErrorCode::PerlandSignedOut,
        "You were signed out of Perland.",
    )
}

fn link_refused() -> LauncherError {
    LauncherError::pack(ErrorCode::LinkRefused, "That link can't be opened")
}

/// What the player sees a server doing: `running`, `asleep`, `starting`, `stopping`,
/// `stopped` or `unknown`, from the container's status and health and the idle shutdown state.
pub(crate) fn manage_state(status: &str, health: &str, idle: Option<&StackIdle>) -> &'static str {
    let idle_state = idle.map(|i| i.state.as_str()).unwrap_or_default();
    if status == "sleeping" || idle_state == "sleeping" || idle.is_some_and(|i| i.paused) {
        return "asleep";
    }
    match idle_state {
        "waking" => return "starting",
        "stopping" => return "stopping",
        _ => {}
    }
    match status {
        "running" if health == "starting" => "starting",
        "running" => "running",
        "restarting" => "starting",
        "exited" | "dead" | "created" => "stopped",
        "removing" => "stopping",
        _ => "unknown",
    }
}

fn is_free(team: &Membership) -> bool {
    team.plan.as_deref() == Some(FREE_PLAN) || team.tenant_name.starts_with(FREE_TEAM_PREFIX)
}

/// A diagnosis parameter as text: strings as they are, anything else as JSON.
fn param_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// The most names one change takes (the Engine's limit).
const MAX_NAMES: usize = 50;

/// `settings.customization.<key>`, when set.
fn custom<'a>(settings: &'a Value, key: &str) -> Option<&'a Value> {
    settings
        .get("customization")
        .and_then(|c| c.get(key))
        .filter(|v| !v.is_null())
}

fn custom_text(settings: &Value, key: &str, default: &str) -> String {
    custom(settings, key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(default)
        .to_string()
}

fn custom_bool(settings: &Value, key: &str, default: bool) -> bool {
    custom(settings, key)
        .and_then(Value::as_bool)
        .unwrap_or(default)
}

/// A modpack decides the server's version: the Engine says so, or the server runs a Modrinth
/// pack (a free server made from one keeps it in `env`).
fn version_locked(doc: &SettingsDoc) -> bool {
    let settings = &doc.settings;
    doc.managed_by_source
        .as_deref()
        .is_some_and(|s| !s.is_empty())
        || settings
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case("MODRINTH"))
        || custom(settings, "env")
            .and_then(|env| env.get("MODRINTH_MODPACK"))
            .is_some_and(|v| !v.is_null())
}

/// What Manage shows of a settings document, with itzg's defaults for what is unset.
fn manage_settings_of(doc: &SettingsDoc) -> ManageSettings {
    let settings = &doc.settings;
    let motd = custom_text(settings, "motd", "");
    ManageSettings {
        revision: doc.revision.clone(),
        // The launcher's editor has no colour codes: one would be lost on save.
        motd_read_only: motd.contains('§'),
        motd,
        mode: custom_text(settings, "mode", "survival"),
        difficulty: custom_text(settings, "difficulty", "easy"),
        max_players: custom(settings, "maxPlayers")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok()),
        pvp: custom_bool(settings, "pvp", true),
        enable_whitelist: custom_bool(settings, "enableWhitelist", false),
        version: settings
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        version_locked: version_locked(doc),
        pending_restart: doc.pending_restart,
    }
}

/// Writes what Manage edits onto the raw settings document `settings`, read as `read`,
/// leaving every other field as it is.
fn patch_settings(
    settings: &mut Value,
    read: &ManageSettings,
    save: &ManageSettingsSave,
) -> Result<()> {
    let malformed = || {
        LauncherError::network(
            ErrorCode::Internal,
            "The server's settings came back in a shape the launcher can't edit.",
        )
    };
    if settings.is_null() {
        *settings = Value::Object(Default::default());
    }
    let root = settings.as_object_mut().ok_or_else(malformed)?;
    let custom = root
        .entry("customization")
        .or_insert_with(|| Value::Object(Default::default()));
    if custom.is_null() {
        *custom = Value::Object(Default::default());
    }
    let custom = custom.as_object_mut().ok_or_else(malformed)?;
    // A key is written only when the document has it or the player changed what Manage
    // showed: writing a default where nothing was changes the compose file (an
    // `ENABLE_WHITELIST: 'FALSE'` would turn off a whitelist a starting list enabled).
    let mut set = |key: &str, value: Value, changed: bool| {
        if changed || custom.get(key).is_some_and(|v| !v.is_null()) {
            custom.insert(key.into(), value);
        }
    };
    set(
        "mode",
        Value::from(save.mode.clone()),
        save.mode != read.mode,
    );
    set(
        "difficulty",
        Value::from(save.difficulty.clone()),
        save.difficulty != read.difficulty,
    );
    // `None` never removes it: itzg's default (20) can be above a free server's limit.
    if let Some(n) = save.max_players {
        set("maxPlayers", Value::from(n), Some(n) != read.max_players);
    }
    set("pvp", Value::from(save.pvp), save.pvp != read.pvp);
    set(
        "enableWhitelist",
        Value::from(save.enable_whitelist),
        save.enable_whitelist != read.enable_whitelist,
    );
    if let Some(motd) = save.motd.as_ref().filter(|_| !read.motd_read_only) {
        custom.insert("motd".into(), Value::from(motd.clone()));
    }
    if let Some(version) = save
        .version
        .as_ref()
        .filter(|v| !read.version_locked && **v != read.version)
    {
        root.insert("version".into(), Value::from(version.clone()));
    }
    Ok(())
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// A fresh JWT: scoped to `tenant`, or the base one with `None`. Through the cache; a
    /// refused `player-session` signs the player out. A cached base JWT that `select-tenant`
    /// refuses is replaced once (the Engine may have ended it while the device session lives);
    /// a base minted for this call is not.
    async fn fresh_jwt(&self, tenant: Option<&str>) -> Result<(Session, SessionJwt)> {
        let session = self.perland_session().await.ok_or_else(signed_out)?;
        let api = self.runner.stack();
        let mut source = SessionSource::new(api.clone(), &session);
        let mut jwt = match tenant {
            Some(tenant) => self.jwts.jwt_for(&source, tenant, now_secs()).await,
            None => self.jwts.base(&source, now_secs()).await,
        };
        if let (Some(tenant), Err(e)) = (tenant, &jwt) {
            let stale_base =
                e.code == ErrorCode::PerlandSignedOut && !source.minted_base.load(Ordering::SeqCst);
            if stale_base {
                self.jwts.forget_base();
                source = SessionSource::new(api, &session);
                jwt = self.jwts.jwt_for(&source, tenant, now_secs()).await;
            }
        }
        match jwt {
            Ok(jwt) => {
                // A sign-out or a new sign-in while the JWT was minted: it may be the old
                // player's, so keep none of them.
                let current = self.perland_session().await;
                if current.is_none_or(|c| c.token != session.token) {
                    self.jwts.clear();
                    return Err(signed_out());
                }
                Ok((session, jwt))
            }
            Err(e) => {
                if source.signed_out.load(Ordering::SeqCst) {
                    self.drop_session(&session.token).await;
                }
                Err(e)
            }
        }
    }

    /// The session and a JWT scoped to `tenant`.
    pub(crate) async fn manage_jwt(&self, tenant: &str) -> Result<(Session, SessionJwt)> {
        self.fresh_jwt(Some(tenant)).await
    }

    /// Run `call` with the session's Engine and a JWT for `tenant`. A 401 drops that JWT and
    /// the base one and retries once with fresh ones; a second 401 is returned as is (the device session may
    /// still be fine).
    pub(crate) async fn with_jwt<T>(
        &self,
        tenant: &str,
        call: impl Fn(Url, SessionJwt) -> BoxFuture<Result<T>>,
    ) -> Result<T> {
        let (session, jwt) = self.manage_jwt(tenant).await?;
        match call(session.engine, jwt).await {
            Err(e) if e.code == ErrorCode::PerlandSignedOut => {
                // The base JWT it was minted from may be what the Engine ended: replace both.
                self.jwts.forget(tenant);
                self.jwts.forget_base();
                let (session, jwt) = self.manage_jwt(tenant).await?;
                call(session.engine, jwt).await
            }
            answer => answer,
        }
    }

    /// The teams the player belongs to, with the base JWT; a 401 retries once with a fresh one.
    async fn memberships(&self) -> Result<Vec<Membership>> {
        let api = self.runner.stack();
        let (session, jwt) = self.fresh_jwt(None).await?;
        match api.memberships(session.engine, jwt).await {
            Err(e) if e.code == ErrorCode::PerlandSignedOut => {
                self.jwts.clear();
                let (session, jwt) = self.fresh_jwt(None).await?;
                api.memberships(session.engine, jwt).await
            }
            answer => answer,
        }
    }

    /// Every server of every team the player owns or is a member of (never a viewer's): the
    /// free team first, then teams by name, servers by name. A team whose list fails is left
    /// out. Signed out, or on an Engine without player sessions: none.
    pub async fn managed_servers(&self) -> Result<Vec<ManagedServer>> {
        if self.perland_session().await.is_none() {
            return Ok(vec![]);
        }
        let teams = match self.memberships().await {
            Ok(teams) => teams,
            Err(e) if e.code == ErrorCode::PlayerAccountsClosed => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        let api = self.runner.stack();
        let lists = futures_util::future::join_all(
            teams
                .into_iter()
                .filter(|t| matches!(t.role.as_str(), "owner" | "member"))
                .map(|team| {
                    let api = api.clone();
                    async move {
                        let list = self
                            .with_jwt(&team.tenant_id, |engine, jwt| api.get_all(engine, jwt))
                            .await;
                        (team, list)
                    }
                }),
        )
        .await;
        let mut servers = vec![];
        for (team, list) in lists {
            let items = match list {
                Ok(items) => items,
                Err(e) => {
                    tracing::info!(code = ?e.code, "couldn't list one team's servers");
                    continue;
                }
            };
            let free = is_free(&team);
            servers.extend(items.into_iter().map(|item| ManagedServer {
                tenant_id: team.tenant_id.clone(),
                team_name: team.display_name.clone(),
                name: item.stack_name.clone(),
                state: manage_state(&item.status, &item.health, None).into(),
                address: item.address.filter(|a| !a.trim().is_empty()),
                stack: item.stack_name,
                free,
            }));
        }
        servers.sort_by(|a, b| {
            (!a.free, a.team_name.to_lowercase(), &a.tenant_id)
                .cmp(&(!b.free, b.team_name.to_lowercase(), &b.tenant_id))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(servers)
    }

    /// One server's state; who is on only while it runs (a failure there leaves the counts
    /// out).
    pub async fn manage_detail(&self, tenant: &str, stack: &str) -> Result<ManageDetail> {
        let api = self.runner.stack();
        let detail = self
            .with_jwt(tenant, |engine, jwt| {
                api.details(engine, jwt, stack.to_string())
            })
            .await?;
        let state = manage_state(&detail.status, "", detail.idle.as_ref());
        let online = if state == "running" {
            match self
                .with_jwt(tenant, |engine, jwt| {
                    api.online(engine, jwt, stack.to_string())
                })
                .await
            {
                Ok(online) => Some(online),
                Err(e) => {
                    tracing::info!(code = ?e.code, "couldn't read who is on a server");
                    None
                }
            }
        } else {
            None
        };
        Ok(ManageDetail {
            tenant_id: tenant.to_string(),
            stack: stack.to_string(),
            state: state.into(),
            address: detail.address.filter(|a| !a.trim().is_empty()),
            online: online.as_ref().map(|o| o.online),
            max: online.as_ref().map(|o| o.max),
            players: online.and_then(|o| o.players).unwrap_or_default(),
            pending_restart: detail.pending_restart,
            version: detail.version,
            stops_at: detail.idle.and_then(|i| i.stops_at),
        })
    }

    /// Start, stop or restart a server.
    pub async fn manage_action(&self, tenant: &str, stack: &str, action: &str) -> Result<()> {
        let action = match action {
            "start" => Lifecycle::Start,
            "stop" => Lifecycle::Stop,
            "restart" => Lifecycle::Restart,
            _ => {
                return Err(LauncherError::game(
                    ErrorCode::Internal,
                    "That isn't something a server can do.",
                ))
            }
        };
        let api = self.runner.stack();
        self.with_jwt(tenant, |engine, jwt| {
            api.lifecycle(engine, jwt, stack.to_string(), action)
        })
        .await
    }

    /// What went wrong in the server's last run, worded by the UI from the cause's code.
    pub async fn manage_diagnosis(&self, tenant: &str, stack: &str) -> Result<ManageDiagnosis> {
        let api = self.runner.stack();
        let diagnosis = self
            .with_jwt(tenant, |engine, jwt| {
                api.diagnosis(engine, jwt, stack.to_string())
            })
            .await?;
        Ok(ManageDiagnosis {
            failed: diagnosis.outcome == "failed",
            oom_killed: diagnosis.oom_killed,
            cause: diagnosis.primary.map(|issue| ManageCause {
                code: issue.code,
                known: issue.known,
                subject: issue
                    .subject
                    .map(|s| s.name)
                    .filter(|n| !n.trim().is_empty()),
                params: issue
                    .params
                    .iter()
                    .map(|(k, v)| (k.clone(), param_text(v)))
                    .collect::<BTreeMap<_, _>>(),
            }),
        })
    }

    /// Who is on (while the server runs), the whitelist, the admins and the bans, read at
    /// once. A list that fails shows empty and marks the answer `unreadable`; so does a list
    /// the Engine could not read itself.
    pub async fn manage_players(&self, tenant: &str, stack: &str) -> Result<ManagePlayers> {
        let api = self.runner.stack();
        let stack = stack.to_string();
        let (online, whitelist, ops, bans) = futures_util::join!(
            self.with_jwt(tenant, |engine, jwt| api.online(engine, jwt, stack.clone())),
            self.with_jwt(tenant, |engine, jwt| {
                api.list(engine, jwt, stack.clone(), ListKind::Whitelist)
            }),
            self.with_jwt(tenant, |engine, jwt| {
                api.list(engine, jwt, stack.clone(), ListKind::Ops)
            }),
            self.with_jwt(tenant, |engine, jwt| api.bans(engine, jwt, stack.clone())),
        );
        // Signed out (or signed out meanwhile) is the answer, not four empty lists.
        let failed = online.is_err() || whitelist.is_err() || ops.is_err() || bans.is_err();
        if failed && self.perland_session().await.is_none() {
            return Err(signed_out());
        }
        let mut unreadable = false;
        let mut take = |what: &str, code: Option<ErrorCode>| {
            if let Some(code) = code {
                tracing::info!(?code, list = what, "couldn't read one of a server's lists");
                unreadable = true;
            }
        };
        take("online", online.as_ref().err().map(|e| e.code));
        take("whitelist", whitelist.as_ref().err().map(|e| e.code));
        take("admins", ops.as_ref().err().map(|e| e.code));
        take("bans", bans.as_ref().err().map(|e| e.code));
        let whitelist = whitelist.unwrap_or_default();
        let ops = ops.unwrap_or_default();
        let bans = bans.unwrap_or_default();
        unreadable |= whitelist.unreadable || ops.unreadable || bans.unreadable;
        let running = match &online {
            Ok(online) => online.running,
            Err(_) => whitelist.running || ops.running || bans.running,
        };
        let online = online
            .ok()
            .filter(|o| o.running)
            .and_then(|o| o.players)
            .unwrap_or_default();
        Ok(ManagePlayers {
            running,
            online,
            whitelist: whitelist.names,
            admins: ops.names,
            bans: bans
                .bans
                .into_iter()
                .map(|b| ManageBan {
                    name: b.name,
                    reason: b.reason.filter(|r| !r.trim().is_empty()),
                })
                .collect(),
            unreadable,
        })
    }

    /// Adds names to a list or takes them off it: `whitelist`, `admins` (ops) or `bans`
    /// (ban, unban), or a kick (`online`, remove only). 1 to 50 names, as the Engine takes.
    pub async fn manage_players_change(
        &self,
        tenant: &str,
        stack: &str,
        change: ManageListChange,
    ) -> Result<Vec<ManageChangeResult>> {
        if change.names.is_empty() || change.names.len() > MAX_NAMES {
            return Err(LauncherError::game(
                ErrorCode::Internal,
                "Pick between 1 and 50 players.",
            ));
        }
        let kind = match (change.list.as_str(), change.add) {
            ("online", false) => None,
            ("whitelist", _) => Some(ListKind::Whitelist),
            ("admins", _) => Some(ListKind::Ops),
            ("bans", _) => Some(ListKind::Bans),
            _ => {
                return Err(LauncherError::game(
                    ErrorCode::Internal,
                    "That isn't a list players can be changed on.",
                ))
            }
        };
        let api = self.runner.stack();
        let names = change.names;
        let results = self
            .with_jwt(tenant, |engine, jwt| match kind {
                None => api.kick(engine, jwt, stack.to_string(), names.clone()),
                Some(kind) => api.change(
                    engine,
                    jwt,
                    stack.to_string(),
                    kind,
                    change.add,
                    names.clone(),
                ),
            })
            .await?;
        Ok(results
            .into_iter()
            .map(|r| ManageChangeResult {
                name: r.name,
                status: r.status,
                message: r.message,
            })
            .collect())
    }

    /// The settings a player edits in Manage, read from the server's settings document.
    pub async fn manage_settings(&self, tenant: &str, stack: &str) -> Result<ManageSettings> {
        let api = self.runner.stack();
        let doc = self
            .with_jwt(tenant, |engine, jwt| {
                api.settings(engine, jwt, stack.to_string())
            })
            .await?;
        Ok(manage_settings_of(&doc))
    }

    /// Saves the settings a player edits. The Engine's PUT replaces the whole document and
    /// returns a field left out to its default (a modpack free server keeps its `MODRINTH_*`
    /// keys in `customization.env`), so this reads the document first and changes only the
    /// fields Manage edits on that raw JSON: everything else goes back as the Engine sent it.
    /// `save.revision` is sent as given, so a change made elsewhere since the player read the
    /// settings answers `SettingsChanged`.
    pub async fn manage_settings_save(
        &self,
        tenant: &str,
        stack: &str,
        save: ManageSettingsSave,
    ) -> Result<ManageSaved> {
        let api = self.runner.stack();
        let doc = self
            .with_jwt(tenant, |engine, jwt| {
                api.settings(engine, jwt, stack.to_string())
            })
            .await?;
        let read = manage_settings_of(&doc);
        let mut settings = doc.settings.clone();
        patch_settings(&mut settings, &read, &save)?;
        let body = serde_json::json!({
            "revision": save.revision,
            "settings": settings,
            "confirmVersionChange": save.confirm_version_change,
            "acknowledgeDataLoss": save.acknowledge_data_loss,
        });
        let apply = match save.apply.as_str() {
            "next_restart" => Apply::NextRestart,
            _ => Apply::Now,
        };
        let saved = self
            .with_jwt(tenant, |engine, jwt| {
                api.put_settings(engine, jwt, stack.to_string(), body.clone(), apply)
            })
            .await?;
        let shown = SettingsDoc {
            revision: saved.revision,
            managed_by_source: doc.managed_by_source,
            pending_restart: saved.pending,
            // An answer without the document: what was sent is what was saved.
            settings: if saved.settings.is_object() {
                saved.settings
            } else {
                settings
            },
        };
        Ok(ManageSaved {
            settings: manage_settings_of(&shown),
            restarted: saved.restarted,
            pending: saved.pending,
        })
    }

    /// Runs one console command. `confirm` lets a command that stops the server through;
    /// without it one answers `CommandNeedsConfirmation`.
    pub async fn manage_console(
        &self,
        tenant: &str,
        stack: &str,
        command: &str,
        confirm: bool,
    ) -> Result<ManageConsoleReply> {
        let api = self.runner.stack();
        let reply = self
            .with_jwt(tenant, |engine, jwt| {
                api.console(engine, jwt, stack.to_string(), command.to_string(), confirm)
            })
            .await?;
        Ok(ManageConsoleReply {
            ok: reply.outcome == "ok",
            output: perland::strip_formatting(&reply.output),
        })
    }

    /// Open the panel in the browser at a server's page (`stack`) or the team's upgrade page
    /// (`None`), signed in with a one-time handoff code when the Engine gives one. Without one
    /// (an older Engine, signed out, or any failure) the page opens plainly and the panel asks
    /// the player to sign in. The URL holding the code is never logged.
    pub async fn manage_open_panel(&self, tenant: &str, stack: Option<String>) -> Result<()> {
        let panel = self.config.panel.as_ref().ok_or_else(link_refused)?;
        let next = match stack.as_deref() {
            Some(stack) if stack.trim().is_empty() => return Err(link_refused()),
            Some(stack) => perland::server_path(stack),
            None => perland::upgrade_path().to_string(),
        };
        let session = self.perland_session().await;
        if let Some(session) = session.filter(|_| !tenant.trim().is_empty()) {
            let handoff = self
                .runner
                .stack()
                .handoff(session.engine, session.token, tenant.to_string())
                .await;
            match handoff {
                Ok(Some(handoff)) => {
                    self.host
                        .open_url(perland::handoff_url(panel, &handoff.code, &next).as_str());
                    return Ok(());
                }
                Ok(None) => {}
                Err(e) => tracing::info!(code = ?e.code, "couldn't get a panel sign-in code"),
            }
        }
        self.host
            .open_url(perland::panel_page(panel, &next).as_str());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        perland::{Issue, IssueSubject, Profile},
    };
    use serde_json::json;

    use super::*;
    use crate::{
        app::tests::{core, core_with_panel, TestCore},
        testing::{player_token, FakeStack},
    };

    const ENGINE: &str = "https://api.example.com/";

    fn sign_in(core: &TestCore) {
        core.perland_store
            .save(&Session {
                engine: Url::parse(ENGINE).unwrap(),
                token: player_token('s'),
                profile: Profile {
                    name: "Steve".into(),
                    email: None,
                    discord: true,
                    minecraft: None,
                },
            })
            .unwrap();
    }

    fn team(id: &str, name: &str, display: &str, role: &str, plan: Option<&str>) -> Membership {
        Membership {
            tenant_id: id.into(),
            tenant_name: name.into(),
            display_name: display.into(),
            role: role.into(),
            plan: plan.map(str::to_string),
        }
    }

    fn item(name: &str, status: &str) -> StackItem {
        StackItem {
            stack_name: name.into(),
            status: status.into(),
            address: Some(format!("{name}.example.com")),
            ..StackItem::default()
        }
    }

    fn unauthorized() -> LauncherError {
        signed_out()
    }

    fn stack(runner: &crate::testing::FakeRunner) -> Arc<FakeStack> {
        runner.stack.clone()
    }

    #[test]
    fn state_words() {
        let idle = |state: &str, paused: bool| StackIdle {
            state: state.into(),
            paused,
            stops_at: None,
        };
        let cases: &[(&str, &str, Option<StackIdle>, &str)] = &[
            ("sleeping", "", None, "asleep"),
            ("running", "", Some(idle("sleeping", false)), "asleep"),
            ("running", "", Some(idle("active", true)), "asleep"),
            ("exited", "", Some(idle("waking", false)), "starting"),
            ("running", "", Some(idle("stopping", false)), "stopping"),
            ("running", "starting", None, "starting"),
            ("running", "healthy", None, "running"),
            ("running", "", Some(idle("active", false)), "running"),
            ("restarting", "", None, "starting"),
            ("exited", "", None, "stopped"),
            ("dead", "", None, "stopped"),
            ("created", "", None, "stopped"),
            ("removing", "", None, "stopping"),
            ("paused", "", None, "unknown"),
            ("", "", None, "unknown"),
        ];
        for (status, health, idle, want) in cases {
            assert_eq!(
                manage_state(status, health, idle.as_ref()),
                *want,
                "{status}/{health}/{idle:?}"
            );
        }
    }

    #[tokio::test]
    async fn managed_servers_lists_owner_and_member_teams_only() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.memberships.lock().unwrap() = Ok(vec![
            team("t-b", "bravo", "Bravo", "member", Some("pro")),
            team("t-v", "viewer", "Viewer", "viewer", None),
            team("t-a", "alpha", "alpha", "owner", None),
            team("t-f", "player-abc", "Steve's server", "owner", None),
        ]);
        fake.serve_all(
            "t-b",
            Ok(vec![item("zeta", "running"), item("Beta", "exited")]),
        );
        fake.serve_all("t-v", Ok(vec![item("hidden", "running")]));
        fake.serve_all("t-a", Ok(vec![item("one", "sleeping")]));
        fake.serve_all("t-f", Ok(vec![item("Survival", "running")]));

        let servers = core.managed_servers().await.unwrap();
        let order: Vec<(&str, &str)> = servers
            .iter()
            .map(|s| (s.tenant_id.as_str(), s.stack.as_str()))
            .collect();
        assert_eq!(
            order,
            [
                ("t-f", "Survival"),
                ("t-a", "one"),
                ("t-b", "Beta"),
                ("t-b", "zeta"),
            ]
        );
        let free = &servers[0];
        assert!(free.free);
        assert_eq!(free.team_name, "Steve's server");
        assert_eq!(free.name, "Survival");
        assert_eq!(free.state, "running");
        assert_eq!(free.address.as_deref(), Some("Survival.example.com"));
        assert!(!servers[1].free);
        assert_eq!(servers[1].state, "asleep");
        assert_eq!(servers[2].state, "stopped");
        assert!(
            fake.calls_for_tenant("get_all", "t-v").is_empty(),
            "a viewer's team is never listed"
        );
        assert_eq!(fake.calls_to("player_session").len(), 1, "one base JWT");
        assert_eq!(
            fake.calls_to("player_session")[0].args,
            [player_token('s')],
            "the device token mints it"
        );
    }

    #[tokio::test]
    async fn managed_servers_marks_the_free_plan() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.memberships.lock().unwrap() = Ok(vec![
            team("t-a", "aaa", "Aaa", "owner", None),
            team("t-z", "zzz", "Zzz", "owner", Some("player-free")),
        ]);
        fake.serve_all("t-a", Ok(vec![item("a", "running")]));
        fake.serve_all("t-z", Ok(vec![item("z", "running")]));
        let servers = core.managed_servers().await.unwrap();
        assert_eq!(servers[0].tenant_id, "t-z");
        assert!(servers[0].free && !servers[1].free);
    }

    #[tokio::test]
    async fn managed_servers_signed_out_is_empty() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert_eq!(core.managed_servers().await.unwrap(), vec![]);
        assert!(stack(&runner).calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn managed_servers_on_an_old_engine_is_empty() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.player_session.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::PlayerAccountsClosed,
            "Perland accounts aren't open yet.",
        ));
        assert_eq!(core.managed_servers().await.unwrap(), vec![]);
        assert!(core.perland_session().await.is_some(), "still signed in");
    }

    #[tokio::test]
    async fn managed_servers_skips_a_failing_team() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.memberships.lock().unwrap() = Ok(vec![
            team("t-a", "alpha", "Alpha", "owner", None),
            team("t-b", "bravo", "Bravo", "owner", None),
        ]);
        fake.serve_all(
            "t-a",
            Err(LauncherError::network(
                ErrorCode::ManageForbidden,
                "You can't manage this server.",
            )),
        );
        fake.serve_all("t-b", Ok(vec![item("b", "running")]));
        let servers = core.managed_servers().await.unwrap();
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].tenant_id, "t-b");
    }

    #[tokio::test]
    async fn a_jwt_401_retries_once_with_a_fresh_token() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.details.lock().unwrap() = Ok(StackDetail {
            status: "exited".into(),
            ..StackDetail::default()
        });
        fake.fail_next("details", unauthorized());
        let detail = core.manage_detail("t1", "survival").await.unwrap();
        assert_eq!(detail.state, "stopped");
        let calls = fake.calls_to("details");
        assert_eq!(calls.len(), 2);
        assert_ne!(calls[0].jwt, calls[1].jwt, "the retry carries a new JWT");
        assert_eq!(fake.calls_to("select_tenant").len(), 2);
        assert!(core.perland_session().await.is_some());

        // A second 401 in a row is the answer, and the device session stays.
        fake.fail_next("details", unauthorized());
        fake.fail_next("details", unauthorized());
        let e = core.manage_detail("t1", "survival").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        assert_eq!(fake.calls_to("details").len(), 4);
        assert!(core.perland_session().await.is_some());
    }

    #[tokio::test]
    async fn a_cached_jwt_is_reused() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        core.manage_action("t1", "survival", "start").await.unwrap();
        core.manage_action("t1", "survival", "stop").await.unwrap();
        assert_eq!(fake.calls_to("player_session").len(), 1);
        assert_eq!(fake.calls_to("select_tenant").len(), 1);
        let calls = fake.calls_to("lifecycle");
        assert_eq!(calls[0].args, ["survival", "Start"]);
        assert_eq!(calls[1].args, ["survival", "Stop"]);
        assert_eq!(calls[0].tenant.as_deref(), Some("t1"));
    }

    #[tokio::test]
    async fn player_session_401_signs_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.player_session.lock().unwrap() = Err(unauthorized());
        let e = core
            .manage_action("t1", "survival", "restart")
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_session().await.is_none(), "signed out");
        assert!(fake.calls_to("lifecycle").is_empty());
    }

    #[tokio::test]
    async fn a_scoped_401_alone_keeps_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        fake.fail_next("select_tenant", unauthorized());
        let e = core.manage_action("t1", "s", "start").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_session().await.is_some());
    }

    #[tokio::test]
    async fn a_refused_base_jwt_is_replaced_on_retry() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        core.manage_action("t1", "s", "start").await.unwrap();
        let old_base = fake.calls_to("select_tenant")[0].jwt.clone();
        // The cached base JWT is refused when a new team's JWT is minted from it.
        fake.fail_next("select_tenant", unauthorized());
        core.manage_action("t2", "s", "start").await.unwrap();
        let selects = fake.calls_to("select_tenant");
        assert_eq!(selects.len(), 3);
        assert_eq!(selects[1].jwt, old_base, "first tried with the cached base");
        assert_ne!(selects[2].jwt, old_base, "then with a fresh one");
        assert_eq!(fake.calls_to("player_session").len(), 2);
        assert!(core.perland_session().await.is_some());

        // A route's 401 replaces the base JWT too, not only the team's.
        fake.fail_next("lifecycle", unauthorized());
        core.manage_action("t2", "s", "stop").await.unwrap();
        assert_eq!(fake.calls_to("player_session").len(), 3);
        let selects = fake.calls_to("select_tenant");
        assert_eq!(selects.len(), 4);
        assert_ne!(selects[3].jwt, selects[2].jwt, "minted from the new base");
    }

    #[tokio::test]
    async fn a_refused_base_jwt_is_replaced_once_only() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        core.manage_action("t1", "s", "start").await.unwrap();
        for _ in 0..3 {
            fake.fail_next("select_tenant", unauthorized());
        }
        let e = core.manage_action("t2", "s", "start").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        assert_eq!(fake.calls_to("select_tenant").len(), 3, "t1, then t2 twice");
        assert_eq!(fake.calls_to("player_session").len(), 2, "one replacement");
        assert!(
            core.perland_session().await.is_some(),
            "the device session stays"
        );

        // A base JWT minted for this very call is not replaced again.
        let (fresh, runner, _host, _t2) = crate::app::tests::core(BuildKind::Dev);
        sign_in(&fresh);
        let fake = stack(&runner);
        fake.fail_next("select_tenant", unauthorized());
        let e = fresh.manage_action("t1", "s", "start").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        assert_eq!(fake.calls_to("select_tenant").len(), 1);
        assert_eq!(fake.calls_to("player_session").len(), 1);
    }

    #[tokio::test]
    async fn an_unknown_action_is_an_app_refusal() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let e = core.manage_action("t1", "s", "delete").await.unwrap_err();
        assert_eq!(e.category, launcher_core::ErrorCategory::Game);
    }

    #[tokio::test]
    async fn sign_out_drops_the_cached_jwts() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        core.manage_action("t1", "s", "start").await.unwrap();
        let session = core.perland_session().await.unwrap();
        core.perland_sign_out().await.unwrap();
        let source = SessionSource::new(runner.stack(), &session);
        core.jwts.jwt_for(&source, "t1", now_secs()).await.unwrap();
        assert_eq!(fake.calls_to("player_session").len(), 2, "nothing was kept");
        assert_eq!(fake.calls_to("select_tenant").len(), 2);
    }

    #[tokio::test]
    async fn a_new_session_drops_the_cached_jwts() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        core.manage_action("t1", "s", "start").await.unwrap();
        core.perland_email_verify("steve@example.com", "123456")
            .await
            .unwrap();
        core.manage_action("t1", "s", "start").await.unwrap();
        let sessions = fake.calls_to("player_session");
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            sessions[1].args,
            [player_token('x')],
            "the new session's token"
        );
    }

    #[tokio::test]
    async fn manage_calls_signed_out_are_refused() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        for e in [
            core.manage_detail("t1", "s").await.unwrap_err(),
            core.manage_action("t1", "s", "start").await.unwrap_err(),
            core.manage_diagnosis("t1", "s").await.unwrap_err(),
        ] {
            assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        }
        assert!(stack(&runner).calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_unknown_action_calls_nothing() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let e = core.manage_action("t1", "s", "delete").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert!(stack(&runner).calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn detail_reads_players_only_while_running() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.details.lock().unwrap() = Ok(StackDetail {
            stack_name: "survival".into(),
            status: "running".into(),
            address: Some("play.example.com".into()),
            version: "1.21.1".into(),
            pending_restart: true,
            idle: Some(StackIdle {
                state: "active".into(),
                paused: false,
                stops_at: Some("2026-10-21T12:00:00Z".into()),
            }),
            ..StackDetail::default()
        });
        *fake.online.lock().unwrap() = Ok(Online {
            running: true,
            online: 2,
            max: 20,
            players: Some(vec!["Alex".into(), "Steve".into()]),
            ..Online::default()
        });
        let detail = core.manage_detail("t1", "survival").await.unwrap();
        assert_eq!(
            detail,
            ManageDetail {
                tenant_id: "t1".into(),
                stack: "survival".into(),
                state: "running".into(),
                address: Some("play.example.com".into()),
                online: Some(2),
                max: Some(20),
                players: vec!["Alex".into(), "Steve".into()],
                pending_restart: true,
                version: "1.21.1".into(),
                stops_at: Some("2026-10-21T12:00:00Z".into()),
            }
        );
        assert_eq!(fake.calls_to("online").len(), 1);

        // A failing player read leaves the counts out.
        *fake.online.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::ServerConsoleUnavailable,
            "x",
        ));
        let detail = core.manage_detail("t1", "survival").await.unwrap();
        assert_eq!((detail.online, detail.max), (None, None));
        assert!(detail.players.is_empty());

        // Asleep: who is on isn't asked.
        *fake.details.lock().unwrap() = Ok(StackDetail {
            status: "sleeping".into(),
            ..StackDetail::default()
        });
        let detail = core.manage_detail("t1", "survival").await.unwrap();
        assert_eq!(detail.state, "asleep");
        assert_eq!(fake.calls_to("online").len(), 2);
    }

    #[tokio::test]
    async fn diagnosis_names_the_cause() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        let mut params = serde_json::Map::new();
        params.insert("required".into(), json!("21"));
        params.insert("found".into(), json!(17));
        params.insert("fatal".into(), json!(true));
        *fake.diagnosis.lock().unwrap() = Ok(Diagnosis {
            outcome: "failed".into(),
            oom_killed: true,
            primary: Some(Issue {
                code: "java_version".into(),
                known: true,
                severity: "fatal".into(),
                params,
                subject: Some(IssueSubject {
                    kind: "plugin".into(),
                    name: "EssentialsX".into(),
                    version: "2.20".into(),
                }),
            }),
        });
        let diagnosis = core.manage_diagnosis("t1", "survival").await.unwrap();
        assert_eq!(
            diagnosis,
            ManageDiagnosis {
                failed: true,
                oom_killed: true,
                cause: Some(ManageCause {
                    code: "java_version".into(),
                    known: true,
                    subject: Some("EssentialsX".into()),
                    params: BTreeMap::from([
                        ("fatal".into(), "true".into()),
                        ("found".into(), "17".into()),
                        ("required".into(), "21".into()),
                    ]),
                }),
            }
        );

        *fake.diagnosis.lock().unwrap() = Ok(Diagnosis {
            outcome: "running".into(),
            ..Diagnosis::default()
        });
        let diagnosis = core.manage_diagnosis("t1", "survival").await.unwrap();
        assert!(!diagnosis.failed && diagnosis.cause.is_none());
    }

    #[tokio::test]
    async fn open_panel_uses_the_handoff() {
        let (core, runner, host, _t) = core_with_panel("https://panel.example.com/");
        sign_in(&core);
        let fake = stack(&runner);
        *fake.handoff.lock().unwrap() = Ok(Some(Handoff {
            code: "hc_secret".into(),
            expires_in: 60,
        }));
        core.manage_open_panel("t1", Some("my world".into()))
            .await
            .unwrap();
        assert_eq!(
            *host.opened.lock().unwrap(),
            ["https://panel.example.com/login/handoff?code=hc_secret&next=%2Fstacks%2Fmy%2520world"]
        );
        let calls = fake.calls_to("handoff");
        assert_eq!(calls[0].args, [player_token('s'), "t1".to_string()]);
    }

    #[tokio::test]
    async fn open_panel_without_handoff_opens_the_page() {
        let (core, runner, host, _t) = core_with_panel("https://panel.example.com/");
        sign_in(&core);
        let fake = stack(&runner);
        *fake.handoff.lock().unwrap() = Ok(None);
        core.manage_open_panel("t1", Some("survival".into()))
            .await
            .unwrap();
        *fake.handoff.lock().unwrap() = Err(LauncherError::network(ErrorCode::Internal, "boom"));
        core.manage_open_panel("t1", Some("survival".into()))
            .await
            .unwrap();
        assert_eq!(
            *host.opened.lock().unwrap(),
            [
                "https://panel.example.com/stacks/survival",
                "https://panel.example.com/stacks/survival"
            ]
        );
    }

    #[tokio::test]
    async fn open_panel_upgrade_goes_to_settings_upgrade() {
        let (core, runner, host, _t) = core_with_panel("https://panel.example.com/");
        sign_in(&core);
        *stack(&runner).handoff.lock().unwrap() = Ok(None);
        core.manage_open_panel("t1", None).await.unwrap();
        assert_eq!(
            *host.opened.lock().unwrap(),
            ["https://panel.example.com/settings/upgrade"]
        );
    }

    #[tokio::test]
    async fn open_panel_needs_a_panel_and_a_name() {
        let (without, _, host, _t1) = core(BuildKind::Dev);
        let e = without.manage_open_panel("t1", None).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::LinkRefused);
        let (with, _, panel_host, _t2) = core_with_panel("https://panel.example.com");
        let e = with
            .manage_open_panel("t1", Some(" ".into()))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::LinkRefused);
        assert!(host.opened.lock().unwrap().is_empty());
        assert!(panel_host.opened.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn open_panel_signed_out_opens_the_page() {
        let (core, runner, host, _t) = core_with_panel("https://panel.example.com");
        core.manage_open_panel("t1", Some("survival".into()))
            .await
            .unwrap();
        assert_eq!(
            *host.opened.lock().unwrap(),
            ["https://panel.example.com/stacks/survival"]
        );
        assert!(stack(&runner).calls.lock().unwrap().is_empty());
    }

    #[derive(Clone, Default)]
    struct Buf(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn open_panel_never_logs_the_code() {
        let buf = Buf::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_max_level(tracing::Level::TRACE)
            .with_writer(move || writer.clone())
            .finish();
        let _guard = tracing::subscriber::set_default(subscriber);

        let (core, runner, host, _t) = core_with_panel("https://panel.example.com/");
        sign_in(&core);
        let fake = stack(&runner);
        *fake.handoff.lock().unwrap() = Ok(Some(Handoff {
            code: "hc_secret_code".into(),
            expires_in: 60,
        }));
        core.manage_open_panel("t1", Some("survival".into()))
            .await
            .unwrap();
        // A failed handoff is logged, by code only.
        *fake.handoff.lock().unwrap() =
            Err(LauncherError::network(ErrorCode::Internal, "boom").with_detail("hc_secret_code"));
        core.manage_open_panel("t1", Some("survival".into()))
            .await
            .unwrap();

        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("Internal"), "the failure is logged: {text}");
        assert!(!text.contains("hc_secret_code"), "{text}");
        assert!(!text.contains(&player_token('s')), "{text}");
        let events = format!("{:?}", host.events.lock().unwrap());
        assert!(!events.contains("hc_secret_code"), "{events}");
        let opened = host.opened.lock().unwrap();
        assert!(
            opened[0].contains("hc_secret_code"),
            "only the opened URL holds it"
        );
        assert!(!opened[1].contains("hc_secret_code"));
    }

    fn results(names: &[&str], status: &str) -> Vec<NameResult> {
        names
            .iter()
            .map(|n| NameResult {
                name: n.to_string(),
                status: status.into(),
                message: None,
            })
            .collect()
    }

    fn names(list: &[&str]) -> NameList {
        NameList {
            names: list.iter().map(|n| n.to_string()).collect(),
            ..NameList::default()
        }
    }

    fn change(list: &str, add: bool, who: &[&str]) -> ManageListChange {
        ManageListChange {
            list: list.into(),
            add,
            names: who.iter().map(|n| n.to_string()).collect(),
        }
    }

    #[tokio::test]
    async fn players_read_four_lists() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.online.lock().unwrap() = Ok(Online {
            running: true,
            online: 1,
            max: 20,
            players: Some(vec!["Alex".into()]),
            ..Online::default()
        });
        *fake.whitelist.lock().unwrap() = Ok(names(&["Alex", "Steve"]));
        *fake.ops.lock().unwrap() = Ok(names(&["Steve"]));
        *fake.bans.lock().unwrap() = Ok(BanList {
            running: true,
            unreadable: false,
            bans: vec![
                perland::Ban {
                    name: "Griefer".into(),
                    reason: Some("TNT".into()),
                },
                perland::Ban {
                    name: "Quiet".into(),
                    reason: None,
                },
            ],
        });
        let players = core.manage_players("t1", "survival").await.unwrap();
        assert_eq!(
            players,
            ManagePlayers {
                running: true,
                online: vec!["Alex".into()],
                whitelist: vec!["Alex".into(), "Steve".into()],
                admins: vec!["Steve".into()],
                bans: vec![
                    ManageBan {
                        name: "Griefer".into(),
                        reason: Some("TNT".into()),
                    },
                    ManageBan {
                        name: "Quiet".into(),
                        reason: None,
                    },
                ],
                unreadable: false,
            }
        );
        let lists: Vec<Vec<String>> = fake.calls_to("list").into_iter().map(|c| c.args).collect();
        assert!(lists.contains(&vec!["survival".into(), "Whitelist".into()]));
        assert!(lists.contains(&vec!["survival".into(), "Ops".into()]));
        assert_eq!(fake.calls_to("bans").len(), 1);
        assert_eq!(fake.calls_to("online").len(), 1);

        // Stopped: who is on isn't asked.
        *fake.online.lock().unwrap() = Ok(Online::default());
        let players = core.manage_players("t1", "survival").await.unwrap();
        assert!(!players.running && players.online.is_empty());
    }

    #[tokio::test]
    async fn a_failing_list_is_unreadable_not_an_error() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.whitelist.lock().unwrap() = Ok(names(&["Alex"]));
        *fake.ops.lock().unwrap() = Err(LauncherError::network(ErrorCode::Internal, "boom"));
        let players = core.manage_players("t1", "survival").await.unwrap();
        assert!(players.unreadable);
        assert!(players.admins.is_empty());
        assert_eq!(players.whitelist, ["Alex"]);

        // A list the Engine read as unreadable is unreadable too.
        *fake.ops.lock().unwrap() = Ok(NameList::default());
        *fake.bans.lock().unwrap() = Ok(BanList {
            unreadable: true,
            ..BanList::default()
        });
        assert!(
            core.manage_players("t1", "survival")
                .await
                .unwrap()
                .unreadable
        );
    }

    #[tokio::test]
    async fn kick_is_online_remove() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.kick.lock().unwrap() = Ok(vec![NameResult {
            name: "Alex".into(),
            status: "kicked".into(),
            message: Some("bye".into()),
        }]);
        let out = core
            .manage_players_change("t1", "survival", change("online", false, &["Alex"]))
            .await
            .unwrap();
        assert_eq!(
            out,
            [ManageChangeResult {
                name: "Alex".into(),
                status: "kicked".into(),
                message: Some("bye".into()),
            }]
        );
        assert_eq!(fake.calls_to("kick")[0].args, ["survival", "Alex"]);
        assert!(fake.calls_to("change").is_empty());

        // Nobody is added to who is on.
        let e = core
            .manage_players_change("t1", "survival", change("online", true, &["Alex"]))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::Internal);
        assert_eq!(fake.calls_to("kick").len(), 1);
    }

    #[tokio::test]
    async fn admins_map_to_ops() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.change.lock().unwrap() = Ok(results(&["Steve"], "opped"));
        core.manage_players_change("t1", "survival", change("admins", true, &["Steve"]))
            .await
            .unwrap();
        core.manage_players_change("t1", "survival", change("whitelist", false, &["Alex"]))
            .await
            .unwrap();
        core.manage_players_change("t1", "survival", change("bans", true, &["Griefer"]))
            .await
            .unwrap();
        let calls: Vec<Vec<String>> = fake
            .calls_to("change")
            .into_iter()
            .map(|c| c.args)
            .collect();
        assert_eq!(
            calls,
            [
                ["survival", "Ops", "true", "Steve"],
                ["survival", "Whitelist", "false", "Alex"],
                ["survival", "Bans", "true", "Griefer"],
            ]
        );
    }

    #[tokio::test]
    async fn unban_is_bans_remove() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.change.lock().unwrap() = Ok(results(&["Griefer"], "pardoned"));
        let out = core
            .manage_players_change("t1", "survival", change("bans", false, &["Griefer"]))
            .await
            .unwrap();
        assert_eq!(out[0].status, "pardoned");
        assert_eq!(
            fake.calls_to("change")[0].args,
            ["survival", "Bans", "false", "Griefer"]
        );
    }

    #[tokio::test]
    async fn changes_refuse_more_than_fifty_names() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let many: Vec<String> = (0..51).map(|i| format!("p{i}")).collect();
        let fifty = ManageListChange {
            list: "whitelist".into(),
            add: true,
            names: many[..50].to_vec(),
        };
        core.manage_players_change("t1", "s", fifty).await.unwrap();
        for bad in [
            ManageListChange {
                list: "whitelist".into(),
                add: true,
                names: many,
            },
            change("whitelist", true, &[]),
            change("friends", true, &["Alex"]),
        ] {
            let e = core
                .manage_players_change("t1", "s", bad)
                .await
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::Internal);
        }
        assert_eq!(stack(&runner).calls_to("change").len(), 1);
    }

    fn doc(settings: Value) -> SettingsDoc {
        SettingsDoc {
            revision: "r1".into(),
            managed_by_source: None,
            pending_restart: false,
            settings,
        }
    }

    #[tokio::test]
    async fn settings_read_defaults() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.settings.lock().unwrap() = Ok(SettingsDoc {
            pending_restart: true,
            ..doc(json!({"type": "PAPER", "version": "1.21.1", "customization": {}}))
        });
        let settings = core.manage_settings("t1", "survival").await.unwrap();
        assert_eq!(
            settings,
            ManageSettings {
                revision: "r1".into(),
                motd: String::new(),
                motd_read_only: false,
                mode: "survival".into(),
                difficulty: "easy".into(),
                max_players: None,
                pvp: true,
                enable_whitelist: false,
                version: "1.21.1".into(),
                version_locked: false,
                pending_restart: true,
            }
        );

        *fake.settings.lock().unwrap() = Ok(doc(json!({
            "version": "1.20.4",
            "customization": {
                "motd": "Hi there", "mode": "creative", "difficulty": "hard",
                "maxPlayers": 12, "pvp": false, "enableWhitelist": true,
            }
        })));
        let settings = core.manage_settings("t1", "survival").await.unwrap();
        assert_eq!(settings.motd, "Hi there");
        assert_eq!(settings.mode, "creative");
        assert_eq!(settings.difficulty, "hard");
        assert_eq!(settings.max_players, Some(12));
        assert!(!settings.pvp && settings.enable_whitelist);
    }

    #[tokio::test]
    async fn settings_motd_with_section_codes_is_read_only() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        *stack(&runner).settings.lock().unwrap() =
            Ok(doc(json!({"customization": {"motd": "§6Gold"}})));
        let settings = core.manage_settings("t1", "s").await.unwrap();
        assert!(settings.motd_read_only);
        assert_eq!(settings.motd, "§6Gold");
    }

    #[tokio::test]
    async fn settings_version_locked_for_a_modpack() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        let cases = [
            SettingsDoc {
                managed_by_source: Some("modpack".into()),
                ..doc(json!({"version": "1.20.1"}))
            },
            doc(json!({"type": "MODRINTH", "version": "1.20.1"})),
            doc(
                json!({"version": "1.20.1", "customization": {"env": {"MODRINTH_MODPACK": "abc"}}}),
            ),
        ];
        for case in cases {
            *fake.settings.lock().unwrap() = Ok(case.clone());
            let settings = core.manage_settings("t1", "s").await.unwrap();
            assert!(settings.version_locked, "{case:?}");
        }
        *fake.settings.lock().unwrap() = Ok(doc(json!({"type": "FABRIC", "version": "1.20.1"})));
        assert!(
            !core
                .manage_settings("t1", "s")
                .await
                .unwrap()
                .version_locked
        );
    }

    fn save(revision: &str) -> ManageSettingsSave {
        ManageSettingsSave {
            revision: revision.into(),
            motd: None,
            mode: "survival".into(),
            difficulty: "normal".into(),
            max_players: Some(10),
            pvp: true,
            enable_whitelist: false,
            version: None,
            apply: "now".into(),
            confirm_version_change: false,
            acknowledge_data_loss: false,
        }
    }

    /// The body of the one PUT the fake saw, and its apply mode.
    fn put_of(fake: &FakeStack) -> (Value, String) {
        let calls = fake.calls_to("put_settings");
        assert_eq!(calls.len(), 1);
        (
            serde_json::from_str(&calls[0].args[1]).unwrap(),
            calls[0].args[2].clone(),
        )
    }

    fn modpack_doc() -> Value {
        json!({
            "type": "MODRINTH",
            "version": "1.20.1",
            "loaderVersion": "0.16.0",
            "customization": {
                "motd": "§aModded",
                "mode": "survival",
                "difficulty": "normal",
                "maxPlayers": 10,
                "pvp": true,
                "enableWhitelist": false,
                "viewDistance": 12,
                "ops": ["Steve"],
                "env": {"MODRINTH_MODPACK": "abc", "MODRINTH_VERSION": "v1", "MODRINTH_LOADER": "fabric"},
                "unknownFutureField": {"nested": [1, 2, 3]}
            }
        })
    }

    #[tokio::test]
    async fn settings_put_keeps_every_field_it_did_not_change() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.settings.lock().unwrap() = Ok(doc(modpack_doc()));
        *fake.put_settings.lock().unwrap() = Ok(SettingsSaved {
            revision: "r2".into(),
            pending: false,
            restarted: true,
            settings: modpack_doc(),
        });
        let mut s = save("r1");
        s.pvp = false;
        // The MOTD is read only (colour codes) and the version locked (a modpack): both are
        // left as they are even when sent.
        s.motd = Some("Plain".into());
        s.version = Some("1.21.1".into());
        let saved = core.manage_settings_save("t1", "pack", s).await.unwrap();

        let (body, apply) = put_of(&fake);
        let mut want = modpack_doc();
        want["customization"]["pvp"] = json!(false);
        assert_eq!(
            body,
            json!({
                "revision": "r1",
                "settings": want,
                "confirmVersionChange": false,
                "acknowledgeDataLoss": false,
            })
        );
        assert_eq!(apply, "Now");
        assert!(saved.restarted && !saved.pending);
        assert_eq!(saved.settings.revision, "r2");
        assert!(saved.settings.version_locked);
    }

    #[tokio::test]
    async fn settings_save_sends_the_revision_and_confirmations() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.settings.lock().unwrap() = Ok(doc(json!({
            "type": "PAPER", "version": "1.20.4", "customization": {"motd": "Old", "maxPlayers": 5}
        })));
        let mut s = save("r0");
        s.motd = Some("New".into());
        s.version = Some("1.21.1".into());
        s.max_players = None;
        s.confirm_version_change = true;
        s.acknowledge_data_loss = true;
        core.manage_settings_save("t1", "s", s).await.unwrap();
        let (body, _) = put_of(&fake);
        assert_eq!(body["revision"], "r0", "the revision the player read");
        assert_eq!(body["confirmVersionChange"], true);
        assert_eq!(body["acknowledgeDataLoss"], true);
        assert_eq!(body["settings"]["version"], "1.21.1");
        assert_eq!(body["settings"]["type"], "PAPER");
        let c = &body["settings"]["customization"];
        assert_eq!(c["motd"], "New");
        assert_eq!(c["difficulty"], "normal");
        assert_eq!(c["maxPlayers"], 5, "None leaves it as it was");
        assert!(body.get("broadcast").is_none() && body.get("skipSafetyBackup").is_none());
    }

    #[tokio::test]
    async fn settings_save_leaves_absent_fields_absent() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        let bare =
            json!({"type": "PAPER", "version": "1.21.1", "customization": {"viewDistance": 8}});
        *fake.settings.lock().unwrap() = Ok(doc(bare.clone()));
        // What Manage reports for that document, sent back unchanged.
        let shown = core.manage_settings("t1", "s").await.unwrap();
        let s = ManageSettingsSave {
            revision: shown.revision,
            motd: None,
            mode: shown.mode,
            difficulty: shown.difficulty,
            max_players: shown.max_players,
            pvp: shown.pvp,
            enable_whitelist: shown.enable_whitelist,
            version: Some(shown.version),
            apply: "now".into(),
            confirm_version_change: false,
            acknowledge_data_loss: false,
        };
        core.manage_settings_save("t1", "s", s.clone())
            .await
            .unwrap();
        let (body, _) = put_of(&fake);
        assert_eq!(body["settings"], bare);

        // A changed default is written; the others stay absent.
        fake.calls.lock().unwrap().clear();
        let mut changed = s;
        changed.enable_whitelist = true;
        changed.max_players = Some(8);
        core.manage_settings_save("t1", "s", changed).await.unwrap();
        let (body, _) = put_of(&fake);
        let c = &body["settings"]["customization"];
        assert_eq!(c["enableWhitelist"], true);
        assert_eq!(c["maxPlayers"], 8);
        for key in ["mode", "difficulty", "pvp"] {
            assert!(c.get(key).is_none(), "{key}");
        }
    }

    #[tokio::test]
    async fn settings_save_skips_a_locked_version() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.settings.lock().unwrap() = Ok(SettingsDoc {
            managed_by_source: Some("modpack".into()),
            ..doc(json!({"type": "FABRIC", "version": "1.20.1", "customization": {}}))
        });
        let mut s = save("r1");
        s.version = Some("1.21.1".into());
        core.manage_settings_save("t1", "s", s).await.unwrap();
        let (body, _) = put_of(&fake);
        assert_eq!(body["settings"]["version"], "1.20.1");
    }

    #[tokio::test]
    async fn settings_save_next_restart_passes_apply() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.settings.lock().unwrap() = Ok(doc(json!({"version": "1.21.1", "customization": {}})));
        *fake.put_settings.lock().unwrap() = Ok(SettingsSaved {
            revision: "r2".into(),
            pending: true,
            restarted: false,
            settings: json!({"version": "1.21.1", "customization": {"pvp": true, "mode": "survival"}}),
        });
        let mut s = save("r1");
        s.apply = "next_restart".into();
        let saved = core.manage_settings_save("t1", "s", s).await.unwrap();
        let (_, apply) = put_of(&fake);
        assert_eq!(apply, "NextRestart");
        assert!(saved.pending && !saved.restarted);
        assert!(saved.settings.pending_restart);
    }

    #[tokio::test]
    async fn console_strips_formatting() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.console.lock().unwrap() = Ok(ConsoleReply {
            outcome: "ok".into(),
            output: "§aThere are §e2§a players online".into(),
            code: None,
        });
        let reply = core.manage_console("t1", "s", "list", false).await.unwrap();
        assert_eq!(
            reply,
            ManageConsoleReply {
                ok: true,
                output: "There are 2 players online".into(),
            }
        );
        assert_eq!(fake.calls_to("console")[0].args, ["s", "list", "false"]);

        *fake.console.lock().unwrap() = Ok(ConsoleReply {
            outcome: "error".into(),
            output: "Unknown command".into(),
            code: None,
        });
        assert!(!core.manage_console("t1", "s", "x", false).await.unwrap().ok);
    }

    #[tokio::test]
    async fn console_needs_confirmation_is_a_code() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        *fake.console.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::CommandNeedsConfirmation,
            "This command stops the server.",
        ));
        let e = core
            .manage_console("t1", "s", "stop", false)
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::CommandNeedsConfirmation);
        *fake.console.lock().unwrap() = Ok(ConsoleReply {
            outcome: "ok".into(),
            ..ConsoleReply::default()
        });
        core.manage_console("t1", "s", "stop", true).await.unwrap();
        assert_eq!(fake.calls_to("console")[1].args, ["s", "stop", "true"]);
    }
}
