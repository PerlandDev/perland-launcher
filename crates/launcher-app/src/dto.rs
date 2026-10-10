//! Shapes the commands take and return that aren't core types.
use launcher_core::{accounts::BuildKind, instances::LoaderSpec};
use serde::{Deserialize, Serialize};

use crate::events::RunStateKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Build {
    Dev,
    Release,
}

impl From<BuildKind> for Build {
    fn from(kind: BuildKind) -> Self {
        match kind {
            BuildKind::Dev => Build::Dev,
            BuildKind::Release => Build::Release,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub build: Build,
    pub offline_allowed: bool,
    pub system_memory_mb: u64,
    pub data_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
}

/// Folders the UI may ask to open. Never a raw path, so the webview can't open arbitrary places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenTarget {
    Instance { id: String },
    Mods { id: String },
    Data,
    Logs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunSnapshot {
    pub instance: String,
    pub state: RunStateKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaTest {
    pub major: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_target_round_trips_tagged() {
        let t: OpenTarget = serde_json::from_str(r#"{"kind":"mods","id":"pack"}"#).unwrap();
        assert_eq!(t, OpenTarget::Mods { id: "pack".into() });
        let d: OpenTarget = serde_json::from_str(r#"{"kind":"data"}"#).unwrap();
        assert_eq!(d, OpenTarget::Data);
    }

    #[test]
    fn app_info_is_camel_case() {
        let info = AppInfo {
            version: "0.1.0".into(),
            build: Build::Dev,
            offline_allowed: true,
            system_memory_mb: 16384,
            data_dir: "C:/x".into(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["offlineAllowed"], true);
        assert_eq!(json["build"], "dev");
        assert_eq!(json["systemMemoryMb"], 16384);
    }
}
