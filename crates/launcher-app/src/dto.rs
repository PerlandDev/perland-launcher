//! Shapes the commands take and return that aren't core types.
use std::path::PathBuf;

use launcher_core::{
    accounts::BuildKind,
    browse::{
        compat::ContentKind,
        plan::{ItemReason, PlanItem, PlanSkip},
        search::SortIndex,
    },
    instances::{Instance, LoaderSpec},
    meta::loaders::LoaderKind,
    mods::LocalMod,
    sync::{ManualFile, PendingUpdate, SyncReport},
    LauncherError,
};
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
    Downloads,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunSnapshot {
    pub instance: String,
    pub state: RunStateKind,
    pub update: Option<PackUpdate>,
}

/// An update the player can take, as the "Update ready" dialog and banner show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdate {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    /// Files the player edited that the update keeps as they are ("Kept your version of N files").
    pub kept: Vec<String>,
}

impl From<&PendingUpdate> for PackUpdate {
    fn from(u: &PendingUpdate) -> Self {
        Self {
            from_version: u.changes.from_version.clone(),
            to_version: u.changes.to_version.clone(),
            added: u.changes.added.clone(),
            updated: u.changes.updated.clone(),
            removed: u.changes.removed.clone(),
            kept: u.kept.clone(),
        }
    }
}

/// What a finished sync changed: "Updated to 1.0.4", "Kept your version of N files".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub kept_modified: Vec<String>,
}

impl From<&SyncReport> for SyncSummary {
    fn from(r: &SyncReport) -> Self {
        Self {
            from_version: r.changes.from_version.clone(),
            to_version: r.changes.to_version.clone(),
            added: r.changes.added.len(),
            updated: r.changes.updated.len(),
            removed: r.changes.removed.len(),
            kept_modified: r.kept_modified.clone(),
        }
    }
}

/// For the banner: the installed version, and the update (or why there's no answer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    pub installed_version: Option<String>,
    pub update: Option<PackUpdate>,
    pub check_error: Option<LauncherError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalChoice {
    pub enabled: bool,
    pub description: Option<String>,
}

/// A mod that comes with the pack. Read-only, except an optional one's toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMod {
    pub path: String,
    pub name: String,
    pub optional: Option<OptionalChoice>,
}

/// A pack instance's Mods screen: the pack's mods, and the player's own jars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackContents {
    pub managed: Vec<PackMod>,
    pub additions: Vec<LocalMod>,
}

/// "Download these manually": how many of the blocked files were found so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualProgress {
    pub found: usize,
    pub total: usize,
    pub remaining: Vec<ManualFile>,
}

/// Pack art as `data:` URLs (the CSP allows no remote images). `None` falls back to the
/// generated art.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackArtData {
    pub icon: Option<String>,
    pub banner: Option<String>,
}

/// What the New-instance screen hands over. A link is never a file path, and a file is only
/// ever a `.mrpack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackAddRequest {
    Link { url: String },
    File { path: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaTest {
    pub major: u32,
}

/// Browse's search box and filters. With `instance`, its game decides `minecraft` and `loader`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub kind: ContentKind,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub minecraft: Option<String>,
    #[serde(default)]
    pub loader: Option<LoaderKind>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub sort: SortIndex,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanTargetRequest {
    Instance { id: String },
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePlanRequest {
    pub project: String,
    #[serde(default)]
    pub version: Option<String>,
    pub target: PlanTargetRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanTargetView {
    Instance {
        id: String,
        name: String,
        minecraft: String,
        loader: Option<LoaderKind>,
    },
    New {
        name: String,
        minecraft: String,
        loader: Option<LoaderKind>,
    },
}

/// One thing an install adds, as the confirm dialog shows it. URLs and hashes stay in the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemView {
    pub project_id: String,
    pub title: String,
    pub kind: ContentKind,
    pub version_number: String,
    pub file_name: String,
    pub size: u64,
    pub reason: ItemReason,
}

impl From<&PlanItem> for PlanItemView {
    fn from(item: &PlanItem) -> Self {
        Self {
            project_id: item.project_id.clone(),
            title: item.title.clone(),
            kind: item.kind,
            version_number: item.version_number.clone(),
            file_name: item.file.file_name.clone(),
            size: item.file.size,
            reason: item.reason.clone(),
        }
    }
}

/// A stored install plan; the webview installs it by `plan_id` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlanView {
    pub plan_id: String,
    pub target: PlanTargetView,
    pub items: Vec<PlanItemView>,
    pub skipped: Vec<PlanSkip>,
    pub total_size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    /// The instance the files went into (a new one for a "New instance" plan).
    pub instance: Instance,
    pub added: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    use launcher_core::{
        instances::SourceRef,
        sources::{DesiredState, PackAbout},
        sync::{plan::Changes, PendingUpdate, SyncReport},
    };

    fn changes() -> Changes {
        Changes {
            from_version: Some("1.0.3".into()),
            to_version: "1.0.4".into(),
            added: vec!["JEI".into(), "Create".into()],
            updated: vec!["Sodium".into()],
            removed: vec!["Zoomify".into()],
        }
    }

    #[test]
    fn an_update_and_a_summary_keep_names_and_counts() {
        let pending = PendingUpdate {
            source: SourceRef::Packwiz {
                url: "https://example.com/pack.toml".into(),
            },
            desired: DesiredState {
                name: "Sky".into(),
                pack_version: "1.0.4".into(),
                minecraft: "1.20.1".into(),
                loader: None,
                files: vec![],
                stamp: "s2".into(),
                about: PackAbout::default(),
                archive: None,
            },
            changes: changes(),
            kept: vec!["config/a.toml".into()],
        };
        let update = PackUpdate::from(&pending);
        assert_eq!(update.added, vec!["JEI".to_string(), "Create".to_string()]);
        assert_eq!(update.kept, vec!["config/a.toml".to_string()]);
        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["fromVersion"], "1.0.3");
        assert_eq!(json["toVersion"], "1.0.4");

        let summary = SyncSummary::from(&SyncReport {
            up_to_date: false,
            changes: changes(),
            kept_modified: vec!["options.txt".into()],
            game_changed: false,
        });
        assert_eq!((summary.added, summary.updated, summary.removed), (2, 1, 1));
        assert_eq!(
            serde_json::to_value(&summary).unwrap()["keptModified"][0],
            "options.txt"
        );
    }

    #[test]
    fn pack_add_requests_are_tagged_by_kind() {
        let link: PackAddRequest =
            serde_json::from_str(r#"{"kind":"link","url":"https://x.example/pack.toml"}"#).unwrap();
        assert_eq!(
            link,
            PackAddRequest::Link {
                url: "https://x.example/pack.toml".into()
            }
        );
        let file: PackAddRequest =
            serde_json::from_str(r#"{"kind":"file","path":"C:/Downloads/Sky.mrpack"}"#).unwrap();
        assert_eq!(
            file,
            PackAddRequest::File {
                path: "C:/Downloads/Sky.mrpack".into()
            }
        );
        let downloads: OpenTarget = serde_json::from_str(r#"{"kind":"downloads"}"#).unwrap();
        assert_eq!(downloads, OpenTarget::Downloads);
    }

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

    #[test]
    fn browse_requests_read_their_wire_shapes() {
        let search: SearchRequest = serde_json::from_str(
            r#"{"query":"sodium","kind":"resource_pack","instance":null,"minecraft":"1.20.1","loader":"fabric","categories":["adventure"],"sort":"downloads","offset":20}"#,
        )
        .unwrap();
        assert_eq!(search.kind, ContentKind::ResourcePack);
        assert_eq!(search.sort, SortIndex::Downloads);
        let plan: BrowsePlanRequest = serde_json::from_str(
            r#"{"project":"sodium","version":null,"target":{"kind":"instance","id":"asalith"}}"#,
        )
        .unwrap();
        assert_eq!(
            plan.target,
            PlanTargetRequest::Instance {
                id: "asalith".into()
            }
        );
        let new: BrowsePlanRequest =
            serde_json::from_str(r#"{"project":"x","version":"v1","target":{"kind":"new"}}"#)
                .unwrap();
        assert_eq!(new.target, PlanTargetRequest::New);
    }

    #[test]
    fn plan_views_hide_urls_and_hashes() {
        let plan = crate::testing::sample_plan(false);
        let view = PlanItemView::from(&plan.items[1]);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["reason"]["kind"], "dependency");
        assert_eq!(json["reason"]["of"], "Sodium");
        assert_eq!(json["fileName"], "fabric-api.jar");
        assert!(json.get("url").is_none() && json.get("hashes").is_none());
    }
}
