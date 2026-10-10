//! Play and Stop: the run state machine. A run is `queued` until it holds the install slot,
//! `preparing` while installing, `running` while the game is up, `stopping` after a kill, and
//! leaves the map (idle) with exactly one final `run_state` event carrying how it ended.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use launcher_core::{
    accounts::{Provider, Session},
    events::{Event, EventSink},
    instances::Instance,
    ErrorCode, LauncherError, Result,
};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::{
    app::{AppCore, Run},
    dto::RunSnapshot,
    events::{AppEvent, ExitInfo, Host, ProgressThrottle, RunStateKind, PROGRESS_INTERVAL},
    logs::spawn_batcher,
    runner::Runner,
};

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

        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if runs.contains_key(id) {
            return Err(LauncherError::game(
                ErrorCode::AlreadyRunning,
                "This instance is already running",
            )
            .with_param("id", id));
        }
        self.logs.reset(id);
        // Emitted under the lock, before the task exists, so `queued` always comes first.
        self.emit_state(id, RunStateKind::Queued, None);
        let core = Arc::clone(self);
        let run_id = id.to_string();
        let task =
            tokio::spawn(async move { core.drive(run_id, instance, session, minimize).await });
        runs.insert(
            id.to_string(),
            Run {
                phase: RunStateKind::Queued,
                abort: Some(task.abort_handle()),
                killer: None,
            },
        );
        Ok(())
    }

    /// Cancel a queued or preparing run, or kill a running game. Unknown or idle ids are fine.
    pub fn stop(&self, id: &str) -> Result<()> {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        let Some(run) = runs.get_mut(id) else {
            return Ok(());
        };
        match run.phase {
            RunStateKind::Queued | RunStateKind::Preparing => {
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
        });
    }

    fn set_phase(&self, id: &str, phase: RunStateKind) {
        let mut runs = self.runs.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(run) = runs.get_mut(id) {
            run.phase = phase;
            self.emit_state(id, phase, None);
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
        session: Session,
        minimize: bool,
    ) {
        let (log_tx, batcher) = spawn_batcher(id.clone(), self.host.clone(), self.logs.clone());
        let sink = self.run_sink(&id, log_tx);
        let Ok(permit) = self.install_slot.clone().acquire_owned().await else {
            return;
        };
        self.set_phase(&id, RunStateKind::Preparing);
        let prepared = match self.runner.prepare(instance.clone(), sink.clone()).await {
            Ok(prepared) => prepared,
            Err(error) => return self.finish(&id, failed(error), sink, batcher).await,
        };
        drop(permit);
        let game = match self
            .runner
            .start(instance, prepared, session, sink.clone())
            .await
        {
            Ok(game) => game,
            Err(error) => return self.finish(&id, failed(error), sink, batcher).await,
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
                ..ExitInfo::default()
            },
            Err(error) => failed(error),
        };
        let crashed = info.is_crash();
        self.finish(&id, info, sink, batcher).await;
        if minimize || crashed {
            self.host.restore();
        }
    }

    async fn finish(&self, id: &str, info: ExitInfo, sink: EventSink, batcher: JoinHandle<()>) {
        drop(sink);
        // Let the last log lines reach the UI before the idle state does.
        let _ = tokio::time::timeout(Duration::from_secs(2), batcher).await;
        let removed = self
            .runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id)
            .is_some();
        // A cancelled run was already removed and reported by `stop`.
        if removed {
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
        accounts::BuildKind, launch::CrashHint, settings::Settings, ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::{core, TestCore},
        dto::NewInstance,
        events::RunStateKind::{self, *},
        testing::{exit, FakeRunner, RecordingHost},
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
}
