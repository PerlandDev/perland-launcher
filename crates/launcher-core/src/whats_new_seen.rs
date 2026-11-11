//! The last launcher version whose release notes this player was shown, kept in
//! `<data root>/whats-new-seen.json` so What's new opens once per update
//! (docs/superpowers/specs/2026-11-10-whats-new-design.md).
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{error::Result, fsutil::write_atomic};

/// The file's name under the data root.
pub const WHATS_NEW_SEEN_FILE: &str = "whats-new-seen.json";

#[derive(Serialize, Deserialize)]
struct Marker {
    version: String,
}

pub struct WhatsNewSeen {
    path: PathBuf,
    lock: Mutex<()>,
}

impl WhatsNewSeen {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// The version stored. A missing, damaged or unreadable file reads as none: the worst case
    /// is that the player sees the newest notes once more.
    pub async fn get(&self) -> Option<String> {
        let _guard = self.lock.lock().await;
        let bytes = tokio::fs::read(&self.path).await.ok()?;
        let marker: Marker = serde_json::from_slice(&bytes).ok()?;
        Some(marker.version).filter(|v| !v.is_empty())
    }

    /// Remember `version`, replacing whatever the file held.
    pub async fn set(&self, version: &str) -> Result<()> {
        let _guard = self.lock.lock().await;
        let marker = Marker {
            version: version.to_string(),
        };
        let bytes = serde_json::to_vec_pretty(&marker).map_err(std::io::Error::other)?;
        write_atomic(&self.path, &bytes).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &tempfile::TempDir) -> WhatsNewSeen {
        WhatsNewSeen::new(dir.path().join(WHATS_NEW_SEEN_FILE))
    }

    #[tokio::test]
    async fn a_missing_file_is_no_marker() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(store(&dir).get().await, None);
    }

    #[tokio::test]
    async fn set_then_get_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let seen = store(&dir);
        seen.set("2026.11.5-beta.2").await.unwrap();
        assert_eq!(seen.get().await.as_deref(), Some("2026.11.5-beta.2"));
        let raw = std::fs::read_to_string(dir.path().join(WHATS_NEW_SEEN_FILE)).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&raw).unwrap(),
            serde_json::json!({ "version": "2026.11.5-beta.2" })
        );
    }

    #[tokio::test]
    async fn a_damaged_file_is_no_marker_and_set_repairs_it() {
        let dir = tempfile::tempdir().unwrap();
        let seen = store(&dir);
        for damaged in ["{nope", "", r#"{"version":7}"#, r#"{"version":""}"#, "[]"] {
            std::fs::write(dir.path().join(WHATS_NEW_SEEN_FILE), damaged).unwrap();
            assert_eq!(seen.get().await, None, "{damaged:?}");
        }
        seen.set("2026.11.5").await.unwrap();
        assert_eq!(seen.get().await.as_deref(), Some("2026.11.5"));
    }
}
