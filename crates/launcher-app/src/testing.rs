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
        EngineView, InviteAddress, InviteGame, InviteLink, InvitePack, InvitePlayers, InviteTeam,
        InviteView, Url,
    },
    launch::GameExit,
    locks::InstanceLock,
    meta::loaders::LoaderKind,
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
use tokio::sync::{oneshot, Semaphore};

use crate::{
    app::AppCore,
    browser::Browser,
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
    pub browser: Arc<FakeBrowser>,
    /// What `invite_view` answers, by canonical invite (missing: `invite_gone`).
    pub views: Mutex<HashMap<String, Result<EngineView>>>,
    pub view_reads: AtomicUsize,
    /// The Engine each `invite_view` call was seeded with.
    pub engine_hints: Mutex<Vec<Option<String>>>,
    /// What `invite_icon` answers.
    pub icon: Mutex<Option<Vec<u8>>>,
    /// The server each `start` was told to join.
    pub started_joins: Mutex<Vec<Option<ServerAddress>>>,
    /// What `check_update` reports per instance id (none: up to date).
    pub update_checks: Mutex<HashMap<String, UpdateCheck>>,
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
            started_joins: Mutex::new(Vec::new()),
            update_checks: Mutex::new(HashMap::new()),
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
        join: Option<ServerAddress>,
        sink: EventSink,
    ) -> BoxFuture<Result<Box<dyn RunningGame>>> {
        self.started_joins.lock().unwrap().push(join);
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

    fn import_manual(
        &self,
        _id: String,
        found: Vec<(ManualFile, PathBuf)>,
    ) -> BoxFuture<Result<()>> {
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

    fn browser(&self) -> Arc<dyn Browser> {
        self.browser.clone()
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
