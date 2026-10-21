//! Fakes for the app tests: a runner whose preparing and exits the test controls, and a host
//! that records everything it was told.
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use launcher_core::{
    accounts::Session,
    browse::{
        compat::{ContentKind, Target},
        identify::Installed,
        images::{Image, ImageKind},
        plan::{
            InstallPlan, ItemReason, NewInstancePlan, PlanItem, PlanRequest, PlanSkip, PlannedFile,
            SkipReason,
        },
        project::{ProjectDetail, VersionInfo},
        search::{SearchPage, SearchQuery},
    },
    download::{hash::sha1_hex, Hashes},
    events::{Event, EventSink, TaskId},
    instances::{GameSpec, Instance, InstanceStore, ServerAddress, SourceRef, UpdatePolicy},
    invite::{
        EngineView, InviteAddress, InviteGame, InviteLink, InviteModrinth, InvitePack,
        InvitePlayers, InviteTeam, InviteView, Url,
    },
    launch::GameExit,
    locks::InstanceLock,
    meta::loaders::LoaderKind,
    news::NewsPage,
    perland::{
        CreateAllowance, CreateFreeServer, DiscordAccount, Disk, EmailStarted, FreeLimits,
        FreeServer, FreeServerState, LinkCode, MemoryLimits, Player, PlayerConfig, PlayerStats,
        SignInMethod, TokenAnswer, YourServer,
    },
    public_list::{FetchedPage, ListQuery, PublicServer},
    server_wake::{Knock, Ping, WaitEnd},
    sources::{
        DesiredState, FileMode, FileSource, OptionalFile, PackAbout, PackFile, PackInput, Side,
        UpdateCheck,
    },
    sync::{
        lockfile::{self, Managed, ManagedFile},
        plan::Changes,
        Cancel, Ensured, ManualFile, PendingUpdate, SyncOutcome, SyncReport,
    },
    ErrorCode, LauncherError, Result,
};
use tokio::sync::{oneshot, Notify, Semaphore};

use crate::{
    app::AppCore,
    browser::Browser,
    dto::{NewInstance, PackUpdate},
    events::{AppEvent, ExitInfo, Host, RunStateKind},
    perland::PlayerApi,
    runner::{BoxFuture, Kill, Runner, RunningGame},
};

pub(crate) fn exit(code: Option<i32>) -> GameExit {
    GameExit {
        code,
        killed: false,
        tail: vec![],
        crash: None,
    }
}

pub(crate) fn up_to_date() -> Ensured {
    Ensured {
        outcome: SyncOutcome::Done(SyncReport {
            up_to_date: true,
            ..SyncReport::default()
        }),
        warning: None,
    }
}

pub(crate) fn pack_file(path: &str, name: &str, optional: Option<bool>) -> PackFile {
    PackFile {
        path: path.into(),
        name: name.into(),
        source: FileSource::Remote { urls: vec![] },
        hashes: Hashes::sha1("a".repeat(40)),
        size: Some(1),
        side: Side::Both,
        optional: optional.map(|default| OptionalFile {
            default,
            description: Some(format!("Optional {name}")),
        }),
        mode: FileMode::Replace,
        manual: None,
    }
}

pub(crate) fn desired(version: &str, files: Vec<PackFile>) -> DesiredState {
    DesiredState {
        name: "Sky Islands".into(),
        pack_version: version.into(),
        minecraft: "1.20.1".into(),
        loader: None,
        files,
        stamp: format!("stamp-{version}"),
        about: PackAbout::default(),
        archive: None,
    }
}

pub(crate) fn pending(from: &str, to: &str) -> PendingUpdate {
    PendingUpdate {
        source: SourceRef::Packwiz {
            url: "https://example.com/pack.toml".into(),
        },
        desired: desired(to, vec![]),
        changes: Changes {
            from_version: Some(from.into()),
            to_version: to.into(),
            added: vec!["JEI".into()],
            updated: vec![],
            removed: vec!["Zoomify".into()],
        },
        kept: vec![],
    }
}

pub(crate) fn manual_file(file_name: &str, body: &[u8]) -> ManualFile {
    ManualFile {
        path: format!("mods/{file_name}"),
        name: file_name.trim_end_matches(".jar").into(),
        file_name: file_name.into(),
        page_url: "https://www.curseforge.com/projects/1".into(),
        hashes: Hashes::sha1(sha1_hex(body)),
    }
}

/// A lockfile that says sync wrote each of `paths`.
pub(crate) fn managed_for(paths: &[&str]) -> Managed {
    Managed {
        version: Managed::VERSION,
        stamp: "stamp-1.0.3".into(),
        pack_version: "1.0.3".into(),
        choices: BTreeMap::new(),
        files: paths
            .iter()
            .map(|p| {
                (
                    p.to_string(),
                    ManagedFile {
                        name: p.to_string(),
                        sha1: "a".repeat(40),
                        size: 1,
                        mode: FileMode::Replace,
                        declared: Hashes::sha1("a".repeat(40)),
                    },
                )
            })
            .collect(),
    }
}

/// A packwiz instance named `name`; `installed` becomes its `pack.json` (as if it had synced).
pub(crate) async fn pack_instance(
    core: &AppCore<FakeRunner, RecordingHost>,
    name: &str,
    installed: Option<&DesiredState>,
) -> Instance {
    let mut instance = core
        .instance_create(NewInstance {
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: None,
        })
        .await
        .unwrap();
    instance.source = SourceRef::Packwiz {
        url: "https://example.com/pack.toml".into(),
    };
    core.instances.save(&instance).await.unwrap();
    if let Some(d) = installed {
        lockfile::save_pack(&core.instances.perland_dir(&instance.id), d)
            .await
            .unwrap();
    }
    instance
}

/// An invite link with a fake token of 43 × `c`.
pub(crate) fn invite(c: char) -> String {
    format!("https://play.example.com/{}", c.to_string().repeat(43))
}

/// A modded server that is online with 3 of 20 players, at play.example.com:25566.
pub(crate) fn server_view(name: &str) -> EngineView {
    EngineView {
        engine: Url::parse("https://api.example.com").unwrap(),
        view: InviteView {
            kind: "stack".into(),
            name: name.into(),
            motd: "§6Survival§r with friends".into(),
            address: InviteAddress {
                host: Some("play.example.com".into()),
                port: 25566,
                display: Some("play.example.com:25566".into()),
            },
            game: InviteGame {
                kind: "fabric".into(),
                version: Some("1.20.1".into()),
                loader: Some("fabric".into()),
                loader_version: Some("0.16.14".into()),
            },
            status: "online".into(),
            players: Some(InvitePlayers {
                online: 3,
                max: Some(20),
            }),
            pack: Some(InvitePack {
                name: "Survival Pack".into(),
                version: Some("1.0.4".into()),
                minecraft: "1.20.1".into(),
                loader: Some("fabric".into()),
                loader_version: Some("0.16.14".into()),
                mod_count: 2,
                state: "ready".into(),
                zip_state: Some("ready".into()),
                commit: Some("c1".into()),
                stamp: None,
            }),
            team: InviteTeam {
                display_name: "Acme".into(),
                accent: "violet".into(),
            },
            ..InviteView::default()
        },
    }
}

/// What `invite_view` answers for `invite` from now on.
pub(crate) fn serve(runner: &FakeRunner, invite: &str, view: Result<EngineView>) {
    let key = InviteLink::parse(invite).unwrap().canonical();
    runner.views.lock().unwrap().insert(key, view);
}

#[derive(Default)]
pub(crate) struct RecordingHost {
    pub events: Mutex<Vec<AppEvent>>,
    pub minimized: AtomicUsize,
    pub restored: AtomicUsize,
    /// One-shot: the next final `idle` is held back this long before it is recorded, to widen
    /// the window between a run leaving the map and its final state being reported.
    pub idle_delay: Mutex<Option<Duration>>,
    /// Called with every event as it is emitted (before it is recorded).
    pub on_emit: Mutex<Option<EmitHook>>,
    /// Every URL the app asked the browser to open.
    pub opened: Mutex<Vec<String>>,
}

pub(crate) type EmitHook = Arc<dyn Fn(&AppEvent) + Send + Sync>;

impl RecordingHost {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn states(&self, id: &str) -> Vec<RunStateKind> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::RunState {
                    instance, state, ..
                } if instance == id => Some(*state),
                _ => None,
            })
            .collect()
    }

    pub fn last_exit(&self, id: &str) -> Option<ExitInfo> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|e| match e {
                AppEvent::RunState {
                    instance,
                    exit: Some(exit),
                    ..
                } if instance == id => Some(exit.clone()),
                _ => None,
            })
    }

    pub fn log_lines(&self, id: &str) -> Vec<String> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Log { instance, lines } if instance == id => Some(lines.clone()),
                _ => None,
            })
            .flatten()
            .collect()
    }

    pub fn progress_count(&self, id: &str) -> usize {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, AppEvent::Progress { instance: Some(i), .. } if i == id))
            .count()
    }

    pub fn warnings(&self) -> Vec<ErrorCode> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Warning { error, .. } => Some(error.code),
                _ => None,
            })
            .collect()
    }

    pub fn last_update(&self, id: &str) -> Option<PackUpdate> {
        self.events
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|e| match e {
                AppEvent::RunState {
                    instance,
                    update: Some(update),
                    ..
                } if instance == id => Some(update.clone()),
                _ => None,
            })
    }

    /// Wait until `id` has reported `state` at least `n` times (panics after 5 s).
    pub async fn wait_for_nth(&self, id: &str, state: RunStateKind, n: usize) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.states(id).iter().filter(|s| **s == state).count() < n {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{id} never reached {state:?} {n} times; saw {:?}",
                self.states(id)
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    /// Wait until the app has opened `n` URLs, and return them (panics after 5 s).
    pub async fn wait_for_opened(&self, n: usize) -> Vec<String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            let opened = self.opened.lock().unwrap().clone();
            if opened.len() >= n {
                return opened;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "only {} URLs were opened",
                opened.len()
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    /// Wait until `id` has reported `state` (panics after 5 s).
    pub async fn wait_for(&self, id: &str, state: RunStateKind) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !self.states(id).contains(&state) {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{id} never reached {state:?}; saw {:?}",
                self.states(id)
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
}

impl Host for RecordingHost {
    fn emit(&self, event: AppEvent) {
        if matches!(
            event,
            AppEvent::RunState {
                state: RunStateKind::Idle,
                exit: Some(_),
                ..
            }
        ) {
            let delay = self.idle_delay.lock().unwrap().take();
            if let Some(delay) = delay {
                std::thread::sleep(delay);
            }
        }
        let hook = self.on_emit.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook(&event);
        }
        self.events.lock().unwrap().push(event);
    }
    fn minimize(&self) {
        self.minimized.fetch_add(1, Ordering::SeqCst);
    }
    fn restore(&self) {
        self.restored.fetch_add(1, Ordering::SeqCst);
    }
    fn open_url(&self, url: &str) {
        self.opened.lock().unwrap().push(url.to_string());
    }
}

type ExitSlot = Arc<Mutex<Option<oneshot::Sender<GameExit>>>>;

/// `prepare` emits `progress_steps + 1` progress events, then waits for one gate permit.
/// `start` emits `start_lines` as log output; the game runs until `finish` or a kill.
/// The pack methods record what they were asked and answer from what the test scripted.
pub(crate) struct FakeRunner {
    gate: Arc<Semaphore>,
    pub prepare_error: Mutex<Option<LauncherError>>,
    pub progress_steps: Mutex<u64>,
    pub start_lines: Mutex<Vec<String>>,
    exits: Mutex<HashMap<String, ExitSlot>>,
    /// Where `add_pack` creates instances (`tests::core` sets it).
    pub instances_dir: Mutex<Option<PathBuf>>,
    pub added: Mutex<Vec<PackInput>>,
    /// The update `find_update` reports per instance id (none: up to date).
    pub updates: Mutex<HashMap<String, Result<Option<PendingUpdate>>>>,
    pub checks: AtomicUsize,
    /// What successive `sync` calls return (empty: up to date).
    pub sync_results: Mutex<VecDeque<Result<Ensured>>>,
    /// Each `sync` call: (instance id, version it was told to accept).
    pub syncs: Mutex<Vec<(String, Option<String>)>>,
    /// While set, `sync` waits for a permit from `open_sync_gate` (or a cancel).
    pub hold_sync: AtomicBool,
    sync_gate: Arc<Semaphore>,
    /// One-shot: the game spec the next `sync` writes into the instance, as a pack that
    /// changed its Minecraft version would.
    pub sync_game: Mutex<Option<GameSpec>>,
    pub imported: Mutex<Vec<ManualFile>>,
    /// The game spec of each instance `prepare` was called for.
    pub prepared: Mutex<Vec<GameSpec>>,
    pub browser: Arc<FakeBrowser>,
    /// What `invite_view` answers, by canonical invite (missing: `invite_gone`).
    pub views: Mutex<HashMap<String, Result<EngineView>>>,
    pub view_reads: AtomicUsize,
    /// The Engine each `invite_view` call was seeded with.
    pub engine_hints: Mutex<Vec<Option<String>>>,
    /// What `invite_icon` answers.
    pub icon: Mutex<Option<Vec<u8>>>,
    /// What `public_page` answers (none: `server_list_closed`).
    pub public_page: Mutex<Option<Result<FetchedPage>>>,
    /// The queries `public_page` was asked.
    pub public_queries: Mutex<Vec<ListQuery>>,
    /// Whether `public_invite` finds a link (it builds `play.example.com/<token>`).
    pub public_invites: Mutex<bool>,
    /// What `news_page` answers (none: no news here).
    pub news_page: Mutex<Option<Result<Option<NewsPage>>>>,
    /// Each (canonical invite, limit, before) `news_page` was asked.
    pub news_calls: Mutex<Vec<(String, u8, Option<String>)>>,
    /// What `news_image` answers.
    pub news_image: Mutex<Option<Vec<u8>>>,
    /// Each (canonical invite, post id, image version) `news_image` was asked.
    pub news_image_calls: Mutex<Vec<(String, String, String)>>,
    /// The server each `start` was told to join.
    pub started_joins: Mutex<Vec<Option<ServerAddress>>>,
    pub started_worlds: Mutex<Vec<Option<String>>>,
    /// What `check_update` reports per instance id (none: up to date).
    pub update_checks: Mutex<HashMap<String, UpdateCheck>>,
    /// What `server_ping` answers (default: nobody), and each address it was asked.
    pub ping_answer: Mutex<Ping>,
    pub pings: Mutex<Vec<ServerAddress>>,
    /// What `server_knock` answers (default: woken), and each (address, name) it was asked.
    pub knock_answer: Mutex<Result<Knock>>,
    pub knocks: Mutex<Vec<(ServerAddress, String)>>,
    /// Signalled by each `server_knock` call.
    pub knocked: Notify,
    /// While set, `server_knock` never returns.
    pub hold_knock: AtomicBool,
    /// What `server_wait` answers (default: up), and each address it waited on.
    pub wait_answer: Mutex<WaitEnd>,
    pub waits: Mutex<Vec<ServerAddress>>,
    /// While set, `server_wait` waits for a permit from `open_wait_gate` (or a cancel).
    pub hold_wait: AtomicBool,
    wait_gate: Arc<Semaphore>,
    /// Signalled once a held `server_wait` is parked.
    pub waiting: Arc<Notify>,
    pub player: Arc<FakePlayer>,
}

impl FakeRunner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gate: Arc::new(Semaphore::new(0)),
            prepare_error: Mutex::new(None),
            progress_steps: Mutex::new(0),
            start_lines: Mutex::new(Vec::new()),
            exits: Mutex::new(HashMap::new()),
            instances_dir: Mutex::new(None),
            added: Mutex::new(Vec::new()),
            updates: Mutex::new(HashMap::new()),
            checks: AtomicUsize::new(0),
            sync_results: Mutex::new(VecDeque::new()),
            syncs: Mutex::new(Vec::new()),
            hold_sync: AtomicBool::new(false),
            sync_gate: Arc::new(Semaphore::new(0)),
            sync_game: Mutex::new(None),
            imported: Mutex::new(Vec::new()),
            prepared: Mutex::new(Vec::new()),
            browser: FakeBrowser::new(),
            views: Mutex::new(HashMap::new()),
            view_reads: AtomicUsize::new(0),
            engine_hints: Mutex::new(Vec::new()),
            icon: Mutex::new(None),
            public_page: Mutex::new(None),
            public_queries: Mutex::new(Vec::new()),
            public_invites: Mutex::new(true),
            news_page: Mutex::new(None),
            news_calls: Mutex::new(Vec::new()),
            news_image: Mutex::new(None),
            news_image_calls: Mutex::new(Vec::new()),
            started_joins: Mutex::new(Vec::new()),
            started_worlds: Mutex::new(Vec::new()),
            update_checks: Mutex::new(HashMap::new()),
            ping_answer: Mutex::new(Ping::NoAnswer),
            pings: Mutex::new(Vec::new()),
            knock_answer: Mutex::new(Ok(Knock::Woken)),
            knocks: Mutex::new(Vec::new()),
            knocked: Notify::new(),
            hold_knock: AtomicBool::new(false),
            wait_answer: Mutex::new(WaitEnd::Up),
            waits: Mutex::new(Vec::new()),
            hold_wait: AtomicBool::new(false),
            wait_gate: Arc::new(Semaphore::new(0)),
            waiting: Arc::new(Notify::new()),
            player: FakePlayer::new(),
        })
    }

    /// Let `n` held waits finish (see `hold_wait`).
    pub fn open_wait_gate(&self, n: usize) {
        self.wait_gate.add_permits(n);
    }

    /// Let `n` held syncs finish (see `hold_sync`).
    pub fn open_sync_gate(&self, n: usize) {
        self.sync_gate.add_permits(n);
    }

    /// Let `n` waiting (or future) prepares finish.
    pub fn open_gate(&self, n: usize) {
        self.gate.add_permits(n);
    }

    /// End a running fake game with this exit.
    pub fn finish(&self, id: &str, exit: GameExit) {
        let slot = self
            .exits
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .expect("game started");
        let tx = slot.lock().unwrap().take();
        if let Some(tx) = tx {
            let _ = tx.send(exit);
        }
    }
}

impl Runner for FakeRunner {
    type Prepared = ();

    fn prepare(&self, instance: Instance, sink: EventSink) -> BoxFuture<Result<()>> {
        self.prepared.lock().unwrap().push(instance.game.clone());
        let gate = self.gate.clone();
        let error = self.prepare_error.lock().unwrap().take();
        let steps = *self.progress_steps.lock().unwrap();
        Box::pin(async move {
            for done in 0..=steps {
                sink(Event::Progress {
                    task: TaskId::new("install_minecraft")
                        .with("version", &instance.game.minecraft),
                    done,
                    total: steps,
                    bytes: done,
                });
            }
            gate.acquire().await.expect("gate is never closed").forget();
            match error {
                Some(e) => Err(e),
                None => Ok(()),
            }
        })
    }

    fn start(
        &self,
        instance: Instance,
        _prepared: (),
        _session: Session,
        join: Option<ServerAddress>,
        world: Option<String>,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>> {
        self.started_joins.lock().unwrap().push(join);
        self.started_worlds.lock().unwrap().push(world);
        let lines = self.start_lines.lock().unwrap().clone();
        let (tx, rx) = oneshot::channel();
        let slot: ExitSlot = Arc::new(Mutex::new(Some(tx)));
        self.exits
            .lock()
            .unwrap()
            .insert(instance.id.clone(), slot.clone());
        Box::pin(async move {
            for line in lines {
                sink(Event::Log { line });
            }
            Ok(Box::new(FakeGame { rx, slot, sink }) as Box<dyn RunningGame>)
        })
    }

    fn add_pack(&self, input: PackInput) -> BoxFuture<Result<Instance>> {
        self.added.lock().unwrap().push(input.clone());
        let dir = self
            .instances_dir
            .lock()
            .unwrap()
            .clone()
            .expect("tests::core sets instances_dir");
        Box::pin(async move {
            let store = InstanceStore::new(dir);
            let mut instance = store
                .create(
                    if matches!(input, PackInput::Perland { .. }) {
                        "Survival"
                    } else {
                        "Sky Islands"
                    },
                    GameSpec {
                        minecraft: "1.20.1".into(),
                        loader: None,
                    },
                )
                .await?;
            instance.source = match input {
                PackInput::Packwiz { url } => SourceRef::Packwiz { url },
                PackInput::MrpackFile { path } => SourceRef::Mrpack {
                    name: path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    sha1: "0".repeat(40),
                },
                PackInput::Modrinth { project, version } => SourceRef::Modrinth {
                    project_id: project,
                    version_id: version.unwrap_or_else(|| "v1".into()),
                },
                PackInput::Perland { link } => SourceRef::Perland {
                    invite: link.canonical(),
                    engine: "https://api.example.com/".into(),
                },
            };
            if matches!(instance.source, SourceRef::Perland { .. }) {
                instance.update_policy = UpdatePolicy::Auto;
            }
            store.save(&instance).await?;
            Ok(instance)
        })
    }

    fn find_update(
        &self,
        instance: Instance,
        _sink: EventSink,
    ) -> BoxFuture<Result<Option<PendingUpdate>>> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        let answer = self
            .updates
            .lock()
            .unwrap()
            .get(&instance.id)
            .cloned()
            .unwrap_or(Ok(None));
        Box::pin(async move { answer })
    }

    fn check_update(&self, instance: Instance) -> BoxFuture<UpdateCheck> {
        let answer = self
            .update_checks
            .lock()
            .unwrap()
            .get(&instance.id)
            .cloned()
            .unwrap_or(UpdateCheck::UpToDate);
        Box::pin(async move { answer })
    }

    fn sync(
        &self,
        _lock: Arc<InstanceLock>,
        id: String,
        accept: Option<PendingUpdate>,
        cancel: Cancel,
        sink: EventSink,
    ) -> BoxFuture<Result<Ensured>> {
        self.syncs
            .lock()
            .unwrap()
            .push((id.clone(), accept.map(|u| u.desired.pack_version)));
        let result = self
            .sync_results
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Ok(up_to_date()));
        let hold = self.hold_sync.load(Ordering::SeqCst);
        let gate = self.sync_gate.clone();
        let game = self.sync_game.lock().unwrap().take();
        let dir = self.instances_dir.lock().unwrap().clone();
        Box::pin(async move {
            sink(Event::Progress {
                task: TaskId::new("sync_pack").with("name", "Sky Islands"),
                done: 0,
                total: 1,
                bytes: 0,
            });
            if hold {
                loop {
                    if cancel.is_cancelled() {
                        return Err(LauncherError::pack(ErrorCode::SyncCancelled, "cancelled"));
                    }
                    if let Ok(permit) = gate.try_acquire() {
                        permit.forget();
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            }
            if let (Some(game), Some(dir)) = (game, dir) {
                let store = InstanceStore::new(dir);
                let mut instance = store.load(&id).await?;
                instance.game = game;
                store.save(&instance).await?;
            }
            result
        })
    }

    fn import_manual(&self, found: Vec<(ManualFile, PathBuf)>) -> BoxFuture<Result<()>> {
        self.imported
            .lock()
            .unwrap()
            .extend(found.into_iter().map(|(file, _)| file));
        Box::pin(async { Ok(()) })
    }

    fn invite_view(
        &self,
        link: InviteLink,
        engine: Option<String>,
    ) -> BoxFuture<Result<EngineView>> {
        self.view_reads.fetch_add(1, Ordering::SeqCst);
        self.engine_hints.lock().unwrap().push(engine);
        let answer = self
            .views
            .lock()
            .unwrap()
            .get(&link.canonical())
            .cloned()
            .unwrap_or_else(|| Err(LauncherError::pack(ErrorCode::InviteGone, "gone")));
        Box::pin(async move { answer })
    }

    fn invite_icon(&self, _link: InviteLink) -> BoxFuture<Option<Vec<u8>>> {
        let icon = self.icon.lock().unwrap().clone();
        Box::pin(async move { icon })
    }

    fn public_page(&self, query: ListQuery) -> BoxFuture<Result<FetchedPage>> {
        self.public_queries.lock().unwrap().push(query);
        let answer = self.public_page.lock().unwrap().clone().unwrap_or_else(|| {
            Err(LauncherError::pack(
                ErrorCode::ServerListClosed,
                "The server list isn't open yet",
            ))
        });
        Box::pin(async move { answer })
    }

    fn public_icon(&self, _token: String, _icon_version: String) -> BoxFuture<Option<Vec<u8>>> {
        let icon = self.icon.lock().unwrap().clone();
        Box::pin(async move { icon })
    }

    fn public_invite(&self, server: &PublicServer) -> Option<InviteLink> {
        if !*self.public_invites.lock().unwrap() {
            return None;
        }
        InviteLink::parse(&format!("https://play.example.com/{}", server.token)).ok()
    }

    fn news_page(
        &self,
        link: InviteLink,
        limit: u8,
        before: Option<String>,
    ) -> BoxFuture<Result<Option<NewsPage>>> {
        self.news_calls
            .lock()
            .unwrap()
            .push((link.canonical(), limit, before));
        let answer = self.news_page.lock().unwrap().clone().unwrap_or(Ok(None));
        Box::pin(async move { answer })
    }

    fn news_image(
        &self,
        link: InviteLink,
        post_id: String,
        image_version: String,
    ) -> BoxFuture<Option<Vec<u8>>> {
        self.news_image_calls
            .lock()
            .unwrap()
            .push((link.canonical(), post_id, image_version));
        let image = self.news_image.lock().unwrap().clone();
        Box::pin(async move { image })
    }

    fn browser(&self) -> Arc<dyn Browser> {
        self.browser.clone()
    }

    fn server_ping(&self, address: ServerAddress) -> BoxFuture<Ping> {
        self.pings.lock().unwrap().push(address);
        let answer = *self.ping_answer.lock().unwrap();
        Box::pin(async move { answer })
    }

    fn server_knock(&self, address: ServerAddress, name: String) -> BoxFuture<Result<Knock>> {
        self.knocks.lock().unwrap().push((address, name));
        self.knocked.notify_one();
        let answer = self.knock_answer.lock().unwrap().clone();
        let hold = self.hold_knock.load(Ordering::SeqCst);
        Box::pin(async move {
            if hold {
                std::future::pending::<()>().await;
            }
            answer
        })
    }

    fn server_wait(&self, address: ServerAddress, cancel: Cancel) -> BoxFuture<WaitEnd> {
        self.waits.lock().unwrap().push(address);
        let answer = *self.wait_answer.lock().unwrap();
        let hold = self.hold_wait.load(Ordering::SeqCst);
        let (gate, waiting) = (self.wait_gate.clone(), self.waiting.clone());
        Box::pin(async move {
            if hold {
                waiting.notify_one();
                tokio::select! {
                    permit = gate.acquire() => permit.expect("gate is never closed").forget(),
                    _ = cancel.cancelled() => return WaitEnd::Cancelled,
                }
            }
            answer
        })
    }

    fn player(&self) -> Arc<dyn PlayerApi> {
        self.player.clone()
    }
}

/// A fake player token: `plt_` and 43 × `c`.
pub(crate) fn player_token(c: char) -> String {
    format!("plt_{}", c.to_string().repeat(43))
}

/// Steve, signed in with Discord and an email.
pub(crate) fn sample_player() -> Player {
    Player {
        id: "p1".into(),
        display_name: "Steve".into(),
        email: Some("steve@example.com".into()),
        discord: Some(DiscordAccount {
            id: "d1".into(),
            username: "steve".into(),
            global_name: Some("Steve".into()),
        }),
        minecraft: None,
        created_at: "2026-10-17T00:00:00Z".into(),
    }
}

/// One call `FakePlayer` saw: the route, the Engine it went to and its arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerCall {
    pub route: &'static str,
    pub engine: Url,
    pub args: Vec<String>,
}

/// The player routes, answered from what the test scripted. Each call is recorded.
pub(crate) struct FakePlayer {
    /// What `site_engine` (and `engine_for`) answers. Default `https://api.example.com`.
    pub site: Mutex<Result<Url>>,
    /// Default: accounts on, Discord and email.
    pub config: Mutex<Result<PlayerConfig>>,
    /// What `token` and `email_verify` answer. Default: Steve, `player_token('x')`, new.
    pub token_answer: Mutex<Result<TokenAnswer>>,
    pub email_started: Mutex<Result<EmailStarted>>,
    /// Default: Steve.
    pub me: Mutex<Result<Player>>,
    pub sign_out: Mutex<Result<()>>,
    pub servers: Mutex<Result<Vec<YourServer>>>,
    /// What `follow`, `unfollow` and `joined` answer.
    pub relation: Mutex<Result<()>>,
    /// What `free_server` answers. Default: the feature is off (`None`).
    pub free: Mutex<Result<Option<FreeServerState>>>,
    /// What `create_free_server` answers. Default: `free_view("starting")`.
    pub created: Mutex<Result<FreeServer>>,
    /// The packs `create_free_server_from_pack` received.
    pub uploaded: Mutex<Vec<Vec<u8>>>,
    /// What `start_free_server` answers. Default: `free_view("starting")`.
    pub started: Mutex<Result<FreeServer>>,
    /// What `delete_free_server` answers. Default: deleted.
    pub deleted: Mutex<Result<bool>>,
    /// Default: `ABCD-EFGH`, for 600 s.
    pub link_code: Mutex<Result<LinkCode>>,
    pub unlink: Mutex<Result<()>>,
    /// Default: no Minecraft account linked, no servers.
    pub stats: Mutex<Result<Option<PlayerStats>>>,
    pub calls: Mutex<Vec<PlayerCall>>,
    /// Routes whose calls wait for a permit of their gate (see `hold`).
    holds: Mutex<HashMap<&'static str, Arc<Semaphore>>>,
}

impl FakePlayer {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            site: Mutex::new(Ok(Url::parse("https://api.example.com").unwrap())),
            config: Mutex::new(Ok(PlayerConfig {
                enabled: true,
                methods: vec![SignInMethod::Discord, SignInMethod::Email],
            })),
            token_answer: Mutex::new(Ok(TokenAnswer {
                token: player_token('x'),
                player: sample_player(),
                new_player: true,
            })),
            email_started: Mutex::new(Ok(EmailStarted {
                expires_in: 600,
                resend_in: 60,
            })),
            me: Mutex::new(Ok(sample_player())),
            sign_out: Mutex::new(Ok(())),
            servers: Mutex::new(Ok(vec![])),
            relation: Mutex::new(Ok(())),
            free: Mutex::new(Ok(None)),
            created: Mutex::new(Ok(free_view("starting"))),
            uploaded: Mutex::new(Vec::new()),
            started: Mutex::new(Ok(free_view("starting"))),
            deleted: Mutex::new(Ok(true)),
            link_code: Mutex::new(Ok(LinkCode {
                code: "ABCD-EFGH".into(),
                expires_in: 600,
            })),
            unlink: Mutex::new(Ok(())),
            stats: Mutex::new(Ok(Some(PlayerStats::default()))),
            calls: Mutex::new(Vec::new()),
            holds: Mutex::new(HashMap::new()),
        })
    }

    /// From now on each call to `route` is recorded at once but answers only after a permit
    /// of the returned gate (`add_permits`).
    pub fn hold(&self, route: &'static str) -> Arc<Semaphore> {
        let gate = Arc::new(Semaphore::new(0));
        self.holds.lock().unwrap().insert(route, gate.clone());
        gate
    }

    /// Wait until `route` was called `n` times (panics after 5 s).
    pub async fn wait_for_calls(&self, route: &str, n: usize) {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while self.calls_to(route).len() < n {
            assert!(
                tokio::time::Instant::now() < deadline,
                "{route} was never called {n} times"
            );
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    /// The calls made to `route`.
    pub fn calls_to(&self, route: &str) -> Vec<PlayerCall> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.route == route)
            .cloned()
            .collect()
    }

    fn record<T: Clone + Send + 'static>(
        &self,
        route: &'static str,
        engine: Url,
        args: &[&str],
        answer: &Mutex<Result<T>>,
    ) -> BoxFuture<Result<T>> {
        self.calls.lock().unwrap().push(PlayerCall {
            route,
            engine,
            args: args.iter().map(|a| a.to_string()).collect(),
        });
        let answer = answer.lock().unwrap().clone();
        let gate = self.holds.lock().unwrap().get(route).cloned();
        Box::pin(async move {
            if let Some(gate) = gate {
                gate.acquire().await.expect("gate is never closed").forget();
            }
            answer
        })
    }
}

impl PlayerApi for FakePlayer {
    fn site_engine(&self) -> BoxFuture<Result<Url>> {
        let site = self.site.lock().unwrap().clone();
        Box::pin(async move { site })
    }
    fn engine_for(&self, _link: InviteLink) -> BoxFuture<Result<Url>> {
        self.site_engine()
    }
    fn config(&self, engine: Url) -> BoxFuture<Result<PlayerConfig>> {
        self.record("config", engine, &[], &self.config)
    }
    fn token(
        &self,
        engine: Url,
        code: String,
        verifier: String,
        redirect: String,
    ) -> BoxFuture<Result<TokenAnswer>> {
        self.record(
            "token",
            engine,
            &[&code, &verifier, &redirect],
            &self.token_answer,
        )
    }
    fn email_start(
        &self,
        engine: Url,
        email: String,
        language: String,
    ) -> BoxFuture<Result<EmailStarted>> {
        self.record(
            "email_start",
            engine,
            &[&email, &language],
            &self.email_started,
        )
    }
    fn email_verify(
        &self,
        engine: Url,
        email: String,
        code: String,
    ) -> BoxFuture<Result<TokenAnswer>> {
        self.record("email_verify", engine, &[&email, &code], &self.token_answer)
    }
    fn me(&self, engine: Url, token: String) -> BoxFuture<Result<Player>> {
        self.record("me", engine, &[&token], &self.me)
    }
    fn sign_out(&self, engine: Url, token: String) -> BoxFuture<Result<()>> {
        self.record("sign_out", engine, &[&token], &self.sign_out)
    }
    fn servers(&self, engine: Url, token: String) -> BoxFuture<Result<Vec<YourServer>>> {
        self.record("servers", engine, &[&token], &self.servers)
    }
    fn follow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        self.record("follow", engine, &[&token, &invite_token], &self.relation)
    }
    fn unfollow(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        self.record("unfollow", engine, &[&token, &invite_token], &self.relation)
    }
    fn joined(&self, engine: Url, token: String, invite_token: String) -> BoxFuture<Result<()>> {
        self.record("joined", engine, &[&token, &invite_token], &self.relation)
    }
    fn link_code(&self, engine: Url, token: String) -> BoxFuture<Result<LinkCode>> {
        self.record("link_code", engine, &[&token], &self.link_code)
    }
    fn unlink(&self, engine: Url, token: String) -> BoxFuture<Result<()>> {
        self.record("unlink", engine, &[&token], &self.unlink)
    }
    fn stats(&self, engine: Url, token: String) -> BoxFuture<Result<Option<PlayerStats>>> {
        self.record("stats", engine, &[&token], &self.stats)
    }
    fn free_server(
        &self,
        engine: Url,
        token: String,
    ) -> BoxFuture<Result<Option<FreeServerState>>> {
        self.record("free_server", engine, &[&token], &self.free)
    }
    fn create_free_server(
        &self,
        engine: Url,
        token: String,
        request: CreateFreeServer,
    ) -> BoxFuture<Result<FreeServer>> {
        let version = request.modrinth_version_id.unwrap_or_default();
        self.record(
            "create_free_server",
            engine,
            &[
                &token,
                &request.name,
                &request.kind,
                &version,
                &request.language,
            ],
            &self.created,
        )
    }
    fn create_free_server_from_pack(
        &self,
        engine: Url,
        token: String,
        name: String,
        language: String,
        pack: Vec<u8>,
    ) -> BoxFuture<Result<FreeServer>> {
        let sha256 = launcher_core::download::hash::sha256_hex(&pack);
        self.uploaded.lock().unwrap().push(pack);
        self.record(
            "create_free_server_from_pack",
            engine,
            &[&token, &name, &language, &sha256],
            &self.created,
        )
    }
    fn start_free_server(&self, engine: Url, token: String) -> BoxFuture<Result<FreeServer>> {
        self.record("start_free_server", engine, &[&token], &self.started)
    }
    fn delete_free_server(&self, engine: Url, token: String) -> BoxFuture<Result<bool>> {
        self.record("delete_free_server", engine, &[&token], &self.deleted)
    }
}

/// The player's free server `Survival` (vanilla 1.21.1) with `status`, its invite
/// `invite('F')`, at play.example.com:25570.
pub(crate) fn free_view(status: &str) -> FreeServer {
    FreeServer {
        name: "Survival".into(),
        kind: "vanilla".into(),
        // A vanilla create sends no version, so the Engine runs (and answers) `LATEST`.
        game: InviteGame {
            kind: "vanilla".into(),
            version: Some("LATEST".into()),
            ..InviteGame::default()
        },
        memory_mb: 2048,
        max_players: 10,
        status: status.into(),
        players: Some(InvitePlayers {
            online: 1,
            max: Some(10),
        }),
        address: Some("play.example.com:25570".into()),
        invite_url: Some(invite('F')),
        join_url: Some(format!(
            "perland://join?invite=https%3A%2F%2Fplay.example.com%2F{}",
            "F".repeat(43)
        )),
        created_at: "2026-10-17T00:00:00Z".into(),
        last_played_at: Some("2026-10-17T01:00:00Z".into()),
        deletion: None,
        disk: Disk {
            used_bytes: 1_000,
            limit_bytes: 10_000,
            checked_at: None,
            over: false,
        },
        ..FreeServer::default()
    }
}

/// A modpack free server as the Engine sends it: the loader in `game.type` only.
pub(crate) fn free_modrinth_view(status: &str) -> FreeServer {
    FreeServer {
        kind: "modrinth".into(),
        game: InviteGame {
            kind: "fabric".into(),
            version: Some("1.20.1".into()),
            ..InviteGame::default()
        },
        modrinth: Some(InviteModrinth {
            project_id: "1KVo5zza".into(),
            version_id: "Yp8wLY1P".into(),
        }),
        memory_mb: 6144,
        ..free_view(status)
    }
}

/// A free-server state that allows a create and holds `server`.
pub(crate) fn free_state(server: Option<FreeServer>) -> FreeServerState {
    FreeServerState {
        limits: FreeLimits {
            max_players: 10,
            memory_mb: MemoryLimits {
                vanilla: 2048,
                modded: 6144,
            },
            disk_bytes: 10_000,
            sleep_minutes: 15,
            unused_days: 30,
            warning_days: 7,
            creates_per_week: 3,
            pack: None,
        },
        create: CreateAllowance {
            allowed: server.is_none(),
            reason: server.as_ref().map(|_| "free_server_exists".into()),
            retry_after: None,
        },
        server,
    }
}

/// Sodium (requested) and Fabric API (its dependency) for Fabric 1.20.1, with Mod Menu skipped
/// as part of the pack. `new_instance` makes it a plan for a new Fabric 1.21.1 instance.
pub(crate) fn sample_plan(new_instance: bool) -> InstallPlan {
    let file = |name: &str| PlannedFile {
        url: format!("https://cdn.modrinth.com/data/x/{name}"),
        file_name: name.into(),
        size: 100,
        hashes: Hashes::sha1("a".repeat(40)),
    };
    let item = |id: &str, title: &str, name: &str, reason: ItemReason| PlanItem {
        project_id: id.into(),
        title: title.into(),
        kind: ContentKind::Mod,
        version_id: format!("{id}-v"),
        version_number: "1.0".into(),
        reason,
        file: file(name),
    };
    let minecraft = if new_instance { "1.21.1" } else { "1.20.1" };
    InstallPlan {
        target: Target {
            minecraft: minecraft.into(),
            loader: Some(LoaderKind::Fabric),
        },
        new_instance: new_instance.then(|| NewInstancePlan {
            name: "Sodium".into(),
            minecraft: minecraft.into(),
            loader: Some(LoaderKind::Fabric),
        }),
        items: vec![
            item("SOD", "Sodium", "sodium.jar", ItemReason::Requested),
            item(
                "API",
                "Fabric API",
                "fabric-api.jar",
                ItemReason::Dependency {
                    of: "Sodium".into(),
                },
            ),
        ],
        skipped: vec![PlanSkip {
            project_id: "MENU".into(),
            title: "Mod Menu".into(),
            reason: SkipReason::InPack,
        }],
    }
}

/// Browse answers the test scripts. `install` records what it was asked and can be held open.
pub(crate) struct FakeBrowser {
    pub searches: Mutex<Vec<SearchQuery>>,
    pub installed: Mutex<Installed>,
    /// Each `identify` call: (game folder, whether a lockfile was passed).
    pub identified: Mutex<Vec<(PathBuf, bool)>>,
    /// What `plan` returns (none: `sample_plan` for the target it was given).
    pub plan_result: Mutex<Option<Result<InstallPlan>>>,
    pub plans: Mutex<Vec<(PlanRequest, Option<Target>)>>,
    /// Each `install` call: (game folder, file names).
    pub installs: Mutex<Vec<(PathBuf, Vec<String>)>>,
    pub install_error: Mutex<Option<LauncherError>>,
    /// While set, `install` waits for `open_install_gate`.
    pub hold_install: AtomicBool,
    install_gate: Arc<Semaphore>,
    pub image: Mutex<Option<Image>>,
    /// Modrinth project icon URLs by jar sha1, for `project_icon_url`.
    pub icon_urls: Mutex<HashMap<String, String>>,
    /// Each `project_icon_url` call's sha1.
    pub icon_lookups: Mutex<Vec<String>>,
    /// While set, `project_icon_url` fails as if offline.
    pub icon_lookup_fails: AtomicBool,
    /// What `share_lookup` knows, by sha1.
    pub share_linked: Mutex<HashMap<String, launcher_core::share::Linked>>,
    /// Each `share_lookup` call's sha1s.
    pub share_lookups: Mutex<Vec<Vec<String>>>,
    /// While set, `share_lookup` fails as if offline.
    pub share_lookup_fails: AtomicBool,
}

impl FakeBrowser {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            searches: Mutex::new(Vec::new()),
            installed: Mutex::new(Installed::default()),
            identified: Mutex::new(Vec::new()),
            plan_result: Mutex::new(None),
            plans: Mutex::new(Vec::new()),
            installs: Mutex::new(Vec::new()),
            install_error: Mutex::new(None),
            hold_install: AtomicBool::new(false),
            install_gate: Arc::new(Semaphore::new(0)),
            image: Mutex::new(None),
            icon_urls: Mutex::new(HashMap::new()),
            icon_lookups: Mutex::new(Vec::new()),
            icon_lookup_fails: AtomicBool::new(false),
            share_linked: Mutex::new(HashMap::new()),
            share_lookups: Mutex::new(Vec::new()),
            share_lookup_fails: AtomicBool::new(false),
        })
    }

    pub fn open_install_gate(&self) {
        self.install_gate.add_permits(1);
    }
}

fn empty_page() -> SearchPage {
    SearchPage {
        hits: vec![],
        total: 0,
        offset: 0,
    }
}

impl Browser for FakeBrowser {
    fn search(&self, query: SearchQuery) -> BoxFuture<Result<SearchPage>> {
        self.searches.lock().unwrap().push(query);
        Box::pin(async { Ok(empty_page()) })
    }
    fn categories(&self, _kind: ContentKind) -> BoxFuture<Result<Vec<String>>> {
        Box::pin(async { Ok(vec!["adventure".to_string()]) })
    }
    fn project(&self, _id: String) -> BoxFuture<Result<ProjectDetail>> {
        Box::pin(async { Err(launcher_core::browse::api::not_found()) })
    }
    fn versions(
        &self,
        _id: String,
        _target: Option<Target>,
    ) -> BoxFuture<Result<Vec<VersionInfo>>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn image(&self, _url: String, _kind: ImageKind) -> BoxFuture<Result<Image>> {
        let image = self.image.lock().unwrap().clone();
        Box::pin(async move {
            image.ok_or_else(|| {
                LauncherError::network(ErrorCode::ImageRefused, "That image can't be shown")
            })
        })
    }
    fn identify(
        &self,
        game_dir: PathBuf,
        managed: Option<Managed>,
    ) -> BoxFuture<Result<Installed>> {
        self.identified
            .lock()
            .unwrap()
            .push((game_dir, managed.is_some()));
        let installed = self.installed.lock().unwrap().clone();
        Box::pin(async move { Ok(installed) })
    }
    fn plan(
        &self,
        request: PlanRequest,
        target: Option<Target>,
        _installed: Installed,
    ) -> BoxFuture<Result<InstallPlan>> {
        let new = target.is_none();
        self.plans.lock().unwrap().push((request, target));
        let scripted = self.plan_result.lock().unwrap().clone();
        Box::pin(async move { scripted.unwrap_or_else(|| Ok(sample_plan(new))) })
    }
    fn install(
        &self,
        game_dir: PathBuf,
        items: Vec<PlanItem>,
        _managed: Option<Managed>,
        task: TaskId,
        sink: EventSink,
    ) -> BoxFuture<Result<Vec<String>>> {
        let names: Vec<String> = items.iter().map(|i| i.file.file_name.clone()).collect();
        self.installs
            .lock()
            .unwrap()
            .push((game_dir, names.clone()));
        let error = self.install_error.lock().unwrap().take();
        let hold = self.hold_install.load(Ordering::SeqCst);
        let gate = self.install_gate.clone();
        Box::pin(async move {
            sink(Event::Progress {
                task,
                done: 0,
                total: names.len() as u64,
                bytes: 0,
            });
            if hold {
                gate.acquire().await.expect("never closed").forget();
            }
            match error {
                Some(e) => Err(e),
                None => Ok(names),
            }
        })
    }
    fn loader_version(&self, _kind: LoaderKind, _minecraft: String) -> BoxFuture<Result<String>> {
        Box::pin(async { Ok("0.16.14".to_string()) })
    }
    fn project_icon_url(&self, sha1: String) -> BoxFuture<Result<Option<String>>> {
        self.icon_lookups.lock().unwrap().push(sha1.clone());
        let answer = if self.icon_lookup_fails.load(Ordering::SeqCst) {
            Err(LauncherError::network(ErrorCode::Offline, "offline"))
        } else {
            Ok(self.icon_urls.lock().unwrap().get(&sha1).cloned())
        };
        Box::pin(async move { answer })
    }
    fn share_lookup(
        &self,
        sha1s: Vec<String>,
    ) -> BoxFuture<Result<HashMap<String, launcher_core::share::Linked>>> {
        self.share_lookups.lock().unwrap().push(sha1s.clone());
        let answer = if self.share_lookup_fails.load(Ordering::SeqCst) {
            Err(LauncherError::network(ErrorCode::Offline, "offline"))
        } else {
            let known = self.share_linked.lock().unwrap();
            Ok(sha1s
                .iter()
                .filter_map(|s| known.get(s).map(|l| (s.clone(), l.clone())))
                .collect())
        };
        Box::pin(async move { answer })
    }
}

struct FakeGame {
    rx: oneshot::Receiver<GameExit>,
    slot: ExitSlot,
    /// Held until the game ends, like a real game's output pump.
    sink: EventSink,
}

struct FakeKill(ExitSlot);

impl Kill for FakeKill {
    fn kill(&self) {
        if let Some(tx) = self.0.lock().unwrap().take() {
            let _ = tx.send(GameExit {
                code: None,
                killed: true,
                tail: vec![],
                crash: None,
            });
        }
    }
}

impl RunningGame for FakeGame {
    fn killer(&self) -> Arc<dyn Kill> {
        Arc::new(FakeKill(self.slot.clone()))
    }
    fn wait(self: Box<Self>) -> BoxFuture<Result<GameExit>> {
        Box::pin(async move {
            let FakeGame { rx, sink, .. } = *self;
            let exit = rx
                .await
                .map_err(|_| LauncherError::game(ErrorCode::Internal, "fake game dropped"));
            drop(sink);
            exit
        })
    }
}
