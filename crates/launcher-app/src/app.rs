//! The app's state and every command except playing (see `runs.rs`).
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use launcher_core::{
    accounts::{AccountStore, AccountsFile, BuildKind},
    instances::{
        is_valid_instance_id, GameSpec, Instance, InstanceListing, InstanceStore, MAX_MEMORY_MB,
        MIN_MEMORY_MB,
    },
    java,
    meta::{
        catalog::{self, LoaderVersion, McVersion},
        loaders::LoaderKind,
        MetaClient,
    },
    mods::{self, LocalMod},
    paths::LauncherDirs,
    settings::{Settings, SettingsStore},
    sync::{manual::Scanner, Cancel, ManualFile},
    ErrorCode, LauncherError, Result,
};
use tokio::{
    sync::{oneshot, Semaphore},
    task::AbortHandle,
};

use crate::{
    dto::{AppInfo, JavaTest, NewInstance, OpenTarget, PackUpdate},
    events::{AppEvent, Host, RunStateKind},
    logs::LogStore,
    runner::{Kill, Runner},
};

pub struct AppConfig {
    pub dirs: LauncherDirs,
    pub build: BuildKind,
    pub launcher_version: String,
    pub system_memory_mb: u64,
    /// Where the player's browser saves files (manual CurseForge downloads).
    pub downloads_dir: Option<PathBuf>,
}

/// The names a Java executable has. Anything else is never run.
const JAVA_NAMES: [&str; 4] = ["java", "javaw", "java.exe", "javaw.exe"];

/// One instance's current run. Absent from `runs` means idle.
pub(crate) struct Run {
    pub(crate) phase: RunStateKind,
    /// Set until the game is running: aborting the task cancels the run.
    pub(crate) abort: Option<AbortHandle>,
    /// Set once the game is running: stopping kills it.
    pub(crate) killer: Option<Arc<dyn Kill>>,
    /// Asks a sync to stop at its next file (Stop while syncing).
    pub(crate) cancel: Cancel,
    /// Set while asking: delivers the player's answer to "Update ready".
    pub(crate) answer: Option<oneshot::Sender<bool>>,
    /// The update on offer while asking, for `run_states` after a reload.
    pub(crate) offer: Option<PackUpdate>,
    /// The banner's Update (no game): a Stop still reports the run as update-only.
    pub(crate) update_only: bool,
}

/// Files the last sync couldn't download, while the player fetches them by hand.
pub(crate) struct ManualState {
    pub(crate) total: usize,
    pub(crate) remaining: Vec<ManualFile>,
    pub(crate) scanner: Scanner,
}

pub struct AppCore<R: Runner, H: Host> {
    pub(crate) config: AppConfig,
    pub(crate) instances: InstanceStore,
    pub(crate) accounts: tokio::sync::Mutex<AccountStore>,
    settings: tokio::sync::Mutex<SettingsStore>,
    settings_warned: AtomicBool,
    accounts_warned: AtomicBool,
    meta: MetaClient,
    pub(crate) runner: Arc<R>,
    pub(crate) host: Arc<H>,
    /// One install at a time, so two installs never write the same shared files at once.
    pub(crate) install_slot: Arc<Semaphore>,
    pub(crate) runs: Mutex<HashMap<String, Run>>,
    pub(crate) logs: LogStore,
    /// Per instance: the files its last sync couldn't download.
    pub(crate) manual: tokio::sync::Mutex<HashMap<String, ManualState>>,
    /// A valid `perland://` pack link waiting for the UI to take it.
    pub(crate) pending_link: Mutex<Option<String>>,
    /// Install plans the webview may install by id (see `browse.rs`).
    pub(crate) plans: Mutex<HashMap<String, crate::browse::StoredPlan>>,
    pub(crate) plan_seq: AtomicU64,
    /// Instances a content install is writing into. Lock order: `runs`, then this.
    pub(crate) installing: Mutex<HashSet<String>>,
}

fn busy(id: &str) -> LauncherError {
    LauncherError::game(
        ErrorCode::InstanceBusy,
        "Stop the game before changing this instance",
    )
    .with_param("id", id)
}

fn valid_id(id: &str) -> Result<()> {
    if is_valid_instance_id(id) {
        Ok(())
    } else {
        Err(LauncherError::disk(
            ErrorCode::InstanceIdInvalid,
            "That instance id isn't valid",
        ))
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    pub fn new(config: AppConfig, meta: MetaClient, runner: Arc<R>, host: Arc<H>) -> Arc<Self> {
        let root = config.dirs.root().to_path_buf();
        Arc::new(Self {
            instances: InstanceStore::new(config.dirs.instances()),
            accounts: tokio::sync::Mutex::new(AccountStore::new(root.join("accounts.json"))),
            settings: tokio::sync::Mutex::new(SettingsStore::new(root.join("settings.json"))),
            settings_warned: AtomicBool::new(false),
            accounts_warned: AtomicBool::new(false),
            meta,
            runner,
            host,
            install_slot: Arc::new(Semaphore::new(1)),
            runs: Mutex::new(HashMap::new()),
            logs: LogStore::default(),
            manual: tokio::sync::Mutex::new(HashMap::new()),
            pending_link: Mutex::new(None),
            plans: Mutex::new(HashMap::new()),
            plan_seq: AtomicU64::new(0),
            installing: Mutex::new(HashSet::new()),
            config,
        })
    }

    pub(crate) fn is_busy(&self, id: &str) -> bool {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    }

    /// A damaged accounts file doesn't fail this: Settings needs it to offer "Open data folder",
    /// which is the fix. Offline then stays locked in release builds, and the damage is reported
    /// once as a warning (without the file's contents).
    pub async fn app_info(&self) -> Result<AppInfo> {
        let licensed_signin_seen = match self.accounts.lock().await.list().await {
            Ok(accounts) => accounts.licensed_signin_seen,
            Err(mut error) => {
                error.detail = None;
                if !self.accounts_warned.swap(true, Ordering::SeqCst) {
                    self.host.emit(AppEvent::Warning {
                        instance: None,
                        error,
                    });
                }
                false
            }
        };
        Ok(AppInfo {
            version: self.config.launcher_version.clone(),
            build: self.config.build.into(),
            offline_allowed: launcher_core::accounts::offline_allowed(
                self.config.build,
                licensed_signin_seen,
            ),
            system_memory_mb: self.config.system_memory_mb,
            data_dir: self.config.dirs.root().display().to_string(),
        })
    }

    /// The saved settings, or the defaults. A damaged file is reported once as a warning.
    pub async fn settings_get(&self) -> Settings {
        let (settings, warning) = self.settings.lock().await.load().await;
        if let Some(error) = warning {
            if !self.settings_warned.swap(true, Ordering::SeqCst) {
                self.host.emit(AppEvent::Warning {
                    instance: None,
                    error,
                });
            }
        }
        settings
    }

    pub async fn settings_set(&self, settings: Settings) -> Result<Settings> {
        self.settings.lock().await.save(&settings).await?;
        Ok(settings)
    }

    pub async fn instances_list(&self) -> Result<InstanceListing> {
        self.instances.list_all().await
    }

    pub async fn instance_create(&self, request: NewInstance) -> Result<Instance> {
        // A hand-edited settings file can hold any number; clamp so create + update can't
        // half-succeed and leave an instance behind.
        let memory = self
            .settings_get()
            .await
            .default_memory_mb
            .clamp(MIN_MEMORY_MB, MAX_MEMORY_MB);
        let mut instance = self
            .instances
            .create(
                &request.name,
                GameSpec {
                    minecraft: request.minecraft,
                    loader: request.loader,
                },
            )
            .await?;
        if instance.java.memory_mb != memory {
            instance.java.memory_mb = memory;
            instance = self.instances.update(&instance).await?;
        }
        Ok(instance)
    }

    /// Version and Java changes wait until the game stops; other edits are fine while it runs.
    /// The source is sync's to change: a settings form loaded before a sync can't revert it.
    pub async fn instance_update(&self, mut instance: Instance) -> Result<Instance> {
        let current = self.instances.load(&instance.id).await?;
        if self.is_busy(&instance.id)
            && (current.game != instance.game || current.java != instance.java)
        {
            return Err(busy(&instance.id));
        }
        instance.source = current.source;
        self.instances.update(&instance).await
    }

    pub async fn instance_rename(&self, id: &str, name: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        self.instances.rename(id, name).await
    }

    pub async fn instance_delete(&self, id: &str) -> Result<()> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        self.instances.delete(id).await
    }

    pub async fn catalog_minecraft(&self) -> Result<Vec<McVersion>> {
        catalog::minecraft_versions(&self.meta).await
    }

    pub async fn catalog_loaders(&self, kind: LoaderKind, mc: &str) -> Result<Vec<LoaderVersion>> {
        catalog::loader_versions(&self.meta, kind, mc).await
    }

    pub async fn accounts_list(&self) -> Result<AccountsFile> {
        self.accounts.lock().await.list().await
    }

    pub async fn account_add_offline(&self, name: &str) -> Result<AccountsFile> {
        let store = self.accounts.lock().await;
        store.add_offline(name, self.config.build).await?;
        store.list().await
    }

    pub async fn account_set_active(&self, uuid: &str) -> Result<AccountsFile> {
        self.accounts.lock().await.set_active(uuid).await
    }

    pub async fn account_remove(&self, uuid: &str) -> Result<AccountsFile> {
        self.accounts.lock().await.remove(uuid).await
    }

    async fn game_dir(&self, id: &str) -> Result<PathBuf> {
        self.instances.load(id).await?;
        Ok(self.instances.game_dir(id))
    }

    pub async fn mods_list(&self, id: &str) -> Result<Vec<LocalMod>> {
        mods::list(&self.game_dir(id).await?).await
    }

    pub async fn mod_set_enabled(
        &self,
        id: &str,
        file: &str,
        enabled: bool,
    ) -> Result<Vec<LocalMod>> {
        // The game holds its jars open; renaming one fails (or silently misses) while it runs.
        if self.is_busy(id) {
            return Err(busy(id));
        }
        self.refuse_managed(id, file).await?;
        let dir = self.game_dir(id).await?;
        mods::set_enabled(&dir, file, enabled).await?;
        mods::list(&dir).await
    }

    pub async fn mods_add(&self, id: &str, paths: Vec<PathBuf>) -> Result<Vec<LocalMod>> {
        let dir = self.game_dir(id).await?;
        for path in &paths {
            mods::add(&dir, path).await?;
        }
        mods::list(&dir).await
    }

    pub async fn mod_remove(&self, id: &str, file: &str) -> Result<Vec<LocalMod>> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        self.refuse_managed(id, file).await?;
        let dir = self.game_dir(id).await?;
        mods::remove(&dir, file).await?;
        mods::list(&dir).await
    }

    /// The instance folder, or `InstanceNotFound`. Works for damaged instances too.
    async fn existing_instance_dir(&self, id: &str) -> Result<PathBuf> {
        valid_id(id)?;
        let dir = self.instances.instance_dir(id);
        if !tokio::fs::try_exists(&dir).await? {
            return Err(LauncherError::disk(
                ErrorCode::InstanceNotFound,
                format!("Instance \"{id}\" wasn't found"),
            )
            .with_param("id", id));
        }
        Ok(dir)
    }

    /// A folder to open in the file manager. Works for damaged instances too.
    pub async fn folder(&self, target: OpenTarget) -> Result<PathBuf> {
        let path = match target {
            OpenTarget::Instance { id } => return self.existing_instance_dir(&id).await,
            OpenTarget::Mods { id } => {
                self.existing_instance_dir(&id).await?;
                self.instances.game_dir(&id).join("mods")
            }
            OpenTarget::Data => self.config.dirs.root().to_path_buf(),
            OpenTarget::Logs => self.config.dirs.logs(),
            OpenTarget::Downloads => self.config.downloads_dir.clone().ok_or_else(|| {
                LauncherError::disk(ErrorCode::Internal, "Couldn't find your Downloads folder")
            })?,
        };
        tokio::fs::create_dir_all(&path).await?;
        Ok(path)
    }

    /// Run `<path> -version`, but only for a regular file named like Java: the path comes
    /// from the webview.
    pub async fn java_test(&self, path: PathBuf) -> Result<JavaTest> {
        let named_java = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| JAVA_NAMES.iter().any(|j| n.eq_ignore_ascii_case(j)));
        let is_file = tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_file());
        if !named_java || !is_file {
            return Err(LauncherError::java(
                ErrorCode::JavaTestFailed,
                "That doesn't look like a working Java",
            )
            .with_fix("Pick the java executable inside a Java installation's bin folder")
            .with_detail(path.display()));
        }
        Ok(JavaTest {
            major: java::probe(&path).await?,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        dto::Build,
        testing::{FakeRunner, RecordingHost},
    };
    use launcher_core::{instances::LoaderSpec, meta::loaders::LoaderKind, ErrorCode};

    pub(crate) type TestCore = AppCore<FakeRunner, RecordingHost>;

    pub(crate) fn core(
        build: BuildKind,
    ) -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = LauncherDirs::new(tmp.path());
        let runner = FakeRunner::new();
        *runner.instances_dir.lock().unwrap() = Some(dirs.instances());
        let host = RecordingHost::new();
        let meta = MetaClient::new(launcher_core::http::client("test").unwrap(), dirs.meta());
        let core = AppCore::new(
            AppConfig {
                dirs,
                build,
                launcher_version: "0.1.0".into(),
                system_memory_mb: 16384,
                downloads_dir: Some(tmp.path().join("Downloads")),
            },
            meta,
            runner.clone(),
            host.clone(),
        );
        (core, runner, host, tmp)
    }

    fn vanilla(name: &str) -> NewInstance {
        NewInstance {
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: None,
        }
    }

    #[tokio::test]
    async fn app_info_reflects_build_and_offline_gate() {
        let (dev, _, _, _t1) = core(BuildKind::Dev);
        let info = dev.app_info().await.unwrap();
        assert!(info.offline_allowed);
        assert_eq!(info.build, Build::Dev);
        assert_eq!(info.system_memory_mb, 16384);
        let (release, _, _, _t2) = core(BuildKind::Release);
        assert!(!release.app_info().await.unwrap().offline_allowed);
        assert_eq!(
            release.account_add_offline("Steve").await.unwrap_err().code,
            ErrorCode::OfflineLocked
        );
    }

    #[tokio::test]
    async fn create_uses_the_default_memory_and_accepts_forge() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.settings_set(Settings {
            default_memory_mb: 6144,
            ..Settings::default()
        })
        .await
        .unwrap();
        let created = core.instance_create(vanilla("Pack")).await.unwrap();
        assert_eq!(created.java.memory_mb, 6144);
        let forge = NewInstance {
            loader: Some(LoaderSpec {
                kind: LoaderKind::Forge,
                version: "47.2.0".into(),
            }),
            ..vanilla("Forge")
        };
        let created_forge = core.instance_create(forge).await.unwrap();
        assert_eq!(created_forge.game.loader.unwrap().kind, LoaderKind::Forge);
        assert_eq!(core.instances_list().await.unwrap().instances.len(), 2);
    }

    #[tokio::test]
    async fn app_info_survives_a_damaged_accounts_file() {
        let (core, _, host, tmp) = core(BuildKind::Release);
        std::fs::write(tmp.path().join("accounts.json"), "{nope").unwrap();
        let info = core.app_info().await.unwrap();
        assert!(!info.offline_allowed);
        assert_eq!(info.system_memory_mb, 16384);
        core.app_info().await.unwrap();
        assert_eq!(host.warnings(), vec![ErrorCode::AccountsDamaged]);
        // The warning never carries the file's contents.
        let events = host.events.lock().unwrap();
        assert!(events.iter().all(|e| match e {
            AppEvent::Warning { error, .. } => error.detail.is_none(),
            _ => true,
        }));
    }

    #[tokio::test]
    async fn create_clamps_an_out_of_range_default_memory() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        for (raw, expected) in [(100_000, MAX_MEMORY_MB), (64, MIN_MEMORY_MB)] {
            std::fs::write(
                tmp.path().join("settings.json"),
                format!("{{\"defaultMemoryMb\": {raw}}}"),
            )
            .unwrap();
            let created = core
                .instance_create(vanilla(&format!("Pack {raw}")))
                .await
                .unwrap();
            assert_eq!(created.java.memory_mb, expected);
        }
        assert_eq!(core.instances_list().await.unwrap().instances.len(), 2);
    }

    #[tokio::test]
    async fn damaged_settings_warn_once() {
        let (core, _, host, tmp) = core(BuildKind::Dev);
        std::fs::write(tmp.path().join("settings.json"), "{nope").unwrap();
        assert_eq!(core.settings_get().await, Settings::default());
        core.settings_get().await;
        assert_eq!(host.warnings(), vec![ErrorCode::SettingsDamaged]);
    }

    #[tokio::test]
    async fn folders_resolve_inside_the_data_root() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        let mods = core
            .folder(OpenTarget::Mods {
                id: pack.id.clone(),
            })
            .await
            .unwrap();
        assert!(mods.is_dir() && mods.starts_with(tmp.path()));
        assert!(core.folder(OpenTarget::Logs).await.unwrap().is_dir());
        assert!(core.folder(OpenTarget::Data).await.unwrap().is_dir());
        assert_eq!(
            core.folder(OpenTarget::Instance { id: "gone".into() })
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceNotFound
        );
        assert_eq!(
            core.folder(OpenTarget::Mods { id: "../x".into() })
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceIdInvalid
        );
    }

    #[tokio::test]
    async fn mods_folder_of_a_missing_instance_is_not_created() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        assert_eq!(
            core.folder(OpenTarget::Mods { id: "ghost".into() })
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceNotFound
        );
        assert!(!tmp.path().join("instances").join("ghost").exists());
    }

    #[tokio::test]
    async fn mods_round_trip_through_the_core() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        let src = tmp.path().join("sodium.jar");
        std::fs::write(&src, b"jar").unwrap();
        let list = core.mods_add(&pack.id, vec![src]).await.unwrap();
        assert_eq!(list.len(), 1);
        let list = core
            .mod_set_enabled(&pack.id, "sodium.jar", false)
            .await
            .unwrap();
        assert_eq!(list[0].file_name, "sodium.jar.disabled");
        let list = core
            .mod_remove(&pack.id, "sodium.jar.disabled")
            .await
            .unwrap();
        assert!(list.is_empty());
    }

    #[tokio::test]
    async fn accounts_add_switch_remove() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let steve = core.account_add_offline("Steve").await.unwrap();
        let file = core.account_add_offline("Alex").await.unwrap();
        assert_eq!(file.accounts.len(), 2);
        let steve_uuid = steve.accounts[0].uuid.clone();
        let file = core.account_set_active(&steve_uuid).await.unwrap();
        assert_eq!(file.active.as_deref(), Some(steve_uuid.as_str()));
        let file = core.account_remove(&steve_uuid).await.unwrap();
        assert_eq!(file.accounts.len(), 1);
    }

    #[tokio::test]
    async fn java_test_reports_failure() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        assert_eq!(
            core.java_test(tmp.path().join("nope"))
                .await
                .unwrap_err()
                .code,
            ErrorCode::JavaTestFailed
        );
    }
}
