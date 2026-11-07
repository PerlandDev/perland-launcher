//! Sign in with Perland (sign-in and Your servers spec §6.2): the player's Perland account,
//! through the Engine the public list uses. The session, its token above all, is read from the
//! credential store only on the first Perland command, never at start, so a player who never
//! opens Accounts or Servers never touches it. Every later player call goes to the session's own
//! Engine. Nothing here logs a token, an email code, the PKCE verifier or an authorization code.
//!
//! Signing in: the one way in is the Perland page in the browser (`perland_sign_in`:
//! authorization code + S256 PKCE as Perland Launcher's first-party client, back to a loopback
//! redirect); the page offers GitHub, Discord and the email code itself. An Engine without
//! OAuth discovery shows no Perland card. A session from before the page (no OAuth client)
//! still signs out at `/player/sign-out`.
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

use launcher_core::{
    http::HttpClient,
    invite::{InviteClient, InviteLink, Url},
    perland::{
        oauth::{self, CLIENT_ID},
        oauth_authorize_url, Callback, CreateFreeServer, Discovery, EmailStarted, FreeServer,
        FreeServerSettings, FreeServerState, Language, LinkCode, Loopback, OAuthClient, Page, Pkce,
        Player, PlayerClient, PlayerConfig, PlayerStats, Profile, Session, TokenAnswer, YourServer,
    },
    ErrorCode, LauncherError, Result,
};
use tokio::sync::{watch, MutexGuard};

use crate::{
    app::AppCore,
    dto::{EmailCodeSent, PerlandAccount, PerlandSignInPhase, PerlandStatus},
    events::{AppEvent, Host},
    minecraft_link::signed_out as signed_out_error,
    runner::{BoxFuture, Runner},
};

/// The marker file under the data root that says a session exists (spec §5, Token storage).
pub const ACCOUNT_MARKER: &str = "perland-account.json";
/// How long a sign-in in the browser waits for the player to come back.
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
    /// RFC 8414 discovery; `None` on an Engine without the OAuth sign-in.
    fn discover(&self, engine: Url) -> BoxFuture<Result<Option<Discovery>>>;
    /// Redeem an OAuth code at `POST /api/v1/oauth/token`.
    fn oauth_token(
        &self,
        engine: Url,
        code: String,
        verifier: String,
        redirect: String,
    ) -> BoxFuture<Result<TokenAnswer>>;
    /// End an OAuth session at `POST /api/v1/oauth/revoke`.
    fn revoke(&self, engine: Url, token: String) -> BoxFuture<Result<()>>;
    fn me(&self, engine: Url, token: String) -> BoxFuture<Result<Player>>;
    fn sign_out(&self, engine: Url, token: String) -> BoxFuture<Result<()>>;
    fn servers(&self, engine: Url, token: String) -> BoxFuture<Result<Vec<YourServer>>>;
    fn follow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
    fn unfollow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
    fn joined(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
    fn forget(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>>;
    /// The player's free server and what they may do; `None` while the feature is off.
    fn free_server(&self, engine: Url, token: String)
        -> BoxFuture<Result<Option<FreeServerState>>>;
    fn create_free_server(
        &self,
        engine: Url,
        token: String,
        request: CreateFreeServer,
    ) -> BoxFuture<Result<FreeServer>>;
    /// The free server made from a `.mrpack` the launcher wrote from an instance.
    fn create_free_server_from_pack(
        &self,
        engine: Url,
        token: String,
        name: String,
        language: String,
        pack: Vec<u8>,
        settings: Option<FreeServerSettings>,
    ) -> BoxFuture<Result<FreeServer>>;
    fn start_free_server(&self, engine: Url, token: String) -> BoxFuture<Result<FreeServer>>;
    fn delete_free_server(&self, engine: Url, token: String) -> BoxFuture<Result<bool>>;
    fn link_code(&self, engine: Url, token: String) -> BoxFuture<Result<LinkCode>>;
    fn unlink(&self, engine: Url, token: String) -> BoxFuture<Result<()>>;
    fn stats(&self, engine: Url, token: String) -> BoxFuture<Result<Option<PlayerStats>>>;
    /// Email a one-time code that adds `email` to the signed-in account.
    fn account_email_start(
        &self,
        engine: Url,
        token: String,
        email: String,
        language: String,
    ) -> BoxFuture<Result<EmailStarted>>;
    /// Check the emailed code; the answer is the verified email.
    fn account_email_verify(
        &self,
        engine: Url,
        token: String,
        email: String,
        code: String,
    ) -> BoxFuture<Result<String>>;
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

    fn oauth(&self, engine: Url) -> Result<OAuthClient> {
        Ok(OAuthClient::new(self.http.clone()?, engine))
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

    fn discover(&self, engine: Url) -> BoxFuture<Result<Option<Discovery>>> {
        let client = self.oauth(engine);
        Box::pin(async move { client?.discover().await })
    }

    fn oauth_token(
        &self,
        engine: Url,
        code: String,
        verifier: String,
        redirect: String,
    ) -> BoxFuture<Result<TokenAnswer>> {
        let client = self.oauth(engine);
        Box::pin(async move { client?.token(&code, &verifier, &redirect).await })
    }

    fn revoke(&self, engine: Url, token: String) -> BoxFuture<Result<()>> {
        let client = self.oauth(engine);
        Box::pin(async move { client?.revoke(&token).await })
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

    fn forget(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        let client = self.client(engine);
        Box::pin(async move { client?.forget(&token, &invite_token).await })
    }

    fn free_server(
        &self,
        engine: Url,
        token: String,
    ) -> BoxFuture<Result<Option<FreeServerState>>> {
        let client = self.client(engine);
        Box::pin(async move { client?.free_server(&token).await })
    }

    fn create_free_server(
        &self,
        engine: Url,
        token: String,
        request: CreateFreeServer,
    ) -> BoxFuture<Result<FreeServer>> {
        let client = self.client(engine);
        Box::pin(async move { client?.create_free_server(&token, &request).await })
    }

    fn create_free_server_from_pack(
        &self,
        engine: Url,
        token: String,
        name: String,
        language: String,
        pack: Vec<u8>,
        settings: Option<FreeServerSettings>,
    ) -> BoxFuture<Result<FreeServer>> {
        let client = self.client(engine);
        Box::pin(async move {
            client?
                .create_free_server_from_pack(&token, &name, &language, pack, settings)
                .await
        })
    }

    fn start_free_server(&self, engine: Url, token: String) -> BoxFuture<Result<FreeServer>> {
        let client = self.client(engine);
        Box::pin(async move { client?.start_free_server(&token).await })
    }

    fn delete_free_server(&self, engine: Url, token: String) -> BoxFuture<Result<bool>> {
        let client = self.client(engine);
        Box::pin(async move { client?.delete_free_server(&token).await })
    }

    fn link_code(&self, engine: Url, token: String) -> BoxFuture<Result<LinkCode>> {
        let client = self.client(engine);
        Box::pin(async move { client?.link_code(&token).await })
    }

    fn unlink(&self, engine: Url, token: String) -> BoxFuture<Result<()>> {
        let client = self.client(engine);
        Box::pin(async move { client?.unlink(&token).await })
    }

    fn stats(&self, engine: Url, token: String) -> BoxFuture<Result<Option<PlayerStats>>> {
        let client = self.client(engine);
        Box::pin(async move { client?.stats(&token).await })
    }

    fn account_email_start(
        &self,
        engine: Url,
        token: String,
        email: String,
        language: String,
    ) -> BoxFuture<Result<EmailStarted>> {
        let client = self.client(engine);
        Box::pin(async move { client?.account_email_start(&token, &email, &language).await })
    }

    fn account_email_verify(
        &self,
        engine: Url,
        token: String,
        email: String,
        code: String,
    ) -> BoxFuture<Result<String>> {
        let client = self.client(engine);
        Box::pin(async move { client?.account_email_verify(&token, &email, &code).await })
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
}

/// The sign-in waiting for the browser, if any: one at a time.
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

pub(crate) fn language_tag(language: &str) -> &'static str {
    match Language::parse(language) {
        Language::Es => "es",
        Language::En => "en",
    }
}

/// The name the player picked (Discord- or GitHub-based), or `""`: `Profile.name` falls back to the email,
/// and an email must never become a server name on a public page.
fn display_name(profile: &Profile) -> String {
    if profile.email.as_deref() == Some(profile.name.as_str()) {
        String::new()
    } else {
        profile.name.clone()
    }
}

fn account(profile: &Profile, new_player: bool) -> PerlandAccount {
    PerlandAccount {
        name: profile.name.clone(),
        display_name: display_name(profile),
        email: profile.email.clone(),
        discord: profile.discord,
        github: profile.github,
        new_player,
        minecraft: profile.minecraft.clone(),
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

    /// Signed out: the Perland page (`perland`) when accounts are on and the Engine answered
    /// discovery (`oauth`), else off: a "Sign in" with nothing behind it would be a dead end.
    fn signed_out(config: &PlayerConfig, oauth: bool) -> Self {
        if !config.enabled || !oauth {
            return Self::off();
        }
        Self {
            enabled: true,
            methods: vec![PERLAND_PAGE.into()],
            account: None,
        }
    }
}

/// The method name of the OAuth sign-in on the Perland page, where the player picks GitHub,
/// Discord or an email code.
const PERLAND_PAGE: &str = "perland";

/// The Engine's host, for error params (never a URL).
pub(crate) fn host_of(engine: &Url) -> &str {
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
        self.jwts.clear();
        // The console's stream follows a server of this session's player.
        self.stop_console();
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
    pub(crate) async fn refresh_profile(&self, token: &str, profile: Profile) {
        self.edit_profile(token, |saved| *saved = profile).await
    }

    /// Change the profile of the session holding `token`, and save it when it changed.
    pub(crate) async fn edit_profile(&self, token: &str, edit: impl FnOnce(&mut Profile)) {
        let mut state = self.perland.lock().await;
        let Some(session) = state.session.as_mut().filter(|s| s.token == token) else {
            return;
        };
        let mut profile = session.profile.clone();
        edit(&mut profile);
        if session.profile == profile {
            return;
        }
        session.profile = profile;
        let (store, session) = (self.perland_store.clone(), session.clone());
        if let Ok(Err(e)) = tokio::task::spawn_blocking(move || store.save(&session)).await {
            tracing::warn!(code = ?e.code, "couldn't save the Perland profile");
        }
    }

    /// End a session nobody keeps (a sign-in canceled after its exchange, or one a newer
    /// sign-in replaced), so it doesn't hold one of the player's sessions on the Engine for
    /// months. In the background, at most `SIGN_OUT_LIMIT`, whatever the Engine answers: it
    /// never holds up the caller.
    fn discard_token(&self, engine: Url, token: String, client: Option<String>) {
        let call = self.end_session(engine, token, client.as_deref());
        tokio::spawn(async move {
            match tokio::time::timeout(SIGN_OUT_LIMIT, call).await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    tracing::info!(code = ?e.code, "the Engine didn't take a discarded sign-in's sign-out")
                }
                Err(_) => tracing::info!(
                    "the Engine didn't answer a discarded sign-in's sign-out in time"
                ),
            }
        });
    }

    /// End a session on its Engine: an OAuth one is revoked, an older one signed out.
    fn end_session(
        &self,
        engine: Url,
        token: String,
        client: Option<&str>,
    ) -> BoxFuture<Result<()>> {
        let player = self.runner.player();
        match client {
            Some(_) => player.revoke(engine, token),
            None => player.sign_out(engine, token),
        }
    }

    /// Count a `/me` the session holding `token` just got as `perland_status`'s once-a-minute
    /// one.
    pub(crate) async fn mark_checked(&self, token: &str) {
        let mut state = self.perland.lock().await;
        if state.session.as_ref().is_some_and(|s| s.token == token) {
            state.checked = Some(Instant::now());
        }
    }

    /// Make `answer` the session, saved for the next launch when the store takes it.
    /// `client` is the OAuth client the token was issued to (`None` only for a session saved
    /// by an older launcher).
    pub(crate) async fn adopt(
        &self,
        engine: Url,
        answer: TokenAnswer,
        client: Option<String>,
    ) -> PerlandAccount {
        let session = Session {
            engine,
            token: answer.token,
            profile: Profile::from_player(&answer.player),
            client,
        };
        let mut state = self.perland_state().await;
        // A session this one replaces is still live on its Engine: end it there.
        if let Some(old) = state.session.take().filter(|s| s.token != session.token) {
            self.discard_token(old.engine, old.token, old.client);
        }
        let (store, saved) = (self.perland_store.clone(), session.clone());
        match tokio::task::spawn_blocking(move || store.save(&saved)).await {
            Ok(Ok(())) => {}
            // The sign-in worked: it lasts until the launcher closes, as without a store.
            Ok(Err(e)) => tracing::warn!(code = ?e.code, "couldn't save the Perland sign-in"),
            Err(_) => tracing::warn!("couldn't save the Perland sign-in"),
        }
        let shown = account(&session.profile, answer.new_player);
        state.session = Some(session);
        // JWTs minted from the session this one replaced are not this player's to use, nor
        // is a console stream it opened.
        self.jwts.clear();
        self.stop_console();
        state.checked = Some(Instant::now());
        state.new_player = answer.new_player;
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
            let engine = match player.site_engine().await {
                Ok(engine) => engine,
                Err(e) => {
                    tracing::info!(code = ?e.code, "couldn't find Perland's Engine");
                    return Ok(PerlandStatus::off());
                }
            };
            return Ok(match player.config(engine.clone()).await {
                Ok(config) => self.signed_out_status(&engine, &config).await,
                Err(e) => {
                    tracing::info!(code = ?e.code, "couldn't read Perland's sign-in config");
                    PerlandStatus::off()
                }
            });
        };
        let methods = match player.config(session.engine.clone()).await {
            Ok(config) if !config.enabled => return Ok(PerlandStatus::off()),
            Ok(config) => {
                self.signed_out_status(&session.engine, &config)
                    .await
                    .methods
            }
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

    /// Email a code that adds `email` to the signed-in Perland account.
    pub async fn perland_account_email_start(
        &self,
        email: &str,
        language: &str,
    ) -> Result<EmailCodeSent> {
        let session = self.signed_in().await?;
        let answer = self
            .runner
            .player()
            .account_email_start(
                session.engine.clone(),
                session.token.clone(),
                email.trim().to_string(),
                language_tag(language).to_string(),
            )
            .await;
        let started = self.forget_on_401(&session, answer).await?;
        Ok(EmailCodeSent {
            resend_in: started.resend_in,
        })
    }

    /// Check the emailed code (spaces ignored). The account then has the verified email, in the
    /// saved profile too.
    pub async fn perland_account_email_verify(
        &self,
        email: &str,
        code: &str,
    ) -> Result<PerlandAccount> {
        let session = self.signed_in().await?;
        let code: String = code.chars().filter(|c| !c.is_whitespace()).collect();
        let answer = self
            .runner
            .player()
            .account_email_verify(
                session.engine.clone(),
                session.token.clone(),
                email.trim().to_string(),
                code,
            )
            .await;
        let verified = self.forget_on_401(&session, answer).await?;
        self.edit_profile(&session.token, |profile| {
            // `Profile.name` falls back to the email until a name is picked.
            if profile.email.as_deref() == Some(profile.name.as_str()) {
                profile.name = verified.clone();
            }
            profile.email = Some(verified.clone());
        })
        .await;
        let state = self.perland.lock().await;
        match state.session.as_ref().filter(|s| s.token == session.token) {
            Some(s) => Ok(account(&s.profile, state.new_player)),
            None => Err(signed_out_error()),
        }
    }

    /// What a signed-out card offers: the Perland page when the Engine answers discovery,
    /// else nothing.
    async fn signed_out_status(&self, engine: &Url, config: &PlayerConfig) -> PerlandStatus {
        let oauth = config.enabled && self.discovery(engine).await.is_some();
        PerlandStatus::signed_out(config, oauth)
    }

    /// The Engine's OAuth discovery. A failure is no OAuth (logged): no Perland sign-in is
    /// offered until it answers.
    async fn discovery(&self, engine: &Url) -> Option<Discovery> {
        match self.runner.player().discover(engine.clone()).await {
            Ok(found) => found,
            Err(e) => {
                tracing::info!(code = ?e.code, "couldn't read Perland's OAuth discovery");
                None
            }
        }
    }

    /// Sign in on the Perland page in the player's browser (OAuth code + PKCE), where the
    /// player picks GitHub, Discord or an email code. `Ok(None)` when the player canceled
    /// here (Cancel, or another sign-in); canceled on the page is `sign_in_canceled`. Refused
    /// (`sign_in_unavailable`) on an Engine without discovery.
    pub async fn perland_sign_in(&self, language: &str) -> Result<Option<PerlandAccount>> {
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
        let found = self.discovery(&engine).await;
        if *cancel.borrow() {
            return Ok(None);
        }
        let discovery = found.ok_or_else(|| {
            LauncherError::network(
                ErrorCode::SignInUnavailable,
                "That way of signing in isn't available.",
            )
            .with_param("host", host_of(&engine))
            .with_param("method", PERLAND_PAGE)
        })?;
        let pkce = Pkce::new();
        let loopback = Loopback::bind().map_err(|e| {
            LauncherError::auth(ErrorCode::SignInFailed, "Couldn't listen for the sign-in.")
                .with_detail(e)
        })?;
        let redirect = loopback.redirect_uri();
        let url = oauth_authorize_url(&discovery, &redirect, &pkce, language).to_string();
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
                Page::Success,
                SIGN_IN_DEADLINE,
                cancel.clone(),
            )
            .await?;
        let code = match callback {
            Callback::Code(code) => code,
            // Canceled on the Perland page: the card says so, apart from a Cancel here.
            Callback::Denied => return Err(oauth::sign_in_canceled(host_of(&engine))),
            Callback::Canceled => return Ok(None),
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
            .oauth_token(engine.clone(), code, pkce.verifier.clone(), redirect)
            .await?;
        let client = Some(CLIENT_ID.to_string());
        // Canceled while the code was redeemed: the player moved on, so keep nothing, and end
        // the session the Engine just made.
        if *cancel.borrow() {
            self.discard_token(engine, answer.token, client);
            return Ok(None);
        }
        Ok(Some(self.adopt(engine, answer, client).await))
    }

    /// Take the sign-in slot for a new sign-in in the browser, canceling the one before.
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

    /// Manage in Perland: the panel's page for `stack`, a server the player owns, in the
    /// browser. Refused without a configured panel (the UI then offers no button) or a name.
    pub fn perland_manage(&self, stack: &str) -> Result<()> {
        let refused = || LauncherError::pack(ErrorCode::LinkRefused, "That link can't be opened");
        let panel = self.config.panel.as_ref().ok_or_else(refused)?;
        if stack.trim().is_empty() {
            return Err(refused());
        }
        self.host
            .open_url(launcher_core::perland::manage_url(panel, stack).as_str());
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

    /// Sign out: stop a waiting sign-in, tell the Engine (at most 5 s, whatever it answers),
    /// then forget the session, unless a sign-in finished meanwhile and replaced it.
    pub async fn perland_sign_out(&self) -> Result<()> {
        self.perland_sign_in_cancel();
        let session = self.perland_session().await;
        let token = session.as_ref().map(|s| s.token.clone());
        if let Some(session) = session {
            let call = self.end_session(
                session.engine.clone(),
                session.token.clone(),
                session.client.as_deref(),
            );
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
        perland::{MemoryStore, SecretError, SecretStore, SignInMethod},
    };

    use super::*;
    use crate::{
        app::tests::{core, core_with_panel, core_with_secrets, TestCore},
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
                github: false,
                minecraft: None,
            },
            client: None,
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
        tokio::spawn(async move { core.perland_sign_in("es").await })
    }

    /// A sign-in on the Perland page that just finished: the fake Engine's answer, adopted.
    async fn signed_in_again(
        core: &TestCore,
        runner: &crate::testing::FakeRunner,
    ) -> PerlandAccount {
        let answer = runner.player.token_answer.lock().unwrap().clone().unwrap();
        core.adopt(
            engine("https://api.example.com"),
            answer,
            Some(CLIENT_ID.into()),
        )
        .await
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
    async fn no_discovery_means_no_card() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        *runner.player.config.lock().unwrap() = Ok(PlayerConfig {
            enabled: true,
            methods: vec![SignInMethod::Discord, SignInMethod::Email],
        });
        *runner.player.discovery.lock().unwrap() = Ok(None);
        let status = core.perland_status().await.unwrap();
        assert!(!status.enabled, "no Perland page: no way in, no card");
        assert!(status.methods.is_empty());
        assert_eq!(status.account, None);
    }

    #[tokio::test]
    async fn legacy_session_still_signs_out_at_player_sign_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        // A session from 0.2.x: no OAuth client.
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        core.perland_sign_out().await.unwrap();
        let call = &runner.player.calls_to("sign_out");
        assert_eq!(call.len(), 1);
        assert_eq!(call[0].engine, engine("https://api.example.com"));
        assert_eq!(call[0].args, [player_token('s')]);
        assert!(runner.player.calls_to("revoke").is_empty());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn status_signed_out_lists_methods() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let status = core.perland_status().await.unwrap();
        assert!(status.enabled);
        assert_eq!(status.methods, ["perland"]);
        assert_eq!(status.account, None);
    }

    #[tokio::test]
    async fn status_keeps_the_account_without_discovery() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        *runner.player.discovery.lock().unwrap() = Ok(None);
        let status = core.perland_status().await.unwrap();
        assert!(status.enabled);
        assert!(status.methods.is_empty());
        assert_eq!(status.account.expect("still signed in").name, "Steve");
    }

    #[test]
    fn display_name_is_never_the_email() {
        let profile = |name: &str, email: Option<&str>| Profile {
            name: name.into(),
            email: email.map(Into::into),
            discord: false,
            github: false,
            minecraft: None,
        };
        let shown = |p: Profile| account(&p, false).display_name;
        assert_eq!(shown(profile("Alex", Some("alex@example.com"))), "Alex");
        assert_eq!(shown(profile("Alex", None)), "Alex");
        assert_eq!(
            shown(profile("alex@example.com", Some("alex@example.com"))),
            "",
            "an email-only player has no display name"
        );
        assert_eq!(shown(profile("", None)), "");
        let full = account(
            &profile("alex@example.com", Some("alex@example.com")),
            false,
        );
        assert_eq!(
            full.name, "alex@example.com",
            "the account card still names them"
        );
    }

    #[test]
    fn a_github_account_is_named_by_its_login() {
        let player = Player {
            github: Some(launcher_core::perland::GitHubAccount {
                login: "octocat".into(),
                avatar_url: String::new(),
            }),
            ..Player::default()
        };
        let shown = account(&Profile::from_player(&player), false);
        assert_eq!(
            (shown.name.as_str(), shown.display_name.as_str()),
            ("octocat", "octocat")
        );
        assert!(shown.github && !shown.discord);
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

    fn panel_discovery() -> Discovery {
        Discovery {
            authorization_endpoint: engine("https://panel.example.com/oauth/authorize"),
        }
    }

    /// The Engine answers discovery: the OAuth sign-in is the way in.
    fn with_oauth(runner: &crate::testing::FakeRunner) {
        *runner.player.discovery.lock().unwrap() = Ok(Some(panel_discovery()));
    }

    fn oauth_sign_in(
        core: &Arc<TestCore>,
    ) -> tokio::task::JoinHandle<Result<Option<PerlandAccount>>> {
        let core = core.clone();
        tokio::spawn(async move { core.perland_sign_in("es-MX").await })
    }

    #[tokio::test]
    async fn status_offers_the_perland_page_when_discovery_answers() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        let status = core.perland_status().await.unwrap();
        assert!(status.enabled);
        assert_eq!(status.methods, ["perland"]);
        // Even an Engine that lists no old method: the page offers its own.
        *runner.player.config.lock().unwrap() = Ok(PlayerConfig {
            enabled: true,
            methods: vec![],
        });
        assert_eq!(core.perland_status().await.unwrap().methods, ["perland"]);
        // Accounts closed stays closed.
        *runner.player.config.lock().unwrap() = Ok(PlayerConfig::off());
        assert!(!core.perland_status().await.unwrap().enabled);
    }

    #[tokio::test]
    async fn oauth_sign_in_opens_the_panel_page_and_redeems_at_oauth_token() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        let task = oauth_sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        let url = &opened[0];
        assert!(
            url.starts_with("https://panel.example.com/oauth/authorize?response_type=code&"),
            "{url}"
        );
        assert_eq!(query(url, "client_id"), CLIENT_ID);
        assert_eq!(query(url, "scope"), "launcher");
        assert_eq!(query(url, "ui_locales"), "es");
        assert_eq!(query(url, "code_challenge_method"), "S256");
        assert!(query(url, "redirect_uri").starts_with("http://127.0.0.1:"));
        assert!(query(url, "redirect_uri").ends_with("/callback"));
        core.perland_sign_in_reopen().unwrap();
        assert_eq!(host.opened.lock().unwrap()[1], *url, "the same page again");
        come_back(url, "code=c").await;
        let account = finish(task).await.unwrap().expect("signed in");
        assert_eq!(account.name, "Steve");
        let token = &runner.player.calls_to("oauth_token")[0];
        assert_eq!(
            token.engine,
            engine("https://api.example.com"),
            "redeemed at the Engine, never the panel"
        );
        assert_eq!(token.args[0], "c");
        let challenge = Pkce::from_verifier(&token.args[1], "").challenge;
        assert_eq!(challenge, query(url, "code_challenge"));
        assert_eq!(token.args[2], query(url, "redirect_uri"), "exactly as sent");
        let saved = core.perland_store.load().unwrap();
        assert_eq!(saved.token, player_token('x'));
        assert_eq!(saved.client.as_deref(), Some(CLIENT_ID));
    }

    #[tokio::test]
    async fn oauth_sign_in_canceled_on_the_page_says_so() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        let task = oauth_sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "error=access_denied").await;
        let e = finish(task).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::SignInCanceled);
        assert!(runner.player.calls_to("oauth_token").is_empty());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn oauth_sign_in_page_error_fails() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        let task = oauth_sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "error=temporarily_unavailable").await;
        let e = finish(task).await.unwrap_err();
        assert_eq!(e.code, ErrorCode::SignInFailed);
        assert_eq!(e.detail.as_deref(), Some("temporarily_unavailable"));
        assert!(runner.player.calls_to("oauth_token").is_empty());
    }

    #[tokio::test]
    async fn oauth_sign_in_cancel_here_is_none() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        let task = oauth_sign_in(&core);
        host.wait_for_opened(1).await;
        core.perland_sign_in_cancel();
        assert_eq!(finish(task).await.unwrap(), None);
        assert!(runner.player.calls_to("oauth_token").is_empty());
    }

    #[tokio::test]
    async fn oauth_token_refusal_is_the_error() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        *runner.player.token_answer.lock().unwrap() = Err(
            launcher_core::perland::oauth::sign_in_expired("api.example.com"),
        );
        let task = oauth_sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c").await;
        assert_eq!(
            finish(task).await.unwrap_err().code,
            ErrorCode::SignInExpired
        );
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn oauth_sign_in_without_discovery_is_unavailable() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        *runner.player.discovery.lock().unwrap() = Ok(None);
        let e = core.perland_sign_in("en").await.unwrap_err();
        assert_eq!(e.code, ErrorCode::SignInUnavailable);
        assert_eq!(e.params.get("method").map(String::as_str), Some("perland"));
        assert!(host.opened.lock().unwrap().is_empty());
        assert!(runner.player.calls_to("oauth_token").is_empty());
    }

    #[tokio::test]
    async fn sign_out_of_an_oauth_session_revokes_it() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&Session {
                client: Some(CLIENT_ID.into()),
                ..saved_session("https://api.example.com")
            })
            .unwrap();
        core.perland_sign_out().await.unwrap();
        let revoke = &runner.player.calls_to("revoke")[0];
        assert_eq!(revoke.engine, engine("https://api.example.com"));
        assert_eq!(revoke.args, [player_token('s')]);
        assert!(runner.player.calls_to("sign_out").is_empty());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn sign_out_forgets_an_oauth_session_even_when_revoke_fails() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&Session {
                client: Some(CLIENT_ID.into()),
                ..saved_session("https://api.example.com")
            })
            .unwrap();
        *runner.player.revoke.lock().unwrap() =
            Err(LauncherError::network(ErrorCode::HostTrouble, "trouble"));
        core.perland_sign_out().await.unwrap();
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn an_oauth_sign_in_replacing_a_session_ends_the_old_one() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        with_oauth(&runner);
        core.perland_store
            .save(&Session {
                client: Some(CLIENT_ID.into()),
                ..saved_session("https://api.example.com")
            })
            .unwrap();
        let task = oauth_sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c").await;
        finish(task).await.unwrap().expect("signed in");
        runner.player.wait_for_calls("revoke", 1).await;
        assert_eq!(
            runner.player.calls_to("revoke")[0].args,
            [player_token('s')]
        );
    }

    #[test]
    fn manage_opens_the_stack_page_in_the_panel() {
        let (core, _, host, _tmp) = core_with_panel("https://panel.example.com/");
        core.perland_manage("my world").unwrap();
        assert_eq!(
            *host.opened.lock().unwrap(),
            ["https://panel.example.com/stacks/my%20world"]
        );
    }

    #[test]
    fn manage_without_a_panel_or_a_name_opens_nothing() {
        let (without, _, host, _t1) = core(BuildKind::Dev);
        let e = without.perland_manage("survival").unwrap_err();
        assert_eq!(e.code, ErrorCode::LinkRefused);
        let (with, _, panel_host, _t2) = core_with_panel("https://panel.example.com");
        let e = with.perland_manage("  ").unwrap_err();
        assert_eq!(e.code, ErrorCode::LinkRefused);
        assert!(host.opened.lock().unwrap().is_empty());
        assert!(panel_host.opened.lock().unwrap().is_empty());
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
    async fn sign_in_refused_while_accounts_are_off() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        *runner.player.config.lock().unwrap() = Ok(PlayerConfig::off());
        let error = core.perland_sign_in("en").await.unwrap_err();
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
        signed_in_again(&core, &runner).await;
        // The sign-out, and the replaced session's own one from the new sign-in.
        gate.add_permits(2);
        out.await.unwrap().unwrap();
        assert_eq!(
            core.perland_session().await.map(|s| s.token),
            Some(player_token('x')),
            "the new sign-in stays"
        );
        assert_eq!(core.perland_store.load().unwrap().token, player_token('x'));
    }

    #[tokio::test]
    async fn cancel_during_the_exchange_revokes_the_discarded_token() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        let gate = runner.player.hold("oauth_token");
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c").await;
        runner.player.wait_for_calls("oauth_token", 1).await;
        core.perland_sign_in_cancel();
        gate.add_permits(1);
        assert_eq!(finish(task).await.unwrap(), None);
        assert!(core.perland_store.load().is_none(), "nothing kept");
        runner.player.wait_for_calls("revoke", 1).await;
        let call = &runner.player.calls_to("revoke")[0];
        assert_eq!(call.engine, engine("https://api.example.com"));
        assert_eq!(call.args, [player_token('x')], "the token nobody keeps");
    }

    #[tokio::test]
    async fn a_sign_in_over_a_session_signs_the_old_token_out_without_waiting() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://engine-a.example.com"))
            .unwrap();
        core.perland_session().await.expect("signed in");
        // The Engine never answers the sign-out: the new sign-in still finishes at once.
        let _gate = runner.player.hold("sign_out");
        let account = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            signed_in_again(&core, &runner),
        )
        .await
        .expect("the sign-in doesn't wait for the old sign-out");
        assert_eq!(account.name, "Steve");
        assert_eq!(core.perland_store.load().unwrap().token, player_token('x'));
        runner.player.wait_for_calls("sign_out", 1).await;
        let call = &runner.player.calls_to("sign_out")[0];
        assert_eq!(call.engine, engine("https://engine-a.example.com"));
        assert_eq!(call.args, [player_token('s')]);
    }

    #[tokio::test]
    async fn a_sign_in_with_the_same_token_signs_nothing_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let mut same = saved_session("https://api.example.com");
        same.token = player_token('x');
        core.perland_store.save(&same).unwrap();
        core.perland_session().await.expect("signed in");
        signed_in_again(&core, &runner).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(runner.player.calls_to("sign_out").is_empty());
        assert!(runner.player.calls_to("revoke").is_empty());
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

    #[tokio::test]
    async fn account_email_start_needs_a_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let err = core
            .perland_account_email_start("a@b.co", "en")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::PerlandSignedOut);
        assert!(runner.player.calls_to("account_email_start").is_empty());
    }

    #[tokio::test]
    async fn account_email_start_sends_the_language_tag_and_answers_the_wait() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        let sent = core
            .perland_account_email_start(" new@example.com ", "es-MX")
            .await
            .unwrap();
        assert_eq!(sent.resend_in, 60);
        let call = &runner.player.calls_to("account_email_start")[0];
        assert_eq!(
            call.args,
            [player_token('s'), "new@example.com".into(), "es".into()]
        );
    }

    #[tokio::test]
    async fn account_email_verify_strips_spaces_and_updates_the_profile() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        let mut me = crate::testing::sample_player();
        me.email = Some("new@example.com".into());
        *runner.player.me.lock().unwrap() = Ok(me);
        let account = core
            .perland_account_email_verify("new@example.com", "123 456")
            .await
            .unwrap();
        assert_eq!(account.email.as_deref(), Some("new@example.com"));
        let call = &runner.player.calls_to("account_email_verify")[0];
        assert_eq!(call.args[2], "123456");
        let saved = core.perland_store.load().unwrap();
        assert_eq!(saved.profile.email.as_deref(), Some("new@example.com"));
        let status = core.perland_status().await.unwrap();
        assert_eq!(
            status.account.unwrap().email.as_deref(),
            Some("new@example.com")
        );
    }

    #[tokio::test]
    async fn account_email_verify_401_forgets_the_session() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        core.perland_store
            .save(&saved_session("https://api.example.com"))
            .unwrap();
        *runner.player.email_verified.lock().unwrap() = Err(LauncherError::auth(
            ErrorCode::PerlandSignedOut,
            "You were signed out of Perland.",
        ));
        let err = core
            .perland_account_email_verify("new@example.com", "123456")
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::PerlandSignedOut);
        assert!(core.perland_store.load().is_none());
        assert!(core.perland_session().await.is_none());
    }
}
