//! Play and Stop: the run state machine. A pack instance's run is `checking` while it looks for
//! an update, `asking` while the player decides on one, `queued` until it holds the install
//! slot, `syncing` while its files change, `preparing` while installing, `running` while the
//! game is up, and `stopping` after a Stop that waits on a sync or a kill. A never-synced or
//! pinned pack starts at `queued`; a vanilla instance goes `queued`, `preparing`, `running`.
//! A run leaves the map (idle) with exactly one final `run_state` event carrying how it ended.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use launcher_core::{
    accounts::{Provider, Session},
    events::{Event, EventSink},
    instances::{Instance, SourceRef, UpdatePolicy},
    locks::InstanceLock,
    sync::{
        lockfile::PACK_FILE, manual::Scanner, Cancel, Ensured, ManualFile, PendingUpdate,
        SyncOutcome,
    },
    ErrorCode, LauncherError, Result,
};
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::{
    app::{AppCore, ManualState, Run},
    dto::{PackUpdate, RunSnapshot, SyncSummary},
    events::{AppEvent, ExitInfo, Host, ProgressThrottle, RunStateKind, PROGRESS_INTERVAL},
    logs::spawn_batcher,
    runner::Runner,
};

/// How long a run waits for an instance lock that a just-ended run is still letting go of.
const LOCK_WAIT: Duration = Duration::from_secs(2);

impl<R: Runner, H: Host> AppCore<R, H> {
    /// Queue a run. Everything after this arrives as events.
    pub async fn play(self: &Arc<Self>, id: &str) -> Result<()> {
        let instance = self.instances.load(id).await?;
        if instance.id != id {
            return Err(LauncherError::disk(
                ErrorCode::InstanceDamaged,
                format!("The settings file of \"{id}\" is damaged"),
            )
            .with_param("id", id)
            .with_fix("Delete the instance and create it again")
            .with_detail(format!("instance.json says id \"{}\"", instance.id)));
        }
        let account = self.accounts.lock().await.active().await?.ok_or_else(|| {
            LauncherError::auth(ErrorCode::NoAccount, "Add an account before playing")
                .with_fix("Open Accounts and add one")
        })?;
        if account.provider != Provider::Offline {
            return Err(LauncherError::auth(
                ErrorCode::MicrosoftUnavailable,
                "Microsoft accounts arrive in a later version",
            ));
        }
        let session = account.offline_session();
        let minimize = self.settings_get().await.minimize_on_play;
        let checks = self.checks_for_updates(&instance).await;
        self.start_run(id, instance, Some((session, minimize)), checks)
    }

    /// Does a run of `instance` start with an update check? Only a pack that synced before
    /// and isn't pinned: a first sync installs the newest version anyway.
    pub(crate) async fn checks_for_updates(&self, instance: &Instance) -> bool {
        instance.source != SourceRef::None
            && instance.update_policy != UpdatePolicy::Pinned
            && tokio::fs::try_exists(self.instances.perland_dir(&instance.id).join(PACK_FILE))
                .await
                .unwrap_or(false)
    }

    /// Start a run. `play` is the session and minimize setting, or `None` to only take the
    /// pack's update (the banner's Update).
    pub(crate) fn start_run(
        self: &Arc<Self>,
        id: &str,
        instance: Instance,
        play: Option<(Session, bool)>,
        checks: bool,
    ) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if runs.contains_key(id) {
            return Err(LauncherError::game(
                ErrorCode::AlreadyRunning,
                "This instance is already running",
            )
            .with_param("id", id));
        }
        self.logs.reset(id);
        let first = if checks {
            RunStateKind::Checking
        } else {
            RunStateKind::Queued
        };
        // Emitted under the lock, before the task exists, so the first state always comes first.
        self.emit_state(id, first, None);
        let cancel = Cancel::default();
        let core = Arc::clone(self);
        let (run_id, run_cancel) = (id.to_string(), cancel.clone());
        let task =
            tokio::spawn(
                async move { core.drive(run_id, instance, play, checks, run_cancel).await },
            );
        runs.insert(
            id.to_string(),
            Run {
                phase: first,
                abort: Some(task.abort_handle()),
                killer: None,
                cancel,
                answer: None,
                offer: None,
            },
        );
        Ok(())
    }

    /// The player's answer to "Update ready": take the update, or play what's installed.
    /// Ignored unless the run is asking, so a double click is harmless.
    pub fn play_answer(&self, id: &str, update: bool) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = runs.get_mut(id) {
            if run.phase == RunStateKind::Asking {
                if let Some(answer) = run.answer.take() {
                    let _ = answer.send(update);
                }
                run.offer = None;
            }
        }
        Ok(())
    }

    /// Cancel a run that hasn't started its game, or kill a running game. A sync is asked to
    /// stop, and the run ends once it has. Unknown or idle ids are fine.
    pub fn stop(&self, id: &str) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        let Some(run) = runs.get_mut(id) else {
            return Ok(());
        };
        match run.phase {
            RunStateKind::Checking
            | RunStateKind::Asking
            | RunStateKind::Queued
            | RunStateKind::Preparing => {
                if let Some(abort) = run.abort.take() {
                    abort.abort();
                }
                runs.remove(id);
                self.emit_state(
                    id,
                    RunStateKind::Idle,
                    Some(ExitInfo {
                        cancelled: true,
                        ..ExitInfo::default()
                    }),
                );
            }
            // Aborting mid-sync could leave a file write running after the lock is gone.
            RunStateKind::Syncing => {
                run.phase = RunStateKind::Stopping;
                run.cancel.cancel();
                self.emit_state(id, RunStateKind::Stopping, None);
            }
            RunStateKind::Running => {
                run.phase = RunStateKind::Stopping;
                if let Some(killer) = &run.killer {
                    killer.kill();
                }
                self.emit_state(id, RunStateKind::Stopping, None);
            }
            RunStateKind::Stopping | RunStateKind::Idle => {}
        }
        Ok(())
    }

    /// Stop everything and wait (up to `wait`) until every run has ended. Used when quitting.
    pub async fn stop_all(&self, wait: Duration) {
        let ids: Vec<String> = self
            .runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .keys()
            .cloned()
            .collect();
        for id in ids {
            let _ = self.stop(&id);
        }
        let deadline = tokio::time::Instant::now() + wait;
        while self.has_active_runs() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub fn has_active_runs(&self) -> bool {
        !self
            .runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    }

    pub fn run_states(&self) -> Vec<RunSnapshot> {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .map(|(id, run)| RunSnapshot {
                instance: id.clone(),
                state: run.phase,
                update: run.offer.clone(),
            })
            .collect()
    }

    pub fn log_tail(&self, id: &str) -> Vec<String> {
        self.logs.tail(id)
    }

    fn emit_state(&self, id: &str, state: RunStateKind, exit: Option<ExitInfo>) {
        self.host.emit(AppEvent::RunState {
            instance: id.to_string(),
            state,
            exit,
            update: None,
        });
    }

    /// Move a run on, unless a Stop already put it in `stopping` (it ends from there).
    /// False when it didn't move.
    fn set_phase(&self, id: &str, phase: RunStateKind) -> bool {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        match runs.get_mut(id) {
            Some(run) if run.phase != RunStateKind::Stopping => {
                run.phase = phase;
                self.emit_state(id, phase, None);
                true
            }
            _ => false,
        }
    }

    /// The instance's lock. A run that just ended may still be letting go of it (a cancelled
    /// task is dropped a moment after its `idle`), so a busy lock is retried briefly.
    async fn lock_instance(&self, id: &str) -> Result<InstanceLock> {
        let dir = self.instances.perland_dir(id);
        let deadline = tokio::time::Instant::now() + LOCK_WAIT;
        loop {
            match InstanceLock::try_acquire(&dir) {
                Err(e)
                    if e.code == ErrorCode::InstanceLocked
                        && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(20)).await
                }
                other => return other,
            }
        }
    }

    /// Check for an update and decide whether this run takes it. `auto` packs and update-only
    /// runs take it, and `ask` asks the player. A failed check is a warning: the installed
    /// version plays.
    async fn decide_update(
        &self,
        id: &str,
        instance: &Instance,
        update_only: bool,
    ) -> Option<PendingUpdate> {
        let update = match self.runner.find_update(instance.clone()).await {
            Ok(update) => update?,
            Err(error) => {
                self.host.emit(AppEvent::Warning {
                    instance: Some(id.to_string()),
                    error,
                });
                return None;
            }
        };
        if update_only || instance.update_policy == UpdatePolicy::Auto {
            return Some(update);
        }
        let (tx, rx) = oneshot::channel();
        {
            let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
            let run = runs.get_mut(id)?;
            let offer = PackUpdate::from(&update);
            run.phase = RunStateKind::Asking;
            run.answer = Some(tx);
            run.offer = Some(offer.clone());
            self.host.emit(AppEvent::RunState {
                instance: id.to_string(),
                state: RunStateKind::Asking,
                exit: None,
                update: Some(offer),
            });
        }
        match rx.await {
            Ok(true) => Some(update),
            _ => None,
        }
    }

    /// The sync stopped for files the player must download by hand: remember them for
    /// `manual_import`, and end the run with the list.
    async fn needs_manual(&self, id: &str, files: Vec<ManualFile>) -> ExitInfo {
        self.manual.lock().await.insert(
            id.to_string(),
            ManualState {
                total: files.len(),
                remaining: files.clone(),
                scanner: Scanner::default(),
            },
        );
        ExitInfo {
            error: Some(
                LauncherError::pack(
                    ErrorCode::ManualDownloadsNeeded,
                    format!("{} files have to be downloaded by hand", files.len()),
                )
                .with_param("count", files.len())
                .with_fix("Download them from CurseForge, then try again"),
            ),
            manual: files,
            ..ExitInfo::default()
        }
    }

    /// Core events for one run become app events tagged with the instance.
    fn run_sink(&self, id: &str, log_tx: mpsc::UnboundedSender<String>) -> EventSink {
        let host = self.host.clone();
        let id = id.to_string();
        let throttle = Mutex::new(ProgressThrottle::new(PROGRESS_INTERVAL));
        Arc::new(move |event| match event {
            Event::Progress {
                task,
                done,
                total,
                bytes,
            } => {
                let admit = throttle.lock().unwrap_or_else(|e| e.into_inner()).admit(
                    &task.key,
                    done,
                    total,
                    Instant::now(),
                );
                if admit {
                    host.emit(AppEvent::Progress {
                        instance: Some(id.clone()),
                        task,
                        done,
                        total,
                        bytes,
                    });
                }
            }
            Event::Log { line } => {
                let _ = log_tx.send(line);
            }
            Event::Warning(error) => host.emit(AppEvent::Warning {
                instance: Some(id.clone()),
                error,
            }),
            Event::StateChanged { .. } => {}
        })
    }

    async fn drive(
        self: Arc<Self>,
        id: String,
        instance: Instance,
        play: Option<(Session, bool)>,
        checks: bool,
        cancel: Cancel,
    ) {
        let update_only = play.is_none();
        let ended = |info: ExitInfo| ExitInfo {
            update_only,
            ..info
        };
        let (log_tx, batcher) = spawn_batcher(id.clone(), self.host.clone(), self.logs.clone());
        let sink = self.run_sink(&id, log_tx);
        // Held from the update check until the run ends: the CLI or another window can't
        // change this instance's files meanwhile (spec addendum §4.2).
        let lock = match self.lock_instance(&id).await {
            Ok(lock) => Arc::new(lock),
            Err(error) => {
                return self
                    .finish(&id, ended(failed(error)), sink, batcher, None)
                    .await
            }
        };
        let accept = if checks {
            let accept = self.decide_update(&id, &instance, update_only).await;
            self.set_phase(&id, RunStateKind::Queued);
            accept
        } else {
            None
        };
        let Ok(permit) = self.install_slot.clone().acquire_owned().await else {
            return;
        };
        let mut instance = instance;
        let mut synced = None;
        if instance.source != SourceRef::None {
            self.set_phase(&id, RunStateKind::Syncing);
            let result = self
                .runner
                .sync(
                    lock.clone(),
                    id.clone(),
                    accept,
                    cancel.clone(),
                    sink.clone(),
                )
                .await;
            let report = match result {
                Ok(Ensured {
                    outcome: SyncOutcome::Done(report),
                    warning,
                }) => {
                    if let Some(error) = warning {
                        self.host.emit(AppEvent::Warning {
                            instance: Some(id.clone()),
                            error,
                        });
                    }
                    self.manual.lock().await.remove(&id);
                    report
                }
                Ok(Ensured {
                    outcome: SyncOutcome::NeedsManual(files),
                    ..
                }) => {
                    let info = self.needs_manual(&id, files).await;
                    return self
                        .finish(&id, ended(info), sink, batcher, Some(lock))
                        .await;
                }
                Err(error) if error.code == ErrorCode::SyncCancelled => {
                    let info = ExitInfo {
                        cancelled: true,
                        ..ExitInfo::default()
                    };
                    return self
                        .finish(&id, ended(info), sink, batcher, Some(lock))
                        .await;
                }
                Err(error) => {
                    return self
                        .finish(&id, ended(failed(error)), sink, batcher, Some(lock))
                        .await
                }
            };
            synced = (!report.up_to_date).then(|| SyncSummary::from(&report));
            // Stop arrived once the swap had begun: the sync finished, but the player wants out.
            if cancel.is_cancelled() {
                let info = ExitInfo {
                    cancelled: true,
                    synced,
                    ..ExitInfo::default()
                };
                return self
                    .finish(&id, ended(info), sink, batcher, Some(lock))
                    .await;
            }
            // A pack update can change the game version.
            instance = match self.instances.load(&id).await {
                Ok(instance) => instance,
                Err(error) => {
                    return self
                        .finish(&id, ended(failed(error)), sink, batcher, Some(lock))
                        .await
                }
            };
        }
        let Some((session, minimize)) = play else {
            drop(permit);
            let info = ExitInfo {
                synced,
                update_only: true,
                ..ExitInfo::default()
            };
            return self.finish(&id, info, sink, batcher, Some(lock)).await;
        };
        // A Stop that came after the sync returned (while the instance reloaded) left the run
        // `stopping`: end it here rather than start a game nobody wants.
        if !self.set_phase(&id, RunStateKind::Preparing) {
            let info = ExitInfo {
                cancelled: true,
                synced,
                ..ExitInfo::default()
            };
            return self.finish(&id, info, sink, batcher, Some(lock)).await;
        }
        let prepared = match self.runner.prepare(instance.clone(), sink.clone()).await {
            Ok(prepared) => prepared,
            Err(error) => {
                let info = ExitInfo {
                    synced,
                    ..failed(error)
                };
                return self.finish(&id, info, sink, batcher, Some(lock)).await;
            }
        };
        drop(permit);
        let game = match self
            .runner
            .start(instance, prepared, session, sink.clone())
            .await
        {
            Ok(game) => game,
            Err(error) => {
                let info = ExitInfo {
                    synced,
                    ..failed(error)
                };
                return self.finish(&id, info, sink, batcher, Some(lock)).await;
            }
        };
        // No `.await` between `start` returning and recording the killer, so a Stop can't slip
        // in and leave the game running with nobody holding its killer.
        {
            let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
            match runs.get_mut(&id) {
                Some(run) => {
                    run.phase = RunStateKind::Running;
                    run.killer = Some(game.killer());
                    run.abort = None;
                    self.emit_state(&id, RunStateKind::Running, None);
                }
                None => {
                    game.killer().kill();
                    return;
                }
            }
        }
        if minimize {
            self.host.minimize();
        }
        let info = match game.wait().await {
            Ok(exit) => ExitInfo {
                code: exit.code,
                killed: exit.killed,
                crash: exit.crash,
                synced,
                ..ExitInfo::default()
            },
            Err(error) => ExitInfo {
                synced,
                ..failed(error)
            },
        };
        let crashed = info.is_crash();
        self.finish(&id, info, sink, batcher, Some(lock)).await;
        if minimize || crashed {
            self.host.restore();
        }
    }

    async fn finish(
        &self,
        id: &str,
        info: ExitInfo,
        sink: EventSink,
        batcher: JoinHandle<()>,
        lock: Option<Arc<InstanceLock>>,
    ) {
        drop(sink);
        // Let the last log lines reach the UI before the idle state does.
        let _ = tokio::time::timeout(Duration::from_secs(2), batcher).await;
        // Let go of the instance before saying so: a Play right after `idle` must get the lock.
        drop(lock);
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        // A cancelled run was already removed and reported by `stop`. The final state goes out
        // under the lock, like `stop` does, so a Play arriving now can't report `queued` first.
        if runs.remove(id).is_some() {
            self.emit_state(id, RunStateKind::Idle, Some(info));
        }
    }
}

fn failed(error: LauncherError) -> ExitInfo {
    ExitInfo {
        error: Some(error),
        ..ExitInfo::default()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::atomic::Ordering, time::Duration};

    use launcher_core::{
        accounts::BuildKind,
        instances::{GameSpec, UpdatePolicy},
        launch::CrashHint,
        locks::InstanceLock,
        settings::Settings,
        sync::{plan::Changes, Ensured, SyncOutcome, SyncReport},
        ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::{core, TestCore},
        dto::NewInstance,
        events::RunStateKind::{self, *},
        testing::{desired, exit, manual_file, pack_instance, pending, FakeRunner, RecordingHost},
    };
    use std::sync::Arc;

    async fn ready(
        minimize: bool,
    ) -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        let (core, runner, host, tmp) = core(BuildKind::Dev);
        core.settings_set(Settings {
            minimize_on_play: minimize,
            ..Settings::default()
        })
        .await
        .unwrap();
        core.account_add_offline("Steve").await.unwrap();
        for name in ["Alpha", "Beta"] {
            core.instance_create(NewInstance {
                name: name.into(),
                minecraft: "1.20.1".into(),
                loader: None,
            })
            .await
            .unwrap();
        }
        (core, runner, host, tmp)
    }

    #[tokio::test]
    async fn play_runs_through_every_state() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert!(core.has_active_runs());
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.states("alpha"), vec![Queued, Preparing, Running, Idle]);
        assert_eq!(host.last_exit("alpha").unwrap().code, Some(0));
        assert_eq!(host.minimized.load(Ordering::SeqCst), 1);
        assert_eq!(host.restored.load(Ordering::SeqCst), 1);
        assert!(!core.has_active_runs());
    }

    #[tokio::test]
    async fn play_needs_an_account() {
        let (core, _, _, _t) = ready(true).await;
        let uuid = core.accounts_list().await.unwrap().accounts[0].uuid.clone();
        core.account_remove(&uuid).await.unwrap();
        assert_eq!(
            core.play("alpha").await.unwrap_err().code,
            ErrorCode::NoAccount
        );
    }

    #[tokio::test]
    async fn same_instance_cannot_run_twice() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        assert_eq!(
            core.play("alpha").await.unwrap_err().code,
            ErrorCode::AlreadyRunning
        );
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }

    #[tokio::test]
    async fn stop_while_queued_cancels_without_touching_the_slot() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        host.wait_for("alpha", Preparing).await; // alpha holds the install slot
        core.play("beta").await.unwrap(); // beta waits for it
        core.stop("beta").unwrap();
        assert_eq!(host.states("beta"), vec![Queued, Idle]);
        assert!(host.last_exit("beta").unwrap().cancelled);
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        core.stop("alpha").unwrap();
        host.wait_for("alpha", Idle).await;
    }

    #[tokio::test]
    async fn stop_while_preparing_releases_the_slot() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        host.wait_for("alpha", Preparing).await;
        core.stop("alpha").unwrap();
        assert!(host.last_exit("alpha").unwrap().cancelled);
        core.play("beta").await.unwrap();
        host.wait_for("beta", Preparing).await;
        runner.open_gate(1);
        host.wait_for("beta", Running).await;
        core.stop("beta").unwrap();
        host.wait_for("beta", Idle).await;
    }

    #[tokio::test]
    async fn stop_while_running_kills_the_game() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        core.stop("alpha").unwrap();
        core.stop("alpha").unwrap(); // a second click is harmless
        host.wait_for("alpha", Idle).await;
        assert_eq!(
            host.states("alpha"),
            vec![Queued, Preparing, Running, Stopping, Idle]
        );
        let exit = host.last_exit("alpha").unwrap();
        assert!(exit.killed && !exit.is_crash());
    }

    #[tokio::test]
    async fn two_instances_run_side_by_side() {
        let (core, runner, host, _t) = ready(false).await;
        core.play("alpha").await.unwrap();
        core.play("beta").await.unwrap();
        runner.open_gate(2);
        host.wait_for("alpha", Running).await;
        host.wait_for("beta", Running).await;
        let mut states = core.run_states();
        states.sort_by(|a, b| a.instance.cmp(&b.instance));
        assert_eq!(states.len(), 2);
        assert!(states.iter().all(|s| s.state == Running));
        runner.finish("alpha", exit(Some(0)));
        runner.finish("beta", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        host.wait_for("beta", Idle).await;
    }

    #[tokio::test]
    async fn prepare_failure_ends_in_idle_with_the_error() {
        let (core, runner, host, _t) = ready(true).await;
        *runner.prepare_error.lock().unwrap() =
            Some(LauncherError::network(ErrorCode::Offline, "offline"));
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Idle).await;
        let exit = host.last_exit("alpha").unwrap();
        assert_eq!(exit.error.unwrap().code, ErrorCode::Offline);
        assert_eq!(host.minimized.load(Ordering::SeqCst), 0);
        assert!(!core.has_active_runs());
    }

    #[tokio::test]
    async fn crash_brings_the_window_back() {
        let (core, runner, host, _t) = ready(false).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        runner.finish(
            "alpha",
            launcher_core::launch::GameExit {
                code: Some(1),
                killed: false,
                tail: vec![],
                crash: Some(CrashHint::OutOfMemory),
            },
        );
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.minimized.load(Ordering::SeqCst), 0);
        assert_eq!(host.restored.load(Ordering::SeqCst), 1);
        assert_eq!(
            host.last_exit("alpha").unwrap().crash,
            Some(CrashHint::OutOfMemory)
        );
    }

    #[tokio::test]
    async fn busy_instances_refuse_changes() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(
            core.instance_rename("alpha", "New").await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        assert_eq!(
            core.instance_delete("alpha").await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        let mut changed = core.instances.load("alpha").await.unwrap();
        changed.java.memory_mb = 8192;
        assert_eq!(
            core.instance_update(changed).await.unwrap_err().code,
            ErrorCode::InstanceBusy
        );
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        core.instance_rename("alpha", "New").await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_finished_runs_idle_is_reported_before_the_next_runs_queued() {
        let (core, runner, host, _t) = ready(false).await;
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        *host.idle_delay.lock().unwrap() = Some(Duration::from_millis(300));
        runner.finish("alpha", exit(Some(0)));
        // The moment the run is gone (or, once fixed, once its idle is out), Play again.
        while core.has_active_runs() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        core.play("alpha").await.unwrap();
        // Only the prefix is pinned: the second run's drive task may already have emitted Preparing.
        let states = host.states("alpha");
        assert_eq!(&states[..5], &[Queued, Preparing, Running, Idle, Queued]);
        core.stop("alpha").unwrap();
    }

    #[tokio::test]
    async fn a_running_instances_mods_can_be_added_but_not_toggled_or_removed() {
        let (core, runner, host, tmp) = ready(true).await;
        let jar = |name: &str| {
            let path = tmp.path().join(name);
            std::fs::write(&path, b"jar").unwrap();
            path
        };
        core.mods_add("alpha", vec![jar("sodium.jar")])
            .await
            .unwrap();
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        assert_eq!(
            core.mod_set_enabled("alpha", "sodium.jar", false)
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceBusy
        );
        assert_eq!(
            core.mod_remove("alpha", "sodium.jar")
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceBusy
        );
        // A new jar doesn't touch loaded files; it applies on the next Play.
        let list = core
            .mods_add("alpha", vec![jar("lithium.jar")])
            .await
            .unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|m| m.enabled));
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        let list = core
            .mod_set_enabled("alpha", "sodium.jar", false)
            .await
            .unwrap();
        assert!(list.iter().any(|m| !m.enabled));
        core.mod_remove("alpha", "sodium.jar.disabled")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn game_logs_are_batched_and_kept_for_log_tail() {
        let (core, runner, host, _t) = ready(true).await;
        *runner.start_lines.lock().unwrap() = vec!["[INFO] one".into(), "[INFO] two".into()];
        core.play("alpha").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await;
        runner.finish("alpha", exit(Some(0)));
        host.wait_for("alpha", Idle).await;
        assert_eq!(host.log_lines("alpha"), vec!["[INFO] one", "[INFO] two"]);
        assert_eq!(core.log_tail("alpha"), vec!["[INFO] one", "[INFO] two"]);
        // The lines arrive before the idle event.
        let events = host.events.lock().unwrap();
        let log_at = events
            .iter()
            .position(|e| matches!(e, crate::AppEvent::Log { .. }))
            .unwrap();
        let idle_at = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    crate::AppEvent::RunState {
                        state: RunStateKind::Idle,
                        ..
                    }
                )
            })
            .unwrap();
        assert!(log_at < idle_at);
    }

    #[tokio::test]
    async fn progress_is_throttled_but_keeps_the_ends() {
        let (core, runner, host, _t) = ready(true).await;
        *runner.progress_steps.lock().unwrap() = 100;
        core.play("alpha").await.unwrap();
        host.wait_for("alpha", Preparing).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(
            host.progress_count("alpha"),
            2,
            "first and last of 101 updates"
        );
        core.stop("alpha").unwrap();
    }

    #[tokio::test]
    async fn a_copied_instance_with_the_wrong_id_cannot_play() {
        let (core, _runner, _host, _t) = ready(true).await;
        let mut copied = core.instances.load("beta").await.unwrap();
        copied.id = "alpha".into();
        std::fs::write(
            core.instances.instance_dir("beta").join("instance.json"),
            serde_json::to_vec_pretty(&copied).unwrap(),
        )
        .unwrap();
        assert_eq!(
            core.play("beta").await.unwrap_err().code,
            ErrorCode::InstanceDamaged
        );
        assert!(!core.has_active_runs());
    }

    #[tokio::test]
    async fn stop_all_waits_for_every_run() {
        let (core, runner, host, _t) = ready(true).await;
        core.play("alpha").await.unwrap();
        core.play("beta").await.unwrap();
        runner.open_gate(1);
        host.wait_for("alpha", Running).await; // beta is still preparing or queued
        core.stop_all(Duration::from_secs(2)).await;
        assert!(!core.has_active_runs());
        assert!(host.last_exit("alpha").unwrap().killed);
        assert!(host.last_exit("beta").unwrap().cancelled);
    }

    /// A pack instance "Sky" (id `sky`) that synced 1.0.3 before, with `policy`.
    async fn pack(core: &TestCore, policy: UpdatePolicy) -> String {
        let installed = desired("1.0.3", vec![]);
        let mut instance = pack_instance(core, "Sky", Some(&installed)).await;
        instance.update_policy = policy;
        core.instances.save(&instance).await.unwrap();
        instance.id
    }

    fn offer(runner: &FakeRunner, id: &str) {
        runner
            .updates
            .lock()
            .unwrap()
            .insert(id.to_string(), Ok(Some(pending("1.0.3", "1.0.4"))));
    }

    fn lock_of(core: &TestCore, id: &str) -> launcher_core::Result<InstanceLock> {
        InstanceLock::try_acquire(&core.instances.perland_dir(id))
    }

    #[tokio::test]
    async fn a_pack_checks_then_syncs_then_plays() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Preparing, Running]
        );
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        assert!(
            host.last_exit(&id).unwrap().synced.is_none(),
            "nothing changed"
        );
    }

    #[tokio::test]
    async fn ask_waits_for_the_player_and_takes_the_update() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        assert_eq!(host.last_update(&id).unwrap().to_version, "1.0.4");
        let snapshot = core.run_states();
        assert_eq!(
            snapshot[0].update.as_ref().unwrap().from_version.as_deref(),
            Some("1.0.3"),
            "a reloaded UI can show the dialog again"
        );
        assert!(
            runner.syncs.lock().unwrap().is_empty(),
            "nothing syncs before the answer"
        );
        core.play_answer(&id, true).unwrap();
        core.play_answer(&id, true).unwrap(); // a second click is harmless
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Asking, Queued, Syncing, Preparing, Running]
        );
        assert_eq!(
            *runner.syncs.lock().unwrap(),
            vec![(id.clone(), Some("1.0.4".to_string()))]
        );
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn playing_the_installed_version_skips_the_update() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        core.play_answer(&id, false).unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn auto_takes_updates_without_asking_and_pinned_never_checks() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Auto).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Preparing, Running]
        );
        assert_eq!(runner.syncs.lock().unwrap()[0].1.as_deref(), Some("1.0.4"));
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;

        let mut pinned = core.instances.load(&id).await.unwrap();
        pinned.update_policy = UpdatePolicy::Pinned;
        core.instances.save(&pinned).await.unwrap();
        let checks = runner.checks.load(Ordering::SeqCst);
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for_nth(&id, Running, 2).await;
        assert_eq!(
            runner.checks.load(Ordering::SeqCst),
            checks,
            "pinned never checks"
        );
        let states = host.states(&id);
        assert_eq!(
            &states[states.len() - 4..],
            &[Queued, Syncing, Preparing, Running]
        );
        core.stop(&id).unwrap();
        host.wait_for_nth(&id, Idle, 2).await;
    }

    #[tokio::test]
    async fn stop_while_asking_cancels_without_syncing() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        core.stop(&id).unwrap();
        assert_eq!(host.states(&id), vec![Checking, Asking, Idle]);
        assert!(host.last_exit(&id).unwrap().cancelled);
        assert!(runner.syncs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_failed_check_warns_and_plays_the_installed_version() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.updates.lock().unwrap().insert(
            id.clone(),
            Err(LauncherError::network(
                ErrorCode::UpdateCheckFailed,
                "Couldn't check for updates",
            )),
        );
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert!(host.warnings().contains(&ErrorCode::UpdateCheckFailed));
        assert_eq!(*runner.syncs.lock().unwrap(), vec![(id.clone(), None)]);
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn stop_while_syncing_waits_for_the_sync_to_stop() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.hold_sync.store(true, Ordering::SeqCst);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Syncing).await;
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
        assert_eq!(
            host.states(&id),
            vec![Checking, Queued, Syncing, Stopping, Idle]
        );
        assert!(host.last_exit(&id).unwrap().cancelled);
        assert!(runner.prepared.lock().unwrap().is_empty());
        lock_of(&core, &id).expect("released once the sync stopped");
    }

    #[tokio::test]
    async fn a_stop_that_lands_after_the_sync_returned_never_starts_the_game() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.hold_sync.store(true, Ordering::SeqCst);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Syncing).await;
        // A Stop the sync never saw: it came once the sync had already returned.
        core.runs.lock().unwrap().get_mut(&id).unwrap().phase = Stopping;
        runner.open_sync_gate(1);
        runner.open_gate(1);
        host.wait_for(&id, Idle).await;
        assert!(host.last_exit(&id).unwrap().cancelled);
        assert!(runner.prepared.lock().unwrap().is_empty());
        assert!(!host.states(&id).contains(&Running));
    }

    #[tokio::test]
    async fn blocked_files_end_the_run_with_the_manual_list() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.sync_results.lock().unwrap().push_back(Ok(Ensured {
            outcome: SyncOutcome::NeedsManual(vec![manual_file("optifine.jar", b"optifine")]),
            warning: None,
        }));
        core.play(&id).await.unwrap();
        host.wait_for(&id, Idle).await;
        let exit = host.last_exit(&id).unwrap();
        let error = exit.error.clone().unwrap();
        assert_eq!(error.code, ErrorCode::ManualDownloadsNeeded);
        assert_eq!(error.params["count"], "1");
        assert_eq!(exit.manual.len(), 1);
        assert!(!exit.is_crash());
        assert!(core.manual.lock().await.contains_key(&id));
    }

    #[tokio::test]
    async fn the_lock_is_held_from_check_to_game_exit() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        assert_eq!(
            lock_of(&core, &id).unwrap_err().code,
            ErrorCode::InstanceLocked
        );
        core.play_answer(&id, false).unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(
            lock_of(&core, &id).unwrap_err().code,
            ErrorCode::InstanceLocked
        );
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        lock_of(&core, &id).expect("free once the game exited");
    }

    #[tokio::test]
    async fn a_locked_instance_ends_the_run_with_instance_locked() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        let _cli = lock_of(&core, &id).unwrap(); // the CLI is syncing it
        core.play(&id).await.unwrap();
        host.wait_for(&id, Idle).await; // after the short retry
        assert_eq!(
            host.last_exit(&id).unwrap().error.unwrap().code,
            ErrorCode::InstanceLocked
        );
        assert!(runner.syncs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn play_again_right_after_a_cancel_gets_the_lock() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        offer(&runner, &id);
        core.play(&id).await.unwrap();
        host.wait_for(&id, Asking).await;
        core.stop(&id).unwrap();
        core.play(&id).await.unwrap(); // the cancelled task may still hold the lock
        host.wait_for_nth(&id, Asking, 2).await;
        core.stop(&id).unwrap();
    }

    #[tokio::test]
    async fn a_pack_update_that_changes_the_game_prepares_the_new_version() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        *runner.sync_game.lock().unwrap() = Some(GameSpec {
            minecraft: "1.21.1".into(),
            loader: None,
        });
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        assert_eq!(runner.prepared.lock().unwrap()[0].minecraft, "1.21.1");
        core.stop(&id).unwrap();
        host.wait_for(&id, Idle).await;
    }

    #[tokio::test]
    async fn sync_results_reach_the_exit_and_warnings_reach_the_ui() {
        let (core, runner, host, _t) = ready(true).await;
        let id = pack(&core, UpdatePolicy::Ask).await;
        runner.sync_results.lock().unwrap().push_back(Ok(Ensured {
            outcome: SyncOutcome::Done(SyncReport {
                up_to_date: false,
                changes: Changes {
                    from_version: Some("1.0.3".into()),
                    to_version: "1.0.4".into(),
                    added: vec!["JEI".into()],
                    updated: vec![],
                    removed: vec![],
                },
                kept_modified: vec!["config/a.toml".into()],
                game_changed: false,
            }),
            warning: Some(
                LauncherError::network(ErrorCode::UpdateDownloadFailed, "x")
                    .with_param("version", "1.0.3"),
            ),
        }));
        core.play(&id).await.unwrap();
        runner.open_gate(1);
        host.wait_for(&id, Running).await;
        runner.finish(&id, exit(Some(0)));
        host.wait_for(&id, Idle).await;
        let synced = host.last_exit(&id).unwrap().synced.unwrap();
        assert_eq!((synced.to_version.as_str(), synced.added), ("1.0.4", 1));
        assert_eq!(synced.kept_modified, vec!["config/a.toml".to_string()]);
        assert!(host.warnings().contains(&ErrorCode::UpdateDownloadFailed));
    }
}
