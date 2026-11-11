//! Anonymous error reports and usage counts, as the app uses them: one [`Telemetry`] that
//! decides whether anything is collected at all, keeps the small state file, and sends in the
//! background. Nothing here can fail towards the player: every problem ends in silence, and
//! what couldn't be sent waits for the next start.
//!
//! Design: `docs/superpowers/specs/2026-11-11-anonymous-usage-and-errors-design.md`.
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex, MutexGuard, RwLock,
    },
    time::Duration,
};

use launcher_core::{
    accounts::BuildKind,
    http::HttpClient,
    invite::{client::check_engine_url, InviteClient, Url},
    telemetry::{
        install_week_to_send, payload::PingChannel, report_for_error, state::BATCH_BYTES, Client,
        Counter, Day, ErrorBatch, Outcome, Ping, Report, ReportKind, Scrubber, Sender, State,
        STATE_FILE,
    },
    ErrorCode, LauncherError,
};

use crate::{
    events::{AppEvent, Host, SignInWindow},
    runner::BoxFuture,
};

/// `PERLAND_LAUNCHER_TELEMETRY`: `0` (or `false`, `off`, `no`) turns everything off.
pub const SWITCH_ENV: &str = "PERLAND_LAUNCHER_TELEMETRY";
/// `PERLAND_LAUNCHER_TELEMETRY_URL`: an Engine to send to instead (https, or http on this
/// computer). The one way a dev build or a scratch data folder sends anything: for testing.
pub const ENDPOINT_ENV: &str = "PERLAND_LAUNCHER_TELEMETRY_URL";

/// A report recorded while the launcher runs is sent this long after, with whatever else
/// arrived meanwhile.
const FLUSH_DELAY: Duration = Duration::from_secs(30);
/// Requests per flush: with 20 reports each, more than the queue holds.
const FLUSH_BATCHES: usize = 5;
/// Reports the window may hand over per run of the launcher: a loop in the UI can't flood.
const UI_REPORTS_PER_RUN: u32 = 30;

/// Not errors worth counting: what the player chose to stop, and having no internet (which
/// could not be reported anyway, and repeats with every poll while it lasts).
const NOT_REPORTED: &[ErrorCode] = &[
    ErrorCode::SignInCanceled,
    ErrorCode::SyncCancelled,
    ErrorCode::ImportCancelled,
    ErrorCode::Offline,
];

/// Whether this launcher collects anything, and where it would send it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Nothing is recorded and nothing is sent.
    Off,
    /// Straight to this Engine (the testing override).
    Engine(Url),
    /// To the Engine this Perland site names, as the public server list finds it.
    Site(Url),
}

/// The mode for this run. `switch` and `endpoint` are the two environment variables,
/// `scratch_root` is whether `PERLAND_LAUNCHER_ROOT` points the launcher at another data
/// folder, and `site` is the Perland site the server list reads.
///
/// The switch wins over everything. Then an endpoint named on purpose. Without one, a dev
/// build and a scratch folder never send.
pub fn mode(
    build: BuildKind,
    switch: Option<&str>,
    scratch_root: bool,
    endpoint: Option<&str>,
    site: Url,
) -> Mode {
    let off = switch.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "no"
        )
    });
    if off {
        return Mode::Off;
    }
    match endpoint.map(str::trim).filter(|raw| !raw.is_empty()) {
        // An endpoint that was asked for but can't be used must not fall back to the real one.
        Some(raw) => check_engine_url(raw).map_or(Mode::Off, Mode::Engine),
        None if build == BuildKind::Dev || scratch_root => Mode::Off,
        None => Mode::Site(site),
    }
}

pub struct TelemetryConfig {
    pub mode: Mode,
    /// The launcher's data folder: where the state file lives, and a path to scrub.
    pub data_root: PathBuf,
    pub launcher_version: String,
    pub http: HttpClient,
}

pub struct Telemetry {
    mode: Mode,
    path: PathBuf,
    data_root: PathBuf,
    version: String,
    sender: Sender,
    invites: InviteClient,
    /// The player's switch. Off until the settings are read, so nothing is recorded before.
    sharing: AtomicBool,
    /// The names already on disk were learned (`names_ready`). Until then no report is
    /// recorded: a scrubber that knows no account, instance or server must not write one.
    names_ready: AtomicBool,
    /// `arm_from_disk` ran: it decides once per run of the launcher.
    begun: AtomicBool,
    /// The player was shown the notice before this run of the launcher started. A notice
    /// shown during this run counts from the next start, so a player who reads it and turns
    /// the switch off has sent nothing.
    armed: AtomicBool,
    /// Read from the file on first use.
    state: Mutex<Option<State>>,
    /// Everything the launcher knows that must never leave: names, addresses, tokens.
    names: Mutex<BTreeSet<String>>,
    scrubber: RwLock<Arc<Scrubber>>,
    locale: Mutex<String>,
    flush_queued: AtomicBool,
    flush_delay: Duration,
    /// One send at a time.
    sending: tokio::sync::Mutex<()>,
    ui_reports: AtomicU32,
    /// A send was not taken (no network, a busy Engine): no more sends until the next start.
    gave_up: AtomicBool,
    /// Today, when a test says so.
    today: Mutex<Option<Day>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Telemetry {
    pub fn new(config: TelemetryConfig) -> Arc<Self> {
        let scrubber = Scrubber::for_current_user(&config.data_root);
        // Its own client, which follows no redirect: a body is never sent on to wherever an
        // answer points.
        let sender = Sender::new(
            launcher_core::http::client_without_redirects(&config.launcher_version)
                .unwrap_or_else(|_| config.http.clone()),
        );
        Arc::new(Self {
            mode: config.mode,
            path: config.data_root.join(STATE_FILE),
            data_root: config.data_root,
            version: config.launcher_version,
            sender,
            invites: InviteClient::new(config.http),
            sharing: AtomicBool::new(false),
            names_ready: AtomicBool::new(false),
            begun: AtomicBool::new(false),
            armed: AtomicBool::new(false),
            state: Mutex::new(None),
            names: Mutex::new(BTreeSet::new()),
            scrubber: RwLock::new(Arc::new(scrubber)),
            locale: Mutex::new(String::new()),
            flush_queued: AtomicBool::new(false),
            flush_delay: FLUSH_DELAY,
            sending: tokio::sync::Mutex::new(()),
            ui_reports: AtomicU32::new(0),
            gave_up: AtomicBool::new(false),
            today: Mutex::new(None),
        })
    }

    /// A telemetry that never records or sends, for tests and tools.
    pub fn off() -> Arc<Self> {
        Self::new(TelemetryConfig {
            mode: Mode::Off,
            data_root: PathBuf::new(),
            launcher_version: String::new(),
            http: HttpClient::new(),
        })
    }

    /// Sharing on and the notice long shown, for tests of everything after that.
    #[cfg(test)]
    pub(crate) fn force_on(&self) {
        self.sharing.store(true, Ordering::SeqCst);
        self.armed.store(true, Ordering::SeqCst);
        self.names_ready.store(true, Ordering::SeqCst);
    }

    /// What is collected right now, for tests.
    #[cfg(test)]
    pub(crate) fn snapshot(&self) -> State {
        lock(&self.state)
            .get_or_insert_with(|| State::load(&self.path))
            .clone()
    }

    /// Is anything being collected right now?
    pub fn is_on(&self) -> bool {
        self.wanted() && self.armed.load(Ordering::SeqCst)
    }

    /// Whether a report (an error, a panic) would be recorded right now: collecting is on and
    /// the names on disk are known. Counters need no names and only ask `is_on`.
    pub fn records_reports(&self) -> bool {
        self.is_on() && self.names_ready.load(Ordering::SeqCst)
    }

    /// The names already on disk have been handed to `add_names`: reports may be recorded.
    pub fn names_ready(&self) {
        self.names_ready.store(true, Ordering::SeqCst);
    }

    /// Sharing is possible in this build and the player's switch is on. Whether anything is
    /// collected also depends on the notice (`is_on`).
    fn wanted(&self) -> bool {
        self.mode != Mode::Off && self.sharing.load(Ordering::SeqCst)
    }

    /// Once, when the launcher starts and after the switch is read: collecting is allowed
    /// from now if the player was shown the notice in an earlier run.
    pub fn arm_from_disk(&self) {
        // Once per run: a notice shown later in this run can't arm it, whoever calls again.
        if !self.wanted() || self.begun.swap(true, Ordering::SeqCst) {
            return;
        }
        let mut guard = lock(&self.state);
        let state = guard.get_or_insert_with(|| State::load(&self.path));
        self.armed
            .store(state.notice_shown().is_some(), Ordering::SeqCst);
    }

    /// Does the player still have to be shown that the launcher shares anonymous data? Never
    /// when nothing would be shared anyway (the switch is off, a dev build).
    pub fn notice_due(&self) -> bool {
        if !self.wanted() {
            return false;
        }
        let mut guard = lock(&self.state);
        guard
            .get_or_insert_with(|| State::load(&self.path))
            .notice_shown()
            .is_none()
    }

    /// The switch can't be trusted any more (the settings couldn't be read): sharing is off,
    /// and the player has to be shown the notice again before anything is collected, even
    /// if the settings are later saved with sharing on.
    pub fn distrust(&self) {
        self.sharing.store(false, Ordering::SeqCst);
        self.armed.store(false, Ordering::SeqCst);
        self.begun.store(true, Ordering::SeqCst);
        if self.mode == Mode::Off || !self.path.is_file() {
            return;
        }
        let mut guard = lock(&self.state);
        let state = guard.get_or_insert_with(|| State::load(&self.path));
        let forgot = state.forget_notice();
        if forgot || !state.is_empty() {
            state.clear_collected();
            self.save(state);
        }
    }

    /// A new run of the launcher, for tests that play several on one `Telemetry`.
    #[cfg(test)]
    pub(crate) fn restart(&self) {
        self.begun.store(false, Ordering::SeqCst);
        self.armed.store(false, Ordering::SeqCst);
    }

    /// The player has now been shown it (What's new, the first run, or by turning the switch
    /// on themselves). Remembered on disk; collecting starts with the launcher's next start.
    pub fn notice_shown(&self) {
        if !self.wanted() {
            return;
        }
        let today = self.today();
        let mut guard = lock(&self.state);
        let state = guard.get_or_insert_with(|| State::load(&self.path));
        // Also the week this install counts from.
        if state.note_notice(today) | state.note_install(today) {
            self.save(state);
        }
    }

    /// The player's switch. Turning it off stops everything at once and deletes the reports
    /// and counters collected so far.
    pub fn set_sharing(&self, share: bool) {
        self.sharing.store(share, Ordering::SeqCst);
        if self.mode == Mode::Off {
            return;
        }
        // Only a file that exists is rewritten: off never creates one.
        if !share && self.path.is_file() {
            let mut guard = lock(&self.state);
            let state = guard.get_or_insert_with(|| State::load(&self.path));
            if !state.is_empty() {
                state.clear_collected();
                self.save(state);
            }
        }
    }

    fn save(&self, state: &State) {
        if let Err(e) = state.save(&self.path) {
            tracing::debug!(code = ?e.code, "couldn't save usage data");
        }
    }

    /// Changes the state and saves it when `change` says it changed. Nothing when off.
    fn with_state<T: Default>(&self, change: impl FnOnce(&mut State) -> (T, bool)) -> T {
        if !self.is_on() {
            return T::default();
        }
        self.change(lock(&self.state), change)
    }

    fn change<T>(
        &self,
        mut guard: MutexGuard<'_, Option<State>>,
        change: impl FnOnce(&mut State) -> (T, bool),
    ) -> T {
        let state = guard.get_or_insert_with(|| State::load(&self.path));
        let (out, changed) = change(state);
        if self.is_on() {
            if changed {
                self.save(state);
            }
        } else {
            // The switch went off while this waited for the lock: what it just added must
            // not sit in memory for the switch to come back on.
            state.clear_collected();
        }
        out
    }

    fn today(&self) -> Day {
        (*lock(&self.today)).unwrap_or_else(Day::today)
    }

    fn scrubber(&self) -> Arc<Scrubber> {
        self.scrubber
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Names, addresses and tokens the launcher has seen: from now on they are masked in
    /// every report. Cheap when nothing is new.
    pub fn add_names<I, S>(&self, names: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        if self.mode == Mode::Off {
            return;
        }
        let mut known = lock(&self.names);
        let before = known.len();
        for name in names {
            let name = name.as_ref().trim();
            if name.chars().count() >= 2 {
                known.insert(name.to_string());
            }
        }
        if known.len() == before {
            return;
        }
        let mut scrubber = Scrubber::for_current_user(&self.data_root);
        scrubber.add_secrets(known.iter());
        *self.scrubber.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(scrubber);
    }

    /// One more of `counter` since the last ping.
    pub fn count(&self, counter: Counter) {
        self.with_state(|state| {
            state.count(counter);
            ((), true)
        });
    }

    /// Takes back a `count` that turned out not to have happened.
    pub fn uncount(&self, counter: Counter) {
        self.with_state(|state| {
            state.drop_counted(&[(counter.as_str().to_string(), 1)].into());
            ((), true)
        });
    }

    /// Queues a finished report and sends it a little later.
    pub fn report(self: &Arc<Self>, report: Report) {
        if !self.records_reports() {
            return;
        }
        self.with_state(|state| {
            state.push(report);
            ((), true)
        });
        self.queue_flush();
    }

    /// An error the player was shown. `command` is the UI command that failed, if one did.
    pub fn report_error(self: &Arc<Self>, error: &LauncherError, command: Option<&str>) {
        if !self.records_reports() || NOT_REPORTED.contains(&error.code) {
            return;
        }
        self.report(report_for_error(error, command, &self.scrubber()));
    }

    /// An error from the window's own code. `kind` is `exception` or `unhandled_rejection`;
    /// anything else is ignored.
    pub fn report_ui(self: &Arc<Self>, kind: &str, message: &str, stack: Option<&str>) {
        let kind = match kind {
            "exception" => ReportKind::Exception,
            "unhandled_rejection" => ReportKind::UnhandledRejection,
            _ => return,
        };
        if !self.records_reports()
            || self.ui_reports.fetch_add(1, Ordering::SeqCst) >= UI_REPORTS_PER_RUN
        {
            return;
        }
        let scrubber = self.scrubber();
        let mut report = Report::new(kind, &scrubber.scrub(message));
        if let Some(stack) = stack {
            report = report.stack(&scrubber.scrub(stack));
        }
        self.report(report);
    }

    /// The game crashed: the crash doctor's verdict (a `crash_*` code, if it had one), the
    /// Minecraft version and the loader. Never a line of the game's log.
    pub fn report_crash(self: &Arc<Self>, cause: Option<ErrorCode>, minecraft: &str, loader: &str) {
        if !self.records_reports() {
            return;
        }
        let code = cause
            .and_then(|code| serde_json::to_value(code).ok())
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "crash_unknown".into());
        self.report(
            Report::new(ReportKind::GameCrash, "The game crashed")
                .code(&code)
                .context("minecraft", minecraft)
                .context("loader", loader),
        );
    }

    /// The launcher panicked. Called from the panic hook, so it never waits for a lock (the
    /// panic may have happened while one was held) and never starts a send: the report goes
    /// out at the next start.
    pub fn report_panic(&self, message: &str, location: Option<&str>, backtrace: &str) {
        if !self.records_reports() {
            return;
        }
        let Ok(scrubber) = self.scrubber.try_read().map(|s| s.clone()) else {
            return;
        };
        let mut report = Report::new(ReportKind::Panic, &scrubber.scrub(message))
            .stack(&scrubber.scrub(backtrace));
        if let Some(location) = location {
            report = report.context("location", &scrubber.scrub(location));
        }
        let guard = match self.state.try_lock() {
            Ok(guard) => guard,
            Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        self.change(guard, |state| {
            state.push(report);
            ((), true)
        });
    }

    fn queue_flush(self: &Arc<Self>) {
        if self.flush_queued.swap(true, Ordering::SeqCst) {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            // No runtime here: the report waits for the next start.
            self.flush_queued.store(false, Ordering::SeqCst);
            return;
        };
        let me = self.clone();
        runtime.spawn(async move {
            tokio::time::sleep(me.flush_delay).await;
            me.flush_queued.store(false, Ordering::SeqCst);
            me.flush().await;
        });
    }

    /// The Engine to send to, or `None` when it can't be found right now.
    async fn engine(&self) -> Option<Url> {
        match &self.mode {
            Mode::Off => None,
            Mode::Engine(engine) => Some(engine.clone()),
            Mode::Site(site) => self.invites.engine_for_site(site).await.ok(),
        }
    }

    /// The launcher's language (`es`, `en-US`), said in both bodies.
    pub fn set_locale(&self, locale: &str) {
        *lock(&self.locale) = locale.to_string();
    }

    fn client(&self) -> Client {
        Client::current(&self.version, &lock(&self.locale))
    }

    /// The first thing after the launcher starts: remembers the install week, sends today's
    /// ping if none went out today, then the reports waiting from earlier runs.
    /// `signed_in` is whether a Perland account is.
    pub async fn start(&self, signed_in: bool, channel: PingChannel) {
        if !self.is_on() {
            return;
        }
        let today = self.today();
        self.with_state(|state| ((), state.note_install(today)));
        self.ping(today, signed_in, channel).await;
        self.flush().await;
    }

    async fn ping(&self, today: Day, signed_in: bool, channel: PingChannel) {
        let client = self.client();
        // The contract's ping knows three systems.
        if client.os == "other" {
            return;
        }
        let _sending = self.sending.lock().await;
        let ping = self.with_state(|state| {
            let ping = state.ping_due(today).and_then(|flags| {
                Some(Ping {
                    version: client.version.clone(),
                    channel,
                    os: client.os,
                    arch: client.arch.clone(),
                    locale: client.locale.clone(),
                    signed_in,
                    first_ever: flags.first_ever,
                    first_this_week: flags.first_this_week,
                    first_this_month: flags.first_this_month,
                    install_week: install_week_to_send(state.install_week()?, today),
                    counters: state.counters().clone(),
                })
            });
            (ping, false)
        });
        let Some(ping) = ping else { return };
        let Some(engine) = self.engine().await else {
            self.gave_up.store(true, Ordering::SeqCst);
            return;
        };
        if !self.is_on() {
            return;
        }
        match self.sender.ping(&engine, &ping).await {
            // Remembered even if the switch went off meanwhile: the Engine counted this day.
            Outcome::Delivered => {
                let mut guard = lock(&self.state);
                let state = guard.get_or_insert_with(|| State::load(&self.path));
                state.ping_delivered(today, &ping.counters);
                if !self.is_on() {
                    state.clear_collected();
                }
                self.save(state);
            }
            // The Engine will never take this body: its counters go, and the next start
            // (not this run) tries a new one.
            Outcome::Rejected => self.with_state(|state| {
                state.drop_counted(&ping.counters);
                ((), true)
            }),
            Outcome::Retry => self.gave_up.store(true, Ordering::SeqCst),
        }
    }

    /// Sends the queued reports. What the Engine takes or refuses for good is forgotten;
    /// anything else stays for the next start.
    pub async fn flush(&self) {
        if !self.is_on() || self.gave_up.load(Ordering::SeqCst) {
            return;
        }
        let _sending = self.sending.lock().await;
        let mut engine = None;
        for _ in 0..FLUSH_BATCHES {
            let batch =
                self.with_state(|state| (state.batches(BATCH_BYTES).into_iter().next(), false));
            let Some(batch) = batch else { return };
            if engine.is_none() {
                engine = self.engine().await;
            }
            let Some(engine) = &engine else {
                self.gave_up.store(true, Ordering::SeqCst);
                return;
            };
            if !self.is_on() {
                return;
            }
            let body = ErrorBatch::new(&self.client(), batch);
            match self.sender.errors(engine, &body).await {
                Outcome::Delivered | Outcome::Rejected => self.with_state(|state| {
                    state.forget(body.reports());
                    ((), true)
                }),
                Outcome::Retry => {
                    self.gave_up.store(true, Ordering::SeqCst);
                    return;
                }
            }
        }
    }
}

static APP: std::sync::OnceLock<Arc<Telemetry>> = std::sync::OnceLock::new();

/// Makes `telemetry` the one [`report_command_error`] reports to. Set once, when the app
/// starts; a second call changes nothing.
pub fn set_app_telemetry(telemetry: Arc<Telemetry>) {
    let _ = APP.set(telemetry);
}

/// An error a UI command answered with, on its way to the player. Nothing happens before
/// [`set_app_telemetry`].
pub fn report_command_error(command: &'static str, error: &LauncherError) {
    if let Some(telemetry) = APP.get() {
        telemetry.report_error(error, Some(command));
    }
}

/// What an accounts file holds that must never leave: every account's name and id.
pub fn account_names(file: &launcher_core::accounts::AccountsFile) -> Vec<String> {
    file.accounts
        .iter()
        .flat_map(|account| {
            // The id in both spellings: with and without dashes.
            let plain = account.uuid.replace('-', "");
            let dashed = if plain.len() == 32 && plain.is_ascii() {
                format!(
                    "{}-{}-{}-{}-{}",
                    &plain[..8],
                    &plain[8..12],
                    &plain[12..16],
                    &plain[16..20],
                    &plain[20..]
                )
            } else {
                plain.clone()
            };
            [account.username.clone(), plain, dashed]
        })
        .collect()
}

/// What a Perland profile holds that must never leave.
pub fn profile_names(profile: &launcher_core::perland::Profile) -> Vec<String> {
    [
        Some(profile.name.clone()),
        profile.email.clone(),
        profile.minecraft.clone(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// What a Perland session holds that must never leave: the profile, the token, and the
/// Engine it belongs to.
pub fn session_names(session: &launcher_core::perland::Session) -> Vec<String> {
    let mut names = profile_names(&session.profile);
    names.push(session.token.clone());
    names.extend(session.engine.host_str().map(str::to_owned));
    names
}

/// What an instance holds that must never leave: its name and folder, its server's name and
/// address, its invite's token and site, and where its pack came from.
pub fn instance_names(instance: &launcher_core::instances::Instance) -> Vec<String> {
    use launcher_core::{instances::SourceRef, invite::InviteLink};
    let host_of = |raw: &str| Url::parse(raw).ok()?.host_str().map(str::to_owned);
    let mut names = vec![instance.id.clone(), instance.name.clone()];
    if let Some(server) = &instance.server {
        names.push(server.name.clone());
        names.extend(server.join.clone());
        names.extend(server.address.as_ref().map(|a| a.host.clone()));
    }
    match &instance.source {
        SourceRef::Perland { invite, engine } => {
            if let Ok(link) = InviteLink::parse(invite) {
                names.push(link.token().to_string());
                names.push(link.host().to_string());
            }
            names.extend(host_of(engine));
        }
        SourceRef::Packwiz { url } => names.extend(host_of(url)),
        SourceRef::Mrpack { name, .. } => names.push(name.clone()),
        SourceRef::Modrinth { .. } | SourceRef::None => {}
    }
    names
}

/// From now on a panic anywhere in the launcher leaves a scrubbed report (its message, where
/// it happened and the backtrace) for the next start to send. The hook that was there before
/// still runs.
pub fn install_panic_hook(telemetry: Arc<Telemetry>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if !telemetry.records_reports() {
            return previous(info);
        }
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "panic".into());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        telemetry.report_panic(&message, location.as_deref(), &backtrace);
        previous(info);
    }));
}

/// A host that reports the errors on their way to the window: every warning, and the error a
/// run ended with. (Errors a command returns are reported where the command is answered.)
pub struct ReportingHost<H: Host> {
    inner: H,
    telemetry: Arc<Telemetry>,
}

impl<H: Host> ReportingHost<H> {
    pub fn new(inner: H, telemetry: Arc<Telemetry>) -> Self {
        Self { inner, telemetry }
    }
}

impl<H: Host> Host for ReportingHost<H> {
    fn emit(&self, event: AppEvent) {
        match &event {
            AppEvent::Warning { error, .. } => self.telemetry.report_error(error, None),
            AppEvent::RunState {
                exit: Some(exit), ..
            } => {
                if let Some(error) = &exit.error {
                    self.telemetry.report_error(error, Some("play"));
                }
            }
            _ => {}
        }
        self.inner.emit(event);
    }
    fn minimize(&self) {
        self.inner.minimize()
    }
    fn restore(&self) {
        self.inner.restore()
    }
    fn open_url(&self, url: &str) {
        self.inner.open_url(url)
    }
    fn open_sign_in_window(&self, window: SignInWindow) -> BoxFuture<Option<String>> {
        self.inner.open_sign_in_window(window)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{events::ExitInfo, testing::RecordingHost};
    use serde_json::{json, Value};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    const ERRORS: &str = "/api/v1/telemetry/errors";
    const PING: &str = "/api/v1/telemetry/launcher/ping";

    fn site() -> Url {
        Url::parse("https://play.perland.online/").unwrap()
    }

    /// A fake Engine answering both routes with `errors` and `ping`.
    pub(crate) async fn engine(errors: u16, ping: u16) -> MockServer {
        let server = MockServer::start().await;
        for (route, status) in [(ERRORS, errors), (PING, ping)] {
            Mock::given(method("POST"))
                .and(path(route))
                .respond_with(ResponseTemplate::new(status))
                .mount(&server)
                .await;
        }
        server
    }

    /// A telemetry that sends to `server`, sharing on, with a flush right after each report.
    pub(crate) fn telemetry_to(server: &MockServer, root: &std::path::Path) -> Arc<Telemetry> {
        let telemetry = with_mode(Mode::Engine(Url::parse(&server.uri()).unwrap()), root);
        telemetry.force_on();
        telemetry
    }

    fn with_mode(mode: Mode, root: &std::path::Path) -> Arc<Telemetry> {
        let mut telemetry = Telemetry::new(TelemetryConfig {
            mode,
            data_root: root.to_path_buf(),
            launcher_version: "9.9.9".into(),
            http: launcher_core::http::client("9.9.9").unwrap(),
        });
        Arc::get_mut(&mut telemetry).unwrap().flush_delay = Duration::ZERO;
        telemetry
    }

    fn set_today(telemetry: &Telemetry, date: &str) {
        *lock(&telemetry.today) = Some(date.parse().unwrap());
    }

    pub(crate) async fn bodies(server: &MockServer, route: &str) -> Vec<Value> {
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|r| r.url.path() == route)
            .map(|r| r.body_json().unwrap())
            .collect()
    }

    fn file(root: &std::path::Path) -> Option<Value> {
        std::fs::read(root.join(STATE_FILE))
            .ok()
            .map(|bytes| serde_json::from_slice(&bytes).unwrap())
    }

    fn disk_error() -> LauncherError {
        LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save settings")
            .with_param("item", "settings")
    }

    /// Lets the queued flush (zero delay) run to its end.
    async fn settle(telemetry: &Telemetry) {
        for _ in 0..200 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if !telemetry.flush_queued.load(Ordering::SeqCst) {
                let _done = telemetry.sending.lock().await;
                return;
            }
        }
    }

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn the_mode_follows_the_switch_the_build_and_the_data_folder() {
        use BuildKind::*;
        let to_site = Mode::Site(site());
        for build in [Release, Beta] {
            assert_eq!(mode(build, None, false, None, site()), to_site);
            assert_eq!(mode(build, Some("1"), false, None, site()), to_site);
            assert_eq!(mode(build, Some(""), false, Some("  "), site()), to_site);
            // A scratch data folder never reaches the real Engine.
            assert_eq!(mode(build, None, true, None, site()), Mode::Off);
        }
        // A dev build never sends.
        assert_eq!(mode(Dev, None, false, None, site()), Mode::Off);
        assert_eq!(mode(Dev, Some("1"), false, None, site()), Mode::Off);
        // The switch turns any build off, whatever else is set.
        for off in ["0", "false", "OFF", " no "] {
            for build in [Release, Beta, Dev] {
                assert_eq!(
                    mode(build, Some(off), false, Some("http://127.0.0.1:9"), site()),
                    Mode::Off
                );
            }
        }
        // An endpoint named on purpose is the one way a dev build or scratch folder sends.
        let local = Mode::Engine(url("http://127.0.0.1:9/"));
        assert_eq!(
            mode(Dev, None, true, Some("http://127.0.0.1:9"), site()),
            local
        );
        assert_eq!(
            mode(Release, None, true, Some(" http://127.0.0.1:9 "), site()),
            local
        );
        // One that can't be used turns it off rather than falling back to the real Engine.
        for bad in [
            "http://example.com",
            "not a url",
            "https://u:p@example.com",
            "ftp://x",
        ] {
            assert_eq!(
                mode(Release, None, false, Some(bad), site()),
                Mode::Off,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn off_records_nothing_and_sends_nothing() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        // Mode off (a dev build, the switch, a scratch folder), with the player's switch on.
        let off = with_mode(Mode::Off, tmp.path());
        off.force_on();
        off.notice_shown();
        assert!(!off.notice_due());
        // The player's switch off, with a mode that would send.
        let unshared = with_mode(Mode::Engine(url(&server.uri())), tmp.path());
        unshared.force_on();
        unshared.set_sharing(false);
        unshared.notice_shown();
        assert!(!unshared.notice_due());
        // Before the settings were read at all.
        let unread = with_mode(Mode::Engine(url(&server.uri())), tmp.path());
        for telemetry in [off, unshared, unread] {
            assert!(!telemetry.is_on());
            telemetry.count(Counter::GameLaunches);
            telemetry.report_error(&disk_error(), Some("settings_set"));
            telemetry.report_ui("exception", "boom", Some("at x"));
            telemetry.report_crash(Some(ErrorCode::CrashOutOfMemory), "1.20.1", "fabric");
            telemetry.report_panic("boom", Some("src/x.rs:1:1"), "frames");
            telemetry.set_locale("es");
            telemetry.start(true, PingChannel::Stable).await;
            telemetry.flush().await;
            settle(&telemetry).await;
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        assert_eq!(file(tmp.path()), None, "no file was written");
        assert!(std::fs::read_dir(tmp.path()).unwrap().next().is_none());
    }

    /// A launcher start: a new `Telemetry` on the same folder, the switch read, armed from disk.
    fn started(server: &MockServer, root: &std::path::Path, share: bool) -> Arc<Telemetry> {
        let telemetry = with_mode(Mode::Engine(url(&server.uri())), root);
        telemetry.set_sharing(share);
        telemetry.arm_from_disk();
        telemetry.names_ready();
        telemetry
    }

    /// Everything a running launcher could record or send.
    async fn use_everything(telemetry: &Arc<Telemetry>) {
        telemetry.count(Counter::GameLaunches);
        telemetry.report_error(&disk_error(), Some("settings_set"));
        telemetry.report_ui("exception", "boom", None);
        telemetry.report_crash(None, "1.20.1", "vanilla");
        telemetry.report_panic("boom", None, "");
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        telemetry.flush().await;
        settle(telemetry).await;
    }

    #[tokio::test]
    async fn nothing_is_collected_until_the_player_was_shown_the_notice_in_an_earlier_run() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();

        // Updated into this version, the notice not shown yet: nothing, however long it runs.
        let first = started(&server, tmp.path(), true);
        assert!(first.notice_due());
        assert!(!first.is_on());
        use_everything(&first).await;
        assert_eq!(file(tmp.path()), None);

        // Shown during this run: remembered, but this run still collects and sends nothing.
        set_today(&first, "2026-11-11");
        first.notice_shown();
        assert!(!first.notice_due());
        assert!(!first.is_on());
        use_everything(&first).await;
        assert_eq!(
            file(tmp.path()).unwrap(),
            json!({
                "version": 1,
                "installWeek": "2026-W46",
                "noticeShown": "2026-11-11",
                "lastPing": null,
                "counters": {},
                "reports": []
            })
        );
        assert!(server.received_requests().await.unwrap().is_empty());

        // The next start, with the notice shown in an earlier run: now it collects and sends.
        let next = started(&server, tmp.path(), true);
        assert!(!next.notice_due());
        assert!(next.is_on());
        set_today(&next, "2026-11-11");
        use_everything(&next).await;
        assert_eq!(bodies(&server, PING).await.len(), 1);
        assert_eq!(bodies(&server, PING).await[0]["firstEver"], true);
        assert!(!bodies(&server, ERRORS).await.is_empty());
    }

    #[tokio::test]
    async fn reading_the_notice_and_turning_the_switch_off_sends_nothing_ever() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let first = started(&server, tmp.path(), true);
        first.notice_shown();
        // The player reads it, opens Settings and turns sharing off, in the same run.
        first.set_sharing(false);
        use_everything(&first).await;
        // Later starts read the switch as off.
        for _ in 0..2 {
            let later = started(&server, tmp.path(), false);
            assert!(!later.is_on());
            assert!(!later.notice_due(), "no notice for what is not shared");
            use_everything(&later).await;
        }
        assert!(server.received_requests().await.unwrap().is_empty());
        let state = file(tmp.path()).unwrap();
        assert_eq!(state["reports"], json!([]));
        assert_eq!(state["counters"], json!({}));
        assert_eq!(state["lastPing"], Value::Null);
    }

    #[tokio::test]
    async fn an_error_is_queued_sent_and_forgotten() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.report_error(&disk_error(), Some("settings_set"));
        // On disk at once: a crash right after would not lose it.
        assert_eq!(
            file(tmp.path()).unwrap()["reports"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        settle(&telemetry).await;
        let sent = bodies(&server, ERRORS).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["source"], "launcher");
        assert_eq!(sent[0]["version"], "9.9.9");
        assert_eq!(
            sent[0]["reports"],
            json!([{
                "kind": "app_error",
                "message": "Couldn't save settings",
                "code": "disk_write_failed",
                "context": { "category": "disk", "command": "settings_set", "p_item": "settings" },
                "count": 1
            }])
        );
        assert_eq!(file(tmp.path()).unwrap()["reports"], json!([]));
    }

    #[tokio::test]
    async fn a_refused_report_is_dropped_and_an_unsent_one_waits_for_the_next_start() {
        let tmp = tempfile::tempdir().unwrap();
        let refusing = engine(400, 202).await;
        let telemetry = telemetry_to(&refusing, tmp.path());
        telemetry.report_error(&disk_error(), None);
        settle(&telemetry).await;
        assert_eq!(bodies(&refusing, ERRORS).await.len(), 1);
        assert_eq!(
            file(tmp.path()).unwrap()["reports"],
            json!([]),
            "never retried"
        );

        let failing = engine(500, 500).await;
        let telemetry = telemetry_to(&failing, tmp.path());
        telemetry.report_error(&disk_error(), None);
        telemetry.report_error(&disk_error(), None);
        settle(&telemetry).await;
        let kept = file(tmp.path()).unwrap();
        assert_eq!(kept["reports"].as_array().unwrap().len(), 1);
        assert_eq!(kept["reports"][0]["count"], 2, "identical reports collapse");

        // The next start, with the Engine back.
        let server = engine(202, 202).await;
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.set_locale("en");
        telemetry.start(false, PingChannel::Stable).await;
        let sent = bodies(&server, ERRORS).await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0]["reports"][0]["count"], 2);
        assert_eq!(file(tmp.path()).unwrap()["reports"], json!([]));
    }

    #[tokio::test]
    async fn after_a_send_that_was_not_taken_nothing_more_is_sent_until_the_next_start() {
        let server = engine(503, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.report_error(&disk_error(), None);
        settle(&telemetry).await;
        assert_eq!(bodies(&server, ERRORS).await.len(), 1);
        // More errors (a poll that keeps failing) queue up without another request.
        for n in 0..5 {
            telemetry.report_ui("exception", &format!("boom {n}"), None);
            settle(&telemetry).await;
        }
        telemetry.flush().await;
        assert_eq!(bodies(&server, ERRORS).await.len(), 1);
        assert_eq!(
            file(tmp.path()).unwrap()["reports"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
    }

    #[tokio::test]
    async fn turning_sharing_off_deletes_what_was_collected_and_stops_at_once() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        set_today(&telemetry, "2026-11-11");
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        telemetry.count(Counter::GameLaunches);
        telemetry.report_error(&disk_error(), None);
        settle(&telemetry).await;
        let before = file(tmp.path()).unwrap();
        assert_eq!(before["counters"], json!({ "game_launches": 1 }));
        assert_eq!(before["reports"].as_array().unwrap().len(), 1);
        let requests = server.received_requests().await.unwrap().len();

        telemetry.set_sharing(false);
        let after = file(tmp.path()).unwrap();
        assert_eq!(after["counters"], json!({}));
        assert_eq!(after["reports"], json!([]));
        assert_eq!(after["installWeek"], "2026-W46", "the dates stay");
        // Nothing more is recorded or sent.
        telemetry.count(Counter::GameLaunches);
        telemetry.report_error(&disk_error(), None);
        telemetry.report_panic("boom", None, "");
        telemetry.flush().await;
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        settle(&telemetry).await;
        assert_eq!(file(tmp.path()).unwrap(), after);
        assert_eq!(server.received_requests().await.unwrap().len(), requests);
    }

    #[tokio::test]
    async fn sharing_turned_off_before_this_run_clears_what_an_earlier_run_left() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let earlier = telemetry_to(&server, tmp.path());
        earlier.report_error(&disk_error(), None);
        settle(&earlier).await;
        started(&server, tmp.path(), false);
        assert_eq!(file(tmp.path()).unwrap()["reports"], json!([]));
    }

    #[tokio::test]
    async fn the_ping_goes_once_a_day_with_the_counters_and_no_identifier() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        // A Wednesday.
        set_today(&telemetry, "2026-11-11");
        telemetry.count(Counter::GameLaunches);
        telemetry.count(Counter::GameLaunches);
        telemetry.count(Counter::SkinsChanged);
        telemetry.set_locale("es");
        telemetry.start(true, PingChannel::Beta).await;
        telemetry.set_locale("es");
        telemetry.start(true, PingChannel::Beta).await;
        let pings = bodies(&server, PING).await;
        assert_eq!(pings.len(), 1, "one ping a day");
        let os = match std::env::consts::OS {
            "windows" => "windows",
            "macos" => "macos",
            _ => "linux",
        };
        assert_eq!(
            pings[0],
            json!({
                "version": "9.9.9",
                "channel": "beta",
                "os": os,
                "arch": std::env::consts::ARCH,
                "locale": "es",
                "signedIn": true,
                "firstEver": true,
                "firstThisWeek": true,
                "firstThisMonth": true,
                "installWeek": "2026-W46",
                "counters": { "game_launches": 2, "skins_changed": 1 }
            })
        );
        let state = file(tmp.path()).unwrap();
        assert_eq!(
            state,
            json!({
                "version": 1,
                "installWeek": "2026-W46",
                "noticeShown": null,
                "lastPing": "2026-11-11",
                "counters": {},
                "reports": []
            }),
            "the whole state: dates, counters and reports"
        );

        // The next day, same week: a new start (a new `Telemetry`, as after a restart).
        let telemetry = telemetry_to(&server, tmp.path());
        set_today(&telemetry, "2026-11-12");
        telemetry.count(Counter::PacksInstalled);
        telemetry.set_locale("");
        telemetry.start(false, PingChannel::Stable).await;
        let pings = bodies(&server, PING).await;
        assert_eq!(pings.len(), 2);
        let second = &pings[1];
        assert_eq!(
            (
                &second["firstEver"],
                &second["firstThisWeek"],
                &second["firstThisMonth"]
            ),
            (&json!(false), &json!(false), &json!(false))
        );
        assert_eq!(second["signedIn"], false);
        assert_eq!(second["locale"], "und");
        assert_eq!(second["installWeek"], "2026-W46");
        assert_eq!(second["counters"], json!({ "packs_installed": 1 }));

        // The clock set back: no ping for a day already pinged, or any day before it.
        for back in ["2026-11-12", "2026-11-11", "2026-10-01"] {
            let telemetry = telemetry_to(&server, tmp.path());
            set_today(&telemetry, back);
            telemetry.set_locale("es");
            telemetry.start(false, PingChannel::Stable).await;
        }
        assert_eq!(bodies(&server, PING).await.len(), 2);

        // A new month.
        let telemetry = telemetry_to(&server, tmp.path());
        set_today(&telemetry, "2026-12-01");
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        let third = bodies(&server, PING).await.remove(2);
        assert_eq!(
            (
                &third["firstEver"],
                &third["firstThisWeek"],
                &third["firstThisMonth"]
            ),
            (&json!(false), &json!(true), &json!(true))
        );
        assert_eq!(third["counters"], json!({}));
    }

    #[tokio::test]
    async fn a_ping_that_fails_keeps_its_counters_and_its_firsts() {
        let tmp = tempfile::tempdir().unwrap();
        let down = engine(202, 503).await;
        let telemetry = telemetry_to(&down, tmp.path());
        set_today(&telemetry, "2026-11-11");
        telemetry.count(Counter::GameLaunches);
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        assert_eq!(bodies(&down, PING).await.len(), 1);
        let state = file(tmp.path()).unwrap();
        assert_eq!(state["lastPing"], Value::Null);
        assert_eq!(state["counters"], json!({ "game_launches": 1 }));

        // The next start that day tries again, still the first ever.
        let up = engine(202, 202).await;
        let telemetry = telemetry_to(&up, tmp.path());
        set_today(&telemetry, "2026-11-11");
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        let ping = bodies(&up, PING).await.remove(0);
        assert_eq!(ping["firstEver"], true);
        assert_eq!(ping["counters"], json!({ "game_launches": 1 }));
    }

    #[tokio::test]
    async fn a_ping_the_engine_refuses_for_good_drops_its_counters() {
        let tmp = tempfile::tempdir().unwrap();
        let refusing = engine(202, 400).await;
        let telemetry = telemetry_to(&refusing, tmp.path());
        set_today(&telemetry, "2026-11-11");
        telemetry.count(Counter::GameLaunches);
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        let state = file(tmp.path()).unwrap();
        assert_eq!(state["counters"], json!({}));
        assert_eq!(state["lastPing"], Value::Null, "not a ping the Engine took");
    }

    #[tokio::test]
    async fn the_site_names_the_engine_as_the_server_list_finds_it() {
        let server = engine(202, 202).await;
        Mock::given(method("GET"))
            .and(path("/config.json"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({ "engineUrl": server.uri() })),
            )
            .mount(&server)
            .await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = with_mode(Mode::Site(url(&server.uri())), tmp.path());
        telemetry.force_on();
        telemetry.set_locale("es");
        telemetry.start(false, PingChannel::Stable).await;
        assert_eq!(bodies(&server, PING).await.len(), 1);

        // A site that can't be reached is silence, and the ping stays due.
        let nowhere = tempfile::tempdir().unwrap();
        let lost = with_mode(Mode::Site(url("http://127.0.0.1:9/")), nowhere.path());
        lost.force_on();
        lost.report_error(&disk_error(), None);
        lost.set_locale("es");
        lost.start(false, PingChannel::Stable).await;
        let state = file(nowhere.path()).unwrap();
        assert_eq!(state["lastPing"], Value::Null);
        assert_eq!(state["reports"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn whole_payloads_from_realistic_errors_hold_nothing_private() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_string_lossy().into_owned();
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.add_names([
            "PedroGamer99",
            "pedro@correo.example",
            "Servidor de Pedro",
            "servidor-de-pedro",
            "play.pedrocraft.net",
            "tok_9f8e7d6c5b4a",
            "Al",
        ]);
        set_today(&telemetry, "2026-11-11");
        telemetry.count(Counter::JoinsByLink);
        telemetry.report_error(
            &LauncherError::network(ErrorCode::HostUnreachable, "Couldn't reach play.pedrocraft.net")
                .with_param("host", "play.pedrocraft.net")
                .with_param("name", "Servidor de Pedro")
                .with_param("address", "play.pedrocraft.net:25565")
                .with_detail(format!(
                    "error sending request for url (https://play.pedrocraft.net/api/v1/invites/tok_9f8e7d6c5b4a/pack?email=pedro@correo.example) from {root}/instances/servidor-de-pedro/mods/a.jar for PedroGamer99 at 203.0.113.9:25565, al"
                )),
            Some("join_add"),
        );
        telemetry.report_ui(
            "unhandled_rejection",
            "Cannot read properties of undefined (reading 'PedroGamer99')",
            Some(&format!(
                "at render (http://tauri.localhost/assets/index-C3x.js:1:2)\nat {root}/x.js:1:1\nServidor de Pedro"
            )),
        );
        telemetry.report_panic(
            &format!("called `Result::unwrap()` on an `Err` value: Os {{ path: \"{root}/instances/servidor-de-pedro/instance.json\" }}"),
            Some("crates/launcher-app/src/runs.rs:120:5"),
            &format!("0: launcher_app::runs::play\n   at {root}/../src/runs.rs:120:5\n1: pedro@correo.example"),
        );
        telemetry.report_crash(Some(ErrorCode::CrashModFailed), "1.20.1", "fabric");
        telemetry.set_locale("es");
        telemetry.start(true, PingChannel::Stable).await;
        settle(&telemetry).await;

        let mut all = bodies(&server, ERRORS).await;
        let reports: usize = all
            .iter()
            .map(|b| b["reports"].as_array().unwrap().len())
            .sum();
        assert_eq!(reports, 4);
        all.extend(bodies(&server, PING).await);
        let sent = serde_json::to_string(&all).unwrap().to_lowercase();
        let user = std::env::var("USER").unwrap_or_default();
        let home = std::env::var("HOME").unwrap_or_default();
        let mut private = vec![
            "pedrogamer99".to_string(),
            "pedro@correo".into(),
            "servidor de pedro".into(),
            "servidor-de-pedro".into(),
            "pedrocraft".into(),
            "tok_9f8e".into(),
            "203.0.113.9".into(),
            "25565".into(),
            ", al".into(),
            root.to_lowercase(),
        ];
        if home.len() > 3 {
            private.push(home.to_lowercase());
        }
        if user.len() > 3 {
            private.push(format!("/{}/", user.to_lowercase()));
        }
        for private in private {
            assert!(!sent.contains(&private), "{private} left in {sent}");
        }
        // What helps is still there.
        assert!(
            sent.contains("crates/launcher-app/src/runs.rs:120:5"),
            "{sent}"
        );
        assert!(sent.contains("<app>/assets/index-c3x.js:1:2"), "{sent}");
        assert!(sent.contains("crash_mod_failed"), "{sent}");
        assert!(sent.contains("host_unreachable"), "{sent}");
    }

    #[tokio::test]
    async fn a_crash_report_is_only_the_verdict_the_version_and_the_loader() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.report_crash(Some(ErrorCode::CrashOutOfMemory), "1.20.1", "fabric");
        telemetry.report_crash(None, "1.21.4", "vanilla");
        settle(&telemetry).await;
        let reports: Vec<Value> = bodies(&server, ERRORS)
            .await
            .iter()
            .flat_map(|b| b["reports"].as_array().unwrap().clone())
            .collect();
        assert_eq!(
            reports,
            [
                json!({
                    "kind": "game_crash",
                    "message": "The game crashed",
                    "code": "crash_out_of_memory",
                    "context": { "minecraft": "1.20.1", "loader": "fabric" },
                    "count": 1
                }),
                json!({
                    "kind": "game_crash",
                    "message": "The game crashed",
                    "code": "crash_unknown",
                    "context": { "minecraft": "1.21.4", "loader": "vanilla" },
                    "count": 1
                })
            ]
        );
    }

    #[tokio::test]
    async fn ui_reports_take_two_kinds_and_stop_at_the_cap() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.report_ui("app_error", "pretending", None);
        telemetry.report_ui("", "nothing", None);
        assert_eq!(file(tmp.path()), None);
        for n in 0..UI_REPORTS_PER_RUN + 20 {
            telemetry.report_ui("exception", &format!("boom {n}"), None);
        }
        let queued = file(tmp.path()).unwrap()["reports"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(queued, UI_REPORTS_PER_RUN as usize);
    }

    #[tokio::test]
    async fn cancelled_things_are_not_errors() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        for code in NOT_REPORTED {
            telemetry.report_error(&LauncherError::auth(*code, "Cancelled"), None);
        }
        assert_eq!(file(tmp.path()), None);
    }

    #[tokio::test]
    async fn a_panic_is_recorded_without_waiting_and_sent_at_the_next_start() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        {
            // The panic happened while telemetry itself held its state: the hook must return.
            let _held = lock(&telemetry.state);
            telemetry.report_panic("held", None, "");
        }
        assert_eq!(file(tmp.path()), None);
        telemetry.report_panic(
            "index out of bounds",
            Some("crates/launcher-app/src/runs.rs:1:2"),
            "0: frame",
        );
        let queued = file(tmp.path()).unwrap();
        assert_eq!(queued["reports"][0]["kind"], "panic");
        assert_eq!(
            queued["reports"][0]["context"]["location"],
            "crates/launcher-app/src/runs.rs:1:2"
        );
        settle(&telemetry).await;
        assert!(
            bodies(&server, ERRORS).await.is_empty(),
            "the hook starts no send"
        );

        let next = telemetry_to(&server, tmp.path());
        next.set_locale("es");
        next.start(false, PingChannel::Stable).await;
        assert_eq!(
            bodies(&server, ERRORS).await[0]["reports"][0]["message"],
            "index out of bounds"
        );
    }

    /// One end-to-end check against a real Engine, run by hand:
    /// `PERLAND_LAUNCHER_TELEMETRY_URL=https://<engine> cargo test -p launcher-app --lib
    /// telemetry::tests::a_real_engine_takes_the_ping_and_an_error_report -- --ignored --nocapture`.
    /// It sends one ping (marked `dev`, so it can be told from players) and one error report
    /// through the same code the launcher runs, and passes only if the Engine took both.
    #[tokio::test]
    #[ignore = "talks to a real Engine; needs PERLAND_LAUNCHER_TELEMETRY_URL"]
    async fn a_real_engine_takes_the_ping_and_an_error_report() {
        let raw = std::env::var(ENDPOINT_ENV).expect("set PERLAND_LAUNCHER_TELEMETRY_URL");
        let Mode::Engine(engine) = mode(BuildKind::Dev, None, true, Some(&raw), site()) else {
            panic!("{ENDPOINT_ENV} isn't an https address (or http on this computer)");
        };
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = Telemetry::new(TelemetryConfig {
            mode: Mode::Engine(engine),
            data_root: tmp.path().to_path_buf(),
            launcher_version: env!("CARGO_PKG_VERSION").into(),
            http: launcher_core::http::client(env!("CARGO_PKG_VERSION")).unwrap(),
        });
        telemetry.force_on();
        telemetry.set_locale("en");
        telemetry.count(Counter::GameLaunches);
        telemetry.report_error(
            &LauncherError::game(
                ErrorCode::Internal,
                "End-to-end check of the launcher's error reports",
            )
            .with_detail("sent by the launcher's own test, not by a player"),
            Some("telemetry_live_check"),
        );
        telemetry.start(false, PingChannel::Dev).await;
        let state = file(tmp.path()).unwrap();
        println!("state after the check: {state}");
        assert!(
            state["lastPing"].is_string(),
            "the Engine didn't take the ping"
        );
        assert_eq!(
            state["counters"],
            json!({}),
            "counters go with a taken ping"
        );
        assert_eq!(
            state["reports"],
            json!([]),
            "the Engine didn't take the report"
        );
    }

    #[tokio::test]
    async fn a_redirect_is_not_followed_with_the_body() {
        let elsewhere = engine(202, 202).await;
        let server = MockServer::start().await;
        for route in [ERRORS, PING] {
            Mock::given(method("POST"))
                .and(path(route))
                .respond_with(
                    ResponseTemplate::new(307)
                        .insert_header("location", format!("{}{route}", elsewhere.uri())),
                )
                .mount(&server)
                .await;
        }
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        set_today(&telemetry, "2026-11-11");
        telemetry.report_error(&disk_error(), None);
        telemetry.start(false, PingChannel::Stable).await;
        settle(&telemetry).await;
        assert!(elsewhere.received_requests().await.unwrap().is_empty());
        // Not taken: kept for the next start.
        let state = file(tmp.path()).unwrap();
        assert_eq!(state["lastPing"], Value::Null);
        assert_eq!(state["reports"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn no_report_is_recorded_until_the_names_on_disk_are_known() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let earlier = started(&server, tmp.path(), true);
        earlier.notice_shown();
        // This start: armed, sharing on, but the accounts and instances not read yet.
        let telemetry = with_mode(Mode::Engine(url(&server.uri())), tmp.path());
        telemetry.set_sharing(true);
        telemetry.arm_from_disk();
        assert!(telemetry.is_on() && !telemetry.records_reports());
        telemetry.report_ui(
            "exception",
            "PedroGamer99 can't open Servidor de Pedro",
            None,
        );
        telemetry.report_error(&disk_error(), Some("instances_list"));
        telemetry.report_crash(None, "1.20.1", "vanilla");
        telemetry.report_panic("PedroGamer99", None, "");
        // A counter needs no names.
        telemetry.count(Counter::GameLaunches);
        let state = file(tmp.path()).unwrap();
        assert_eq!(state["reports"], json!([]));
        assert_eq!(state["counters"], json!({ "game_launches": 1 }));

        telemetry.add_names(["PedroGamer99", "Servidor de Pedro"]);
        telemetry.names_ready();
        telemetry.report_ui(
            "exception",
            "PedroGamer99 can't open Servidor de Pedro",
            None,
        );
        let state = file(tmp.path()).unwrap();
        assert_eq!(
            state["reports"][0]["message"],
            "<redacted> can't open <redacted>"
        );
    }

    #[tokio::test]
    async fn arming_is_decided_once_per_run() {
        let server = engine(202, 202).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = started(&server, tmp.path(), true);
        telemetry.notice_shown();
        // Whoever reads the switch again in this run doesn't start it.
        telemetry.set_sharing(true);
        telemetry.arm_from_disk();
        assert!(!telemetry.is_on());
        use_everything(&telemetry).await;
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn settings_that_cannot_be_trusted_make_the_notice_due_again() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let earlier = started(&server, tmp.path(), true);
        earlier.notice_shown();
        let running = started(&server, tmp.path(), true);
        running.report_error(&disk_error(), None);
        assert!(running.is_on());

        // A start where the settings couldn't be read.
        let damaged = with_mode(Mode::Engine(url(&server.uri())), tmp.path());
        damaged.distrust();
        assert!(!damaged.is_on());
        let state = file(tmp.path()).unwrap();
        assert_eq!(state["noticeShown"], Value::Null);
        assert_eq!(state["reports"], json!([]));
        // The defaults (share on) get saved later: still nothing until a new notice and
        // the start after it.
        damaged.set_sharing(true);
        damaged.arm_from_disk();
        assert!(!damaged.is_on());
        let next = started(&server, tmp.path(), true);
        assert!(next.notice_due());
        assert!(!next.is_on());
    }

    #[tokio::test]
    async fn uncount_takes_one_back() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        telemetry.count(Counter::UpdatesInstalled);
        telemetry.count(Counter::UpdatesInstalled);
        telemetry.uncount(Counter::UpdatesInstalled);
        assert_eq!(
            file(tmp.path()).unwrap()["counters"],
            json!({ "updates_installed": 1 })
        );
        telemetry.uncount(Counter::UpdatesInstalled);
        telemetry.uncount(Counter::UpdatesInstalled);
        assert_eq!(file(tmp.path()).unwrap()["counters"], json!({}));
    }

    #[tokio::test]
    async fn the_reporting_host_reports_warnings_and_failed_runs_and_passes_everything_on() {
        let server = engine(500, 500).await;
        let tmp = tempfile::tempdir().unwrap();
        let telemetry = telemetry_to(&server, tmp.path());
        let host = ReportingHost::new(RecordingHost::default(), telemetry.clone());
        host.emit(AppEvent::Warning {
            instance: None,
            error: LauncherError::disk(ErrorCode::SettingsDamaged, "Settings couldn't be read"),
        });
        host.emit(AppEvent::RunState {
            instance: "servidor-de-pedro".into(),
            state: crate::events::RunStateKind::Idle,
            exit: Some(ExitInfo {
                error: Some(LauncherError::java(
                    ErrorCode::JavaStartFailed,
                    "Java didn't start",
                )),
                ..ExitInfo::default()
            }),
            update: None,
        });
        // A clean exit and a crash's cause are not this host's to report.
        host.emit(AppEvent::RunState {
            instance: "servidor-de-pedro".into(),
            state: crate::events::RunStateKind::Idle,
            exit: Some(ExitInfo {
                code: Some(1),
                cause: Some(LauncherError::game(
                    ErrorCode::CrashOutOfMemory,
                    "Out of memory",
                )),
                ..ExitInfo::default()
            }),
            update: None,
        });
        host.emit(AppEvent::DeepLink);
        assert_eq!(host.inner.events.lock().unwrap().len(), 4);
        let queued = file(tmp.path()).unwrap();
        let codes: Vec<&str> = queued["reports"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["code"].as_str().unwrap())
            .collect();
        assert_eq!(codes, ["settings_damaged", "java_start_failed"]);
        assert!(!queued.to_string().contains("servidor-de-pedro"));
    }
}
