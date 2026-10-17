//! Time played per instance, kept in `<instance>/.perland/playtime.json` so it moves with a
//! rename and goes with a delete. Counted on this computer only, plus what another launcher had
//! counted for an imported instance.
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{error::Result, fsutil::write_atomic};

const FILE: &str = "playtime.json";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Playtime {
    /// Whole seconds the game was running, summed over every run.
    pub seconds: u64,
    /// Seconds since the Unix epoch when the last run ended (0: never played).
    pub last_played: u64,
    /// The other launcher's game folder whose time played started this count, when the
    /// instance was imported. An import of the same folder later doesn't count it again.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub imported_from: Option<PathBuf>,
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
        imported_from: old.imported_from,
    };
    write(perland_dir, &new).await?;
    Ok(new)
}

/// Start a new instance's count with the `seconds` another launcher counted in the game folder
/// `from`, last played at `last_played` (Unix seconds, 0: unknown). Only when the instance has
/// no record yet: false, and nothing written, when it has one.
pub async fn import(
    perland_dir: &Path,
    seconds: u64,
    last_played: u64,
    from: &Path,
) -> Result<bool> {
    if tokio::fs::try_exists(path(perland_dir)).await? {
        return Ok(false);
    }
    let new = Playtime {
        seconds,
        last_played,
        imported_from: Some(from.to_path_buf()),
    };
    write(perland_dir, &new).await?;
    Ok(true)
}

async fn write(perland_dir: &Path, played: &Playtime) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(played).map_err(std::io::Error::other)?;
    write_atomic(&path(perland_dir), &bytes).await?;
    Ok(())
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
            imported_from: None,
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

    #[tokio::test]
    async fn imported_time_starts_the_count_once() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join(".perland");
        let from = Path::new("/prism/instances/Pack/.minecraft");
        assert!(import(&dir, 3600, 1_700_000_000, from).await.unwrap());
        assert!(
            !import(&dir, 3600, 1_700_000_000, from).await.unwrap(),
            "never twice"
        );
        let played = add(&dir, 60, 1_800_000_000).await.unwrap();
        assert_eq!(played.seconds, 3660);
        assert_eq!(played.last_played, 1_800_000_000);
        assert_eq!(played.imported_from.as_deref(), Some(from));
        assert_eq!(read(&dir).await, played);
    }

    #[tokio::test]
    async fn records_without_an_import_read_as_before() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join(FILE), br#"{"seconds": 5, "lastPlayed": 1}"#).unwrap();
        assert_eq!(read(tmp.path()).await.imported_from, None);
        add(tmp.path(), 1, 2).await.unwrap();
        let raw = std::fs::read_to_string(tmp.path().join(FILE)).unwrap();
        assert!(!raw.contains("importedFrom"), "{raw}");
    }
}
