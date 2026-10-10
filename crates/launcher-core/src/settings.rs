//! Launcher-wide preferences in `settings.json`.
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    instances::validate_memory,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Es,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub version: u32,
    /// `None` until the player picks one; the UI then follows the OS language.
    pub language: Option<Lang>,
    pub default_memory_mb: u32,
    pub minimize_on_play: bool,
    pub first_run_done: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            language: None,
            default_memory_mb: 4096,
            minimize_on_play: true,
            first_run_done: false,
        }
    }
}

pub struct SettingsStore {
    path: PathBuf,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Never fails: a missing file means defaults, a damaged one means defaults plus a warning.
    pub async fn load(&self) -> (Settings, Option<LauncherError>) {
        let bytes = match tokio::fs::read(&self.path).await {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return (Settings::default(), None)
            }
            Err(e) => return (Settings::default(), Some(damaged(e))),
        };
        match serde_json::from_slice(&bytes) {
            Ok(settings) => (settings, None),
            Err(e) => (Settings::default(), Some(damaged(e))),
        }
    }

    pub async fn save(&self, settings: &Settings) -> Result<()> {
        validate_memory(settings.default_memory_mb)?;
        let bytes = serde_json::to_vec_pretty(settings).map_err(|e| {
            LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save settings")
                .with_param("item", "settings")
                .with_detail(e)
        })?;
        write_atomic(&self.path, &bytes).await
    }
}

fn damaged(e: impl std::fmt::Display) -> LauncherError {
    LauncherError::disk(
        ErrorCode::SettingsDamaged,
        "Your launcher settings couldn't be read, so the defaults are in use",
    )
    .with_fix("Set your preferences again in Settings")
    .with_detail(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ErrorCode;

    #[tokio::test]
    async fn missing_file_gives_defaults_without_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        let (settings, warning) = store.load().await;
        assert_eq!(settings, Settings::default());
        assert!(warning.is_none());
        assert_eq!(settings.default_memory_mb, 4096);
        assert!(settings.minimize_on_play);
        assert!(!settings.first_run_done);
    }

    #[tokio::test]
    async fn round_trip_in_camel_case() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        let settings = Settings {
            language: Some(Lang::Es),
            first_run_done: true,
            ..Settings::default()
        };
        store.save(&settings).await.unwrap();
        let raw = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
        assert!(raw.contains("\"language\": \"es\""), "{raw}");
        assert!(raw.contains("\"firstRunDone\": true"), "{raw}");
        assert_eq!(store.load().await, (settings, None));
    }

    #[tokio::test]
    async fn settings_unknown_language_falls_back() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"fr","futureField":1}"#).unwrap();
        let (settings, warning) = SettingsStore::new(&path).load().await;
        assert_eq!(settings, Settings::default());
        assert_eq!(warning.unwrap().code, ErrorCode::SettingsDamaged);
    }

    #[tokio::test]
    async fn unknown_fields_alone_are_fine() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("settings.json");
        std::fs::write(&path, r#"{"language":"en","futureField":1}"#).unwrap();
        let (settings, warning) = SettingsStore::new(&path).load().await;
        assert_eq!(settings.language, Some(Lang::En));
        assert!(warning.is_none());
    }

    #[tokio::test]
    async fn save_validates_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(tmp.path().join("settings.json"));
        let bad = Settings {
            default_memory_mb: 64,
            ..Settings::default()
        };
        assert_eq!(
            store.save(&bad).await.unwrap_err().code,
            ErrorCode::MemoryOutOfRange
        );
    }
}
