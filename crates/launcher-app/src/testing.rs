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
    download::{hash::sha1_hex, Hashes},
    events::{Event, EventSink, TaskId},
    instances::{GameSpec, Instance, InstanceStore, SourceRef},
    launch::GameExit,
    locks::InstanceLock,
    sources::{
        DesiredState, FileMode, FileSource, OptionalFile, PackAbout, PackFile, PackInput, Side,
    },
    sync::{
        lockfile::{self, Managed, ManagedFile},
        plan::Changes,
        Cancel, Ensured, ManualFile, PendingUpdate, SyncOutcome, SyncReport,
    },
    ErrorCode, LauncherError, Result,
};
use tokio::sync::{oneshot, Semaphore};

use crate::{
    app::AppCore,
    dto::{NewInstance, PackUpdate},
    events::{AppEvent, ExitInfo, Host, RunStateKind},
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

#[derive(Default)]
pub(crate) struct RecordingHost {
    pub events: Mutex<Vec<AppEvent>>,
    pub minimized: AtomicUsize,
    pub restored: AtomicUsize,
    /// One-shot: the next final `idle` is held back this long before it is recorded, to widen
    /// the window between a run leaving the map and its final state being reported.
    pub idle_delay: Mutex<Option<Duration>>,
}

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
        self.events.lock().unwrap().push(event);
    }
    fn minimize(&self) {
        self.minimized.fetch_add(1, Ordering::SeqCst);
    }
    fn restore(&self) {
        self.restored.fetch_add(1, Ordering::SeqCst);
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
        })
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
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>> {
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
                    "Sky Islands",
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
            store.save(&instance).await?;
            Ok(instance)
        })
    }

    fn find_update(&self, instance: Instance) -> BoxFuture<Result<Option<PendingUpdate>>> {
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
