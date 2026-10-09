use std::sync::Arc;

use serde::Serialize;

/// Everything the core reports while it works. The UI only renders these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Progress {
        task: String,
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
}

pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

pub fn noop_sink() -> EventSink {
    Arc::new(|_| {})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn sink_receives_events_and_serializes_tagged() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let s2 = seen.clone();
        let sink: EventSink = Arc::new(move |e| s2.lock().unwrap().push(e));
        sink(Event::Progress {
            task: "t".into(),
            done: 1,
            total: 2,
            bytes: 3,
        });
        assert_eq!(seen.lock().unwrap().len(), 1);
        let json = serde_json::to_string(&seen.lock().unwrap()[0]).unwrap();
        assert!(json.starts_with("{\"type\":\"progress\""));
        noop_sink()(Event::Log { line: "x".into() });
    }
}
