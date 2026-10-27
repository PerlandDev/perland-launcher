//! Sign in with Microsoft (Microsoft sign-in spec): the browser sign-in, the memo of a client id
//! still waiting on Mojang's approval, and what the app keeps of a signed-in account: the
//! refresh token in the credential store, the Minecraft token and xuid in memory only.
//! Nothing here logs or formats a token, an authorization code or the PKCE verifier.
use std::sync::Mutex;

use launcher_core::{
    accounts::{hides_offline, visible, Account, AccountsFile, MsaWaiting, Provider},
    http::HttpClient,
    msa::{
        minecraft::{dashed, McProfile, McToken, SignedIn},
        secrets::{delete_refresh, load_refresh, save_refresh},
        MsaClient, MsaEndpoints,
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
    /// Redeem the code the browser brought back and run the whole chain.
    fn redeem(
        &self,
        code: String,
        verifier: String,
        redirect: String,
    ) -> BoxFuture<Result<SignedIn>>;
    /// Trade a refresh token for a new one and run the whole chain.
    fn refresh(&self, refresh_token: String) -> BoxFuture<Result<SignedIn>>;
    /// The Minecraft profile the token belongs to; `None` when it owns no profile.
    fn profile(&self, token: McToken) -> BoxFuture<Result<Option<McProfile>>>;
}

/// The refusal of a build that has no Microsoft client id.
pub(crate) fn not_configured() -> LauncherError {
    LauncherError::auth(
        ErrorCode::MicrosoftNotConfigured,
        "Microsoft sign-in isn't set up in this build.",
    )
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
    ) -> BoxFuture<Result<SignedIn>> {
        let client = self.client();
        Box::pin(async move {
            let client = client?;
            let tokens = client.redeem_code(&code, &verifier, &redirect).await?;
            client.finish(tokens, now_secs()).await
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
        let client_id = msa.client_id().ok_or_else(not_configured)?;
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
        let redeemed = msa.redeem(code, pkce.verifier.clone(), redirect).await;
        // Canceled while the code was redeemed: the player moved on, so keep nothing.
        if *cancel.borrow() {
            return Ok(None);
        }
        match redeemed {
            Ok(signed) => self.keep_microsoft(signed).await.map(Some),
            Err(e) if e.code == ErrorCode::MicrosoftAwaitingApproval => {
                let waiting = MsaWaiting {
                    client_id,
                    at: now_secs(),
                };
                if let Err(w) = self
                    .accounts
                    .lock()
                    .await
                    .set_msa_waiting(Some(waiting))
                    .await
                {
                    tracing::warn!(code = ?w.code, "couldn't remember the approval wait");
                }
                Err(e)
            }
            // These come only after Minecraft accepted the Xbox token, so the client id is
            // approved: the memo of a wait no longer holds.
            Err(e)
                if matches!(
                    e.code,
                    ErrorCode::MinecraftNotOwned | ErrorCode::MinecraftNoProfile
                ) =>
            {
                self.clear_msa_waiting().await;
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    /// Forget the approval wait, if one is remembered. A failure is only logged: the sign-in's
    /// own answer matters more.
    async fn clear_msa_waiting(&self) {
        let store = self.accounts.lock().await;
        let cleared = match store.list().await {
            Ok(file) if file.msa_waiting.is_some() => store.set_msa_waiting(None).await,
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        };
        if let Err(e) = cleared {
            tracing::warn!(code = ?e.code, "couldn't forget the approval wait");
        }
    }

    /// Keep a finished sign-in: the refresh token in the credential store, the Minecraft token
    /// in memory, the account in the accounts file as the active one.
    async fn keep_microsoft(&self, signed: SignedIn) -> Result<AccountsFile> {
        let uuid = dashed(&signed.profile.id);
        let store = self.accounts.lock().await;
        if store.list().await?.msa_waiting.is_some() {
            store.set_msa_waiting(None).await?;
        }
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
        Ok(visible(file, self.config.build))
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
    /// Microsoft account with a stored refresh token. Also whether this build's client id is
    /// waiting on approval, and whether the "offline accounts are gone" notice is due.
    pub async fn account_ready(&self) -> Result<AccountReady> {
        let file = self.accounts.lock().await.list().await?;
        let offline_notice =
            hides_offline(&file, self.config.build) && !file.offline_notice_dismissed;
        let awaiting_approval = match (&file.msa_waiting, self.runner.msa().client_id()) {
            (Some(waiting), Some(id)) => waiting.client_id == id,
            _ => false,
        };
        let shown = visible(file, self.config.build);
        let active = shown
            .active
            .and_then(|uuid| shown.accounts.into_iter().find(|a| a.uuid == uuid))
            .filter(|a| a.provider == Provider::Microsoft);
        let ready = match active {
            Some(account) => self.has_refresh(account.uuid).await,
            None => false,
        };
        Ok(AccountReady {
            ready,
            awaiting_approval,
            offline_notice,
        })
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

    /// Don't show the "offline accounts are gone" notice again.
    pub async fn account_dismiss_offline_notice(&self) -> Result<()> {
        self.accounts.lock().await.dismiss_offline_notice().await
    }

    /// Forget what a removed account kept: its Minecraft token and its refresh token. A
    /// credential store that fails is logged; the account is gone either way.
    pub(crate) async fn forget_microsoft(&self, uuid: &str) {
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
    use std::sync::Arc;

    use launcher_core::{
        accounts::{offline_account, Account, AccountsFile, BuildKind, Provider},
        invite::Url,
        msa::{
            minecraft::dashed,
            secrets::{load_refresh, save_refresh},
        },
        ErrorCode, LauncherError, Result,
    };

    use crate::{
        app::tests::{core, TestCore},
        dto::PerlandSignInPhase,
        events::AppEvent,
        testing::{signed_in, MS_CLIENT_ID},
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

    #[tokio::test]
    async fn awaiting_approval_is_remembered_then_cleared() {
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
        let ready = core.account_ready().await.unwrap();
        assert!(ready.awaiting_approval);
        assert!(!ready.ready);
        let file = core.accounts.lock().await.list().await.unwrap();
        let waiting = file.msa_waiting.expect("remembered");
        assert_eq!(waiting.client_id, MS_CLIENT_ID);
        assert!(waiting.at > 0);
        assert!(file.accounts.is_empty(), "no account is kept");

        let task = sign_in(&core);
        let opened = host.wait_for_opened(2).await;
        come_back(&opened[1], "code=c2").await;
        finish(task).await.unwrap().expect("signed in");
        let ready = core.account_ready().await.unwrap();
        assert!(!ready.awaiting_approval);
        assert!(ready.ready);
        let file = core.accounts.lock().await.list().await.unwrap();
        assert_eq!(file.msa_waiting, None);
    }

    #[tokio::test]
    async fn awaiting_approval_is_cleared_once_past_xbox() {
        let (core, runner, host, _t) = core(BuildKind::Release);
        {
            let mut redeems = runner.msa.redeems.lock().unwrap();
            redeems.push_back(Err(awaiting()));
            redeems.push_back(Err(LauncherError::auth(
                ErrorCode::MinecraftNotOwned,
                "doesn't own Minecraft",
            )));
        }
        let task = sign_in(&core);
        let opened = host.wait_for_opened(1).await;
        come_back(&opened[0], "code=c1").await;
        finish(task).await.unwrap_err();
        assert!(core.account_ready().await.unwrap().awaiting_approval);

        let task = sign_in(&core);
        let opened = host.wait_for_opened(2).await;
        come_back(&opened[1], "code=c2").await;
        let err = finish(task).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::MinecraftNotOwned);
        assert!(
            !core.account_ready().await.unwrap().awaiting_approval,
            "Minecraft answered, so the app is approved"
        );
        let file = core.accounts.lock().await.list().await.unwrap();
        assert_eq!(file.msa_waiting, None);
        assert!(file.accounts.is_empty());
    }

    #[tokio::test]
    async fn awaiting_approval_ignores_another_client_id() {
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
        finish(task).await.unwrap_err();
        assert!(core.account_ready().await.unwrap().awaiting_approval);

        *runner.msa.client_id.lock().unwrap() = Some("another-client-id".into());
        assert!(
            !core.account_ready().await.unwrap().awaiting_approval,
            "a build with another client id has its own approval"
        );
        *runner.msa.client_id.lock().unwrap() = None;
        assert!(!core.account_ready().await.unwrap().awaiting_approval);
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
    async fn ready_is_false_for_an_offline_account() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        core.account_add_offline("Steve").await.unwrap();
        let uuid = offline_account("Steve").unwrap().uuid;
        save_refresh(&*core.msa_secrets, &uuid, "refresh").unwrap();
        assert!(!core.account_ready().await.unwrap().ready);
    }

    #[tokio::test]
    async fn offline_notice_shows_until_dismissed() {
        let (core, _runner, _host, _t) = core(BuildKind::Beta);
        let steve = offline_account("Steve").unwrap();
        let file = AccountsFile {
            active: Some(steve.uuid.clone()),
            accounts: vec![steve],
            ..AccountsFile::default()
        };
        core.accounts.lock().await.save(&file).await.unwrap();
        let ready = core.account_ready().await.unwrap();
        assert!(ready.offline_notice);
        assert!(!ready.ready, "a hidden offline account is no account");
        core.account_dismiss_offline_notice().await.unwrap();
        assert!(!core.account_ready().await.unwrap().offline_notice);
    }

    #[tokio::test]
    async fn no_offline_notice_in_dev() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        core.account_add_offline("Steve").await.unwrap();
        assert!(!core.account_ready().await.unwrap().offline_notice);
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
    async fn set_active_refuses_a_hidden_offline_account() {
        let (core, _runner, _host, _t) = core(BuildKind::Beta);
        let steve = offline_account("Steve").unwrap();
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

    #[tokio::test]
    async fn set_active_takes_an_offline_account_in_dev() {
        let (core, _runner, _host, _t) = core(BuildKind::Dev);
        core.account_add_offline("Steve").await.unwrap();
        core.account_add_offline("Alex").await.unwrap();
        let steve = offline_account("Steve").unwrap().uuid;
        assert_eq!(
            core.account_set_active(&steve).await.unwrap().active,
            Some(steve)
        );
    }
}
