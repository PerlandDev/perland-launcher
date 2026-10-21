//! The live console of a managed server (Manage your server spec §5.2): its log, followed
//! through the Engine's stream and handed to the UI as `server_log` events. One stream at a
//! time for the whole app: the Engine allows a team only a few, and the console shows one
//! server. JWTs come from `AppCore::jwts` and never reach a log.

use std::{sync::Arc, time::Duration};

use launcher_core::{ErrorCode, LauncherError, Result};
use tokio::{
    sync::{mpsc, oneshot, watch},
    time::Instant,
};

use crate::{
    app::AppCore,
    events::{AppEvent, Host},
    manage::{signed_out, LogJwt},
    runner::{BoxFuture, Runner},
};

/// The shortest gap between two `server_log` events of a stream.
pub const LOG_BATCH: Duration = Duration::from_millis(100);
/// The most lines one `server_log` event holds; when more arrived the newest are kept.
pub const LOG_BATCH_LINES: usize = 200;

/// The stream's task stopped before it said how the first connect went (it panicked).
fn stream_lost() -> LauncherError {
    LauncherError::network(ErrorCode::Internal, "The live log couldn't start.")
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// Follows the console log of `stack` in team `tenant`, sending `server_log` events, and
    /// stops the stream already open, whichever server it follows. Returns once the first
    /// connect opened the stream, or with its refusal (`TooManyStreams`, `ServerNotFound`, a
    /// sign-out). A stream that ends on its own later sends `server_log_ended`.
    pub async fn manage_console_open(self: &Arc<Self>, tenant: &str, stack: &str) -> Result<()> {
        let session = self.perland_session().await.ok_or_else(signed_out)?;
        let (stop_tx, stop) = watch::channel(false);
        let previous = self
            .console_stream
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace((tenant.to_string(), stack.to_string(), stop_tx));
        if let Some((_, _, previous)) = previous {
            let _ = previous.send(true);
        }

        let core = self.clone();
        let team = tenant.to_string();
        let jwt: LogJwt = Arc::new(move |refused| {
            let core = core.clone();
            let team = team.clone();
            Box::pin(async move {
                if refused {
                    // The Engine ended this JWT (or the base one it came from): mint both again.
                    core.jwts.forget(&team);
                    core.jwts.forget_base();
                }
                core.manage_jwt(&team).await.map(|(_, jwt)| jwt)
            })
        });
        let (sink, lines) = mpsc::unbounded_channel();
        let (connected_tx, connected) = oneshot::channel();
        let follow = self.runner.stack().follow_logs(
            session.engine,
            jwt,
            stack.to_string(),
            sink,
            connected_tx,
            stop.clone(),
        );
        let (started_tx, started) = oneshot::channel();
        tokio::spawn(self.clone().console_task(
            tenant.to_string(),
            stack.to_string(),
            Stream {
                follow,
                connected,
                lines,
                stop,
            },
            started_tx,
        ));
        started.await.unwrap_or_else(|_| Err(stream_lost()))
    }

    /// Stops the console stream if it follows `stack` of `tenant`; another server's stays.
    pub fn manage_console_close(&self, tenant: &str, stack: &str) {
        let mut slot = self
            .console_stream
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if slot
            .as_ref()
            .is_some_and(|(t, s, _)| t == tenant && s == stack)
        {
            if let Some((_, _, stop)) = slot.take() {
                let _ = stop.send(true);
            }
        }
    }

    /// Clears the console slot when it still holds the stream `stop` belongs to.
    fn forget_console(&self, stop: &watch::Receiver<bool>) {
        let mut slot = self
            .console_stream
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if slot
            .as_ref()
            .is_some_and(|(_, _, s)| s.subscribe().same_channel(stop))
        {
            *slot = None;
        }
    }

    /// Drives one stream: reports how its first connect went through `started`, then groups
    /// its lines into `server_log` events until it is stopped or ends.
    async fn console_task(
        self: Arc<Self>,
        tenant: String,
        stack: String,
        stream: Stream,
        started: oneshot::Sender<Result<()>>,
    ) {
        let Stream {
            mut follow,
            connected,
            mut lines,
            stop,
        } = stream;
        // Waited on in the loop below, while `stop` itself is read in its arms.
        let mut stopping = stop.clone();
        let opened = tokio::select! {
            biased;
            answer = &mut follow => Err(answer),
            c = connected => match c {
                Ok(()) => Ok(()),
                // The stream ended without opening: its answer is the open's.
                Err(_) => Err((&mut follow).await),
            },
        };
        if let Err(answer) = opened {
            self.forget_console(&stop);
            let _ = started.send(answer);
            return;
        }
        let _ = started.send(Ok(()));

        let mut batch = Batch::default();
        let mut lines_open = true;
        loop {
            let deadline = batch.due;
            tokio::select! {
                _ = stopping.wait_for(|s| *s) => return,
                ended = &mut follow => {
                    while let Ok(more) = lines.try_recv() {
                        batch.push(more);
                    }
                    let stopped = *stop.borrow();
                    if !stopped {
                        self.emit_lines(&tenant, &stack, &mut batch);
                    }
                    self.forget_console(&stop);
                    if let (Err(e), false) = (ended, stopped) {
                        tracing::info!(code = ?e.code, "a server's live log ended");
                        self.host.emit(AppEvent::ServerLogEnded {
                            tenant_id: tenant,
                            stack,
                        });
                    }
                    return;
                }
                more = lines.recv(), if lines_open => match more {
                    Some(more) => batch.push(more),
                    None => lines_open = false,
                },
                _ = tokio::time::sleep_until(deadline.unwrap_or_else(Instant::now)), if deadline.is_some() => {
                    if !*stop.borrow() {
                        self.emit_lines(&tenant, &stack, &mut batch);
                    }
                }
            }
        }
    }

    fn emit_lines(&self, tenant: &str, stack: &str, batch: &mut Batch) {
        let lines = batch.take();
        if !lines.is_empty() {
            self.host.emit(AppEvent::ServerLog {
                tenant_id: tenant.to_string(),
                stack: stack.to_string(),
                lines,
            });
        }
    }
}

/// What `follow_logs` handed back for one stream.
struct Stream {
    follow: BoxFuture<Result<()>>,
    connected: oneshot::Receiver<()>,
    lines: mpsc::UnboundedReceiver<Vec<String>>,
    stop: watch::Receiver<bool>,
}

/// Lines waiting for the next event: at most `LOG_BATCH_LINES`, the newest.
#[derive(Default)]
struct Batch {
    lines: std::collections::VecDeque<String>,
    /// When the waiting lines go out: `LOG_BATCH` after the first of them arrived.
    due: Option<Instant>,
}

impl Batch {
    fn push(&mut self, more: Vec<String>) {
        if more.is_empty() {
            return;
        }
        self.due.get_or_insert_with(|| Instant::now() + LOG_BATCH);
        self.lines.extend(more);
        let extra = self.lines.len().saturating_sub(LOG_BATCH_LINES);
        self.lines.drain(..extra);
    }

    fn take(&mut self) -> Vec<String> {
        self.due = None;
        std::mem::take(&mut self.lines).into()
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        invite::Url,
        perland::{Profile, Session},
    };

    use super::*;
    use crate::{
        app::tests::{core, TestCore},
        testing::{player_token, RecordingHost},
    };

    fn sign_in(core: &TestCore) {
        core.perland_store
            .save(&Session {
                engine: Url::parse("https://api.example.com/").unwrap(),
                token: player_token('s'),
                profile: Profile {
                    name: "Steve".into(),
                    email: None,
                    discord: true,
                    minecraft: None,
                },
            })
            .unwrap();
    }

    /// The `server_log` events so far, as (stack, lines).
    fn logs(host: &RecordingHost) -> Vec<(String, Vec<String>)> {
        host.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::ServerLog { stack, lines, .. } => Some((stack.clone(), lines.clone())),
                _ => None,
            })
            .collect()
    }

    fn ended(host: &RecordingHost) -> Vec<AppEvent> {
        host.events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| matches!(e, AppEvent::ServerLogEnded { .. }))
            .cloned()
            .collect()
    }

    async fn wait(ms: u64) {
        tokio::time::sleep(Duration::from_millis(ms)).await;
    }

    fn texts(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn server_log_event_serializes() {
        let e = AppEvent::ServerLog {
            tenant_id: "t".into(),
            stack: "s".into(),
            lines: vec!["a".into()],
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"type":"server_log","tenantId":"t","stack":"s","lines":["a"]})
        );
        let e = AppEvent::ServerLogEnded {
            tenant_id: "t".into(),
            stack: "s".into(),
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"type":"server_log_ended","tenantId":"t","stack":"s"})
        );
    }

    #[tokio::test(start_paused = true)]
    async fn lines_are_batched_every_100ms() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        core.manage_console_open("t1", "survival").await.unwrap();
        let fake = runner.stack.clone();
        assert_eq!(fake.log_streams.lock().unwrap()[0].stack, "survival");
        assert_eq!(
            fake.calls_to("select_tenant").len(),
            1,
            "the stream carries a JWT for the team"
        );

        fake.log_lines(0, &["a"]);
        wait(40).await;
        fake.log_lines(0, &["b", "c"]);
        wait(40).await;
        assert!(logs(&host).is_empty(), "nothing before 100 ms");
        wait(40).await;
        assert_eq!(
            logs(&host),
            [("survival".to_string(), texts(&["a", "b", "c"]))]
        );

        fake.log_lines(0, &["d"]);
        wait(150).await;
        assert_eq!(logs(&host)[1].1, texts(&["d"]));
        assert_eq!(logs(&host).len(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn a_batch_holds_at_most_200_lines() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        core.manage_console_open("t1", "survival").await.unwrap();
        let all: Vec<String> = (0..450).map(|n| format!("l{n}")).collect();
        let refs: Vec<&str> = all.iter().map(String::as_str).collect();
        runner.stack.log_lines(0, &refs[..300]);
        wait(10).await;
        runner.stack.log_lines(0, &refs[300..]);
        wait(150).await;
        let events = logs(&host);
        assert!(events.iter().all(|(_, l)| l.len() <= LOG_BATCH_LINES));
        let last = &events.last().unwrap().1;
        assert_eq!(last.last().unwrap(), "l449");
        assert_eq!(last.first().unwrap(), "l250", "the newest 200 are kept");
    }

    #[tokio::test(start_paused = true)]
    async fn close_stops_the_stream_and_nothing_is_emitted_after() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        core.manage_console_open("t1", "survival").await.unwrap();
        runner.stack.log_lines(0, &["a"]);
        wait(10).await;
        core.manage_console_close("t1", "survival");
        assert!(*runner.stack.log_streams.lock().unwrap()[0].stop.borrow());
        runner.stack.log_lines(0, &["b"]);
        wait(300).await;
        assert!(logs(&host).is_empty());
        assert!(ended(&host).is_empty());
        assert!(core.console_stream.lock().unwrap().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_second_open_stops_the_first() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        core.manage_console_open("t1", "one").await.unwrap();
        core.manage_console_open("t2", "two").await.unwrap();
        let fake = runner.stack.clone();
        assert!(*fake.log_streams.lock().unwrap()[0].stop.borrow());
        assert!(!*fake.log_streams.lock().unwrap()[1].stop.borrow());
        fake.log_lines(0, &["old"]);
        fake.log_lines(1, &["new"]);
        wait(150).await;
        assert_eq!(logs(&host), [("two".to_string(), texts(&["new"]))]);
        let slot = core.console_stream.lock().unwrap();
        let (tenant, stack, _) = slot.as_ref().unwrap();
        assert_eq!((tenant.as_str(), stack.as_str()), ("t2", "two"));
    }

    #[tokio::test(start_paused = true)]
    async fn close_for_another_server_keeps_the_stream() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        core.manage_console_open("t1", "survival").await.unwrap();
        core.manage_console_close("t1", "creative");
        core.manage_console_close("t2", "survival");
        assert!(!*runner.stack.log_streams.lock().unwrap()[0].stop.borrow());
        runner.stack.log_lines(0, &["a"]);
        wait(150).await;
        assert_eq!(logs(&host).len(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stream_error_after_connect_emits_ended() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        core.manage_console_open("t1", "survival").await.unwrap();
        runner.stack.log_lines(0, &["last words"]);
        wait(10).await;
        runner.stack.end_log_stream(
            0,
            LauncherError::network(ErrorCode::TooManyStreams, "Too many streams"),
        );
        wait(10).await;
        assert_eq!(
            logs(&host),
            [("survival".to_string(), texts(&["last words"]))],
            "lines before the end still show"
        );
        assert_eq!(
            ended(&host),
            [AppEvent::ServerLogEnded {
                tenant_id: "t1".into(),
                stack: "survival".into(),
            }]
        );
        assert!(core.console_stream.lock().unwrap().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_refused_first_connect_is_the_answer() {
        let (core, runner, host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        *runner.stack.logs_refusal.lock().unwrap() = Some(LauncherError::network(
            ErrorCode::ServerNotFound,
            "That server is gone",
        ));
        let e = core
            .manage_console_open("t1", "survival")
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::ServerNotFound);
        assert!(ended(&host).is_empty());
        assert!(core.console_stream.lock().unwrap().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn open_signed_out_is_refused() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        let e = core
            .manage_console_open("t1", "survival")
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PerlandSignedOut);
        assert!(runner.stack.calls_to("follow_logs").is_empty());
    }
}
