//! Game output reaches the UI in small batches, and the latest run's lines are kept for re-opening.
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{sync::mpsc, task::JoinHandle, time::Instant};

use crate::events::{AppEvent, Host};

/// Lines kept per instance for `log_tail`.
pub const LOG_KEEP: usize = 5_000;
/// Most lines in one `log` event.
pub const BATCH_MAX: usize = 200;
/// How long a batch waits for more lines before it's sent.
pub const BATCH_WINDOW: Duration = Duration::from_millis(50);

/// The latest run's output for each instance.
#[derive(Clone, Default)]
pub struct LogStore(Arc<Mutex<HashMap<String, VecDeque<String>>>>);

impl LogStore {
    pub fn reset(&self, id: &str) {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.to_string(), VecDeque::new());
    }

    pub fn push(&self, id: &str, lines: &[String]) {
        let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let kept = all.entry(id.to_string()).or_default();
        kept.extend(lines.iter().cloned());
        while kept.len() > LOG_KEEP {
            kept.pop_front();
        }
    }

    pub fn tail(&self, id: &str) -> Vec<String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(id)
            .map(|lines| lines.iter().cloned().collect())
            .unwrap_or_default()
    }
}

/// Start the task that groups one run's output lines into `log` events. Send lines through the
/// returned sender; once every sender is dropped the task flushes what's left and ends.
pub fn spawn_batcher<H: Host>(
    instance: String,
    host: Arc<H>,
    store: LogStore,
) -> (mpsc::UnboundedSender<String>, JoinHandle<()>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let task = tokio::spawn(async move {
        while let Some(first) = rx.recv().await {
            let mut lines = vec![first];
            let deadline = Instant::now() + BATCH_WINDOW;
            while lines.len() < BATCH_MAX {
                match tokio::time::timeout_at(deadline, rx.recv()).await {
                    Ok(Some(line)) => lines.push(line),
                    Ok(None) | Err(_) => break,
                }
            }
            store.push(&instance, &lines);
            host.emit(AppEvent::Log {
                instance: instance.clone(),
                lines,
            });
        }
    });
    (tx, task)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AppEvent, Host};
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct Batches(StdMutex<Vec<Vec<String>>>);
    impl Host for Batches {
        fn emit(&self, event: AppEvent) {
            if let AppEvent::Log { lines, .. } = event {
                self.0.lock().unwrap().push(lines);
            }
        }
        fn minimize(&self) {}
        fn restore(&self) {}
    }

    #[tokio::test(start_paused = true)]
    async fn bursts_are_split_into_batches_of_200() {
        let host = Arc::new(Batches::default());
        let store = LogStore::default();
        let (tx, task) = spawn_batcher("pack".into(), host.clone(), store.clone());
        for i in 0..450 {
            tx.send(format!("line {i}")).unwrap();
        }
        drop(tx);
        task.await.unwrap();
        let sizes: Vec<usize> = host.0.lock().unwrap().iter().map(Vec::len).collect();
        assert_eq!(sizes, vec![200, 200, 50]);
        assert_eq!(store.tail("pack").len(), 450);
        assert_eq!(store.tail("pack")[449], "line 449");
    }

    #[tokio::test(start_paused = true)]
    async fn a_lone_line_is_flushed_after_the_window() {
        let host = Arc::new(Batches::default());
        let (tx, _task) = spawn_batcher("pack".into(), host.clone(), LogStore::default());
        tx.send("hello".into()).unwrap();
        tokio::time::sleep(BATCH_WINDOW * 2).await;
        assert_eq!(*host.0.lock().unwrap(), vec![vec!["hello".to_string()]]);
    }

    #[test]
    fn store_keeps_the_last_5000_and_resets() {
        let store = LogStore::default();
        let lines: Vec<String> = (0..(LOG_KEEP + 10)).map(|i| i.to_string()).collect();
        store.push("a", &lines);
        let tail = store.tail("a");
        assert_eq!(tail.len(), LOG_KEEP);
        assert_eq!(tail[0], "10");
        store.reset("a");
        assert!(store.tail("a").is_empty());
        assert!(store.tail("unknown").is_empty());
    }
}
