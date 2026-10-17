//! Writes one sample of every type the UI receives into `ui/src/ipc/contract/`, or checks the
//! committed copies are current. The UI's tests parse these with its zod schemas, so a renamed or
//! retyped field fails on one side or the other.
use std::{collections::BTreeMap, path::PathBuf};

use launcher_app::dto::{
    BrowsePlanRequest, InstallPlanView, InstallResult, PlanItemView, PlanTargetRequest,
    PlanTargetView, SearchRequest,
};
use launcher_app::{
    deep_link::DeepLink,
    dto::{
        AppInfo, ArtChoice, Build, ContentFile, EmailCodeSent, FreeServerCard, FreeServerPage,
        IconChoice, ImportPreview, ImportRequest, InstanceArtData, JavaFound, JavaOverview,
        JoinPreview, LinkCodeView, ManualProgress, NameListView, NewInstance, NewsChangesView,
        NewsPostView, OpenTarget, OptionalChoice, PackAddRequest, PackContents, PackMod,
        PackStatus, PackUpdate, PerlandAccount, PerlandSignInPhase, PerlandStats, PerlandStatus,
        PlaytimeEntry, PublicServerCard, PublicServerPage, RunSnapshot, ServerGame, ServerNews,
        ServerPack, ServerPlayers, ServerStatsView, ServerStatusView, ServerTeam, SyncSummary,
        YourServerCard, YourServersPage,
    },
    events::{AppEvent, ExitInfo, RunStateKind},
    updates::UpdateState,
};
use launcher_core::browse::{
    compat::ContentKind,
    plan::{ItemReason, PlanSkip, SkipReason},
    project::{GalleryImage, ProjectDetail, SideSupport, VersionInfo},
    search::{SearchHit, SearchPage, SortIndex},
};
use launcher_core::{
    accounts::{offline_account, AccountsFile},
    download::Hashes,
    events::TaskId,
    instances::{
        ArtMode, DamagedInstance, Focus, GameSpec, IconMode, Instance, InstanceListing,
        JavaSettings, LoaderSpec, ServerAddress, ServerLink, ServerStatus, SourceRef, UpdatePolicy,
    },
    invite::invite_link_cases,
    java::JavaFit,
    launch::CrashHint,
    meta::{
        catalog::{LoaderVersion, McKind, McVersion},
        loaders::LoaderKind,
    },
    mods::LocalMod,
    perland::{CreateAllowance, Deletion, Disk, FreeLimits, MemoryLimits},
    public_list::{Facets, ListQuery, ListSort},
    settings::{IconShape, Lang, Settings, Theme, UiFont},
    sync::ManualFile,
    ErrorCode, LauncherError,
};
use serde::Serialize;

fn sample_update() -> PackUpdate {
    PackUpdate {
        from_version: Some("1.0.3".into()),
        to_version: "1.0.4".into(),
        added: vec![
            "Just Enough Items".into(),
            "Create".into(),
            "Farmer's Delight".into(),
        ],
        updated: vec!["Sodium".into(), "Iris".into()],
        removed: vec!["Zoomify".into()],
        kept: vec!["config/sodium-options.json".into()],
    }
}

fn sample_manual() -> ManualFile {
    ManualFile {
        path: "mods/optifine.jar".into(),
        name: "OptiFine".into(),
        file_name: "OptiFine_1.20.1_HD_U_I6.jar".into(),
        page_url: "https://www.curseforge.com/projects/235216".into(),
        hashes: Hashes::sha1("3f786850e387550fdab836ed7e6dc881de23001b"),
    }
}

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
            args: String::new(),
        },
        server: None,
        art_seed: None,
        art: ArtMode::Auto,
        icon: None,
        banner_focus: Some(Focus { x: 30, y: 60 }),
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
            args: "-XX:+UseG1GC -Dperland.note=\"a b\"".into(),
        },
        server: None,
        art_seed: Some("creative-flat".into()),
        art: ArtMode::Generated,
        icon: Some(IconMode::Generated),
        banner_focus: None,
    }
}

/// A fake invite (43 × A): fixtures never hold a real token.
fn sample_invite() -> String {
    format!("https://play.example.com/{}", "A".repeat(43))
}

fn sample_address() -> ServerAddress {
    ServerAddress {
        host: "play.example.com".into(),
        port: 25566,
    }
}

/// One owner server that is offline, one invited, one followed and already joined.
fn your_servers_page() -> YourServersPage {
    let invite = |c: char| format!("https://play.example.com/{}", c.to_string().repeat(43));
    let card =
        |c: char, name: &str, status: ServerStatus, instance: Option<&str>| PublicServerCard {
            token: c.to_string().repeat(43),
            invite: Some(invite(c)),
            joinable: true,
            address: Some("play.example.com:25566".into()),
            name: name.into(),
            icon_version: None,
            game: ServerGame {
                version: Some("1.21.1".into()),
                loader: None,
                loader_version: None,
            },
            modded: false,
            status,
            players: None,
            whitelisted: false,
            pack: None,
            categories: vec![],
            language: "en".into(),
            region: None,
            instance: instance.map(Into::into),
        };
    YourServersPage {
        servers: vec![
            YourServerCard {
                card: card('A', "My world", ServerStatus::Offline, None),
                relation: "owner".into(),
                following: false,
            },
            YourServerCard {
                card: card('B', "Friends", ServerStatus::Online, None),
                relation: "invited".into(),
                following: true,
            },
            YourServerCard {
                card: card('C', "Survival", ServerStatus::Sleeping, Some("survival")),
                relation: "followed".into(),
                following: true,
            },
        ],
    }
}

/// Signed in with a modpack server that is starting and was warned about its deletion, and
/// allowed to create one without a server.
fn free_server_pages() -> Vec<FreeServerPage> {
    let limits = FreeLimits {
        max_players: 10,
        memory_mb: MemoryLimits {
            vanilla: 2048,
            modded: 6144,
        },
        disk_bytes: 10_737_418_240,
        sleep_minutes: 15,
        unused_days: 30,
        warning_days: 7,
        creates_per_week: 3,
    };
    let card = PublicServerCard {
        token: "F".repeat(43),
        invite: Some(format!("https://play.example.com/{}", "F".repeat(43))),
        joinable: true,
        address: Some("play.example.com:25570".into()),
        name: "Steve".into(),
        icon_version: None,
        game: ServerGame {
            version: Some("1.20.1".into()),
            loader: Some("fabric".into()),
            loader_version: None,
        },
        modded: true,
        status: ServerStatus::Starting,
        players: Some(ServerPlayers {
            online: 0,
            max: Some(10),
        }),
        whitelisted: false,
        pack: None,
        categories: vec![],
        language: "".into(),
        region: None,
        instance: Some("steve".into()),
    };
    vec![
        FreeServerPage {
            limits,
            create: CreateAllowance {
                allowed: false,
                reason: Some("free_server_exists".into()),
                retry_after: None,
            },
            server: Some(FreeServerCard {
                card,
                kind: "modrinth".into(),
                memory_mb: 6144,
                max_players: 10,
                free_status: "starting".into(),
                deletion: Some(Deletion {
                    deletes_at: "2026-11-16T00:00:00Z".into(),
                    warned: true,
                }),
                disk: Disk {
                    used_bytes: 1_073_741_824,
                    limit_bytes: 10_737_418_240,
                    checked_at: Some("2026-10-17T12:00:00Z".into()),
                    over: false,
                },
                invite_url: Some(format!("https://play.example.com/{}", "F".repeat(43))),
                join_url: Some(format!(
                    "perland://join?invite=https%3A%2F%2Fplay.example.com%2F{}",
                    "F".repeat(43)
                )),
            }),
        },
        FreeServerPage {
            limits,
            create: CreateAllowance {
                allowed: true,
                reason: None,
                retry_after: None,
            },
            server: None,
        },
    ]
}

fn public_server_page() -> PublicServerPage {
    let invite = |c: char| format!("https://play.example.com/{}", c.to_string().repeat(43));
    PublicServerPage {
        servers: vec![
            PublicServerCard {
                token: "A".repeat(43),
                invite: Some(invite('A')),
                joinable: true,
                address: Some("play.example.com:25566".into()),
                name: "Survival".into(),
                icon_version: Some("1a2b3c4d5e6f7a8b".into()),
                game: ServerGame {
                    version: Some("1.20.1".into()),
                    loader: Some("fabric".into()),
                    loader_version: Some("0.16.14".into()),
                },
                modded: true,
                status: ServerStatus::Online,
                players: Some(ServerPlayers {
                    online: 7,
                    max: Some(20),
                }),
                whitelisted: false,
                pack: Some(ServerPack {
                    name: "Survival Pack".into(),
                    version: Some("1.0.4".into()),
                    mod_count: 142,
                    state: "ready".into(),
                }),
                categories: vec!["survival".into(), "modded".into()],
                language: "en".into(),
                region: Some("eu".into()),
                instance: Some("survival".into()),
            },
            PublicServerCard {
                token: "B".repeat(43),
                invite: Some(invite('B')),
                joinable: true,
                address: Some("play.example.com:25567".into()),
                name: "Creative".into(),
                icon_version: None,
                game: ServerGame {
                    version: Some("1.21.1".into()),
                    loader: None,
                    loader_version: None,
                },
                modded: false,
                status: ServerStatus::Sleeping,
                players: None,
                whitelisted: true,
                pack: None,
                categories: vec!["creative".into()],
                language: "es".into(),
                region: None,
                instance: None,
            },
            PublicServerCard {
                token: "C".repeat(43),
                invite: Some(invite('C')),
                joinable: false,
                address: Some("play.example.com:25568".into()),
                name: "Tech".into(),
                icon_version: Some("9f8e7d6c5b4a3921".into()),
                game: ServerGame {
                    version: Some("1.20.1".into()),
                    loader: Some("fabric".into()),
                    loader_version: None,
                },
                modded: true,
                status: ServerStatus::Starting,
                players: Some(ServerPlayers {
                    online: 0,
                    max: None,
                }),
                whitelisted: false,
                pack: None,
                categories: vec![],
                language: "en".into(),
                region: None,
                instance: None,
            },
        ],
        total: 3,
        facets: Facets {
            categories: vec!["creative".into(), "survival".into()],
            languages: vec!["en".into(), "es".into()],
            minecraft_versions: vec!["1.21.1".into(), "1.20.1".into()],
            regions: vec!["eu".into()],
        },
    }
}

fn server_news() -> ServerNews {
    let names = |count: u32, names: &[&str]| NameListView {
        count,
        names: names.iter().map(|n| n.to_string()).collect(),
    };
    let post = |id: &str, kind: &str, title: &str, created_at: &str| NewsPostView {
        id: id.into(),
        kind: kind.into(),
        title: title.into(),
        body: "A line\nAnother line".into(),
        starts_at: None,
        ends_at: None,
        image_version: None,
        modpack: None,
        created_at: created_at.into(),
    };
    let mut event = post(
        "3f2b8c1e-5d4a-4e6b-9c7d-1a2b3c4d5e6f",
        "event",
        "Build battle",
        "2026-10-16T10:00:00Z",
    );
    event.starts_at = Some("2026-10-18T18:00:00Z".into());
    event.ends_at = Some("2026-10-18T20:00:00Z".into());
    let mut update = post(
        "7c1d9e2a-8b3f-4a5c-b6d7-2e3f4a5b6c7d",
        "update",
        "New spawn",
        "2026-10-15T09:30:00Z",
    );
    update.image_version = Some("v3".into());
    let mut pack = post(
        "9a8b7c6d-5e4f-4321-8765-4c3b2a1f0e9d",
        "modpack",
        "Modpack 1.0.4",
        "2026-10-14T12:00:00Z",
    );
    pack.modpack = Some(NewsChangesView {
        added: names(3, &["Sodium", "Iris", "Lithium"]),
        removed: names(1, &["Zoomify"]),
        updated: names(0, &[]),
    });
    let bare_pack = post(
        "1b2c3d4e-5f60-4718-8293-a4b5c6d7e8f9",
        "modpack",
        "Modpack 1.0.3",
        "2026-10-13T12:00:00Z",
    );
    ServerNews {
        posts: vec![event, update, pack, bare_pack],
        next_before: Some("41".into()),
        seen: Some("2026-10-15T00:00:00Z".into()),
    }
}

fn server_instance() -> Instance {
    Instance {
        id: "survival".into(),
        name: "Survival".into(),
        source: SourceRef::Perland {
            invite: sample_invite(),
            engine: "https://api.example.com/".into(),
        },
        game: GameSpec {
            minecraft: "1.20.1".into(),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
        },
        update_policy: UpdatePolicy::Auto,
        optional_choices: BTreeMap::new(),
        java: JavaSettings {
            path: None,
            memory_mb: 4096,
            args: String::new(),
        },
        server: Some(ServerLink {
            address: Some(sample_address()),
            name: "survival".into(),
            accent: Some("violet".into()),
            whitelisted: false,
            last_status: Some(ServerStatus::Online),
            last_seen: Some(1_791_720_000),
            gone: false,
        }),
        art_seed: None,
        art: ArtMode::Auto,
        icon: None,
        banner_focus: None,
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

fn sample_hit() -> SearchHit {
    SearchHit {
        project_id: "AANobbMI".into(),
        slug: "sodium".into(),
        title: "Sodium".into(),
        author: "jellysquid3".into(),
        summary: "A modern rendering engine for Minecraft".into(),
        icon_url: Some("https://cdn.modrinth.com/data/AANobbMI/icon.png".into()),
        downloads: 81_000_000,
        follows: 25_000,
        categories: vec!["optimization".into()],
        kind: ContentKind::Mod,
    }
}

fn sample_project() -> ProjectDetail {
    ProjectDetail {
        project_id: "AANobbMI".into(),
        slug: "sodium".into(),
        title: "Sodium".into(),
        author: Some("jellysquid3".into()),
        summary: "A modern rendering engine for Minecraft".into(),
        body: "# Sodium\n\nFaster rendering. <b>bold</b>\n\n![shot](https://cdn.modrinth.com/data/AANobbMI/images/shot.png)".into(),
        icon_url: Some("https://cdn.modrinth.com/data/AANobbMI/icon.png".into()),
        downloads: 81_000_000,
        follows: 25_000,
        categories: vec!["optimization".into()],
        kind: ContentKind::Mod,
        client_side: SideSupport::Required,
        server_side: SideSupport::Unsupported,
        license: Some("Polyform Shield 1.0.0".into()),
        source_url: Some("https://github.com/CaffeineMC/sodium".into()),
        issues_url: Some("https://github.com/CaffeineMC/sodium/issues".into()),
        wiki_url: None,
        gallery: vec![GalleryImage {
            url: "https://cdn.modrinth.com/data/AANobbMI/images/shot.png".into(),
            title: Some("Before and after".into()),
            featured: true,
        }],
    }
}

fn sample_versions() -> Vec<VersionInfo> {
    vec![VersionInfo {
        id: "4OZL6q9h".into(),
        name: "Sodium 0.5.11".into(),
        number: "mc1.20.1-0.5.11".into(),
        version_type: "release".into(),
        game_versions: vec!["1.20.1".into()],
        loaders: vec!["fabric".into(), "quilt".into()],
        date: "2024-07-25T00:00:00Z".into(),
        size: Some(1_234_567),
    }]
}

fn sample_plan_views() -> Vec<InstallPlanView> {
    let items = vec![
        PlanItemView {
            project_id: "AANobbMI".into(),
            title: "Sodium".into(),
            kind: ContentKind::Mod,
            version_number: "mc1.20.1-0.5.11".into(),
            file_name: "sodium-fabric-0.5.11+mc1.20.1.jar".into(),
            size: 1_234_567,
            reason: ItemReason::Requested,
        },
        PlanItemView {
            project_id: "P7dR8mSH".into(),
            title: "Fabric API".into(),
            kind: ContentKind::Mod,
            version_number: "0.92.2+1.20.1".into(),
            file_name: "fabric-api-0.92.2+1.20.1.jar".into(),
            size: 2_000_000,
            reason: ItemReason::Dependency {
                of: "Sodium".into(),
            },
        },
    ];
    vec![
        InstallPlanView {
            plan_id: "plan-1".into(),
            target: PlanTargetView::Instance {
                id: "asalith-fields".into(),
                name: "Asalith Fields".into(),
                minecraft: "1.20.1".into(),
                loader: Some(LoaderKind::Fabric),
            },
            items: items.clone(),
            skipped: vec![PlanSkip {
                project_id: "mOgUt4GM".into(),
                title: "Mod Menu".into(),
                reason: SkipReason::InPack,
            }],
            total_size: 3_234_567,
        },
        InstallPlanView {
            plan_id: "plan-2".into(),
            target: PlanTargetView::New {
                name: "Complementary Shaders".into(),
                minecraft: "1.21.1".into(),
                loader: Some(LoaderKind::Fabric),
            },
            items: vec![PlanItemView {
                reason: ItemReason::ShaderMod,
                ..items[1].clone()
            }],
            skipped: vec![],
            total_size: 2_000_000,
        },
    ]
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
    check(
        "playtime",
        vec![
            PlaytimeEntry {
                id: "asalith-fields".into(),
                seconds: 43_200,
                last_played: 1_791_878_400,
            },
            PlaytimeEntry {
                id: "vanilla-1-20-1".into(),
                seconds: 0,
                last_played: 0,
            },
        ],
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
            rail_expanded: true,
            icon_shape: IconShape::Round,
            theme: Theme::Night,
            font: UiFont::System,
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
        "content_files",
        vec![
            ContentFile {
                file_name: "Faithful 32x.zip".into(),
                size: 18_874_368,
                folder: false,
                managed: true,
            },
            ContentFile {
                file_name: "Fresh Animations.zip".into(),
                size: 4_194_304,
                folder: false,
                managed: false,
            },
            ContentFile {
                file_name: "My Pack".into(),
                size: 0,
                folder: true,
                managed: false,
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
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    code: Some(1),
                    crash: Some(CrashHint::OutOfMemory),
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "creative-flat".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    error: Some(sample_error()),
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "creative-flat".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    cancelled: true,
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Checking,
                exit: None,
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Asking,
                exit: None,
                update: Some(sample_update()),
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Syncing,
                exit: None,
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Waking,
                exit: None,
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    code: Some(0),
                    synced: Some(SyncSummary {
                        from_version: Some("1.0.3".into()),
                        to_version: "1.0.4".into(),
                        added: 3,
                        updated: 2,
                        removed: 1,
                        kept_modified: vec!["options.txt".into()],
                        set_aside: vec!["mods/old-zoom.jar".into()],
                    }),
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    error: Some(
                        LauncherError::pack(
                            ErrorCode::ManualDownloadsNeeded,
                            "1 files have to be downloaded by hand",
                        )
                        .with_param("count", 1)
                        .with_fix("Download them from CurseForge, then try again"),
                    ),
                    manual: vec![sample_manual()],
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::RunState {
                instance: "asalith-fields".into(),
                state: RunStateKind::Idle,
                exit: Some(ExitInfo {
                    update_only: true,
                    ..ExitInfo::default()
                }),
                update: None,
            },
            AppEvent::DeepLink,
            AppEvent::PerlandSignIn {
                phase: PerlandSignInPhase::Waiting,
            },
            AppEvent::PerlandSignIn {
                phase: PerlandSignInPhase::Exchanging,
            },
            AppEvent::Update {
                state: UpdateState::Ready {
                    version: "0.2.0-beta.2".into(),
                },
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
        "perland_status",
        vec![
            PerlandStatus {
                enabled: true,
                methods: vec!["discord".into(), "email".into()],
                account: None,
            },
            PerlandStatus {
                enabled: true,
                methods: vec!["discord".into(), "email".into()],
                account: Some(PerlandAccount {
                    name: "Steve".into(),
                    display_name: "Steve".into(),
                    email: Some("steve@example.com".into()),
                    discord: true,
                    new_player: true,
                    minecraft: Some("Steve".into()),
                }),
            },
            PerlandStatus {
                enabled: true,
                methods: vec!["email".into()],
                account: Some(PerlandAccount {
                    name: "alex@example.com".into(),
                    display_name: "".into(),
                    email: Some("alex@example.com".into()),
                    discord: false,
                    new_player: false,
                    minecraft: None,
                }),
            },
            PerlandStatus {
                enabled: false,
                methods: vec![],
                account: None,
            },
        ],
    );
    check("email_code_sent", EmailCodeSent { resend_in: 60 });
    check(
        "link_code",
        LinkCodeView {
            code: "ABCD-EFGH".into(),
            expires_in: 600,
        },
    );
    check(
        "perland_stats",
        vec![
            Some(PerlandStats {
                servers: vec![
                    ServerStatsView {
                        token: "A".repeat(43),
                        instance: Some("asalith-fields".into()),
                        name: "Asalith".into(),
                        playtime_seconds: 45_000,
                        advancements: 12,
                    },
                    ServerStatsView {
                        token: "B".repeat(43),
                        instance: None,
                        name: "Skyblock".into(),
                        playtime_seconds: 0,
                        advancements: 0,
                    },
                ],
            }),
            None,
        ],
    );
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
        "import_preview",
        ImportPreview {
            path: PathBuf::from("C:/Users/player/Games/instances/Skyblock Adventures"),
            name: Some("Skyblock Adventures".into()),
            minecraft: Some("1.20.1".into()),
            loader: Some(LoaderSpec {
                kind: LoaderKind::Fabric,
                version: "0.16.14".into(),
            }),
            size_bytes: 1_234_000_000,
        },
    );
    check(
        "import_request",
        ImportRequest {
            path: PathBuf::from("C:/Users/player/Games/instances/Skyblock Adventures"),
            name: "Skyblock".into(),
            minecraft: "1.20.1".into(),
            loader: None,
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
            OpenTarget::ResourcePacks {
                id: "asalith-fields".into(),
            },
            OpenTarget::ShaderPacks {
                id: "asalith-fields".into(),
            },
            OpenTarget::Data,
            OpenTarget::Logs,
            OpenTarget::Downloads,
        ],
    );
    check(
        "run_snapshots",
        vec![
            RunSnapshot {
                instance: "asalith-fields".into(),
                state: RunStateKind::Running,
                update: None,
            },
            RunSnapshot {
                instance: "sky-islands".into(),
                state: RunStateKind::Asking,
                update: Some(sample_update()),
            },
        ],
    );
    let temurin = JavaFound {
        path: PathBuf::from(
            "C:/Program Files/Eclipse Adoptium/jdk-17.0.12.7-hotspot/bin/javaw.exe",
        ),
        major: 17,
        version: "17.0.12".into(),
        vendor: Some("Eclipse Temurin".into()),
        fit: Some(JavaFit::Matches),
        recommended: true,
    };
    check(
        "java_test",
        vec![
            temurin.clone(),
            JavaFound {
                path: PathBuf::from("/usr/lib/jvm/java-21/bin/java"),
                major: 21,
                version: "21.0.4".into(),
                vendor: None,
                fit: None,
                recommended: false,
            },
        ],
    );
    check(
        "java_overview",
        vec![
            JavaOverview {
                required: Some(17),
                installs: vec![
                    temurin,
                    JavaFound {
                        path: PathBuf::from("C:/Program Files/Zulu/zulu-21/bin/javaw.exe"),
                        major: 21,
                        version: "21.0.4".into(),
                        vendor: Some("Azul Zulu".into()),
                        fit: Some(JavaFit::Newer),
                        recommended: false,
                    },
                    JavaFound {
                        path: PathBuf::from("C:/Program Files/Java/jre1.8.0_421/bin/javaw.exe"),
                        major: 8,
                        version: "1.8.0_421".into(),
                        vendor: Some("Oracle".into()),
                        fit: Some(JavaFit::TooOld),
                        recommended: false,
                    },
                ],
            },
            JavaOverview {
                required: None,
                installs: vec![],
            },
        ],
    );
    check(
        "pack_status",
        vec![
            PackStatus {
                installed_version: Some("1.0.3".into()),
                update: Some(sample_update()),
                check_error: None,
            },
            PackStatus {
                installed_version: Some("1.0.3".into()),
                update: None,
                check_error: Some(
                    LauncherError::network(
                        ErrorCode::UpdateCheckFailed,
                        "Couldn't check for updates",
                    )
                    .with_fix("You can still play the version you have"),
                ),
            },
        ],
    );
    check(
        "pack_contents",
        PackContents {
            managed: vec![
                PackMod {
                    path: "mods/create.jar".into(),
                    name: "Create".into(),
                    optional: None,
                },
                PackMod {
                    path: "mods/iris.jar".into(),
                    name: "Iris Shaders".into(),
                    optional: Some(OptionalChoice {
                        enabled: false,
                        description: Some("Shaders, for strong computers".into()),
                    }),
                },
            ],
            additions: vec![LocalMod {
                file_name: "zoomify-2.11.2.jar".into(),
                enabled: true,
                size: 524_288,
            }],
        },
    );
    check(
        "manual_progress",
        ManualProgress {
            found: 1,
            total: 2,
            remaining: vec![sample_manual()],
        },
    );
    check(
        "instance_art",
        InstanceArtData {
            icon: Some("data:image/png;base64,iVBORw0KGgo=".into()),
            banner: None,
        },
    );
    check(
        "art_choices",
        vec![
            ArtChoice::Generated {
                seed: "dunes".into(),
            },
            ArtChoice::Pack,
            ArtChoice::Upload {
                path: PathBuf::from("C:/Users/player/Pictures/my island.png"),
            },
        ],
    );
    check(
        "icon_choices",
        vec![
            IconChoice::Generated,
            IconChoice::Pack,
            IconChoice::Upload {
                path: PathBuf::from("C:/Users/player/Pictures/my icon.png"),
            },
        ],
    );
    check(
        "pack_add_requests",
        vec![
            PackAddRequest::Link {
                url: "https://perlanddev.github.io/asalith-modpack/pack.toml".into(),
            },
            PackAddRequest::File {
                path: PathBuf::from("C:/Users/player/Downloads/Sky Islands 1.0.4.mrpack"),
            },
        ],
    );
    check(
        "search_page",
        SearchPage {
            hits: vec![sample_hit()],
            total: 1,
            offset: 0,
        },
    );
    check("project_detail", sample_project());
    check("versions", sample_versions());
    check("install_plans", sample_plan_views());
    check(
        "install_result",
        InstallResult {
            instance: fabric_instance(),
            added: vec!["sodium-fabric-0.5.11+mc1.20.1.jar".into()],
        },
    );
    check(
        "search_requests",
        vec![SearchRequest {
            query: "sodium".into(),
            kind: ContentKind::Mod,
            instance: Some("asalith-fields".into()),
            minecraft: None,
            loader: None,
            categories: vec!["optimization".into()],
            sort: SortIndex::Relevance,
            offset: 0,
        }],
    );
    check(
        "list_queries",
        vec![
            ListQuery::default(),
            ListQuery {
                sort: ListSort::Newest,
                modded: Some(true),
                minecraft: Some("1.20.1".into()),
                has_room: true,
                category: Some("survival".into()),
                language: Some("es".into()),
                region: Some("sa".into()),
                offset: 50,
            },
        ],
    );
    check(
        "plan_requests",
        vec![
            BrowsePlanRequest {
                project: "sodium".into(),
                version: None,
                target: PlanTargetRequest::Instance {
                    id: "asalith-fields".into(),
                },
            },
            BrowsePlanRequest {
                project: "complementary".into(),
                version: Some("v5".into()),
                target: PlanTargetRequest::New,
            },
        ],
    );
    check("server_instance", server_instance());
    check("public_server_page", public_server_page());
    check("your_servers_page", your_servers_page());
    check("free_server_page", free_server_pages());
    check("server_news", server_news());
    check(
        "join_preview",
        vec![
            JoinPreview {
                host: "play.example.com".into(),
                name: "survival".into(),
                motd: "§6Survival§r
with friends"
                    .into(),
                kind: "stack".into(),
                status: Some(ServerStatus::Online),
                players: Some(ServerPlayers {
                    online: 3,
                    max: Some(20),
                }),
                game: ServerGame {
                    version: Some("1.20.1".into()),
                    loader: Some("fabric".into()),
                    loader_version: Some("0.16.14".into()),
                },
                pack: Some(ServerPack {
                    name: "Survival Pack".into(),
                    version: Some("1.0.4".into()),
                    mod_count: 142,
                    state: "ready".into(),
                }),
                whitelisted: false,
                team: Some(ServerTeam {
                    display_name: "Acme".into(),
                    accent: Some("violet".into()),
                }),
                address: Some(sample_address()),
                icon: Some("data:image/png;base64,iVBORw0KGgo=".into()),
                existing: None,
            },
            JoinPreview {
                host: "play.example.com".into(),
                name: "creative".into(),
                motd: "Build".into(),
                kind: "stack".into(),
                status: Some(ServerStatus::Sleeping),
                players: None,
                game: ServerGame {
                    version: Some("1.21.1".into()),
                    loader: None,
                    loader_version: None,
                },
                pack: None,
                whitelisted: true,
                team: None,
                address: Some(ServerAddress {
                    host: "play.example.com".into(),
                    port: 25570,
                }),
                icon: None,
                existing: Some("creative".into()),
            },
        ],
    );
    check(
        "server_status",
        vec![
            ServerStatusView {
                status: Some(ServerStatus::Online),
                players: Some(ServerPlayers {
                    online: 3,
                    max: Some(20),
                }),
                whitelisted: false,
                pack_state: Some("ready".into()),
                gone: false,
                address: Some(sample_address()),
            },
            ServerStatusView {
                status: None,
                players: None,
                whitelisted: true,
                pack_state: None,
                gone: true,
                address: Some(sample_address()),
            },
        ],
    );
    check(
        "deep_links",
        vec![
            DeepLink::Pack {
                url: "https://perlanddev.github.io/asalith-modpack/pack.toml".into(),
            },
            DeepLink::Join {
                invite: sample_invite(),
            },
        ],
    );
    check("invite_links", invite_link_cases());
    check(
        "update_states",
        vec![
            UpdateState::Off,
            UpdateState::Idle,
            UpdateState::Checking,
            UpdateState::Downloading { percent: Some(40) },
            UpdateState::Downloading { percent: None },
            UpdateState::Ready {
                version: "0.2.0-beta.2".into(),
            },
            UpdateState::UpToDate,
            UpdateState::Failed {
                error: LauncherError::network(
                    ErrorCode::LauncherUpdateFailed,
                    "Couldn't check for launcher updates",
                )
                .with_fix("You can keep playing. It tries again next time you open the launcher"),
            },
        ],
    );
}
