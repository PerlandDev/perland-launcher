//! Sign in with Microsoft (Microsoft sign-in spec): the browser sign-in, each step of the chain
//! reported to the window as it runs, and what the app keeps of a signed-in account: the
//! refresh token in the credential store, the Minecraft token and xuid in memory only. Every
//! sign-in runs the whole chain; nothing is remembered that would skip a step.
//! Nothing here logs or formats a token, an authorization code or the PKCE verifier.
use std::sync::Mutex;

use launcher_core::{
    accounts::{visible, Account, AccountsFile, Provider, Session},
    http::HttpClient,
    msa::{
        minecraft::{dashed, ownership, McProfile, McToken, SignedIn},
        secrets::{delete_refresh, load_refresh, save_refresh},
        MsaClient, MsaEndpoints, MsaStep,
    },
    perland::{Callback, Language, Loopback, Page, Pkce, SecretError},
    ErrorCode, LauncherError, Result,
};
use tokio::sync::watch;

use crate::{
    app::AppCore,
    dto::{AccountReady, PerlandSignInPhase},
    events::{AppEvent, Host},
    perland::SIGN_IN_DEADLINE,
    restore::now_secs,
    runner::{BoxFuture, Runner},
};

/// Microsoft, Xbox and Minecraft services. The real one is `CoreMsa`; tests use `FakeMsa`.
pub trait MsaApi: Send + Sync + 'static {
    /// The Azure app this build signs in with; `None` when the build has none.
    fn client_id(&self) -> Option<String>;
    /// The page the browser opens to sign in.
    fn authorize_url(&self, redirect: &str, pkce: &Pkce) -> Result<String>;
    /// Redeem the code the browser brought back and run the whole chain, reporting each step
    /// to `step` as it starts.
    fn redeem(
        &self,
        code: String,
        verifier: String,
        redirect: String,
        step: StepReport,
    ) -> BoxFuture<Result<SignedIn>>;
    /// Trade a refresh token for a new one and run the whole chain.
    fn refresh(&self, refresh_token: String) -> BoxFuture<Result<SignedIn>>;
    /// The Minecraft profile the token belongs to; `None` when it owns no profile.
    fn profile(&self, token: McToken) -> BoxFuture<Result<Option<McProfile>>>;
}

/// Where a sign-in reports each step of the chain as it starts.
pub type StepReport = std::sync::Arc<dyn Fn(MsaStep) + Send + Sync>;

/// The refusal of a build that has no Microsoft client id.
pub(crate) fn not_configured() -> LauncherError {
    LauncherError::auth(
        ErrorCode::MicrosoftNotConfigured,
        "Microsoft sign-in isn't set up in this build.",
    )
}

/// A Minecraft token with less time than this left is refreshed before a launch.
const MIN_TOKEN_LIFE: u64 = 3600;

/// A launch whose Microsoft sign-in can't be used any more.
fn sign_in_again() -> LauncherError {
    LauncherError::auth(
        ErrorCode::MicrosoftSignInAgain,
        "Sign in with Microsoft again to keep playing.",
    )
    .with_fix("Sign in")
}

/// `MsaApi` over `launcher_core::msa::MsaClient`.
pub struct CoreMsa {
    client: Option<MsaClient>,
    client_id: Option<String>,
}

impl CoreMsa {
    /// `http` should never follow a redirect (one would replay a code or refresh token to
    /// another host). Without `client_id` every call answers `microsoft_not_configured`.
    pub fn new(http: HttpClient, endpoints: MsaEndpoints, client_id: Option<String>) -> Self {
        Self {
            client: client_id
                .clone()
                .map(|id| MsaClient::new(http, endpoints, id)),
            client_id,
        }
    }

    fn client(&self) -> Result<MsaClient> {
        self.client.clone().ok_or_else(not_configured)
    }
}

impl MsaApi for CoreMsa {
    fn client_id(&self) -> Option<String> {
        self.client_id.clone()
    }

    fn authorize_url(&self, redirect: &str, pkce: &Pkce) -> Result<String> {
        Ok(self.client()?.authorize_url(redirect, pkce).to_string())
    }

    fn redeem(
        &self,
        code: String,
        verifier: String,
        redirect: String,
        step: StepReport,
    ) -> BoxFuture<Result<SignedIn>> {
        let client = self.client();
        Box::pin(async move {
            let client = client?;
            step(MsaStep::Microsoft);
            let tokens = client.redeem_code(&code, &verifier, &redirect).await?;
            client.finish_with(tokens, now_secs(), &*step).await
        })
    }

    fn refresh(&self, refresh_token: String) -> BoxFuture<Result<SignedIn>> {
        let client = self.client();
        Box::pin(async move {
            let client = client?;
            let tokens = client.refresh(&refresh_token).await?;
            client.finish(tokens, now_secs()).await
        })
    }

    fn profile(&self, token: McToken) -> BoxFuture<Result<Option<McProfile>>> {
        let client = self.client();
        Box::pin(async move { client?.profile(&token).await })
    }
}

/// The waiting Microsoft sign-in: clears the slot when it ends, unless a newer one took it.
struct MsaAttempt<'a> {
    slot: &'a Mutex<Option<watch::Sender<bool>>>,
    mine: watch::Sender<bool>,
}

impl Drop for MsaAttempt<'_> {
    fn drop(&mut self) {
        let mut slot = self.slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.as_ref().is_some_and(|s| s.same_channel(&self.mine)) {
            *slot = None;
        }
    }
}

/// A short phrase for a credential store failure; never the secret.
fn secret_phrase(e: &SecretError) -> String {
    match e {
        SecretError::NoBackend => "no credential store".into(),
        SecretError::Other(why) => why.clone(),
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// Sign in with Microsoft in the player's browser. `Ok(None)` when the player canceled (on
    /// Microsoft's page, with Cancel, or by starting another sign-in). The account becomes the
    /// active one; the answer is the accounts the player may see.
    pub async fn account_sign_in_microsoft(&self, language: &str) -> Result<Option<AccountsFile>> {
        // Take the slot first, so a Cancel (or another sign-in) before the browser opens counts.
        let (_attempt, cancel) = self.start_msa_sign_in();
        let msa = self.runner.msa();
        msa.client_id().ok_or_else(not_configured)?;
        let pkce = Pkce::new();
        let loopback = Loopback::bind().map_err(|e| {
            LauncherError::auth(ErrorCode::SignInFailed, "Couldn't listen for the sign-in.")
                .with_detail(e)
        })?;
        let redirect = loopback.redirect_uri();
        let url = msa.authorize_url(&redirect, &pkce)?;
        if *cancel.borrow() {
            return Ok(None);
        }
        self.host.open_url(&url);
        self.host.emit(AppEvent::MicrosoftSignIn {
            phase: PerlandSignInPhase::Waiting,
        });
        let callback = loopback
            .wait(
                &pkce.state,
                Language::parse(language),
                Page::MicrosoftContinue,
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
                    "Couldn't sign you in with Microsoft. Try again.",
                )
                .with_detail(why))
            }
        };
        if *cancel.borrow() {
            return Ok(None);
        }
        self.host.emit(AppEvent::MicrosoftSignIn {
            phase: PerlandSignInPhase::Exchanging,
        });
        // A sign-in canceled (or replaced by a newer one) stops reporting, so its late steps
        // never show on the newer sign-in.
        let (host, canceled) = (self.host.clone(), cancel.clone());
        let report: StepReport = std::sync::Arc::new(move |step| {
            if !*canceled.borrow() {
                host.emit(AppEvent::MicrosoftSignInStep { step });
            }
        });
        let redeemed = msa
            .redeem(code, pkce.verifier.clone(), redirect, report)
            .await;
        // Canceled while the code was redeemed: the player moved on, so keep nothing.
        if *cancel.borrow() {
            return Ok(None);
        }
        match redeemed {
            Ok(signed) => self.keep_microsoft(signed).await.map(Some),
            Err(e) => Err(e),
        }
    }

    /// Keep a finished sign-in: the refresh token in the credential store, the Minecraft token
    /// in memory, the account in the accounts file as the active one.
    pub(crate) async fn keep_microsoft(&self, signed: SignedIn) -> Result<AccountsFile> {
        let uuid = dashed(&signed.profile.id);
        let store = self.accounts.lock().await;
        let secrets = self.msa_secrets.clone();
        let (key, refresh) = (uuid.clone(), signed.refresh_token);
        tokio::task::spawn_blocking(move || save_refresh(&*secrets, &key, &refresh))
            .await
            .unwrap_or_else(|_| Err(SecretError::Other("the credential store stopped".into())))
            .map_err(|e| {
                LauncherError::auth(
                    ErrorCode::SignInFailed,
                    "Couldn't keep your Microsoft sign-in on this computer.",
                )
                .with_detail(secret_phrase(&e))
            })?;
        self.msa_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(uuid.clone(), (signed.minecraft, signed.xuid));
        let file = store
            .add_microsoft(Account {
                provider: Provider::Microsoft,
                username: signed.profile.name,
                uuid,
            })
            .await?;
        Ok(visible(file))
    }

    /// Take the sign-in slot for a new Microsoft sign-in, canceling the one before.
    fn start_msa_sign_in(&self) -> (MsaAttempt<'_>, watch::Receiver<bool>) {
        let (cancel, canceled) = watch::channel(false);
        let mut slot = self.msa_sign_in.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = slot.replace(cancel.clone()) {
            old.send_replace(true);
        }
        let attempt = MsaAttempt {
            slot: &self.msa_sign_in,
            mine: cancel,
        };
        (attempt, canceled)
    }

    /// Stop the waiting Microsoft sign-in: it answers `Ok(None)`.
    pub fn account_sign_in_cancel(&self) {
        let current = self
            .msa_sign_in
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(cancel) = current {
            cancel.send_replace(true);
        }
    }

    /// Whether the player can play now, without a network call: the active account is a
    /// Microsoft account with a stored refresh token.
    pub async fn account_ready(&self) -> Result<AccountReady> {
        let file = self.accounts.lock().await.list().await?;
        let shown = visible(file);
        let active = shown
            .active
            .and_then(|uuid| shown.accounts.into_iter().find(|a| a.uuid == uuid));
        let ready = match active {
            Some(account) if account.provider == Provider::Microsoft => {
                self.has_refresh(account.uuid).await
            }
            _ => false,
        };
        Ok(AccountReady { ready })
    }

    /// A refresh token is stored for `uuid`. A credential store that can't answer is no.
    async fn has_refresh(&self, uuid: String) -> bool {
        let secrets = self.msa_secrets.clone();
        match tokio::task::spawn_blocking(move || load_refresh(&*secrets, &uuid)).await {
            Ok(Ok(found)) => found.is_some(),
            Ok(Err(e)) => {
                tracing::warn!(
                    "couldn't read the Microsoft sign-in from the credential store: {}",
                    secret_phrase(&e)
                );
                false
            }
            Err(_) => false,
        }
    }

    /// The check before every launch: the session the game starts with. Only a Microsoft
    /// account plays, and it always passes an online check of its
    /// Minecraft token, refreshed first when it has less than an hour left; any failure,
    /// a network one included, is returned and the game doesn't start.
    pub(crate) async fn session_for(&self, account: Account) -> Result<Session> {
        match account.provider {
            // Only an accounts file from an earlier beta holds one, and it never plays.
            Provider::Offline => Err(LauncherError::auth(
                ErrorCode::NoAccount,
                "Sign in with Microsoft to play",
            )),
            Provider::Microsoft => self.microsoft_session(account).await,
        }
    }

    async fn microsoft_session(&self, account: Account) -> Result<Session> {
        let msa = self.runner.msa();
        // Held for the whole check, so launches at once share one refresh: the second one
        // reads the token the first one stored.
        let _held = self.msa_refresh.lock().await;
        let uuid = account.uuid.clone();
        let refresh = self
            .stored_refresh(&uuid)
            .await?
            .ok_or_else(sign_in_again)?;
        let cached = self
            .msa_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&uuid)
            .cloned();
        let mut refreshed = false;
        let (mut token, mut xuid) = match cached {
            Some((token, xuid))
                if token.expires_at.saturating_sub(now_secs()) >= MIN_TOKEN_LIFE =>
            {
                (token, xuid)
            }
            _ => {
                refreshed = true;
                self.refresh_minecraft(&*msa, &uuid, refresh.clone())
                    .await?
            }
        };
        let profile = loop {
            match msa.profile(token.clone()).await {
                Ok(Some(profile)) => break profile,
                Ok(None) => return Err(ownership(None, false).unwrap_err()),
                // The token was turned down: one refresh, then ask again.
                Err(e) if e.code == ErrorCode::MicrosoftSignInAgain && !refreshed => {
                    refreshed = true;
                    (token, xuid) = self
                        .refresh_minecraft(&*msa, &uuid, refresh.clone())
                        .await?;
                }
                Err(e) => return Err(e),
            }
        };
        // The token speaks for another Minecraft profile than the one this account was signed
        // in as: never launch as someone else, and leave the stored account as it is.
        if !dashed(&profile.id).eq_ignore_ascii_case(&uuid) {
            tracing::warn!("the Microsoft sign-in answered for another Minecraft profile");
            return Err(sign_in_again());
        }
        if profile.name != account.username {
            if let Err(e) = self
                .accounts
                .lock()
                .await
                .rename(&uuid, &profile.name)
                .await
            {
                tracing::warn!(code = ?e.code, "couldn't keep the new player name");
            }
        }
        Ok(Session {
            username: profile.name,
            uuid: profile.id.replace('-', ""),
            access_token: token.access_token,
            user_type: "msa".into(),
            xuid,
            client_id: msa.client_id(),
        })
    }

    /// The refresh token stored for `uuid`.
    async fn stored_refresh(&self, uuid: &str) -> Result<Option<String>> {
        let secrets = self.msa_secrets.clone();
        let key = uuid.to_string();
        tokio::task::spawn_blocking(move || load_refresh(&*secrets, &key))
            .await
            .unwrap_or_else(|_| Err(SecretError::Other("the credential store stopped".into())))
            .map_err(|e| {
                let phrase = secret_phrase(&e);
                tracing::warn!("couldn't read the Microsoft sign-in: {phrase}");
                LauncherError::auth(
                    ErrorCode::SignInFailed,
                    "Couldn't read your Microsoft sign-in on this computer.",
                )
                .with_detail(phrase)
            })
    }

    /// Trade the refresh token for a new Minecraft token. The rotated refresh token is stored
    /// before the new Minecraft token is kept or used; if it can't be, nothing is kept.
    /// A refused refresh deletes nothing.
    async fn refresh_minecraft(
        &self,
        msa: &dyn MsaApi,
        uuid: &str,
        refresh: String,
    ) -> Result<(McToken, Option<String>)> {
        let signed = msa.refresh(refresh).await?;
        let secrets = self.msa_secrets.clone();
        let (key, rotated) = (uuid.to_string(), signed.refresh_token);
        tokio::task::spawn_blocking(move || save_refresh(&*secrets, &key, &rotated))
            .await
            .unwrap_or_else(|_| Err(SecretError::Other("the credential store stopped".into())))
            .map_err(|e| {
                let phrase = secret_phrase(&e);
                tracing::warn!("couldn't keep the refreshed Microsoft sign-in: {phrase}");
                LauncherError::auth(
                    ErrorCode::SignInFailed,
                    "Couldn't keep your Microsoft sign-in on this computer.",
                )
                .with_detail(phrase)
            })?;
        let kept = (signed.minecraft, signed.xuid);
        self.msa_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(uuid.to_string(), kept.clone());
        Ok(kept)
    }

    /// Forget what a removed account kept: its Minecraft token and its refresh token. A
    /// credential store that fails is logged; the account is gone either way.
    pub(crate) async fn forget_microsoft(&self, uuid: &str) {
        // A launch check under way could store a rotated refresh token: let it finish first.
        let _held = self.msa_refresh.lock().await;
        self.msa_tokens
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(uuid);
        let secrets = self.msa_secrets.clone();
        let uuid = uuid.to_string();
        match tokio::task::spawn_blocking(move || delete_refresh(&*secrets, &uuid)).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!(
                "couldn't delete the Microsoft sign-in from the credential store: {}",
                secret_phrase(&e)
            ),
            Err(_) => tracing::warn!("couldn't delete the Microsoft sign-in"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    use launcher_core::{
        accounts::{Account, AccountsFile, BuildKind, Provider},
        invite::Url,
        msa::{
            minecraft::{dashed, McProfile, McToken},
            secrets::{load_refresh, save_refresh, KeyedSecrets, MemorySecrets},
            MsaStep,
        },
        perland::SecretError,
        ErrorCode, LauncherError, Result,
    };

    use crate::{
        app::tests::{core, core_with_msa_secrets, TestCore},
        dto::PerlandSignInPhase,
        events::AppEvent,
        restore::now_secs,
        testing::{old_offline_account, old_offline_file, signed_in, MS_CLIENT_ID},
    };

    const ALEX_ID: &str = "0123456789abcdef0123456789abcdef";
    const STEVE_ID: &str = "fedcba9876543210fedcba9876543210";

    /// The query value `key` of `url`.
    fn query(url: &str, key: &str) -> String {
        Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
            .unwrap_or_else(|| panic!("no {key} in the authorize URL"))
    }

    /// Plays the browser: back to the loopback with `params`. Returns the page it was shown.
    async fn come_back(authorize: &str, params: &str) -> String {
        let redirect = query(authorize, "redirect_uri");
        let state = query(authorize, "state");
        let url = format!("{redirect}?{params}&state={state}");
        launcher_core::http::client("test")
            .unwrap()
            .get(url)
            .send()
            .await
            .unwrap()
            .text()
            .await
            .unwrap()
    }

    fn sign_in(core: &Arc<TestCore>) -> tokio::task::JoinHandle<Result<Option<AccountsFile>>> {
        let core = core.clone();
        tokio::spawn(async move { core.account_sign_in_microsoft("en").await })
    }

    /// The sign-in's answer, within 5 s.
    async fn finish(
        task: tokio::task::JoinHandle<Result<Option<AccountsFile>>>,
    ) -> Result<Option<AccountsFile>> {
        tokio::time::timeout(std::time::Duration::from_secs(5), task)
            .await
            .expect("the sign-in ended")
            .unwrap()
    }

    fn refresh_of(core: &TestCore, uuid: &str) -> Option<String> {
        load_refresh(&*core.msa_secrets, uuid).unwrap()
    }

    fn awaiting() -> LauncherError {
        LauncherError::auth(ErrorCode::MicrosoftAwaitingApproval, "waiting on approval")
    }

    #[tokio::test]
    async fn sign_in_stores_account_and_secret() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        let url = opened[0].clone();
        assert_eq!(query(&url, "client_id"), MS_CLIENT_ID);
        let page = come_back(&url, "code=the-code").await;
        assert!(page.contains("Go back to Perland Launcher to finish signing in"));
        let file = finish(task).await.unwrap().expect("signed in");

        let uuid = dashed(ALEX_ID);
        assert_eq!(
            file.accounts,
            vec![Account {
                provider: Provider::Microsoft,
                username: "Alex".into(),
                uuid: uuid.clone(),
            }]
        );
        assert_eq!(file.active.as_deref(), Some(uuid.as_str()));
        assert_eq!(core.accounts_list().await.unwrap(), file);
        assert_eq!(refresh_of(&core, &uuid).as_deref(), Some("refresh-Alex"));
        let (mc, xuid) = core
            .msa_tokens
            .lock()
            .unwrap()
            .get(&uuid)
            .cloned()
            .expect("the Minecraft token is kept");
        assert_eq!(mc.access_token, "mc-Alex");
        assert_eq!(xuid.as_deref(), Some("xuid-Alex"));

        let redeem = &runner.msa.calls_to("redeem")[0];
        assert_eq!(redeem.args[0], "the-code");
        let challenge = launcher_core::perland::Pkce::from_verifier(&redeem.args[1], "").challenge;
        assert_eq!(challenge, query(&url, "code_challenge"));
        assert_eq!(redeem.args[2], query(&url, "redirect_uri"));

        let phases: Vec<_> = host
            .events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::MicrosoftSignIn { phase } => Some(*phase),
                _ => None,
            })
            .collect();
        assert_eq!(
            phases,
            [PerlandSignInPhase::Waiting, PerlandSignInPhase::Exchanging]
        );
        assert!(core.account_ready().await.unwrap().ready);
    }

    #[tokio::test]
    async fn denied_in_the_browser_keeps_nothing() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "error=access_denied").await;
        assert_eq!(finish(task).await.unwrap(), None);
        assert!(runner.msa.calls_to("redeem").is_empty());
        assert!(core.accounts_list().await.unwrap().accounts.is_empty());
    }

    /// Every call of the chain reaches the window as it starts, after the browser step.
    #[tokio::test]
    async fn sign_in_reports_each_step_to_the_window() {
        let (core, _runner, host, _t) = core(BuildKind::Release);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c").await;
        finish(task).await.unwrap().expect("signed in");
        let steps: Vec<_> = host
            .events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::MicrosoftSignInStep { step } => Some(*step),
                _ => None,
            })
            .collect();
        assert_eq!(
            steps,
            [
                MsaStep::Microsoft,
                MsaStep::Xbox,
                MsaStep::Xsts,
                MsaStep::Minecraft,
                MsaStep::Profile
            ]
        );
    }

    /// Minecraft turning the app down keeps nothing, and the next sign-in runs the whole chain
    /// again: nothing is remembered that would skip it.
    #[tokio::test]
    async fn awaiting_approval_keeps_nothing_and_the_next_sign_in_tries_again() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        runner
            .msa
            .redeems
            .lock()
            .unwrap()
            .push_back(Err(awaiting()));
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c1").await;
        let err = finish(task).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftAwaitingApproval);
        assert!(core.accounts_list().await.unwrap().accounts.is_empty());
        assert!(!core.account_ready().await.unwrap().ready);

        let task = sign_in(&core);
        let opened = host.wait_for_opened(2).await;
        come_back(&opened[1], "code=c2").await;
        finish(task).await.unwrap().expect("signed in");
        assert_eq!(runner.msa.calls_to("redeem").len(), 2);
    }

    #[tokio::test]
    async fn second_sign_in_cancels_first() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        let gate = runner.msa.hold("redeem");
        {
            let mut redeems = runner.msa.redeems.lock().unwrap();
            redeems.push_back(Ok(signed_in("Steve", STEVE_ID)));
            redeems.push_back(Ok(signed_in("Alex", ALEX_ID)));
        }
        let first = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=first").await;
        runner.msa.wait_for_calls("redeem", 1).await;

        let second = sign_in(&core);
        let opened = host.wait_for_opened(2).await;
        gate.add_permits(1);
        assert_eq!(finish(first).await.unwrap(), None, "the first gives way");
        assert!(core.accounts_list().await.unwrap().accounts.is_empty());
        assert_eq!(refresh_of(&core, &dashed(STEVE_ID)), None);

        come_back(&opened[1], "code=second").await;
        gate.add_permits(1);
        let file = finish(second).await.unwrap().expect("signed in");
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(file.accounts[0].username, "Alex");
        assert_eq!(refresh_of(&core, &dashed(STEVE_ID)), None);
        assert!(!core
            .msa_tokens
            .lock()
            .unwrap()
            .contains_key(&dashed(STEVE_ID)));
        assert!(refresh_of(&core, &dashed(ALEX_ID)).is_some());
    }

    #[tokio::test]
    async fn cancel_while_redeeming_keeps_nothing() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        let gate = runner.msa.hold("redeem");
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c").await;
        runner.msa.wait_for_calls("redeem", 1).await;
        core.account_sign_in_cancel();
        gate.add_permits(1);
        assert_eq!(finish(task).await.unwrap(), None);
        let file = core.accounts.lock().await.list().await.unwrap();
        assert!(file.accounts.is_empty());
        assert_eq!(file.active, None);
        assert_eq!(refresh_of(&core, &dashed(ALEX_ID)), None);
        assert!(core.msa_tokens.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancel_while_waiting_answers_none() {
        let (core, _runner, host, _t) = core(BuildKind::Release);
        let task = sign_in(&core);
        host.wait_for_opened(1).await;
        core.account_sign_in_cancel();
        assert_eq!(finish(task).await.unwrap(), None);
    }

    #[tokio::test]
    async fn not_configured_refuses_before_opening_the_browser() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        *runner.msa.client_id.lock().unwrap() = None;
        let err = core.account_sign_in_microsoft("en").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftNotConfigured);
        assert!(host.opened.lock().unwrap().is_empty());
        assert!(host
            .events
            .lock()
            .unwrap()
            .iter()
            .all(|e| !matches!(e, AppEvent::MicrosoftSignIn { .. })));
    }

    #[tokio::test]
    async fn ready_needs_a_stored_refresh_token() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        assert!(!core.account_ready().await.unwrap().ready, "no account");
        let uuid = dashed(ALEX_ID);
        core.accounts
            .lock()
            .await
            .add_microsoft(Account {
                provider: Provider::Microsoft,
                username: "Alex".into(),
                uuid: uuid.clone(),
            })
            .await
            .unwrap();
        assert!(
            !core.account_ready().await.unwrap().ready,
            "no refresh token"
        );
        save_refresh(&*core.msa_secrets, &uuid, "refresh").unwrap();
        assert!(core.account_ready().await.unwrap().ready);
    }

    #[tokio::test]
    async fn an_old_offline_account_is_never_ready() {
        for build in [BuildKind::Dev, BuildKind::Beta, BuildKind::Release] {
            let (core, _runner, _host, _t) = core(build);
            let file = old_offline_file();
            save_refresh(
                &*core.msa_secrets,
                file.active.as_deref().unwrap(),
                "refresh",
            )
            .unwrap();
            core.accounts.lock().await.save(&file).await.unwrap();
            assert!(!core.account_ready().await.unwrap().ready, "{build:?}");
        }
    }

    #[tokio::test]
    async fn remove_deletes_the_secret() {
        let (core, _runner, host, _t) = core(BuildKind::Release);
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c").await;
        finish(task).await.unwrap().expect("signed in");
        let uuid = dashed(ALEX_ID);
        assert!(refresh_of(&core, &uuid).is_some());

        let file = core.account_remove(&uuid).await.unwrap();
        assert!(file.accounts.is_empty());
        assert_eq!(refresh_of(&core, &uuid), None);
        assert!(!core.msa_tokens.lock().unwrap().contains_key(&uuid));
        assert!(!core.account_ready().await.unwrap().ready);
    }

    #[tokio::test]
    async fn set_active_refuses_an_old_offline_account() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        let steve = old_offline_account();
        let alex = Account {
            provider: Provider::Microsoft,
            username: "Alex".into(),
            uuid: dashed(ALEX_ID),
        };
        let file = AccountsFile {
            active: Some(alex.uuid.clone()),
            accounts: vec![steve.clone(), alex.clone()],
            ..AccountsFile::default()
        };
        core.accounts.lock().await.save(&file).await.unwrap();
        let err = core.account_set_active(&steve.uuid).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::NoAccount);
        let stored = core.accounts.lock().await.list().await.unwrap();
        assert_eq!(stored.active, Some(alex.uuid.clone()), "nothing changed");
        assert_eq!(
            core.account_set_active(&alex.uuid).await.unwrap().active,
            Some(alex.uuid)
        );
    }

    // ---- The check before every launch ----

    /// Alex as a stored Microsoft account (the active one) with refresh token `refresh-Alex`,
    /// and, with `expires_in`, Minecraft token `mc-cached` in memory for that many seconds.
    async fn signed_alex(core: &TestCore, expires_in: Option<u64>) -> Account {
        let account = Account {
            provider: Provider::Microsoft,
            username: "Alex".into(),
            uuid: dashed(ALEX_ID),
        };
        core.accounts
            .lock()
            .await
            .add_microsoft(account.clone())
            .await
            .unwrap();
        save_refresh(&*core.msa_secrets, &account.uuid, "refresh-Alex").unwrap();
        if let Some(secs) = expires_in {
            core.msa_tokens.lock().unwrap().insert(
                account.uuid.clone(),
                (
                    McToken {
                        access_token: "mc-cached".into(),
                        expires_at: now_secs() + secs,
                    },
                    Some("xuid-cached".into()),
                ),
            );
        }
        account
    }

    fn sign_in_again() -> LauncherError {
        LauncherError::auth(ErrorCode::MicrosoftSignInAgain, "sign in again")
    }

    fn token_of(core: &TestCore, uuid: &str) -> Option<String> {
        core.msa_tokens
            .lock()
            .unwrap()
            .get(uuid)
            .map(|(t, _)| t.access_token.clone())
    }

    #[tokio::test]
    async fn fresh_token_skips_refresh_but_checks_profile() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        let session = core.session_for(alex).await.unwrap();
        assert!(runner.msa.calls_to("refresh").is_empty());
        let profiles = runner.msa.calls_to("profile");
        assert_eq!(profiles.len(), 1, "the online check always runs");
        assert_eq!(profiles[0].args, ["mc-cached"]);
        assert_eq!(session.access_token, "mc-cached");
    }

    #[tokio::test]
    async fn stale_token_refreshes_and_stores_rotated_token() {
        for cached in [None, Some(600)] {
            let (core, runner, _host, _t) = core(BuildKind::Release);
            let alex = signed_alex(&core, cached).await;
            let mut rotated = signed_in("Alex", ALEX_ID);
            rotated.refresh_token = "refresh-rotated".into();
            rotated.minecraft.access_token = "mc-new".into();
            runner.msa.refreshes.lock().unwrap().push_back(Ok(rotated));
            let session = core.session_for(alex.clone()).await.unwrap();
            let refreshes = runner.msa.calls_to("refresh");
            assert_eq!(refreshes.len(), 1, "{cached:?}");
            assert_eq!(refreshes[0].args, ["refresh-Alex"]);
            assert_eq!(
                refresh_of(&core, &alex.uuid).as_deref(),
                Some("refresh-rotated")
            );
            assert_eq!(token_of(&core, &alex.uuid).as_deref(), Some("mc-new"));
            assert_eq!(runner.msa.calls_to("profile")[0].args, ["mc-new"]);
            assert_eq!(session.access_token, "mc-new");
            assert_eq!(session.xuid.as_deref(), Some("xuid-Alex"));
        }
    }

    #[tokio::test]
    async fn concurrent_launches_share_one_refresh() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, None).await;
        let gate = runner.msa.hold("refresh");
        let launch = |core: &Arc<TestCore>, account: Account| {
            let core = core.clone();
            tokio::spawn(async move { core.session_for(account).await })
        };
        let first = launch(&core, alex.clone());
        let second = launch(&core, alex.clone());
        runner.msa.wait_for_calls("refresh", 1).await;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        gate.add_permits(2);
        let a = first.await.unwrap().unwrap();
        let b = second.await.unwrap().unwrap();
        assert_eq!(runner.msa.calls_to("refresh").len(), 1);
        assert_eq!(
            runner.msa.calls_to("profile").len(),
            2,
            "each launch checks"
        );
        assert_eq!(a.access_token, "mc-Alex");
        assert_eq!(b.access_token, "mc-Alex");
    }

    #[tokio::test]
    async fn refresh_refused_asks_to_sign_in_again() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, None).await;
        runner
            .msa
            .refreshes
            .lock()
            .unwrap()
            .push_back(Err(sign_in_again()));
        let err = core.session_for(alex.clone()).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
        assert!(runner.msa.calls_to("profile").is_empty());
        assert_eq!(
            refresh_of(&core, &alex.uuid).as_deref(),
            Some("refresh-Alex"),
            "nothing is deleted"
        );
        assert_eq!(core.accounts_list().await.unwrap().accounts, vec![alex]);
    }

    #[tokio::test]
    async fn missing_refresh_token_asks_to_sign_in_again() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        launcher_core::msa::secrets::delete_refresh(&*core.msa_secrets, &alex.uuid).unwrap();
        let err = core.session_for(alex).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
        assert!(runner.msa.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn profile_401_refreshes_once() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        runner
            .msa
            .profiles
            .lock()
            .unwrap()
            .push_back(Err(sign_in_again()));
        let session = core.session_for(alex.clone()).await.unwrap();
        assert_eq!(runner.msa.calls_to("refresh").len(), 1);
        let profiles = runner.msa.calls_to("profile");
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[1].args, ["mc-Alex"]);
        assert_eq!(session.access_token, "mc-Alex");

        // A second 401, after the refresh, is the end.
        let (core, runner, _host, _t) = crate::app::tests::core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        {
            let mut profiles = runner.msa.profiles.lock().unwrap();
            profiles.push_back(Err(sign_in_again()));
            profiles.push_back(Err(sign_in_again()));
        }
        let err = core.session_for(alex).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
        assert_eq!(runner.msa.calls_to("refresh").len(), 1);
        assert_eq!(runner.msa.calls_to("profile").len(), 2);
    }

    #[tokio::test]
    async fn no_profile_is_not_owned() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        runner.msa.profiles.lock().unwrap().push_back(Ok(None));
        let err = core.session_for(alex).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MinecraftNotOwned);
    }

    #[tokio::test]
    async fn network_failure_never_launches() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        runner
            .msa
            .profiles
            .lock()
            .unwrap()
            .push_back(Err(LauncherError::network(
                ErrorCode::HostUnreachable,
                "Couldn't reach Minecraft",
            )));
        let err = core.session_for(alex).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::HostUnreachable);
        assert!(runner.msa.calls_to("refresh").is_empty());

        // The same on the refresh.
        let (core, runner, _host, _t) = crate::app::tests::core(BuildKind::Release);
        let alex = signed_alex(&core, None).await;
        runner
            .msa
            .refreshes
            .lock()
            .unwrap()
            .push_back(Err(LauncherError::network(
                ErrorCode::HostUnreachable,
                "Couldn't reach Microsoft",
            )));
        let err = core.session_for(alex).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::HostUnreachable);
        assert!(runner.msa.calls_to("profile").is_empty());
    }

    #[tokio::test]
    async fn msa_session_is_msa_with_xuid_and_clientid() {
        let (core, _runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        let session = core.session_for(alex).await.unwrap();
        assert_eq!(session.username, "Alex");
        assert_eq!(session.uuid, ALEX_ID, "undashed");
        assert_eq!(session.access_token, "mc-cached");
        assert_eq!(session.user_type, "msa");
        assert_eq!(session.xuid.as_deref(), Some("xuid-cached"));
        assert_eq!(session.client_id.as_deref(), Some(MS_CLIENT_ID));
        assert!(!format!("{session:?}").contains("mc-cached"));
    }

    #[tokio::test]
    async fn an_old_offline_account_never_gets_a_session() {
        for build in [BuildKind::Dev, BuildKind::Beta, BuildKind::Release] {
            let (core, runner, _host, _t) = core(build);
            let err = core.session_for(old_offline_account()).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::NoAccount, "{build:?}");
            assert!(runner.msa.calls.lock().unwrap().is_empty());
        }
    }

    #[tokio::test]
    async fn renamed_profile_updates_the_account_and_keeps_active() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        let steve = Account {
            provider: Provider::Microsoft,
            username: "Steve".into(),
            uuid: dashed(STEVE_ID),
        };
        core.accounts
            .lock()
            .await
            .add_microsoft(steve.clone())
            .await
            .unwrap();
        runner
            .msa
            .profiles
            .lock()
            .unwrap()
            .push_back(Ok(Some(McProfile {
                id: ALEX_ID.into(),
                name: "Alexis".into(),
            })));
        let session = core.session_for(alex.clone()).await.unwrap();
        assert_eq!(session.username, "Alexis");
        let file = core.accounts_list().await.unwrap();
        assert_eq!(file.active, Some(steve.uuid), "the active account stays");
        let renamed = file.accounts.iter().find(|a| a.uuid == alex.uuid).unwrap();
        assert_eq!(renamed.username, "Alexis");
    }

    /// The token answers for another Minecraft profile than the stored account's: the account
    /// can't be trusted to be the one the player signed in as, so no launch and no rename.
    #[tokio::test]
    async fn a_profile_of_another_player_asks_to_sign_in_again() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, Some(7200)).await;
        runner
            .msa
            .profiles
            .lock()
            .unwrap()
            .push_back(Ok(Some(McProfile {
                id: STEVE_ID.into(),
                name: "Steve".into(),
            })));
        let err = core.session_for(alex.clone()).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftSignInAgain);
        assert_eq!(
            core.accounts_list().await.unwrap().accounts,
            vec![alex],
            "the stored account keeps its name"
        );
    }

    #[tokio::test]
    async fn refresh_awaiting_approval_is_returned() {
        let (core, runner, _host, _t) = core(BuildKind::Release);
        let alex = signed_alex(&core, None).await;
        runner
            .msa
            .refreshes
            .lock()
            .unwrap()
            .push_back(Err(awaiting()));
        let err = core.session_for(alex).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MicrosoftAwaitingApproval);
    }

    /// Microsoft refresh tokens in memory, whose saves fail while `fail` is set.
    #[derive(Default)]
    struct FailingSaves {
        inner: MemorySecrets,
        fail: AtomicBool,
    }

    impl KeyedSecrets for FailingSaves {
        fn load(&self, key: &str) -> std::result::Result<Option<String>, SecretError> {
            self.inner.load(key)
        }
        fn save(&self, key: &str, value: &str) -> std::result::Result<(), SecretError> {
            if self.fail.load(Ordering::SeqCst) {
                return Err(SecretError::Other("locked".into()));
            }
            self.inner.save(key, value)
        }
        fn delete(&self, key: &str) -> std::result::Result<(), SecretError> {
            self.inner.delete(key)
        }
    }

    #[tokio::test]
    async fn rotated_token_that_cannot_be_kept_never_launches() {
        let secrets = Arc::new(FailingSaves::default());
        let (core, runner, _host, _t) = core_with_msa_secrets(BuildKind::Release, secrets.clone());
        let alex = signed_alex(&core, Some(600)).await;
        secrets.fail.store(true, Ordering::SeqCst);
        let err = core.session_for(alex.clone()).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::SignInFailed);
        assert_eq!(runner.msa.calls_to("refresh").len(), 1);
        assert!(runner.msa.calls_to("profile").is_empty());
        assert_eq!(
            token_of(&core, &alex.uuid).as_deref(),
            Some("mc-cached"),
            "the new token is not used"
        );
        assert_eq!(
            refresh_of(&core, &alex.uuid).as_deref(),
            Some("refresh-Alex")
        );
    }
}
