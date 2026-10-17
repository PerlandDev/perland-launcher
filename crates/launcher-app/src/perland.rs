//! Sign in with Perland (sign-in and Your servers spec §6.2): the player's Perland account,
//! through the Engine the public list uses. The session, its token above all, is read from the
//! credential store only on the first Perland command, never at start, so a player who never
//! opens Accounts or Servers never touches it. Every later player call goes to the session's own
//! Engine. Nothing here logs a token, an email code, the PKCE verifier or an authorization code.
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use launcher_core::{
    http::HttpClient,
    invite::{InviteClient, InviteLink, Url},
    perland::{
        authorize_url, Callback, EmailStarted, Language, Loopback, Pkce, Player, PlayerClient,
        PlayerConfig, Profile, Session, SignInMethod, TokenAnswer, YourServer,
    },
    ErrorCode, LauncherError, Result,
};
use tokio::sync::{watch, MutexGuard};

use crate::{
    app::AppCore,
    dto::{EmailCodeSent, PerlandAccount, PerlandSignInPhase, PerlandStatus},
    events::{AppEvent, Host},
    runner::{BoxFuture, Runner},
};

/// The marker file under the data root that says a session exists (spec §5, Token storage).
pub const ACCOUNT_MARKER: &str = "perland-account.json";
/// How long a Discord sign-in waits for the browser: the Engine forgets its state after that.
pub const SIGN_IN_DEADLINE: Duration = Duration::from_secs(10 * 60);
/// `GET /me` is asked at most this often.
pub const ME_EVERY: Duration = Duration::from_secs(60);
/// The longest `POST /sign-out` is waited for before the session is forgotten anyway.
pub const SIGN_OUT_LIMIT: Duration = Duration::from_secs(5);

/// The Engine's player routes. Every call names the Engine it goes to: the session's, or for a
/// sign-in the one `site_engine` found. The real one is `CorePlayer`; tests use `FakePlayer`.
pub trait PlayerApi: Send + Sync + 'static {
    /// The Engine the public list's site names: where a sign-in starts.
    fn site_engine(&self) -> BoxFuture<Result<Url>>;
    /// The Engine an invite's site names, found the way the invite flow finds it.
    fn engine_for(&self, link: InviteLink) -> BoxFuture<Result<Url>>;
    fn config(&self, engine: Url) -> BoxFuture<Result<PlayerConfig>>;
    fn token(
        &self,
        engine: Url,
        code: String,
        verifier: String,
        redirect: String,
    ) -> BoxFuture<Result<TokenAnswer>>;
    fn email_start(
        &self,
        engine: Url,
        email: String,
        language: String,
    ) -> BoxFuture<Result<EmailStarted>>;
    fn email_verify(
        &self,
        engine: Url,
        email: String,
        code: String,
    ) -> BoxFuture<Result<TokenAnswer>>;
    fn me(&self, engine: Url, token: String) -> BoxFuture<Result<Player>>;
    fn sign_out(&self, engine: Url, token: String) -> BoxFuture<Result<()>>;
    fn servers(&self, engine: Url, token: String) -> BoxFuture<Result<Vec<YourServer>>>;
    fn follow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
    fn unfollow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
    fn joined(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
}

/// `PlayerApi` over `launcher_core::perland::PlayerClient`.
pub struct CorePlayer {
    /// Never follows a redirect: one would replay a code or verifier body to another host.
    http: Result<HttpClient>,
    invites: InviteClient,
    site: Url,
}

impl CorePlayer {
    /// `http` should come from `launcher_core::http::client_without_redirects`; an error is
    /// what every call then answers. `site` is the public list's site.
    pub fn new(http: Result<HttpClient>, invites: InviteClient, site: Url) -> Self {
        Self {
            http,
            invites,
            site,
        }
    }

    fn client(&self, engine: Url) -> Result<PlayerClient> {
        Ok(PlayerClient::new(self.http.clone()?, engine))
    }
}

impl PlayerApi for CorePlayer {
    fn site_engine(&self) -> BoxFuture<Result<Url>> {
        let (invites, site) = (self.invites.clone(), self.site.clone());
        Box::pin(async move { invites.engine_for_site(&site).await })
    }

    fn engine_for(&self, link: InviteLink) -> BoxFuture<Result<Url>> {
        let invites = self.invites.clone();
        Box::pin(async move { invites.engine_for(&link).await })
    }

    fn config(&self, engine: Url) -> BoxFuture<Result<PlayerConfig>> {
        let client = self.client(engine);
        Box::pin(async move { client?.config().await })
    }

    fn token(
        &self,
        engine: Url,
        code: String,
        verifier: String,
        redirect: String,
    ) -> BoxFuture<Result<TokenAnswer>> {
        let client = self.client(engine);
        Box::pin(async move { client?.token(&code, &verifier, &redirect).await })
    }

    fn email_start(
        &self,
        engine: Url,
        email: String,
        language: String,
    ) -> BoxFuture<Result<EmailStarted>> {
        let client = self.client(engine);
        Box::pin(async move { client?.email_start(&email, &language).await })
    }

    fn email_verify(
        &self,
        engine: Url,
        email: String,
        code: String,
    ) -> BoxFuture<Result<TokenAnswer>> {
        let client = self.client(engine);
        Box::pin(async move { client?.email_verify(&email, &code).await })
    }

    fn me(&self, engine: Url, token: String) -> BoxFuture<Result<Player>> {
        let client = self.client(engine);
        Box::pin(async move { client?.me(&token).await })
    }

    fn sign_out(&self, engine: Url, token: String) -> BoxFuture<Result<()>> {
        let client = self.client(engine);
        Box::pin(async move { client?.sign_out(&token).await })
    }

    fn servers(&self, engine: Url, token: String) -> BoxFuture<Result<Vec<YourServer>>> {
        let client = self.client(engine);
        Box::pin(async move { client?.servers(&token).await })
    }

    fn follow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        let client = self.client(engine);
        Box::pin(async move { client?.follow(&token, &invite_token).await })
    }

    fn unfollow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        let client = self.client(engine);
        Box::pin(async move { client?.unfollow(&token, &invite_token).await })
    }

    fn joined(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        let client = self.client(engine);
        Box::pin(async move { client?.joined(&token, &invite_token).await })
    }
}

/// What the app knows about the Perland account. Every write to the session store happens
/// while this is locked, so a sign-out and a sign-in never interleave there.
#[derive(Default)]
pub(crate) struct PerlandState {
    /// The session store was read (at most once per launch).
    loaded: bool,
    session: Option<Session>,
    /// When `/me` was last asked for, so it is asked at most once a minute.
    checked: Option<Instant>,
    /// The sign-in that made this session created the player.
    new_player: bool,
    /// The Engine the last email code came from: its verify goes there too.
    email_engine: Option<Url>,
}

/// The Discord sign-in waiting for the browser, if any: one at a time.
#[derive(Default)]
pub(crate) struct SignInSlot {
    next: u64,
    current: Option<SignInAttempt>,
}

struct SignInAttempt {
    id: u64,
    /// The authorize URL, for "Open the page again"; `None` until the browser opens.
    url: Option<String>,
    cancel: watch::Sender<bool>,
}

/// Clears the slot when its sign-in ends, unless a newer one took it meanwhile.
struct AttemptGuard<'a> {
    slot: &'a Mutex<SignInSlot>,
    id: u64,
}

impl Drop for AttemptGuard<'_> {
    fn drop(&mut self) {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.current.as_ref().is_some_and(|a| a.id == self.id) {
            slot.current = None;
        }
    }
}

fn method_name(method: SignInMethod) -> &'static str {
    match method {
        SignInMethod::Discord => "discord",
        SignInMethod::Email => "email",
    }
}

fn language_tag(language: &str) -> &'static str {
    match Language::parse(language) {
        Language::Es => "es",
        Language::En => "en",
    }
}

fn account(profile: &Profile, new_player: bool) -> PerlandAccount {
    PerlandAccount {
        name: profile.name.clone(),
        email: profile.email.clone(),
        discord: profile.discord,
        new_player,
    }
}

impl PerlandStatus {
    /// Accounts are off, or the Engine can't say: nothing Perland-account related shows.
    fn off() -> Self {
        Self {
            enabled: false,
            methods: vec![],
            account: None,
        }
    }

    fn signed_out(config: &PlayerConfig) -> Self {
        if !config.enabled {
            return Self::off();
        }
        Self {
            enabled: true,
            methods: config
                .methods
                .iter()
                .map(|m| method_name(*m).into())
                .collect(),
            account: None,
        }
    }
}

/// The Engine's host, for error params (never a URL).
fn host_of(engine: &Url) -> &str {
    engine.host_str().unwrap_or("the server")
}

fn accounts_closed(engine: &Url) -> LauncherError {
    LauncherError::network(
        ErrorCode::PlayerAccountsClosed,
        "Perland accounts aren't open yet.",
    )
    .with_param("host", host_of(engine))
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The state, with the saved session read on the first call.
    async fn perland_state(&self) -> MutexGuard<'_, PerlandState> {
        let mut state = self.perland.lock().await;
        if !state.loaded {
            let store = self.perland_store.clone();
            state.session = tokio::task::spawn_blocking(move || store.load())
                .await
                .unwrap_or(None);
            state.loaded = true;
        }
        state
    }

    /// The signed-in player's session, read from the store on first use.
    pub(crate) async fn perland_session(&self) -> Option<Session> {
        self.perland_state().await.session.clone()
    }

    /// Forget the session, in memory and in the store. The caller holds the state.
    async fn forget_session(&self, state: &mut PerlandState) {
        state.session = None;
        state.checked = None;
        state.new_player = false;
        let store = self.perland_store.clone();
        // `forget` logs its own store failures (by kind, never the secret).
        if let Err(e) = tokio::task::spawn_blocking(move || store.forget()).await {
            let kind = if e.is_panic() {
                "panicked"
            } else {
                "was canceled"
            };
            tracing::warn!("forgetting the Perland sign-in {kind}");
        }
    }

    /// Forget the session if it is still the one holding `token` (a 401 answered it).
    pub(crate) async fn drop_session(&self, token: &str) {
        let mut state = self.perland.lock().await;
        if state.session.as_ref().is_some_and(|s| s.token == token) {
            tracing::info!("the Perland session ended on the Engine; signed out");
            self.forget_session(&mut state).await;
        }
    }

    /// Keep a fresher profile for the session holding `token`, so the offline view is current.
    async fn refresh_profile(&self, token: &str, profile: Profile) {
        let mut state = self.perland.lock().await;
        let Some(session) = state.session.as_mut().filter(|s| s.token == token) else {
            return;
        };
        if session.profile == profile {
            return;
        }
        session.profile = profile;
        let (store, session) = (self.perland_store.clone(), session.clone());
        if let Ok(Err(e)) = tokio::task::spawn_blocking(move || store.save(&session)).await {
            tracing::warn!(code = ?e.code, "couldn't save the Perland profile");
        }
    }

    /// Make `answer` the session, saved for the next launch when the store takes it.
    async fn adopt(&self, engine: Url, answer: TokenAnswer) -> PerlandAccount {
        let session = Session {
            engine,
            token: answer.token,
            profile: Profile::from_player(&answer.player),
        };
        let mut state = self.perland_state().await;
        let (store, saved) = (self.perland_store.clone(), session.clone());
        match tokio::task::spawn_blocking(move || store.save(&saved)).await {
            Ok(Ok(())) => {}
            // The sign-in worked: it lasts until the launcher closes, as without a store.
            Ok(Err(e)) => tracing::warn!(code = ?e.code, "couldn't save the Perland sign-in"),
            Err(_) => tracing::warn!("couldn't save the Perland sign-in"),
        }
        let shown = account(&session.profile, answer.new_player);
        state.session = Some(session);
        state.checked = Some(Instant::now());
        state.new_player = answer.new_player;
        state.email_engine = None;
        shown
    }

    /// The Perland account card. Signed out, it reads the site's Engine; signed in, the
    /// session's. `/me` is asked at most once a minute: a 401 signs the player out, any other
    /// failure keeps the session and shows the saved profile.
    pub async fn perland_status(&self) -> Result<PerlandStatus> {
        let player = self.runner.player();
        let (session, due) = {
            let mut state = self.perland_state().await;
            match state.session.clone() {
                None => (None, false),
                Some(session) => {
                    let due = state.checked.is_none_or(|at| at.elapsed() >= ME_EVERY);
                    if due {
                        state.checked = Some(Instant::now());
                    }
                    (Some(session), due)
                }
            }
        };
        let Some(session) = session else {
            let config = match player.site_engine().await {
                Ok(engine) => player.config(engine).await,
                Err(e) => Err(e),
            };
            return Ok(match config {
                Ok(config) => PerlandStatus::signed_out(&config),
                Err(e) => {
                    tracing::info!(code = ?e.code, "couldn't read Perland's sign-in config");
                    PerlandStatus::off()
                }
            });
        };
        let methods = match player.config(session.engine.clone()).await {
            Ok(config) if !config.enabled => return Ok(PerlandStatus::off()),
            Ok(config) => PerlandStatus::signed_out(&config).methods,
            Err(e) => {
                tracing::info!(code = ?e.code, "couldn't read Perland's sign-in config");
                vec![]
            }
        };
        if due {
            match player
                .me(session.engine.clone(), session.token.clone())
                .await
            {
                Ok(me) => {
                    self.refresh_profile(&session.token, Profile::from_player(&me))
                        .await
                }
                Err(e) if e.code == ErrorCode::PerlandSignedOut => {
                    self.drop_session(&session.token).await
                }
                Err(e) if e.code == ErrorCode::PlayerAccountsClosed => {
                    return Ok(PerlandStatus::off())
                }
                Err(e) => tracing::info!(code = ?e.code, "couldn't refresh the Perland profile"),
            }
        }
        let state = self.perland.lock().await;
        Ok(PerlandStatus {
            enabled: true,
            methods,
            account: state
                .session
                .as_ref()
                .map(|s| account(&s.profile, state.new_player)),
        })
    }

    /// Sign in through Discord in the player's browser. `Ok(None)` when the player canceled
    /// (on Discord's page, with Cancel, or by starting another sign-in).
    pub async fn perland_sign_in_discord(&self, language: &str) -> Result<Option<PerlandAccount>> {
        // Take the slot first, so a Cancel (or another sign-in) before the browser opens counts.
        let (guard, cancel) = self.start_sign_in();
        let player = self.runner.player();
        // A sign-in canceled while it got ready answers no error, whatever these said.
        let engine = player.site_engine().await;
        if *cancel.borrow() {
            return Ok(None);
        }
        let engine = engine?;
        let config = player.config(engine.clone()).await;
        if *cancel.borrow() {
            return Ok(None);
        }
        let config = config?;
        if !config.enabled {
            return Err(accounts_closed(&engine));
        }
        if !config.methods.contains(&SignInMethod::Discord) {
            return Err(LauncherError::network(
                ErrorCode::SignInUnavailable,
                "That way of signing in isn't available.",
            )
            .with_param("host", host_of(&engine))
            .with_param("method", "discord"));
        }
        let pkce = Pkce::new();
        let loopback = Loopback::bind().map_err(|e| {
            LauncherError::auth(ErrorCode::SignInFailed, "Couldn't listen for the sign-in.")
                .with_detail(e)
        })?;
        let redirect = loopback.redirect_uri();
        let url = authorize_url(&engine, &redirect, &pkce).to_string();
        if !self.set_sign_in_url(&guard, &url) || *cancel.borrow() {
            return Ok(None);
        }
        self.host.open_url(&url);
        self.host.emit(AppEvent::PerlandSignIn {
            phase: PerlandSignInPhase::Waiting,
        });
        let callback = loopback
            .wait(
                &pkce.state,
                Language::parse(language),
                SIGN_IN_DEADLINE,
                cancel.clone(),
            )
            .await?;
        let code = match callback {
            Callback::Code(code) => code,
            Callback::Denied | Callback::Canceled => return Ok(None),
            Callback::Failed(why) => {
                return Err(LauncherError::auth(
                    ErrorCode::SignInFailed,
                    "Couldn't sign you in. Try again.",
                )
                .with_param("host", host_of(&engine))
                .with_detail(why))
            }
        };
        if *cancel.borrow() {
            return Ok(None);
        }
        self.host.emit(AppEvent::PerlandSignIn {
            phase: PerlandSignInPhase::Exchanging,
        });
        let answer = player
            .token(engine.clone(), code, pkce.verifier.clone(), redirect)
            .await?;
        // Canceled while the code was redeemed: the player moved on, so keep nothing.
        if *cancel.borrow() {
            return Ok(None);
        }
        Ok(Some(self.adopt(engine, answer).await))
    }

    /// Take the sign-in slot for a new Discord sign-in, canceling the one before.
    fn start_sign_in(&self) -> (AttemptGuard<'_>, watch::Receiver<bool>) {
        let (cancel, canceled) = watch::channel(false);
        let mut slot = self
            .perland_sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(old) = slot.current.take() {
            old.cancel.send_replace(true);
        }
        slot.next += 1;
        let id = slot.next;
        slot.current = Some(SignInAttempt {
            id,
            url: None,
            cancel,
        });
        let guard = AttemptGuard {
            slot: &self.perland_sign_in,
            id,
        };
        (guard, canceled)
    }

    /// Record the authorize URL for "Open the page again". False when the attempt no longer
    /// holds the slot (it was canceled or replaced).
    fn set_sign_in_url(&self, guard: &AttemptGuard<'_>, url: &str) -> bool {
        let mut slot = self
            .perland_sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match slot.current.as_mut().filter(|a| a.id == guard.id) {
            Some(attempt) => {
                attempt.url = Some(url.to_string());
                true
            }
            None => false,
        }
    }

    /// Open the waiting sign-in's page again. Nothing waits, or its page isn't open yet:
    /// nothing to open.
    pub fn perland_sign_in_reopen(&self) -> Result<()> {
        let url = self
            .perland_sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current
            .as_ref()
            .and_then(|a| a.url.clone());
        if let Some(url) = url {
            self.host.open_url(&url);
        }
        Ok(())
    }

    /// Stop waiting for the browser: the sign-in answers `Ok(None)`.
    pub fn perland_sign_in_cancel(&self) {
        let current = self
            .perland_sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current
            .take();
        if let Some(attempt) = current {
            attempt.cancel.send_replace(true);
        }
    }

    /// Ask the site's Engine to email a sign-in code.
    pub async fn perland_email_start(&self, email: &str, language: &str) -> Result<EmailCodeSent> {
        let player = self.runner.player();
        let engine = player.site_engine().await?;
        let started = player
            .email_start(
                engine.clone(),
                email.trim().to_string(),
                language_tag(language).to_string(),
            )
            .await?;
        self.perland.lock().await.email_engine = Some(engine);
        Ok(EmailCodeSent {
            resend_in: started.resend_in,
        })
    }

    /// Redeem the emailed code (spaces allowed) at the Engine that sent it.
    pub async fn perland_email_verify(&self, email: &str, code: &str) -> Result<PerlandAccount> {
        // One sign-in at a time: this one wins over a Discord one waiting for the browser.
        self.perland_sign_in_cancel();
        let player = self.runner.player();
        let saved = self.perland.lock().await.email_engine.clone();
        let engine = match saved {
            Some(engine) => engine,
            None => player.site_engine().await?,
        };
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        let answer = player
            .email_verify(engine.clone(), email.trim().to_string(), code)
            .await?;
        Ok(self.adopt(engine, answer).await)
    }

    /// Sign out: stop a waiting sign-in, tell the Engine (at most 5 s, whatever it answers),
    /// then forget the session, unless a sign-in finished meanwhile and replaced it.
    pub async fn perland_sign_out(&self) -> Result<()> {
        self.perland_sign_in_cancel();
        let session = self.perland_session().await;
        let token = session.as_ref().map(|s| s.token.clone());
        if let Some(session) = session {
            let call = self
                .runner
                .player()
                .sign_out(session.engine.clone(), session.token.clone());
            match tokio::time::timeout(SIGN_OUT_LIMIT, call).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::info!(code = ?e.code, "the Engine didn't take the sign-out"),
                Err(_) => tracing::info!("the Engine didn't answer the sign-out in time"),
            }
        }
        let mut state = self.perland.lock().await;
        if state.session.as_ref().map(|s| &s.token) == token.as_ref() {
            self.forget_session(&mut state).await;
        } else {
            tracing::info!("a sign-in finished during the sign-out; it stays");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use launcher_core::{
        accounts::BuildKind,
        perland::{MemoryStore, SecretError, SecretStore},
    };

    use super::*;
    use crate::{
        app::tests::{core, core_with_secrets, TestCore},
        testing::player_token,
    };

    fn engine(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    fn saved_session(engine_url: &str) -> Session {
        Session {
            engine: engine(engine_url),
            token: player_token('s'),
            profile: Profile {
                name: "Alex".into(),
                email: Some("alex@example.com".into()),
                discord: false,
            },
        }
    }

    /// A store counting how often it is read.
    #[derive(Default)]
    struct Counting {
        inner: MemoryStore,
        loads: AtomicUsize,
    }

    impl SecretStore for Counting {
        fn load(&self) -> std::result::Result<Option<String>, SecretError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            self.inner.load()
        }
        fn save(&self, secret: &str) -> std::result::Result<(), SecretError> {
            self.inner.save(secret)
        }
        fn delete(&self) -> std::result::Result<(), SecretError> {
            self.inner.delete()
        }
    }

    /// The query value `key` of `url`.
    fn query(url: &str, key: &str) -> String {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_else(|| panic!("no {key} in the authorize URL"))
    }

    /// Plays the browser: back to the loopback with `params`.
    async fn come_back(authorize: &str, params: &str) {
        let redirect = query(authorize, "redirect_uri");
        let state = query(authorize, "state");
        let url = format!("{redirect}?{params}&state={state}");
        launcher_core::http::client("test")
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap();
    }

    /// The sign-in's answer, within 5 s.
    async fn finish(
        task: tokio::task::JoinHandle<Result<Option<PerlandAccount>>>,
    ) -> Result<Option<PerlandAccount>> {
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("the sign-in ended")
            .unwrap()
    }

    fn sign_in(core: &Arc<TestCore>) -> tokio::task::JoinHandle<Result<Option<PerlandAccount>>> {
        let core = core.clone();
        tokio::spawn(async move { core.perland_sign_in_discord("es").await })
    }

    #[tokio::test]
    async fn status_off_when_config_disabled() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        *runner.player.config.lock().unwrap() = Ok(PlayerConfig::off());
        let status = core.perland_status().await.unwrap();
        assert_eq!(
            status,
            PerlandStatus {
                enabled: false,
                methods: vec![],
                account: None
            }
        );
    }

    #[tokio::test]
    async fn status_signed_out_lists_methods() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let status = core.perland_status().await.unwrap();
        assert!(status.enabled);
        assert_eq!(status.methods, ["discord", "email"]);
        assert_eq!(status.account, None);
    }

    #[tokio::test]
    async fn email_verify_saves_session_and_returns_account() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let sent = core
            .perland_email_start(" steve@example.com ", "es-MX")
            .await
            .unwrap();
        assert_eq!(sent, EmailCodeSent { resend_in: 60 });
        let account = core
            .perland_email_verify("steve@example.com", " 123 456 ")
            .await
            .unwrap();
        assert_eq!(
            account,
            PerlandAccount {
                name: "Steve".into(),
                email: Some("steve@example.com".into()),
                discord: true,
                new_player: true,
            }
        );
        let site = engine("https://api.example.com");
        let start = &runner.player.calls_to("email_start")[0];
        assert_eq!(start.args, ["steve@example.com", "es"]);
        let verify = &runner.player.calls_to("email_verify")[0];
        assert_eq!(verify.engine, site);
        assert_eq!(verify.args, ["steve@example.com", "123456"]);
        let saved = core.perland_store.load().expect("the session is saved");
        assert_eq!(saved.engine, site);
        assert_eq!(saved.token, player_token('x'));
        assert_eq!(
            core.perland_status().await.unwrap().account,
            Some(account),
            "the status shows the new account without asking /me again"
        );
        assert!(runner.player.calls_to("me").is_empty());
    }

    #[tokio::test]
    async fn status_after_401_forgets_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        *runner.player.me.lock().unwrap() = Err(LauncherError::auth(
            ErrorCode::PerlandSignedOut,
            "You were signed out of Perland.",
        ));
        let status = core.perland_status().await.unwrap();
        assert!(status.enabled);
        assert_eq!(status.account, None);
        assert!(core.perland_store.load().is_none(), "the store is empty");
        assert!(core.perland_session().await.is_none());
    }

    #[tokio::test]
    async fn status_keeps_session_when_engine_unreachable() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        *runner.player.me.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::Offline,
            "Couldn't reach api.example.com",
        ));
        let status = core.perland_status().await.unwrap();
        let account = status.account.expect("still signed in");
        assert_eq!(account.name, "Alex");
        assert!(!account.new_player);
        assert!(core.perland_store.load().is_some(), "the session stays");
    }

    #[tokio::test]
    async fn status_reads_store_lazily_once() {
        let secret = Arc::new(Counting::default());
        let (core, runner, _host, _t) = core_with_secrets(BuildKind::Dev, secret.clone());
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        assert_eq!(
            secret.loads.load(Ordering::SeqCst),
            0,
            "nothing reads at start"
        );
        core.instances_list().await.unwrap();
        assert_eq!(secret.loads.load(Ordering::SeqCst), 0);
        let first = core.perland_status().await.unwrap();
        let second = core.perland_status().await.unwrap();
        assert_eq!(
            first.account.unwrap().name,
            "Steve",
            "/me refreshed the profile"
        );
        assert_eq!(second.account.unwrap().name, "Steve");
        assert_eq!(secret.loads.load(Ordering::SeqCst), 1, "read once");
        assert_eq!(
            runner.player.calls_to("me").len(),
            1,
            "/me at most once a minute"
        );
        assert_eq!(
            core.perland_store.load().unwrap().profile.name,
            "Steve",
            "the fresher profile is saved for showing offline"
        );
    }

    #[tokio::test]
    async fn discord_sign_in_opens_the_authorize_url_and_exchanges() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        let url = &opened[0];
        assert!(url.starts_with("https://api.example.com/api/v1/player/discord/authorize?"));
        core.perland_sign_in_reopen().unwrap();
        assert_eq!(host.opened.lock().unwrap()[1], *url, "the same page again");
        come_back(url, "code=c").await;
        let account = task.await.unwrap().unwrap().expect("signed in");
        assert_eq!(account.name, "Steve");
        let token = &runner.player.calls_to("token")[0];
        assert_eq!(token.engine, engine("https://api.example.com"));
        assert_eq!(token.args[0], "c");
        let challenge = Pkce::from_verifier(&token.args[1], "").challenge;
        assert_eq!(challenge, query(url, "code_challenge"));
        assert_eq!(token.args[2], query(url, "redirect_uri"));
        assert_eq!(core.perland_store.load().unwrap().token, player_token('x'));
        let phases: Vec<_> = host
            .events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::PerlandSignIn { phase } => Some(*phase),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            [PerlandSignInPhase::Waiting, PerlandSignInPhase::Exchanging]
        );
        core.perland_sign_in_reopen().unwrap();
        assert_eq!(host.opened.lock().unwrap().len(), 2, "nothing waits now");
    }

    #[tokio::test]
    async fn discord_sign_in_cancel_is_none() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let task = sign_in(&core);
        host.wait_for_opened(1).await;
        core.perland_sign_in_cancel();
        assert_eq!(task.await.unwrap().unwrap(), None);
        assert!(runner.player.calls_to("token").is_empty());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn starting_a_second_sign_in_cancels_the_first() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        let first = sign_in(&core);
        host.wait_for_opened(1).await;
        let second = sign_in(&core);
        let opened = host.wait_for_opened(2).await;
        assert_ne!(opened[0], opened[1], "a new state and port");
        assert_eq!(first.await.unwrap().unwrap(), None);
        come_back(&opened[1], "code=c").await;
        assert!(second.await.unwrap().unwrap().is_some());
    }

    #[tokio::test]
    async fn discord_denied_is_none() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "error=access_denied").await;
        assert_eq!(task.await.unwrap().unwrap(), None);
        assert!(runner.player.calls_to("token").is_empty());
    }

    #[tokio::test]
    async fn discord_failed_is_sign_in_failed() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "error=server_error").await;
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.code, ErrorCode::SignInFailed);
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn discord_sign_in_refused_while_accounts_are_off() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        *runner.player.config.lock().unwrap() = Ok(PlayerConfig::off());
        let error = core.perland_sign_in_discord("en").await.unwrap_err();
        assert_eq!(error.code, ErrorCode::PlayerAccountsClosed);
        assert!(host.opened.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn sign_out_forgets_even_when_engine_fails() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        *runner.player.sign_out.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::HostTrouble,
            "api.example.com had trouble",
        ));
        core.perland_sign_out().await.unwrap();
        let call = &runner.player.calls_to("sign_out")[0];
        assert_eq!(call.args, [player_token('s')]);
        assert!(core.perland_store.load().is_none());
        assert_eq!(core.perland_status().await.unwrap().account, None);
    }

    #[tokio::test]
    async fn perland_calls_use_the_session_engine() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://engine-a.example.com"))
            .unwrap();
        *runner.player.site.lock().unwrap() = Ok(engine("https://engine-b.example.com"));
        core.perland_status().await.unwrap();
        let a = engine("https://engine-a.example.com");
        assert_eq!(runner.player.calls_to("me")[0].engine, a);
        assert!(runner
            .player
            .calls
            .lock()
            .unwrap()
            .iter()
            .all(|c| c.engine == a));
    }

    #[tokio::test]
    async fn cancel_before_the_browser_opens_is_none() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let gate = runner.player.hold("config");
        let task = sign_in(&core);
        runner.player.wait_for_calls("config", 1).await;
        core.perland_sign_in_cancel();
        gate.add_permits(1);
        assert_eq!(finish(task).await.unwrap(), None);
        assert!(host.opened.lock().unwrap().is_empty(), "no browser opened");
        core.perland_sign_in_reopen().unwrap();
        assert!(host.opened.lock().unwrap().is_empty(), "nothing to reopen");
    }

    #[tokio::test]
    async fn cancel_before_the_browser_wins_over_a_failed_config() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        *runner.player.config.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::Offline,
            "Couldn't reach api.example.com",
        ));
        let gate = runner.player.hold("config");
        let task = sign_in(&core);
        runner.player.wait_for_calls("config", 1).await;
        core.perland_sign_in_cancel();
        gate.add_permits(1);
        assert_eq!(finish(task).await.unwrap(), None);
        assert!(host.opened.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_second_sign_in_before_the_browser_cancels_the_first() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let gate = runner.player.hold("config");
        let first = sign_in(&core);
        runner.player.wait_for_calls("config", 1).await;
        let second = sign_in(&core);
        runner.player.wait_for_calls("config", 2).await;
        gate.add_permits(2);
        assert_eq!(finish(first).await.unwrap(), None);
        host.wait_for_opened(1).await;
        core.perland_sign_in_cancel();
        assert_eq!(finish(second).await.unwrap(), None);
        assert_eq!(
            host.opened.lock().unwrap().len(),
            1,
            "only the second opened"
        );
    }

    #[tokio::test]
    async fn sign_out_cancels_a_waiting_sign_in() {
        let (core, _runner, host, _t) = core(BuildKind::Dev);
        let task = sign_in(&core);
        host.wait_for_opened(1).await;
        core.perland_sign_out().await.unwrap();
        assert_eq!(finish(task).await.unwrap(), None);
    }

    #[tokio::test]
    async fn sign_out_keeps_a_sign_in_that_finished_meanwhile() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        let gate = runner.player.hold("sign_out");
        let out = {
            let core = core.clone();
            tokio::spawn(async move { core.perland_sign_out().await })
        };
        runner.player.wait_for_calls("sign_out", 1).await;
        core.perland_email_start("steve@example.com", "en")
            .await
            .unwrap();
        core.perland_email_verify("steve@example.com", "123456")
            .await
            .unwrap();
        gate.add_permits(1);
        out.await.unwrap().unwrap();
        assert_eq!(
            core.perland_session().await.map(|s| s.token),
            Some(player_token('x')),
            "the new sign-in stays"
        );
        assert_eq!(core.perland_store.load().unwrap().token, player_token('x'));
    }

    #[tokio::test]
    async fn email_verify_cancels_a_waiting_discord_sign_in() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let task = sign_in(&core);
        host.wait_for_opened(1).await;
        core.perland_email_verify("steve@example.com", "123456")
            .await
            .unwrap();
        assert_eq!(finish(task).await.unwrap(), None);
        assert!(runner.player.calls_to("token").is_empty());
        assert_eq!(core.perland_store.load().unwrap().token, player_token('x'));
    }

    #[tokio::test]
    async fn engine_for_an_invite_uses_the_invite_lookup() {
        use wiremock::{
            matchers::{method, path},
            Mock, MockServer, ResponseTemplate,
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/config.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
            .mount(&server)
            .await;
        let http = launcher_core::http::client("test").unwrap();
        let player = CorePlayer::new(
            Ok(http.clone()),
            InviteClient::new(http),
            engine("https://perland.example.com"),
        );
        let link = InviteLink::parse(&format!("{}/{}", server.uri(), "x".repeat(43))).unwrap();
        let error = player.engine_for(link).await.unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::InviteSiteUnreachable,
            "an invite site without an Engine is an unreachable invite site, not a closed list"
        );
    }
}
