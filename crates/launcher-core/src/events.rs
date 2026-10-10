use std::{collections::BTreeMap, fmt::Display, sync::Arc};

use serde::Serialize;

use crate::LauncherError;

/// A progress task the UI can translate: a stable key plus the values its label interpolates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TaskId {
    pub key: String,
    pub params: BTreeMap<String, String>,
}

impl TaskId {
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            params: BTreeMap::new(),
        }
    }
    pub fn with(mut self, key: &str, value: impl Display) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }
}

/// Everything the core reports while it works. The UI only renders these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Progress {
        task: TaskId,
        done: u64,
        total: u64,
        bytes: u64,
    },
    Log {
        line: String,
    },
    StateChanged {
        instance: String,
        state: String,
    },
    /// Something went wrong that doesn't stop the current job (e.g. the game log stopped saving).
    Warning(LauncherError),
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub fn noop_sink() -> EventSink {
    Arc::new(|_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ErrorCode, LauncherError};
    use std::sync::Mutex;

    #[test]
    fn sink_receives_events_and_serializes_tagged() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let sink: EventSink = Arc::new(move |e| s2.lock().unwrap().push(e));
        sink(Event::Progress {
            task: TaskId::new("install_minecraft").with("version", "1.20.1"),
            done: 1,
            total: 2,
            bytes: 3,
        });
        assert_eq!(seen.lock().unwrap().len(), 1);
        let json = serde_json::to_value(&seen.lock().unwrap()[0]).unwrap();
        assert_eq!(json["type"], "progress");
        assert_eq!(json["task"]["key"], "install_minecraft");
        assert_eq!(json["task"]["params"]["version"], "1.20.1");
        noop_sink()(Event::Log { line: "x".into() });
    }

    #[test]
    fn warning_carries_a_launcher_error() {
        let e = Event::Warning(LauncherError::disk(ErrorCode::LogWriteStopped, "stopped"));
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "warning");
        assert_eq!(json["code"], "log_write_stopped");
    }
}
