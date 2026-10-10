//! Fakes for the app tests: a runner whose preparing and exits the test controls, and a host
//! that records everything it was told.
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use launcher_core::{
    accounts::Session,
    events::{Event, EventSink, TaskId},
    instances::Instance,
    launch::GameExit,
    ErrorCode, LauncherError, Result,
};
use tokio::sync::{oneshot, Semaphore};

use crate::{
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
pub(crate) struct FakeRunner {
    gate: Arc<Semaphore>,
    pub prepare_error: Mutex<Option<LauncherError>>,
    pub progress_steps: Mutex<u64>,
    pub start_lines: Mutex<Vec<String>>,
    exits: Mutex<HashMap<String, ExitSlot>>,
}

impl FakeRunner {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            gate: Arc::new(Semaphore::new(0)),
            prepare_error: Mutex::new(None),
            progress_steps: Mutex::new(0),
            start_lines: Mutex::new(Vec::new()),
            exits: Mutex::new(HashMap::new()),
        })
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
