//! What the app tells the UI, and the host (the window) it tells it through.
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use launcher_core::{events::TaskId, launch::CrashHint, sync::ManualFile, LauncherError};
use serde::Serialize;

use crate::dto::{PackUpdate, SyncSummary};

/// Progress for one task reaches the UI at most this often (first and last updates always do).
pub const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStateKind {
    Idle,
    /// Looking for a pack update.
    Checking,
    /// Waiting for the player's answer to "Update ready".
    Asking,
    Queued,
    /// Bringing the pack's files in line.
    Syncing,
    Preparing,
    /// Waiting for a sleeping server to start before the game joins it.
    Waking,
    Running,
    Stopping,
}

/// How a run ended. Exactly one of these is usually interesting: an error, a cancel, a kill, or an exit code.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub killed: bool,
    pub cancelled: bool,
    pub crash: Option<CrashHint>,
    pub error: Option<LauncherError>,
    /// What the run's sync changed (None when nothing changed or nothing synced).
    pub synced: Option<SyncSummary>,
    /// Files the sync needs the player to download by hand (`manual_downloads_needed`).
    pub manual: Vec<ManualFile>,
    /// The banner's Update: the run synced and stopped there, with no game.
    pub update_only: bool,
}

impl ExitInfo {
    /// An exit nobody asked for that wasn't clean: the window comes back and the crash card shows.
    pub fn is_crash(&self) -> bool {
        !self.killed
            && !self.cancelled
            && self.error.is_none()
            && self.code != Some(0)
            && !self.update_only
    }
}

// A run-state event carries a whole `ExitInfo`; events are short-lived and rare, so boxing would
// only make every literal in the app and its tests noisier.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppEvent {
    Progress {
        instance: Option<String>,
        task: TaskId,
        done: u64,
        total: u64,
        bytes: u64,
    },
    Log {
        instance: String,
        lines: Vec<String>,
    },
    RunState {
        instance: String,
        state: RunStateKind,
        exit: Option<ExitInfo>,
        /// The update on offer while `asking`.
        update: Option<PackUpdate>,
    },
    Warning {
        instance: Option<String>,
        error: LauncherError,
    },
    /// A `perland://` link arrived; the UI asks for it with `deep_link_take`.
    DeepLink,
    /// The launcher's own update moved on (see `updates.rs`).
    Update { state: crate::updates::UpdateState },
}

/// The window the app runs in. Tauri implements it; tests record what it was asked to do.
pub trait Host: Send + Sync + 'static {
    fn emit(&self, event: AppEvent);
    fn minimize(&self);
    fn restore(&self);
}

/// Drops progress updates that arrive faster than the UI can usefully show them.
pub struct ProgressThrottle {
    interval: Duration,
    last: HashMap<String, Instant>,
}

impl ProgressThrottle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: HashMap::new(),
        }
    }

    /// True when this update should reach the UI: a task's first and last updates always do,
    /// the ones between at most once per interval.
    pub fn admit(&mut self, task: &str, done: u64, total: u64, now: Instant) -> bool {
        let edge = done == 0 || done >= total;
        match self.last.get(task) {
            Some(prev) if !edge && now.duration_since(*prev) < self.interval => false,
            _ => {
                self.last.insert(task.to_string(), now);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::ErrorCode;

    #[test]
    fn an_update_only_run_is_never_a_crash() {
        assert!(!ExitInfo {
            update_only: true,
            ..ExitInfo::default()
        }
        .is_crash());
        let json = serde_json::to_value(ExitInfo::default()).unwrap();
        assert_eq!(json["updateOnly"], false);
        assert!(json["manual"].as_array().unwrap().is_empty());
        assert!(json["synced"].is_null());
    }

    #[test]
    fn new_phases_and_the_deep_link_event_serialize() {
        let e = AppEvent::RunState {
            instance: "sky".into(),
            state: RunStateKind::Asking,
            exit: None,
            update: None,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["state"], "asking");
        assert!(json["update"].is_null());
        assert_eq!(
            serde_json::to_value(AppEvent::DeepLink).unwrap(),
            serde_json::json!({ "type": "deep_link" })
        );
    }

    #[test]
    fn throttle_keeps_first_and_last_and_spaces_the_rest() {
        let mut t = ProgressThrottle::new(Duration::from_millis(100));
        let start = Instant::now();
        assert!(t.admit("a", 0, 10, start));
        assert!(!t.admit("a", 1, 10, start + Duration::from_millis(10)));
        assert!(t.admit("a", 2, 10, start + Duration::from_millis(150)));
        assert!(!t.admit("a", 3, 10, start + Duration::from_millis(160)));
        assert!(
            t.admit("a", 10, 10, start + Duration::from_millis(170)),
            "the last update always passes"
        );
        assert!(
            t.admit("b", 1, 10, start + Duration::from_millis(20)),
            "tasks are tracked separately"
        );
    }

    #[test]
    fn events_serialize_with_snake_case_tags() {
        let e = AppEvent::RunState {
            instance: "pack".into(),
            state: RunStateKind::Preparing,
            exit: None,
            update: None,
        };
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "run_state");
        assert_eq!(json["state"], "preparing");
        assert!(json["exit"].is_null());
        let w = AppEvent::Warning {
            instance: None,
            error: launcher_core::LauncherError::disk(ErrorCode::SettingsDamaged, "x"),
        };
        assert_eq!(
            serde_json::to_value(&w).unwrap()["error"]["code"],
            "settings_damaged"
        );
    }

    #[test]
    fn crash_means_an_unrequested_unclean_exit() {
        assert!(ExitInfo {
            code: Some(1),
            ..ExitInfo::default()
        }
        .is_crash());
        assert!(ExitInfo {
            code: None,
            ..ExitInfo::default()
        }
        .is_crash());
        assert!(!ExitInfo {
            code: Some(0),
            ..ExitInfo::default()
        }
        .is_crash());
        assert!(!ExitInfo {
            code: None,
            killed: true,
            ..ExitInfo::default()
        }
        .is_crash());
        assert!(!ExitInfo {
            cancelled: true,
            ..ExitInfo::default()
        }
        .is_crash());
        let failed = ExitInfo {
            error: Some(launcher_core::LauncherError::network(
                ErrorCode::Offline,
                "x",
            )),
            ..ExitInfo::default()
        };
        assert!(
            !failed.is_crash(),
            "a failed prepare is an error, not a crash"
        );
    }
}
