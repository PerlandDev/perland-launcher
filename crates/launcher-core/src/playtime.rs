//! Time played per instance, kept in `<instance>/.perland/playtime.json` so it moves with a
//! rename and goes with a delete. Counted on this computer only.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{error::Result, fsutil::write_atomic};

const FILE: &str = "playtime.json";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playtime {
    /// Whole seconds the game was running, summed over every run.
    pub seconds: u64,
    /// Seconds since the Unix epoch when the last run ended (0: never played).
    pub last_played: u64,
}

fn path(perland_dir: &Path) -> PathBuf {
    perland_dir.join(FILE)
}

/// The stored playtime. A missing, unreadable or damaged file reads as zero: losing the count
/// is never worth an error.
pub async fn read(perland_dir: &Path) -> Playtime {
    match tokio::fs::read(path(perland_dir)).await {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Playtime::default(),
    }
}

/// Add one run's `seconds`, ended at `now` (Unix seconds), and return the new total.
pub async fn add(perland_dir: &Path, seconds: u64, now: u64) -> Result<Playtime> {
    let old = read(perland_dir).await;
    let new = Playtime {
        seconds: old.seconds.saturating_add(seconds),
        last_played: now,
    };
    let bytes = serde_json::to_vec_pretty(&new).map_err(std::io::Error::other)?;
    write_atomic(&path(perland_dir), &bytes).await?;
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn missing_file_reads_zero() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read(tmp.path()).await, Playtime::default());
    }

    #[tokio::test]
    async fn add_accumulates() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".perland");
        add(&dir, 60, 100).await.unwrap();
        let total = add(&dir, 30, 200).await.unwrap();
        let expected = Playtime {
            seconds: 90,
            last_played: 200,
        };
        assert_eq!(total, expected);
        assert_eq!(read(&dir).await, expected);
        let raw = std::fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(raw.contains("\"lastPlayed\": 200"), "{raw}");
    }

    #[tokio::test]
    async fn damaged_file_restarts_from_zero() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE), b"nope").unwrap();
        assert_eq!(read(tmp.path()).await, Playtime::default());
        assert_eq!(add(tmp.path(), 5, 1).await.unwrap().seconds, 5);
    }
}
