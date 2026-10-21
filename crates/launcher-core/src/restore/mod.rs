//! One restore point per instance, kept under `<instance>/.perland/restore/`, so the player can
//! go back to how the instance was before the last update. Mod, pack and shader archives are
//! hard-linked into the point, everything else is copied (see `snapshot`). Taking, extending,
//! sealing and pruning the point is in `point`.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::instances::{GameSpec, SourceRef};

// Used by `point` in the next commit.
#[allow(dead_code)]
pub mod snapshot;

/// A point older than this (30 days) is deleted when it is read or a new one is made.
pub const MAX_AGE_SECS: u64 = 30 * 24 * 60 * 60;
/// The most a point may copy (1 GiB). Past it the change goes ahead without a point.
pub const COPY_CAP: u64 = 1 << 30;

/// Top-level folders that are never snapshotted. Any folder starting with `.` is out too.
const DENYLIST: [&str; 5] = [
    "saves",
    "screenshots",
    "logs",
    "crash-reports",
    "mods-before-link",
];

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum PointKind {
    BeforeChange,
    BeforeGoingBack,
}

/// One thing that happened after the point was taken, for display.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Change {
    PackUpdate { from: String, to: String },
    Installed { name: String },
    Removed { name: String },
}

/// A file in the point. `path` is relative to the game folder, with `/` separators.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SnapFile {
    pub path: String,
    pub size: u64,
    /// Unix seconds.
    pub mtime: u64,
    /// Hard-linked to the game's file rather than copied.
    pub linked: bool,
}

/// `point/point.json`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PointMeta {
    pub version: u32,
    pub created_at: u64,
    pub kind: PointKind,
    /// Top-level folders held in `point/files/`, sorted.
    pub scope: Vec<String>,
    pub game: GameSpec,
    pub source: SourceRef,
    pub optional_choices: BTreeMap<String, bool>,
    pub pack_version: Option<String>,
    pub changes: Vec<Change>,
    /// Set when the game launches after the point was taken.
    pub sealed: bool,
    pub last_change_at: u64,
    /// A run that started after `last_change_at` ended without a crash.
    pub clean_run_since: bool,
    pub files: Vec<SnapFile>,
}

/// Whether the top-level folder `top` may be part of a point.
pub fn in_scope(top: &str) -> bool {
    !top.is_empty() && !top.starts_with('.') && !DENYLIST.contains(&top)
}

/// The top-level folder of `rel` (a path relative to the game folder), or `None` for a file
/// in the root, which is never in a point.
pub fn top_of(rel: &str) -> Option<&str> {
    rel.split_once('/').map(|(top, _)| top)
}

#[allow(dead_code)]
pub(crate) fn restore_dir(perland: &Path) -> PathBuf {
    perland.join("restore")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_scope_refuses_the_denylist() {
        for top in [
            "saves",
            "screenshots",
            "logs",
            "crash-reports",
            "mods-before-link",
            ".fabric",
        ] {
            assert!(!in_scope(top), "{top}");
        }
        for top in ["mods", "config", "kubejs"] {
            assert!(in_scope(top), "{top}");
        }
    }

    #[test]
    fn top_of_root_file_is_none() {
        assert_eq!(top_of("options.txt"), None);
        assert_eq!(top_of("config/a/b.toml"), Some("config"));
        assert_eq!(top_of("mods/x.jar"), Some("mods"));
    }
}
