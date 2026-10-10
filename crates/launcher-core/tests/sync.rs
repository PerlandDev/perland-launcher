mod common;

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

use common::{hits, sources, PackBuilder};
use launcher_core::{
    error::{LauncherError, Result},
    events::{Event, EventSink},
    instances::{GameSpec, Instance, InstanceStore, LoaderSpec},
    locks::InstanceLock,
    meta::loaders::LoaderKind,
    sources::{DesiredState, PackInput},
    sync::{lockfile::Managed, BoxFuture, GameInstall, SyncOutcome, SyncReport, Syncer},
    ErrorCode,
};
use wiremock::MockServer;

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
