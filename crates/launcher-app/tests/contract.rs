//! Writes one sample of every type the UI receives into `ui/src/ipc/contract/`, or checks the
//! committed copies are current. The UI's tests parse these with its zod schemas, so a renamed or
//! retyped field fails on one side or the other.
use std::{collections::BTreeMap, path::PathBuf};

use launcher_app::{
    dto::{AppInfo, Build, JavaTest, NewInstance, OpenTarget, RunSnapshot},
    events::{AppEvent, ExitInfo, RunStateKind},
};
use launcher_core::{
    accounts::{offline_account, AccountsFile},
    events::TaskId,
    instances::{
        DamagedInstance, GameSpec, Instance, InstanceListing, JavaSettings, LoaderSpec, SourceRef,
        UpdatePolicy,
    },
    launch::CrashHint,
    meta::{
        catalog::{LoaderVersion, McKind, McVersion},
        loaders::LoaderKind,
    },
    mods::LocalMod,
    settings::{Lang, Settings},
    ErrorCode, LauncherError,
};
use serde::Serialize;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/ipc/contract")
}

fn check(name: &str, value: impl Serialize) {
    let path = dir().join(format!("{name}.json"));
    let fresh = serde_json::to_string_pretty(&value).unwrap() + "\n";
    if std::env::var_os("UPDATE_CONTRACT").is_some() {
        std::fs::create_dir_all(dir()).unwrap();
        std::fs::write(&path, fresh).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        current, fresh,
        "{name}.json is stale: run `UPDATE_CONTRACT=1 cargo test -p launcher-app --test contract` and commit the result"
    );
}

fn fabric_instance() -> Instance {
    Instance {
        id: "asalith-fields".into(),
        name: "Asalith Fields".into(),
        source: SourceRef::None,
        game: GameSpec {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        },
        update_policy: UpdatePolicy::Ask,
        optional_choices: BTreeMap::new(),
        java: JavaSettings {
            path: None,
            memory_mb: 4096,
        },
    }
}

fn vanilla_instance() -> Instance {
    Instance {
        id: "creative-flat".into(),
        name: "Creative Flat".into(),
        source: SourceRef::None,
        game: GameSpec {
            minecraft: "26.3".into(),
            loader: None,
        },
        update_policy: UpdatePolicy::Ask,
        optional_choices: BTreeMap::new(),
        java: JavaSettings {
            path: Some(PathBuf::from("C:/Java/bin/java.exe")),
            memory_mb: 6144,
        },
    }
}

fn sample_error() -> LauncherError {
    LauncherError::loader(
        ErrorCode::MinecraftVersionNotFound,
        "Minecraft 9.9 doesn't exist",
    )
    .with_param("version", "9.9")
    .with_fix("Check the version number")
    .with_detail("manifest has no 9.9")
}

#[test]
fn contract_fixtures_are_current() {
    check("instance", fabric_instance());
    check(
        "instance_listing",
        InstanceListing {
            instances: vec![fabric_instance(), vanilla_instance()],
            damaged: vec![DamagedInstance {
                id: "old-pack".into(),
                error: LauncherError::disk(
                    ErrorCode::InstanceDamaged,
                    "The settings file of \"old-pack\" is damaged",
                )
                .with_param("id", "old-pack")
                .with_fix("Restore instance.json from a backup or recreate the instance")
                .with_detail("expected value at line 1 column 1"),
            }],
        },
    );
    let steve = offline_account("Steve").unwrap();
    let alex = offline_account("Alex_Builds").unwrap();
    check(
        "accounts",
        AccountsFile {
            active: Some(steve.uuid.clone()),
            accounts: vec![steve, alex],
            licensed_signin_seen: false,
        },
    );
    check(
        "settings",
        Settings {
            language: Some(Lang::Es),
            ..Settings::default()
        },
    );
    check(
        "app_info",
        AppInfo {
            version: "0.1.0".into(),
            build: Build::Dev,
            offline_allowed: true,
            system_memory_mb: 16384,
            data_dir: "C:/Users/player/AppData/Roaming/PerlandLauncher".into(),
        },
    );
    check(
        "mc_versions",
        vec![
            McVersion {
                id: "26.4-snapshot-3".into(),
                kind: McKind::Snapshot,
                release_time: "2026-10-01T09:00:00+00:00".into(),
            },
            McVersion {
                id: "26.3".into(),
                kind: McKind::Release,
                release_time: "2026-09-15T09:00:00+00:00".into(),
            },
            McVersion {
                id: "1.21.1".into(),
                kind: McKind::Release,
                release_time: "2024-08-08T12:00:00+00:00".into(),
            },
            McVersion {
                id: "1.20.1".into(),
                kind: McKind::Release,
                release_time: "2023-06-12T13:25:51+00:00".into(),
            },
            McVersion {
                id: "b1.7.3".into(),
                kind: McKind::OldBeta,
                release_time: "2011-07-07T22:00:00+00:00".into(),
            },
        ],
    );
    check(
        "loader_versions",
        vec![
            LoaderVersion {
                version: "0.19.5".into(),
                stable: true,
            },
            LoaderVersion {
                version: "0.19.4".into(),
                stable: true,
            },
            LoaderVersion {
                version: "0.20.0-beta.1".into(),
                stable: false,
            },
        ],
    );
    check(
        "local_mods",
        vec![
            LocalMod {
                file_name: "fabric-api-0.92.2+1.20.1.jar".into(),
                enabled: true,
                size: 2_310_144,
            },
            LocalMod {
                file_name: "sodium-fabric-0.5.8+mc1.20.1.jar".into(),
                enabled: true,
                size: 1_048_576,
            },
            LocalMod {
                file_name: "zoomify-2.11.2.jar.disabled".into(),
                enabled: false,
                size: 524_288,
            },
        ],
    );
    check(
        "events",
        vec![
            AppEvent::Progress {
                instance: Some("asalith-fields".into()),
                task: TaskId::new("install_minecraft").with("version", "1.20.1"),
                done: 1204,
                total: 3575,
                bytes: 312_000_000,
            },
            AppEvent::Progress {
                instance: None,
                task: TaskId::new("download_java").with("version", "17.0.15"),
                done: 0,
                total: 1,
                bytes: 0,
            },
            AppEvent::Log {
                instance: "asalith-fields".into(),
                lines: vec![
                    "[12:00:01] [Render thread/INFO]: Setting user: Steve".into(),
                    "[12:00:02] [Render thread/INFO]: Backend library: LWJGL version 3.3.1".into(),
                ],
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Queued,
                exit: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    code: Some(1),
                    crash: Some(CrashHint::OutOfMemory),
                    ..ExitInfo::default()
                }),
            },
            AppEvent::RunState {
                instance: "creative-flat".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    error: Some(sample_error()),
                    ..ExitInfo::default()
                }),
            },
            AppEvent::RunState {
                instance: "creative-flat".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    cancelled: true,
                    ..ExitInfo::default()
                }),
            },
            AppEvent::Warning {
                instance: Some("asalith-fields".into()),
                error: LauncherError::disk(
                    ErrorCode::LogWriteStopped,
                    "The game log stopped being saved to disk",
                )
                .with_fix("Check free disk space. The game keeps running."),
            },
        ],
    );
    check("error", sample_error());
    check("error_codes", ErrorCode::ALL);
    check(
        "new_instance",
        NewInstance {
            name: "Asalith Fields".into(),
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.19.5".into(),
            }),
        },
    );
    check(
        "open_targets",
        vec![
            OpenTarget::Instance {
                id: "asalith-fields".into(),
            },
            OpenTarget::Mods {
                id: "asalith-fields".into(),
            },
            OpenTarget::Data,
            OpenTarget::Logs,
        ],
    );
    check(
        "run_snapshots",
        vec![RunSnapshot {
            instance: "asalith-fields".into(),
            state: RunStateKind::Running,
        }],
    );
    check("java_test", JavaTest { major: 17 });
}
