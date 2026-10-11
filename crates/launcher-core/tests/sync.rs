mod common;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use common::{hits, sources, write_zip, PackBuilder};
use launcher_core::{
    error::{LauncherError, Result},
    events::{Event, EventSink},
    instances::{GameSpec, Instance, InstanceStore, LoaderSpec, SourceRef, UpdatePolicy},
    locks::InstanceLock,
    meta::loaders::LoaderKind,
    sources::{DesiredState, PackInput, UpdateCheck},
    sync::{
        lockfile::{additions, Managed},
        BoxFuture, GameInstall, SyncOutcome, SyncReport, Syncer,
    },
    ErrorCode,
};
use serde_json::json;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

/// Records installs instead of installing; `fail` makes them fail.
#[derive(Default)]
struct FakeGame {
    installs: Mutex<Vec<GameSpec>>,
    fail: AtomicBool,
    /// A file whose existence is recorded at install time (it must not exist yet).
    watch: Mutex<Option<PathBuf>>,
    seen: Mutex<Vec<bool>>,
    /// (store dir, instance id, new name): renamed mid-install, like a player in the UI.
    rename: Mutex<Option<(PathBuf, String, String)>>,
}

impl GameInstall for FakeGame {
    fn install_game<'a>(
        &'a self,
        game: &'a GameSpec,
        _sink: &'a EventSink,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            self.installs.lock().unwrap().push(game.clone());
            let watched = self.watch.lock().unwrap().clone();
            if let Some(path) = watched {
                self.seen.lock().unwrap().push(path.exists());
            }
            let rename = self.rename.lock().unwrap().clone();
            if let Some((dir, id, name)) = rename {
                InstanceStore::new(dir).rename(&id, &name).await.unwrap();
            }
            if self.fail.load(Ordering::SeqCst) {
                Err(LauncherError::loader(
                    ErrorCode::LoaderVersionNotFound,
                    "no such Forge",
                ))
            } else {
                Ok(())
            }
        })
    }
}

struct Env {
    server: MockServer,
    _tmp: tempfile::TempDir,
    store: InstanceStore,
    syncer: Syncer,
    game: Arc<FakeGame>,
    events: Arc<Mutex<Vec<Event>>>,
}

impl Env {
    async fn new() -> Self {
        let server = MockServer::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let store = InstanceStore::new(tmp.path().join("instances"));
        let game = Arc::new(FakeGame::default());
        let syncer = Syncer::new(game.clone(), sources(&server, tmp.path()), tmp.path());
        Self {
            server,
            _tmp: tmp,
            store,
            syncer,
            game,
            events: Arc::default(),
        }
    }

    fn sink(&self) -> EventSink {
        let events = self.events.clone();
        Arc::new(move |e| events.lock().unwrap().push(e))
    }

    /// An instance for the pack at `url`, created the way `add_pack` (Task 17) does.
    async fn instance(&self, url: &str) -> (Instance, DesiredState) {
        let (source, desired) = self
            .syncer
            .sources
            .open(&PackInput::Packwiz { url: url.into() })
            .await
            .unwrap();
        let mut instance = self
            .store
            .create(
                &desired.name,
                GameSpec {
                    minecraft: desired.minecraft.clone(),
                    loader: desired.loader.clone(),
                },
            )
            .await
            .unwrap();
        instance.source = source;
        self.store.save(&instance).await.unwrap();
        (instance, desired)
    }

    fn game_file(&self, id: &str, rel: &str) -> PathBuf {
        self.store.game_dir(id).join(rel)
    }

    async fn sync(&self, id: &str, desired: &DesiredState) -> Result<SyncOutcome> {
        self.syncer
            .sync(&self.store, id, desired, &self.sink())
            .await
    }
}

fn done(outcome: SyncOutcome) -> SyncReport {
    match outcome {
        SyncOutcome::Done(report) => report,
        other => panic!("expected Done, got {other:?}"),
    }
}

fn v1(server: &MockServer) -> PackBuilder {
    PackBuilder::new(server, "Tiny", "1.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .url_mod("zoom", "zoom.jar", b"zoom", "client", None)
        .url_mod("backups", "backups.jar", b"backup", "server", None)
        .url_mod("shaders", "shaders.jar", b"shade", "client", Some(false))
        .file("config/a.toml", b"a = 1", true)
}

#[tokio::test]
async fn fresh_sync_installs_client_files_and_writes_the_lockfile_last() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    let report = done(env.sync(&instance.id, &desired).await.unwrap());

    assert!(!report.up_to_date && !report.game_changed);
    assert_eq!(report.changes.added, vec!["a.toml", "appleskin", "zoom"]);
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/appleskin.jar")).unwrap(),
        b"apple v1"
    );
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());
    assert!(env.game_file(&instance.id, "config/a.toml").exists());
    assert!(
        !env.game_file(&instance.id, "mods/backups.jar").exists(),
        "server-only"
    );
    assert!(
        !env.game_file(&instance.id, "mods/shaders.jar").exists(),
        "optional, off by default"
    );

    let perland = env.store.perland_dir(&instance.id);
    let managed = Managed::load(&perland).await.unwrap();
    assert_eq!(managed.stamp, desired.stamp);
    assert_eq!(managed.files.len(), 3);
    assert!(
        !managed.choices["mods/shaders.jar"],
        "the optional default is recorded"
    );
    assert!(perland.join("pack.json").exists());
    assert!(!perland.join("staging").exists());
    assert!(
        env.game.installs.lock().unwrap().is_empty(),
        "same game: nothing to install"
    );
    let progress = env
        .events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| {
            matches!(e,
                Event::Progress { task, .. } if task.key == "sync_pack" && task.params["name"] == "Tiny")
        })
        .count();
    assert!(progress >= 2, "{progress}");
}

#[tokio::test]
async fn an_unchanged_pack_takes_the_fast_path() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());
    let report = done(env.sync(&instance.id, &desired).await.unwrap());
    assert!(report.up_to_date);
    assert_eq!(hits(&env.server, "/files/appleskin.jar").await, 1);

    // A deleted mod is noticed and restored from the cache, without downloading it again.
    std::fs::remove_file(env.game_file(&instance.id, "mods/zoom.jar")).unwrap();
    let report = done(env.sync(&instance.id, &desired).await.unwrap());
    assert!(!report.up_to_date);
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());
    assert_eq!(hits(&env.server, "/files/zoom.jar").await, 1);
}

#[tokio::test]
async fn a_locked_instance_is_refused() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    let held = InstanceLock::try_acquire(&env.store.perland_dir(&instance.id)).unwrap();
    let err = env.sync(&instance.id, &desired).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InstanceLocked);
    assert!(!env.game_file(&instance.id, "mods/appleskin.jar").exists());
    drop(held);
}

#[tokio::test]
async fn a_new_loader_is_installed_before_any_file_moves() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .with_loader("forge", "47.4.30")
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .mount(&env.server)
        .await;
    let v2 = env.syncer.sources.resolve(&instance.source).await.unwrap();
    *env.game.watch.lock().unwrap() = Some(env.game_file(&instance.id, "mods/jei.jar"));
    let report = done(env.sync(&instance.id, &v2).await.unwrap());
    assert!(report.game_changed);
    assert_eq!(
        *env.game.seen.lock().unwrap(),
        vec![false],
        "the game is installed before any pack file is moved into place"
    );
    let wanted = GameSpec {
        minecraft: "1.20.1".into(),
        loader: Some(LoaderSpec {
            kind: LoaderKind::Forge,
            version: "47.4.30".into(),
        }),
    };
    assert_eq!(*env.game.installs.lock().unwrap(), vec![wanted.clone()]);
    assert_eq!(env.store.load(&instance.id).await.unwrap().game, wanted);
    assert!(env.game_file(&instance.id, "mods/jei.jar").exists());
}

#[tokio::test]
async fn failed_game_install_leaves_the_instance_untouched() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "2.0.0")
        .with_loader("forge", "99.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v2", "both", None)
        .mount(&env.server)
        .await;
    let v2 = env.syncer.sources.resolve(&instance.source).await.unwrap();
    env.game.fail.store(true, Ordering::SeqCst);
    let err = env.sync(&instance.id, &v2).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::LoaderVersionNotFound);

    assert_eq!(
        env.store.load(&instance.id).await.unwrap().game,
        instance.game
    );
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/appleskin.jar")).unwrap(),
        b"apple v1"
    );
    assert!(
        env.game_file(&instance.id, "mods/zoom.jar").exists(),
        "nothing was removed"
    );
    let managed = Managed::load(&env.store.perland_dir(&instance.id))
        .await
        .unwrap();
    assert_eq!(
        managed.stamp, desired.stamp,
        "the old lockfile still describes the instance"
    );
}

#[tokio::test]
async fn a_rename_during_a_loader_change_survives() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    done(env.sync(&instance.id, &desired).await.unwrap());

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .with_loader("forge", "47.4.30")
        .mount(&env.server)
        .await;
    let v2 = env.syncer.sources.resolve(&instance.source).await.unwrap();
    *env.game.rename.lock().unwrap() = Some((
        env._tmp.path().join("instances"),
        instance.id.clone(),
        "Renamed".into(),
    ));
    let report = done(env.sync(&instance.id, &v2).await.unwrap());
    assert!(report.game_changed);

    let saved = env.store.load(&instance.id).await.unwrap();
    assert_eq!(saved.name, "Renamed", "the sync must not revert the rename");
    assert_eq!(saved.game.loader.unwrap().version, "47.4.30");
}

#[tokio::test]
async fn blocked_curseforge_files_wait_for_a_manual_download() {
    let env = Env::new().await;
    let url = PackBuilder::new(&env.server, "Tiny", "1.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .cf_mod("cozy", "cozy.jar", 1234567, 42, b"cozy", true)
        .mount(&env.server)
        .await;
    let (instance, desired) = env.instance(&url).await;
    let files = match env.sync(&instance.id, &desired).await.unwrap() {
        SyncOutcome::NeedsManual(files) => files,
        other => panic!("{other:?}"),
    };
    assert_eq!(files.len(), 1);
    assert_eq!(
        (files[0].path.as_str(), files[0].file_name.as_str()),
        ("mods/cozy.jar", "cozy.jar")
    );
    assert_eq!(files[0].page_url, "https://www.curseforge.com/projects/42");
    assert!(
        !env.game_file(&instance.id, "mods/appleskin.jar").exists(),
        "nothing applied"
    );
    assert!(Managed::load(&env.store.perland_dir(&instance.id))
        .await
        .is_none());

    let downloads = env._tmp.path().join("Downloads");
    std::fs::create_dir_all(&downloads).unwrap();
    std::fs::write(downloads.join("cozy (1).jar"), b"cozy").unwrap();
    assert_eq!(
        env.syncer.import_manual(&downloads, &files).await.unwrap(),
        1
    );
    done(env.sync(&instance.id, &desired).await.unwrap());
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/cozy.jar")).unwrap(),
        b"cozy"
    );
    assert!(
        downloads.join("cozy (1).jar").exists(),
        "the player's download is left alone"
    );
}

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

#[tokio::test]
async fn add_pack_names_the_instance_after_the_pack_and_takes_its_policy() {
    let env = Env::new().await;
    let url = v1(&env.server)
        .sidecar("[art]\nicon = \"icon.png\"\n[sync]\npolicy = \"auto\"\n")
        .host("/pack/icon.png", PNG)
        .mount(&env.server)
        .await;
    let (instance, desired) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url: url.clone() })
        .await
        .unwrap();
    assert_eq!(instance.name, "Tiny");
    assert_eq!(instance.source, SourceRef::Packwiz { url });
    assert_eq!(instance.update_policy, UpdatePolicy::Auto);
    assert_eq!(instance.game.loader.clone().unwrap().version, "47.4.22");
    assert_eq!(env.store.load(&instance.id).await.unwrap(), instance);
    let art = env.store.perland_dir(&instance.id).join("art/icon.png");
    assert_eq!(std::fs::read(art).unwrap(), PNG);
    assert!(
        !env.game_file(&instance.id, "mods/appleskin.jar").exists(),
        "files come with the first sync"
    );
    done(env.sync(&instance.id, &desired).await.unwrap());
    assert!(env.game_file(&instance.id, "mods/appleskin.jar").exists());
}

#[tokio::test]
async fn ensure_takes_updates_and_preview_names_them() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    assert_eq!(instance.update_policy, UpdatePolicy::Ask, "no sidecar: ask");
    let first = env
        .syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    assert!(first.warning.is_none());
    assert!(!done(first.outcome).up_to_date);
    assert_eq!(
        env.syncer.check(&env.store, &instance).await,
        UpdateCheck::UpToDate
    );

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "1.1.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v1", "both", None)
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .file("config/a.toml", b"a = 1", true)
        .mount(&env.server)
        .await;
    let UpdateCheck::Available {
        source, version, ..
    } = env.syncer.check(&env.store, &instance).await
    else {
        panic!("expected an update");
    };
    assert_eq!(version, "1.1.0");
    let v2 = env.syncer.sources.resolve(&source).await.unwrap();
    let preview = env.syncer.preview(&env.store, &instance, &v2).await;
    assert_eq!(
        (preview.added.clone(), preview.removed.clone()),
        (vec!["jei".to_string()], vec!["zoom".to_string()])
    );
    assert_eq!(preview.from_version.as_deref(), Some("1.0.0"));

    let report = done(
        env.syncer
            .ensure(&env.store, &instance.id, &env.sink())
            .await
            .unwrap()
            .outcome,
    );
    assert_eq!(report.changes, preview);
    assert!(env.game_file(&instance.id, "mods/jei.jar").exists());
    assert!(!env.game_file(&instance.id, "mods/zoom.jar").exists());
}

#[tokio::test]
async fn pinned_instances_stay_on_their_version() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (mut instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    instance.update_policy = UpdatePolicy::Pinned;
    env.store.save(&instance).await.unwrap();

    env.server.reset().await;
    v1(&env.server)
        .with_version("9.9.9")
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .mount(&env.server)
        .await;
    let ensured = env
        .syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    assert!(done(ensured.outcome).up_to_date);
    assert!(!env.game_file(&instance.id, "mods/jei.jar").exists());
    assert_eq!(
        hits(&env.server, "/pack/pack.toml").await,
        0,
        "pinned never checks"
    );
}

#[tokio::test]
async fn offline_ensure_uses_the_installed_snapshot_with_a_warning() {
    let env = Env::new().await;
    let url = v1(&env.server)
        .sidecar("[art]\nicon = \"icon.png\"\n")
        .host("/pack/icon.png", PNG)
        .mount(&env.server)
        .await;
    let (instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    std::fs::remove_file(env.game_file(&instance.id, "mods/zoom.jar")).unwrap();

    env.server.reset().await; // the pack host is gone: everything answers 404
    let ensured = env
        .syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    assert_eq!(ensured.warning.unwrap().code, ErrorCode::UpdateCheckFailed);
    done(ensured.outcome);
    assert!(
        env.game_file(&instance.id, "mods/zoom.jar").exists(),
        "repaired from the cache"
    );
    assert_eq!(
        hits(&env.server, "/pack/icon.png").await,
        0,
        "an offline repair doesn't wait on the art"
    );

    // A pack that was never synced can't be played offline: a clear error, nothing half-made.
    let mut never = env
        .store
        .create("Never", instance.game.clone())
        .await
        .unwrap();
    never.source = SourceRef::Packwiz {
        url: format!("{}/other/pack.toml", env.server.uri()),
    };
    env.store.save(&never).await.unwrap();
    let err = env
        .syncer
        .ensure(&env.store, &never.id, &env.sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::PackNotFound);
    assert!(Managed::load(&env.store.perland_dir(&never.id))
        .await
        .is_none());

    let vanilla = env
        .store
        .create("Vanilla", instance.game.clone())
        .await
        .unwrap();
    assert_eq!(
        env.syncer
            .ensure(&env.store, &vanilla.id, &env.sink())
            .await
            .unwrap_err()
            .code,
        ErrorCode::NoPack
    );
}

fn trashed(env: &Env, id: &str, rel: &str) -> Option<PathBuf> {
    let trash = env.store.perland_dir(id).join("trash");
    std::fs::read_dir(trash)
        .ok()?
        .flatten()
        .map(|d| d.path().join(rel))
        .find(|p| p.exists())
}

#[tokio::test]
async fn update_keeps_additions_and_preserved_configs() {
    let env = Env::new().await;
    let url = v1(&env.server)
        .url_mod("gone", "gone.jar", b"gone", "both", None)
        .mount(&env.server)
        .await;
    let (instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    std::fs::write(env.game_file(&instance.id, "mods/mine.jar"), b"my own mod").unwrap();
    std::fs::write(
        env.game_file(&instance.id, "config/a.toml"),
        b"a = 42 # mine",
    )
    .unwrap();

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "2.0.0")
        .url_mod("appleskin", "appleskin.jar", b"apple v2", "both", None)
        .url_mod("zoom", "zoom.jar", b"zoom", "client", None)
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .file("config/a.toml", b"a = 2", true)
        .mount(&env.server)
        .await;
    let report = done(
        env.syncer
            .ensure(&env.store, &instance.id, &env.sink())
            .await
            .unwrap()
            .outcome,
    );

    assert_eq!(report.changes.added, vec!["jei"]);
    assert_eq!(report.changes.updated, vec!["appleskin"]);
    assert_eq!(report.changes.removed, vec!["gone"]);
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/appleskin.jar")).unwrap(),
        b"apple v2"
    );
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/mine.jar")).unwrap(),
        b"my own mod"
    );
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "config/a.toml")).unwrap(),
        b"a = 42 # mine"
    );
    assert!(!env.game_file(&instance.id, "mods/gone.jar").exists());
    assert!(
        trashed(&env, &instance.id, "mods/gone.jar").is_some(),
        "removed files go to the trash"
    );
    let managed = Managed::load(&env.store.perland_dir(&instance.id)).await;
    assert_eq!(
        additions(&env.store.game_dir(&instance.id), managed.as_ref())
            .await
            .unwrap(),
        vec!["mine.jar"]
    );
}

#[tokio::test]
async fn a_player_file_where_the_pack_now_puts_one_goes_to_the_trash() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    std::fs::write(env.game_file(&instance.id, "mods/jei.jar"), b"player's jei").unwrap();

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .url_mod("jei", "jei.jar", b"pack jei", "both", None)
        .mount(&env.server)
        .await;
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/jei.jar")).unwrap(),
        b"pack jei"
    );
    let kept = trashed(&env, &instance.id, "mods/jei.jar").expect("kept in the trash");
    assert_eq!(std::fs::read(kept).unwrap(), b"player's jei");
}

#[tokio::test]
async fn optional_toggles_apply_from_the_snapshot() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (mut instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    instance.update_policy = UpdatePolicy::Pinned; // no update checks: only the snapshot
    env.store.save(&instance).await.unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    assert!(!env.game_file(&instance.id, "mods/shaders.jar").exists());
    let pack_reads = hits(&env.server, "/pack/pack.toml").await;

    instance
        .optional_choices
        .insert("mods/shaders.jar".into(), true);
    env.store.save(&instance).await.unwrap();
    let report = done(
        env.syncer
            .ensure(&env.store, &instance.id, &env.sink())
            .await
            .unwrap()
            .outcome,
    );
    assert!(!report.up_to_date);
    assert!(env.game_file(&instance.id, "mods/shaders.jar").exists());

    instance
        .optional_choices
        .insert("mods/shaders.jar".into(), false);
    env.store.save(&instance).await.unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();
    assert!(!env.game_file(&instance.id, "mods/shaders.jar").exists());
    assert!(trashed(&env, &instance.id, "mods/shaders.jar").is_some());
    assert_eq!(
        hits(&env.server, "/pack/pack.toml").await,
        pack_reads,
        "toggles apply from the installed snapshot, without reading the pack"
    );
}

#[tokio::test]
async fn interrupted_sync_keeps_the_old_state_and_resumes_from_the_cache() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    env.syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap();

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .url_mod("alpha", "alpha.jar", b"alpha", "both", None)
        .url_mod("beta", "beta.jar", b"beta", "both", None)
        .mount(&env.server)
        .await;
    // beta.jar fails (slowly, so alpha finishes first) for the whole first attempt.
    Mock::given(method("GET"))
        .and(path("/files/beta.jar"))
        .respond_with(ResponseTemplate::new(500).set_delay(Duration::from_millis(100)))
        .with_priority(1)
        .up_to_n_times(4)
        .mount(&env.server)
        .await;
    let err = env
        .syncer
        .ensure(&env.store, &instance.id, &env.sink())
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::HostTrouble);
    let managed = Managed::load(&env.store.perland_dir(&instance.id))
        .await
        .unwrap();
    assert_eq!(
        managed.stamp, desired.stamp,
        "the old lockfile still describes the instance"
    );
    assert!(
        !env.game_file(&instance.id, "mods/alpha.jar").exists(),
        "nothing was swapped in"
    );
    assert!(env.game_file(&instance.id, "mods/zoom.jar").exists());

    done(
        env.syncer
            .ensure(&env.store, &instance.id, &env.sink())
            .await
            .unwrap()
            .outcome,
    );
    assert!(env.game_file(&instance.id, "mods/alpha.jar").exists());
    assert!(env.game_file(&instance.id, "mods/beta.jar").exists());
    assert_eq!(
        hits(&env.server, "/files/alpha.jar").await,
        1,
        "alpha came from the cache the second time"
    );
}

fn mrpack(env: &Env, name: &str, overrides: &[(&str, &[u8])]) -> PathBuf {
    let index = serde_json::to_vec(&json!({
        "formatVersion": 1, "game": "minecraft", "versionId": name, "name": "Sky",
        "files": [], "dependencies": { "minecraft": "1.20.1" }
    }))
    .unwrap();
    let file = env._tmp.path().join(format!("{name}.mrpack"));
    let mut entries: Vec<(&str, &[u8])> = vec![("modrinth.index.json", &index[..])];
    entries.extend_from_slice(overrides);
    write_zip(&file, &entries);
    file
}

#[tokio::test]
async fn mrpack_override_edits_survive_an_update() {
    let env = Env::new().await;
    let first = mrpack(
        &env,
        "1.0",
        &[
            ("overrides/config/c.toml", &b"v1"[..]),
            ("overrides/options.txt", &b"fov:1"[..]),
        ],
    );
    let (instance, desired) = env
        .syncer
        .add_pack(&env.store, &PackInput::MrpackFile { path: first })
        .await
        .unwrap();
    assert_eq!(instance.game.loader, None);
    done(env.sync(&instance.id, &desired).await.unwrap());
    std::fs::write(env.game_file(&instance.id, "config/c.toml"), b"mine").unwrap();

    let second = mrpack(
        &env,
        "2.0",
        &[
            ("overrides/config/c.toml", &b"v2"[..]),
            ("overrides/options.txt", &b"fov:2"[..]),
        ],
    );
    let (_, desired2) = env
        .syncer
        .sources
        .open(&PackInput::MrpackFile { path: second })
        .await
        .unwrap();
    let report = done(env.sync(&instance.id, &desired2).await.unwrap());
    assert_eq!(report.kept_modified, vec!["config/c.toml"]);
    assert!(
        !report.changes.updated.contains(&"c.toml".to_string()),
        "kept, so not updated: {:?}",
        report.changes
    );
    assert!(
        !report.changes.updated.is_empty(),
        "options.txt was updated"
    );
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "config/c.toml")).unwrap(),
        b"mine"
    );
    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "options.txt")).unwrap(),
        b"fov:2"
    );
}

/// A packwiz instance made by `add_pack` and synced once.
async fn synced(env: &Env, pack: PackBuilder) -> Instance {
    let url = pack.mount(&env.server).await;
    let (instance, _) = env
        .syncer
        .add_pack(&env.store, &PackInput::Packwiz { url })
        .await
        .unwrap();
    done(
        env.syncer
            .ensure(&env.store, &instance.id, &env.sink())
            .await
            .unwrap()
            .outcome,
    );
    instance
}

async fn ensure(env: &Env, id: &str) -> Result<launcher_core::sync::Ensured> {
    env.syncer.ensure(&env.store, id, &env.sink()).await
}

#[tokio::test]
async fn a_case_only_rename_keeps_exactly_one_copy_of_the_mod() {
    let env = Env::new().await;
    let instance = synced(
        &env,
        PackBuilder::new(&env.server, "Tiny", "1.0.0")
            .url_mod("foo", "foo.jar", b"foo", "both", None),
    )
    .await;

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "1.1.0")
        .url_mod("foo", "Foo.jar", b"foo", "both", None)
        .mount(&env.server)
        .await;
    done(ensure(&env, &instance.id).await.unwrap().outcome);

    assert_eq!(
        std::fs::read(env.game_file(&instance.id, "mods/Foo.jar")).unwrap(),
        b"foo"
    );
    let copies: Vec<String> = std::fs::read_dir(env.game_file(&instance.id, "mods"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.eq_ignore_ascii_case("foo.jar"))
        .collect();
    assert_eq!(copies.len(), 1, "{copies:?}");
    let managed = Managed::load(&env.store.perland_dir(&instance.id))
        .await
        .unwrap();
    assert_eq!(
        managed.files.keys().collect::<Vec<_>>(),
        vec!["mods/Foo.jar"]
    );
    assert!(done(ensure(&env, &instance.id).await.unwrap().outcome).up_to_date);
}

#[tokio::test]
async fn a_folder_where_the_pack_wants_a_file_stops_the_sync() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;
    let folder = env.game_file(&instance.id, "mods/jei.jar");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("notes.txt"), b"mine").unwrap();

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .url_mod("alpha", "alpha.jar", b"alpha", "both", None)
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .mount(&env.server)
        .await;
    let err = ensure(&env, &instance.id).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::PackPathIsFolder);
    assert_eq!(err.params["path"], "mods/jei.jar");
    assert_eq!(std::fs::read(folder.join("notes.txt")).unwrap(), b"mine");
    assert!(
        !env.game_file(&instance.id, "mods/alpha.jar").exists(),
        "nothing applied"
    );
    let managed = Managed::load(&env.store.perland_dir(&instance.id))
        .await
        .unwrap();
    assert_eq!(managed.pack_version, "1.0.0");
}

#[tokio::test]
async fn an_interrupted_swap_converges_to_the_new_version_offline() {
    let env = Env::new().await;
    let instance = synced(
        &env,
        PackBuilder::new(&env.server, "Tiny", "1.0.0").url_mod(
            "create",
            "create-0.5.1.jar",
            b"create 0.5.1",
            "both",
            None,
        ),
    )
    .await;

    env.server.reset().await;
    PackBuilder::new(&env.server, "Tiny", "1.1.0")
        .url_mod("create", "create-0.5.2.jar", b"create 0.5.2", "both", None)
        .mount(&env.server)
        .await;
    // The swap stops after the new jar is placed: a file where the trash folder goes makes
    // moving the old jar out fail.
    let trash = env.store.perland_dir(&instance.id).join("trash");
    let _ = std::fs::remove_dir_all(&trash);
    std::fs::write(&trash, b"in the way").unwrap();
    ensure(&env, &instance.id).await.unwrap_err();
    assert!(env
        .game_file(&instance.id, "mods/create-0.5.2.jar")
        .exists());
    assert!(
        env.game_file(&instance.id, "mods/create-0.5.1.jar")
            .exists(),
        "stopped midway"
    );

    std::fs::remove_file(&trash).unwrap();
    env.server.reset().await; // offline from now on
    let ensured = ensure(&env, &instance.id).await.unwrap();
    assert_eq!(ensured.warning.unwrap().code, ErrorCode::UpdateCheckFailed);
    done(ensured.outcome);
    assert!(env
        .game_file(&instance.id, "mods/create-0.5.2.jar")
        .exists());
    assert!(
        !env.game_file(&instance.id, "mods/create-0.5.1.jar")
            .exists(),
        "no duplicate mod left behind"
    );
    assert!(trashed(&env, &instance.id, "mods/create-0.5.1.jar").is_some());
    let managed = Managed::load(&env.store.perland_dir(&instance.id))
        .await
        .unwrap();
    assert_eq!(managed.pack_version, "1.1.0");
}

#[tokio::test]
async fn ensure_and_sync_check_the_id_before_touching_the_disk() {
    let env = Env::new().await;
    let url = v1(&env.server).mount(&env.server).await;
    let (instance, desired) = env.instance(&url).await;
    for bad in ["../x", r"..\x"] {
        let err = ensure(&env, bad).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InstanceIdInvalid, "{bad}");
        let err = env.sync(bad, &desired).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::InstanceIdInvalid, "{bad}");
    }
    let err = ensure(&env, "ghost").await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InstanceNotFound);
    let err = env.sync("ghost", &desired).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::InstanceNotFound);
    assert!(!env._tmp.path().join("x").exists(), "nothing made outside");
    let folders: Vec<String> = std::fs::read_dir(env._tmp.path().join("instances"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(folders, vec![instance.id]);
}

#[tokio::test]
async fn an_update_that_fails_to_resolve_plays_the_installed_version() {
    let env = Env::new().await;
    let instance = synced(&env, v1(&env.server)).await;

    env.server.reset().await;
    v1(&env.server)
        .with_version("1.1.0")
        .url_mod("jei", "jei.jar", b"jei", "both", None)
        .corrupt_index()
        .mount(&env.server)
        .await;
    let ensured = ensure(&env, &instance.id).await.unwrap();
    let warning = ensured.warning.unwrap();
    assert_eq!(warning.code, ErrorCode::UpdateCheckFailed);
    assert_eq!(
        warning.fix.as_deref(),
        Some("You can still play the version you have")
    );
    assert!(warning.detail.is_some());
    assert!(done(ensured.outcome).up_to_date);
    assert!(!env.game_file(&instance.id, "mods/jei.jar").exists());
}
