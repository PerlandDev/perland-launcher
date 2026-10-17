//! What matters from the app's events goes to the launcher log, then on to the window.
use crate::{
    events::{AppEvent, Host, RunStateKind},
    updates::UpdateState,
};

pub struct LoggedHost<H: Host>(pub H);

impl<H: Host> Host for LoggedHost<H> {
    fn emit(&self, event: AppEvent) {
        log_event(&event);
        self.0.emit(event);
    }
    fn minimize(&self) {
        self.0.minimize()
    }
    fn restore(&self) {
        self.0.restore()
    }
    fn open_url(&self, url: &str) {
        self.0.open_url(url)
    }
}

pub fn log_event(event: &AppEvent) {
    match event {
        AppEvent::Warning { instance, error } => tracing::warn!(
            instance = instance.as_deref().unwrap_or(""),
            code = ?error.code,
            detail = error.detail.as_deref().unwrap_or(""),
            "{}",
            error.message
        ),
        AppEvent::RunState {
            instance,
            state: RunStateKind::Running,
            ..
        } => {
            tracing::info!(instance = instance.as_str(), "game started")
        }
        AppEvent::RunState {
            instance,
            exit: Some(exit),
            ..
        } => tracing::info!(
            instance = instance.as_str(),
            code = ?exit.code,
            killed = exit.killed,
            cancelled = exit.cancelled,
            crashed = exit.is_crash(),
            error = ?exit.error.as_ref().map(|e| e.code),
            synced = ?exit.synced.as_ref().map(|s| (s.added, s.updated, s.removed)),
            "run ended"
        ),
        AppEvent::Update {
            state: UpdateState::Downloading { .. },
        } => {}
        AppEvent::Update { state } => tracing::info!(state = ?state, "launcher update"),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{events::ExitInfo, testing::RecordingHost, updates::UpdateState};
    use launcher_core::{ErrorCode, LauncherError};
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Default)]
    struct Buf(Arc<Mutex<Vec<u8>>>);
    impl std::io::Write for Buf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn logged(events: Vec<AppEvent>) -> (String, LoggedHost<RecordingHost>) {
        let buf = Buf::default();
        let writer = buf.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let host = LoggedHost(RecordingHost::default());
        tracing::subscriber::with_default(subscriber, || {
            for e in events {
                host.emit(e);
            }
        });
        let text = String::from_utf8(buf.0.lock().unwrap().clone()).unwrap();
        (text, host)
    }

    #[test]
    fn warnings_runs_and_updates_are_logged_and_passed_on() {
        let (text, host) = logged(vec![
            AppEvent::Warning {
                instance: None,
                error: LauncherError::disk(
                    ErrorCode::SettingsDamaged,
                    "Your launcher settings couldn't be read",
                ),
            },
            AppEvent::RunState {
                instance: "asalith".into(),
                state: crate::events::RunStateKind::Running,
                exit: None,
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith".into(),
                state: crate::events::RunStateKind::Idle,
                exit: Some(ExitInfo {
                    code: Some(1),
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::Update {
                state: UpdateState::Ready {
                    version: "0.2.0-beta.2".into(),
                },
            },
        ]);
        assert!(text.contains("SettingsDamaged"), "{text}");
        assert!(text.contains("game started"), "{text}");
        assert!(
            text.contains("run ended") && text.contains("Some(1)"),
            "{text}"
        );
        assert!(text.contains("0.2.0-beta.2"), "{text}");
        assert_eq!(
            host.0.events.lock().unwrap().len(),
            4,
            "every event still reaches the window"
        );
    }

    #[test]
    fn progress_and_download_ticks_stay_out_of_the_log() {
        let (text, _) = logged(vec![AppEvent::Update {
            state: UpdateState::Downloading { percent: Some(40) },
        }]);
        assert!(text.is_empty(), "{text}");
    }
}
