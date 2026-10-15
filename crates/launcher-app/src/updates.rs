//! The launcher's own updates (spec §4): check on start, download in the background, install on
//! quit or Restart now, and never while something runs. Tauri's updater sits behind
//! `UpdateSource`, so all of this runs in tests with a fake.
use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use launcher_core::{accounts::BuildKind, settings::Channel, ErrorCode, LauncherError, Result};
use serde::Serialize;

use crate::{
    events::{AppEvent, Host},
    runner::BoxFuture,
};

pub const BETA_ENDPOINT: &str =
    "https://github.com/PerlandDev/perland-launcher/releases/download/channel-beta/latest.json";
pub const STABLE_ENDPOINT: &str =
    "https://github.com/PerlandDev/perland-launcher/releases/download/channel-stable/latest.json";

/// How long after start the first check waits, so it doesn't compete with the first screen.
pub const START_DELAY: Duration = Duration::from_secs(5);

pub fn endpoint(channel: Channel) -> &'static str {
    match channel {
        Channel::Beta => BETA_ENDPOINT,
        Channel::Stable => STABLE_ENDPOINT,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum UpdateState {
    /// Dev builds: nothing is checked.
    Off,
    Idle,
    Checking,
    Downloading {
        percent: Option<u8>,
    },
    /// Downloaded and verified; installs on quit or Restart now.
    Ready {
        version: String,
    },
    UpToDate,
    Failed {
        error: LauncherError,
    },
}

/// Called with each downloaded chunk's length and the total size when known.
pub type Progress = Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Where updates come from. The real one wraps `tauri-plugin-updater` (src-tauri/src/updater.rs).
pub trait UpdateSource: Send + Sync + 'static {
    type Update: Send + Sync + 'static;
    /// A newer version at `endpoint`, if there is one.
    fn check(&self, endpoint: &'static str) -> BoxFuture<Result<Option<Self::Update>>>;
    fn version(&self, update: &Self::Update) -> String;
    /// The installer's bytes, signature already verified.
    fn download(&self, update: Arc<Self::Update>, progress: Progress)
        -> BoxFuture<Result<Vec<u8>>>;
    /// Run the installer. On Windows this ends the process.
    fn install(
        &self,
        update: Arc<Self::Update>,
        bytes: Arc<Vec<u8>>,
        relaunch: bool,
    ) -> BoxFuture<Result<()>>;
}

/// A downloaded update with its installer bytes.
struct Downloaded<U> {
    update: Arc<U>,
    bytes: Arc<Vec<u8>>,
}

pub struct Updates<S: UpdateSource, H: Host> {
    source: Arc<S>,
    host: Arc<H>,
    enabled: bool,
    channel: Mutex<Channel>,
    state: Arc<Mutex<UpdateState>>,
    ready: Mutex<Option<Downloaded<S::Update>>>,
    /// Checks run one at a time; a channel change waits for the running one.
    flight: tokio::sync::Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Restart now while a game, an install or a sync runs.
pub fn busy_error() -> LauncherError {
    LauncherError::game(
        ErrorCode::LauncherUpdateBusy,
        "Wait for games and installs to finish before restarting to update",
    )
    .with_fix("The update also installs when you close the launcher")
}

fn nothing_ready() -> LauncherError {
    LauncherError::network(
        ErrorCode::LauncherUpdateFailed,
        "No launcher update is ready yet",
    )
}

impl<S: UpdateSource, H: Host> Updates<S, H> {
    pub fn new(source: Arc<S>, host: Arc<H>, build: BuildKind, channel: Channel) -> Arc<Self> {
        let enabled = build != BuildKind::Dev;
        Arc::new(Self {
            source,
            host,
            enabled,
            channel: Mutex::new(channel),
            state: Arc::new(Mutex::new(if enabled {
                UpdateState::Idle
            } else {
                UpdateState::Off
            })),
            ready: Mutex::new(None),
            flight: tokio::sync::Mutex::new(()),
        })
    }

    pub fn status(&self) -> UpdateState {
        lock(&self.state).clone()
    }

    fn set(&self, state: UpdateState) -> UpdateState {
        *lock(&self.state) = state.clone();
        self.host.emit(AppEvent::Update {
            state: state.clone(),
        });
        state
    }

    fn ready_state(&self) -> Option<UpdateState> {
        lock(&self.ready).as_ref().map(|r| UpdateState::Ready {
            version: self.source.version(&r.update),
        })
    }

    /// A failure keeps a downloaded update installable; otherwise it is `Failed`.
    fn fail(&self, error: LauncherError) -> UpdateState {
        let state = self.ready_state().unwrap_or(UpdateState::Failed { error });
        self.set(state)
    }

    /// The check on start, after `delay`. Dev builds never check.
    pub async fn start(&self, delay: Duration) {
        if !self.enabled {
            return;
        }
        tokio::time::sleep(delay).await;
        self.check().await;
    }

    /// Look for a newer version on the current channel and download it.
    pub async fn check(&self) -> UpdateState {
        if !self.enabled {
            return UpdateState::Off;
        }
        let _flight = self.flight.lock().await;
        self.set(UpdateState::Checking);
        let channel = *lock(&self.channel);
        let update = match self.source.check(endpoint(channel)).await {
            Err(error) => return self.fail(error),
            Ok(None) => {
                let state = self.ready_state().unwrap_or(UpdateState::UpToDate);
                return self.set(state);
            }
            Ok(Some(update)) => update,
        };
        let version = self.source.version(&update);
        if self.ready_state()
            == Some(UpdateState::Ready {
                version: version.clone(),
            })
        {
            return self.set(UpdateState::Ready { version });
        }
        self.set(UpdateState::Downloading { percent: Some(0) });
        let update = Arc::new(update);
        match self.source.download(update.clone(), self.progress()).await {
            Err(error) => self.fail(error),
            Ok(bytes) => {
                *lock(&self.ready) = Some(Downloaded {
                    update,
                    bytes: Arc::new(bytes),
                });
                self.set(UpdateState::Ready { version })
            }
        }
    }

    /// Progress events, sent only when the percentage changes.
    fn progress(&self) -> Progress {
        let host = self.host.clone();
        let state = self.state.clone();
        let received = Arc::new(Mutex::new(0u64));
        Arc::new(move |chunk, total| {
            let got = {
                let mut got = lock(&received);
                *got += chunk;
                *got
            };
            let percent = total
                .filter(|t| *t > 0)
                .map(|t| (got.min(t) * 100 / t) as u8);
            let next = UpdateState::Downloading { percent };
            let changed = {
                let mut current = lock(&state);
                let changed = *current != next;
                *current = next.clone();
                changed
            };
            if changed {
                host.emit(AppEvent::Update { state: next });
            }
        })
    }

    /// A new channel drops any update downloaded for the old one, then checks again.
    pub async fn set_channel(&self, channel: Channel) -> UpdateState {
        {
            let _flight = self.flight.lock().await;
            let mut current = lock(&self.channel);
            if *current == channel {
                return self.status();
            }
            *current = channel;
            *lock(&self.ready) = None;
        }
        self.check().await
    }

    /// Restart now: install and relaunch. Refused while anything runs.
    pub async fn restart_now(&self, busy: bool) -> Result<()> {
        if busy {
            return Err(busy_error());
        }
        self.install_ready(true).await.ok_or_else(nothing_ready)?
    }

    /// Install a ready update as the launcher quits, unless something still runs (spec §4.3).
    /// True when the installer started.
    pub async fn on_quit(&self, busy: bool) -> bool {
        if busy {
            return false;
        }
        match self.install_ready(false).await {
            Some(Ok(())) => true,
            Some(Err(e)) => {
                tracing::warn!(
                    code = ?e.code,
                    detail = e.detail.as_deref().unwrap_or(""),
                    "launcher update didn't install on quit"
                );
                false
            }
            None => false,
        }
    }

    /// Install the ready update, consuming it: when Restart now exits the process, the exit hook
    /// must not find the same download and install it a second time. A failed install puts it
    /// back, so quitting can still try. None when nothing is ready.
    async fn install_ready(&self, relaunch: bool) -> Option<Result<()>> {
        let taken = lock(&self.ready).take()?;
        let result = self
            .source
            .install(taken.update.clone(), taken.bytes.clone(), relaunch)
            .await;
        if result.is_err() {
            // A newer download that arrived meanwhile wins.
            lock(&self.ready).get_or_insert(taken);
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::RecordingHost;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::Semaphore;

    #[derive(Default)]
    struct FakeSource {
        found: StdMutex<Option<Result<Option<String>>>>,
        download: StdMutex<Option<Result<Vec<u8>>>>,
        chunks: Vec<(u64, Option<u64>)>,
        endpoints: StdMutex<Vec<&'static str>>,
        installs: StdMutex<Vec<(String, Vec<u8>, bool)>>,
        /// What the next installs return, in order (empty: success).
        install_results: StdMutex<Vec<Result<()>>>,
        /// What a given endpoint offers (None: nothing newer); overrides `found` for that endpoint.
        per_endpoint: StdMutex<Vec<(&'static str, Option<String>)>>,
        /// When set, check waits for one permit (to hold a check in flight).
        check_gate: Option<Arc<Semaphore>>,
        /// When set, download waits for one permit.
        download_gate: Option<Arc<Semaphore>>,
    }

    impl FakeSource {
        fn offering(version: &str) -> Self {
            Self {
                found: StdMutex::new(Some(Ok(Some(version.to_string())))),
                ..Self::default()
            }
        }
    }

    impl UpdateSource for FakeSource {
        type Update = String;
        fn check(&self, endpoint: &'static str) -> BoxFuture<Result<Option<String>>> {
            self.endpoints.lock().unwrap().push(endpoint);
            let offered = self
                .per_endpoint
                .lock()
                .unwrap()
                .iter()
                .find(|(e, _)| *e == endpoint)
                .map(|(_, version)| Ok(version.clone()));
            let result =
                offered.unwrap_or_else(|| self.found.lock().unwrap().clone().unwrap_or(Ok(None)));
            let gate = self.check_gate.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.acquire().await.unwrap().forget();
                }
                result
            })
        }
        fn version(&self, update: &String) -> String {
            update.clone()
        }
        fn download(&self, _update: Arc<String>, progress: Progress) -> BoxFuture<Result<Vec<u8>>> {
            for (chunk, total) in &self.chunks {
                progress(*chunk, *total);
            }
            let result = self
                .download
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| Ok(b"setup".to_vec()));
            let gate = self.download_gate.clone();
            Box::pin(async move {
                if let Some(gate) = gate {
                    gate.acquire().await.unwrap().forget();
                }
                result
            })
        }
        fn install(
            &self,
            update: Arc<String>,
            bytes: Arc<Vec<u8>>,
            relaunch: bool,
        ) -> BoxFuture<Result<()>> {
            self.installs
                .lock()
                .unwrap()
                .push(((*update).clone(), (*bytes).clone(), relaunch));
            let result = {
                let mut results = self.install_results.lock().unwrap();
                if results.is_empty() {
                    Ok(())
                } else {
                    results.remove(0)
                }
            };
            Box::pin(async move { result })
        }
    }

    fn states(host: &RecordingHost) -> Vec<UpdateState> {
        host.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Update { state } => Some(state.clone()),
                _ => None,
            })
            .collect()
    }

    fn ready(version: &str) -> UpdateState {
        UpdateState::Ready {
            version: version.into(),
        }
    }

    fn net_error() -> LauncherError {
        LauncherError::network(
            ErrorCode::LauncherUpdateFailed,
            "Couldn't check for launcher updates",
        )
    }

    #[tokio::test]
    async fn dev_builds_are_off_and_never_check() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let host = RecordingHost::new();
        let updates = Updates::new(source.clone(), host.clone(), BuildKind::Dev, Channel::Beta);
        assert_eq!(updates.status(), UpdateState::Off);
        assert_eq!(updates.check().await, UpdateState::Off);
        updates.start(Duration::ZERO).await;
        assert!(source.endpoints.lock().unwrap().is_empty());
        assert!(states(&host).is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn start_waits_then_downloads_and_is_ready() {
        let source = Arc::new(FakeSource {
            chunks: vec![(50, Some(100)), (50, Some(100))],
            ..FakeSource::offering("0.2.0-beta.2")
        });
        let host = RecordingHost::new();
        let updates = Updates::new(source.clone(), host.clone(), BuildKind::Beta, Channel::Beta);
        assert_eq!(updates.status(), UpdateState::Idle);
        let started = tokio::time::Instant::now();
        updates.start(START_DELAY).await;
        assert!(started.elapsed() >= START_DELAY);
        assert_eq!(*source.endpoints.lock().unwrap(), vec![BETA_ENDPOINT]);
        assert_eq!(
            states(&host),
            vec![
                UpdateState::Checking,
                UpdateState::Downloading { percent: Some(0) },
                UpdateState::Downloading { percent: Some(50) },
                UpdateState::Downloading { percent: Some(100) },
                ready("0.2.0-beta.2"),
            ]
        );
        assert_eq!(updates.status(), ready("0.2.0-beta.2"));
    }

    #[tokio::test]
    async fn nothing_newer_is_up_to_date() {
        let source = Arc::new(FakeSource::default());
        let host = RecordingHost::new();
        let updates = Updates::new(source, host.clone(), BuildKind::Release, Channel::Stable);
        assert_eq!(updates.check().await, UpdateState::UpToDate);
        assert_eq!(
            states(&host),
            vec![UpdateState::Checking, UpdateState::UpToDate]
        );
    }

    #[tokio::test]
    async fn a_failed_check_is_failed_and_quiet() {
        let source = Arc::new(FakeSource {
            found: StdMutex::new(Some(Err(net_error()))),
            ..FakeSource::default()
        });
        let host = RecordingHost::new();
        let updates = Updates::new(source, host.clone(), BuildKind::Beta, Channel::Beta);
        assert_eq!(
            updates.check().await,
            UpdateState::Failed { error: net_error() }
        );
        assert!(host.warnings().is_empty(), "no warning toast on start");
    }

    #[tokio::test]
    async fn a_failed_download_is_failed_and_installs_nothing() {
        let source = Arc::new(FakeSource {
            download: StdMutex::new(Some(Err(net_error()))),
            ..FakeSource::offering("0.2.0-beta.2")
        });
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        assert!(matches!(updates.check().await, UpdateState::Failed { .. }));
        assert!(!updates.on_quit(false).await);
        assert!(source.installs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_recheck_keeps_the_ready_update() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        updates.check().await;
        *source.found.lock().unwrap() = Some(Err(net_error()));
        assert_eq!(updates.check().await, ready("0.2.0-beta.2"));
    }

    #[tokio::test]
    async fn the_same_version_isnt_downloaded_twice() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let host = RecordingHost::new();
        let updates = Updates::new(source, host.clone(), BuildKind::Beta, Channel::Beta);
        updates.check().await;
        host.events.lock().unwrap().clear();
        assert_eq!(updates.check().await, ready("0.2.0-beta.2"));
        assert_eq!(
            states(&host),
            vec![UpdateState::Checking, ready("0.2.0-beta.2")]
        );
    }

    #[tokio::test]
    async fn restart_now_is_refused_while_busy() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        updates.check().await;
        let refused = updates.restart_now(true).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::LauncherUpdateBusy);
        // Busy also means a Browse install or an instance install, not only a game.
        assert_eq!(
            refused.message,
            "Wait for games and installs to finish before restarting to update"
        );
        assert!(source.installs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn restart_now_installs_and_relaunches() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        assert_eq!(
            updates.restart_now(false).await.unwrap_err().code,
            ErrorCode::LauncherUpdateFailed
        );
        updates.check().await;
        updates.restart_now(false).await.unwrap();
        assert_eq!(
            *source.installs.lock().unwrap(),
            vec![("0.2.0-beta.2".to_string(), b"setup".to_vec(), true)]
        );
    }

    #[tokio::test]
    async fn an_install_consumes_the_download() {
        // Restart now ends the process through the exit hook, which asks on_quit: it must not
        // install the same download again.
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        updates.check().await;
        updates.restart_now(false).await.unwrap();
        assert!(!updates.on_quit(false).await, "already installed");
        assert_eq!(source.installs.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_failed_install_keeps_the_download_for_quit() {
        let failure = LauncherError::network(ErrorCode::LauncherUpdateFailed, "no");
        let source = Arc::new(FakeSource {
            install_results: StdMutex::new(vec![Err(failure)]),
            ..FakeSource::offering("0.2.0-beta.2")
        });
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        updates.check().await;
        assert!(updates.restart_now(false).await.is_err());
        assert_eq!(updates.status(), ready("0.2.0-beta.2"));
        assert!(updates.on_quit(false).await);
        assert_eq!(
            *source.installs.lock().unwrap(),
            vec![
                ("0.2.0-beta.2".to_string(), b"setup".to_vec(), true),
                ("0.2.0-beta.2".to_string(), b"setup".to_vec(), false),
            ]
        );
    }

    #[tokio::test]
    async fn quit_installs_without_relaunching_unless_busy() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        assert!(!updates.on_quit(false).await, "nothing ready yet");
        updates.check().await;
        assert!(!updates.on_quit(true).await, "a game still runs");
        assert!(source.installs.lock().unwrap().is_empty());
        assert!(updates.on_quit(false).await);
        assert_eq!(
            *source.installs.lock().unwrap(),
            vec![("0.2.0-beta.2".to_string(), b"setup".to_vec(), false)]
        );
    }

    #[tokio::test]
    async fn quit_during_a_download_installs_nothing() {
        let gate = Arc::new(Semaphore::new(0));
        let source = Arc::new(FakeSource {
            download_gate: Some(gate.clone()),
            ..FakeSource::offering("0.2.0-beta.2")
        });
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        let running = {
            let u = updates.clone();
            tokio::spawn(async move { u.check().await })
        };
        while !matches!(updates.status(), UpdateState::Downloading { .. }) {
            tokio::task::yield_now().await;
        }
        assert!(!updates.on_quit(false).await);
        gate.add_permits(1);
        running.await.unwrap();
        assert!(source.installs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn channel_change_drops_the_ready_update_and_checks_the_new_endpoint() {
        let source = Arc::new(FakeSource::offering("0.2.0-beta.2"));
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        updates.check().await;
        *source.found.lock().unwrap() = Some(Ok(None));
        assert_eq!(
            updates.set_channel(Channel::Stable).await,
            UpdateState::UpToDate
        );
        assert_eq!(
            *source.endpoints.lock().unwrap(),
            vec![BETA_ENDPOINT, STABLE_ENDPOINT]
        );
        assert!(!updates.on_quit(false).await, "the beta download is gone");
        // The same channel again doesn't check.
        updates.set_channel(Channel::Stable).await;
        assert_eq!(source.endpoints.lock().unwrap().len(), 2);
    }

    /// Holds a beta check in flight, switches to stable meanwhile, and lets both finish.
    async fn switch_during_a_check(
        stable: Option<&str>,
    ) -> (Arc<FakeSource>, Arc<Updates<FakeSource, RecordingHost>>) {
        let gate = Arc::new(Semaphore::new(0));
        let source = Arc::new(FakeSource {
            check_gate: Some(gate.clone()),
            per_endpoint: StdMutex::new(vec![
                (BETA_ENDPOINT, Some("0.2.0-beta.2".to_string())),
                (STABLE_ENDPOINT, stable.map(str::to_string)),
            ]),
            ..FakeSource::default()
        });
        let updates = Updates::new(
            source.clone(),
            RecordingHost::new(),
            BuildKind::Beta,
            Channel::Beta,
        );
        let first = {
            let u = updates.clone();
            tokio::spawn(async move { u.check().await })
        };
        while source.endpoints.lock().unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
        let switch = {
            let u = updates.clone();
            tokio::spawn(async move { u.set_channel(Channel::Stable).await })
        };
        gate.add_permits(2);
        first.await.unwrap();
        switch.await.unwrap();
        assert_eq!(
            *source.endpoints.lock().unwrap(),
            vec![BETA_ENDPOINT, STABLE_ENDPOINT]
        );
        (source, updates)
    }

    #[tokio::test]
    async fn channel_change_waits_for_the_running_check() {
        // The beta download lands first, then the switch drops it; the stable check offers a
        // different version, so the final state can only be the stable one.
        let (source, updates) = switch_during_a_check(Some("0.2.0")).await;
        assert_eq!(updates.status(), ready("0.2.0"));
        assert!(updates.on_quit(false).await);
        assert_eq!(
            *source.installs.lock().unwrap(),
            vec![("0.2.0".to_string(), b"setup".to_vec(), false)]
        );
    }

    #[tokio::test]
    async fn a_beta_download_finishing_during_a_switch_never_survives_it() {
        // Stable has nothing newer, so a beta update that outlived the switch would show up as
        // `Ready` here (the failure mode of switching without waiting for the running check).
        let (source, updates) = switch_during_a_check(None).await;
        assert_eq!(updates.status(), UpdateState::UpToDate);
        assert!(!updates.on_quit(false).await);
        assert!(source.installs.lock().unwrap().is_empty());
    }
}
