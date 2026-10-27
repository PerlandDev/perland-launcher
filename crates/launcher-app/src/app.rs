//! The app's state and every command except playing (see `runs.rs`).
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Instant,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use launcher_core::{
    accounts::{visible, AccountStore, AccountsFile, BuildKind},
    browse::{compat::ContentKind, images::ImageKind},
    content,
    diagnostics::{self, SystemInfo},
    fsutil::path_key,
    instance_order::{apply_order, keep_in_place, rename_in_order, OrderStore},
    instances::{
        is_valid_instance_id, GameSpec, Instance, InstanceListing, InstanceStore, SourceRef,
        UpdatePolicy, MAX_MEMORY_MB, MIN_MEMORY_MB,
    },
    java,
    meta::{
        catalog::{self, LoaderVersion, McVersion},
        loaders::LoaderKind,
        MetaClient,
    },
    mod_icons,
    mods::{self, LocalMod},
    msa::{minecraft::McToken, secrets::KeyedSecrets},
    news_seen::{NewsSeen, NEWS_SEEN_FILE},
    paths::LauncherDirs,
    perland::{KeyringStore, SecretStore, SessionStore},
    platform::Platform,
    playtime,
    restore::{self, Change, ChangeCtx},
    settings::{Channel, Settings, SettingsStore},
    sync::{lockfile::Managed, manual::Scanner, Cancel, ManualFile},
    ErrorCode, LauncherError, Result,
};
use tokio::{
    sync::{oneshot, watch, Semaphore},
    task::AbortHandle,
};

use crate::{
    deep_link::DeepLink,
    dto::{
        AppInfo, ContentFile, JavaFound, JavaOverview, NewInstance, OpenTarget, PackUpdate,
        PlaytimeEntry,
    },
    events::{AppEvent, Host, RunStateKind},
    logs::LogStore,
    perland::{PerlandState, SignInSlot, ACCOUNT_MARKER},
    restore::{lock_or_busy, now_secs},
    runner::{Kill, Runner},
};

pub struct AppConfig {
    pub dirs: LauncherDirs,
    pub build: BuildKind,
    pub launcher_version: String,
    pub system_memory_mb: u64,
    /// Where the player's browser saves files (manual CurseForge downloads).
    pub downloads_dir: Option<PathBuf>,
    /// The Perland panel (`launcher_core::perland::panel_url`), where the player manages the
    /// servers they own. Without one, Manage in Perland isn't offered.
    pub panel: Option<launcher_core::invite::Url>,
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
    /// What this run's sync changed, set before a wait that a Stop aborts (`waking`), so the
    /// cancelled run still reports it.
    pub(crate) synced: Option<crate::dto::SyncSummary>,
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
    /// The player's instance order. Held across every read-modify-write of the stored order.
    order: tokio::sync::Mutex<OrderStore>,
    settings_warned: AtomicBool,
    accounts_warned: AtomicBool,
    meta: MetaClient,
    /// The newest news post seen per server (`<data root>/news-seen.json`).
    pub(crate) news_seen: NewsSeen,
    pub(crate) runner: Arc<R>,
    pub(crate) host: Arc<H>,
    /// One install at a time, so two installs never write the same shared files at once.
    pub(crate) install_slot: Arc<Semaphore>,
    pub(crate) runs: Mutex<HashMap<String, Run>>,
    pub(crate) logs: LogStore,
    /// Per instance: the files its last sync couldn't download.
    pub(crate) manual: tokio::sync::Mutex<HashMap<String, ManualState>>,
    /// A valid `perland://` pack link waiting for the UI to take it.
    pub(crate) pending_link: Mutex<Option<DeepLink>>,
    /// Install plans the webview may install by id (see `browse.rs`).
    pub(crate) plans: Mutex<HashMap<String, crate::browse::StoredPlan>>,
    pub(crate) plan_seq: AtomicU64,
    /// Instances a content install is writing into. Lock order: `runs`, then this.
    pub(crate) installing: Mutex<HashSet<String>>,
    /// The file the last diagnostics export wrote (for "Show in folder").
    last_export: Mutex<Option<PathBuf>>,
    /// The file the last share wrote (for "Show in folder").
    pub(crate) last_share: Mutex<Option<PathBuf>>,
    /// Per instance: the plan the Share dialog showed, taken by the export that follows.
    pub(crate) share_plans: Mutex<HashMap<String, launcher_core::share::SharePlan>>,
    /// The running import from another launcher, if any (see `import.rs`): one at a time.
    pub(crate) import_slot: Mutex<Option<Cancel>>,
    /// Per running game: when it started, until its time is counted (see `runs.rs`).
    pub(crate) playing_since: Mutex<HashMap<String, Instant>>,
    /// The Perland account (see `perland.rs`), read from `perland_store` on first use.
    pub(crate) perland: tokio::sync::Mutex<PerlandState>,
    /// The marker file plus the credential store. Only `perland.rs` touches it, and only
    /// after a Perland command, never at start.
    pub(crate) perland_store: Arc<SessionStore>,
    /// The Discord sign-in waiting for the browser, if any.
    pub(crate) perland_sign_in: Mutex<SignInSlot>,
    /// Microsoft refresh tokens, one per account (see `msa.rs`).
    pub(crate) msa_secrets: Arc<dyn KeyedSecrets>,
    /// Per Microsoft account (by dashed uuid): its Minecraft token and xuid, in memory only.
    pub(crate) msa_tokens: Mutex<HashMap<String, (McToken, Option<String>)>>,
    /// Held across a refresh of the Minecraft token, so launches at once share one.
    pub(crate) msa_refresh: tokio::sync::Mutex<()>,
    /// The Microsoft sign-in waiting for the browser, if any: its cancel switch.
    pub(crate) msa_sign_in: Mutex<Option<watch::Sender<bool>>>,
    /// Session JWTs for Manage (see `manage.rs`), cleared whenever the session is forgotten or
    /// replaced.
    pub(crate) jwts: launcher_core::perland::JwtCache,
    /// The one console log being followed (see `manage_console.rs`).
    pub(crate) console_stream: Mutex<Option<crate::manage_console::ConsoleSlot>>,
    /// How many times each server's console was closed (see `manage_console.rs`): an open
    /// still waiting when its server's count moves was closed meanwhile, and starts nothing.
    pub(crate) console_closes: Mutex<crate::manage_console::CloseCounts>,
    /// Makes the next import fail while it moves the copied files into the new instance.
    #[cfg(test)]
    pub(crate) import_fail_move: AtomicBool,
    /// The free space an import sees, in bytes, instead of the disk's.
    #[cfg(test)]
    pub(crate) import_free_bytes: Mutex<Option<u64>>,
    /// Seconds the run clock is ahead of the real one, so a test can play a long game at once.
    #[cfg(test)]
    pub(crate) clock_ahead: AtomicU64,
}

pub(crate) fn busy(id: &str) -> LauncherError {
    LauncherError::game(
        ErrorCode::InstanceBusy,
        "Stop the game before changing this instance",
    )
    .with_param("id", id)
}

fn damaged_ids(listing: &InstanceListing) -> HashSet<&str> {
    listing.damaged.iter().map(|d| d.id.as_str()).collect()
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
    /// `msa_secrets` keeps Microsoft refresh tokens (the OS credential store in the app).
    pub fn new(
        config: AppConfig,
        meta: MetaClient,
        runner: Arc<R>,
        host: Arc<H>,
        msa_secrets: Arc<dyn KeyedSecrets>,
    ) -> Arc<Self> {
        Self::with_secret_store(
            config,
            meta,
            runner,
            host,
            Arc::new(KeyringStore::new()),
            msa_secrets,
        )
    }

    /// Like `new`, with the Perland token kept in `secret` instead of the OS credential store.
    /// Building it reads nothing: the store is first read by a Perland command.
    pub fn with_secret_store(
        config: AppConfig,
        meta: MetaClient,
        runner: Arc<R>,
        host: Arc<H>,
        secret: Arc<dyn SecretStore>,
        msa_secrets: Arc<dyn KeyedSecrets>,
    ) -> Arc<Self> {
        let root = config.dirs.root().to_path_buf();
        Arc::new(Self {
            instances: InstanceStore::new(config.dirs.instances()),
            accounts: tokio::sync::Mutex::new(AccountStore::new(root.join("accounts.json"))),
            settings: tokio::sync::Mutex::new(SettingsStore::new(root.join("settings.json"))),
            order: tokio::sync::Mutex::new(OrderStore::new(config.dirs.instance_order())),
            settings_warned: AtomicBool::new(false),
            accounts_warned: AtomicBool::new(false),
            meta,
            news_seen: NewsSeen::new(root.join(NEWS_SEEN_FILE)),
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
            last_export: Mutex::new(None),
            last_share: Mutex::new(None),
            share_plans: Mutex::new(HashMap::new()),
            import_slot: Mutex::new(None),
            playing_since: Mutex::new(HashMap::new()),
            perland: tokio::sync::Mutex::new(PerlandState::default()),
            perland_store: Arc::new(SessionStore::new(root.join(ACCOUNT_MARKER), secret)),
            perland_sign_in: Mutex::new(SignInSlot::default()),
            msa_secrets,
            msa_tokens: Mutex::new(HashMap::new()),
            msa_refresh: tokio::sync::Mutex::new(()),
            msa_sign_in: Mutex::new(None),
            jwts: launcher_core::perland::JwtCache::default(),
            console_stream: Mutex::new(None),
            console_closes: Mutex::new(Default::default()),
            #[cfg(test)]
            import_fail_move: AtomicBool::new(false),
            #[cfg(test)]
            import_free_bytes: Mutex::new(None),
            #[cfg(test)]
            clock_ahead: AtomicU64::new(0),
            config,
        })
    }

    pub(crate) fn is_busy(&self, id: &str) -> bool {
        self.runs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    }

    /// Is a content install writing into this instance? (Not a run: see `browse.rs`.)
    pub(crate) fn is_installing(&self, id: &str) -> bool {
        self.installing
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(id)
    }

    /// Restart now and install-on-quit wait while a game runs, an install holds the slot, a
    /// content install writes into an instance, or an import is copying.
    pub fn update_blocked(&self) -> bool {
        self.has_active_runs()
            || self.install_slot.available_permits() == 0
            || self
                .import_slot
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_some()
            || !self
                .installing
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .is_empty()
    }

    /// A damaged accounts file doesn't fail this: Settings needs it to offer "Open data folder",
    /// which is the fix. The damage is reported once as a warning (without the file's contents).
    pub async fn app_info(&self) -> Result<AppInfo> {
        if let Err(mut error) = self.accounts.lock().await.list().await {
            error.detail = None;
            if !self.accounts_warned.swap(true, Ordering::SeqCst) {
                self.host.emit(AppEvent::Warning {
                    instance: None,
                    error,
                });
            }
        }
        Ok(AppInfo {
            version: self.config.launcher_version.clone(),
            build: self.config.build.into(),
            offline_allowed: launcher_core::accounts::offline_allowed(self.config.build),
            system_memory_mb: self.config.system_memory_mb,
            data_dir: self.config.dirs.root().display().to_string(),
            panel: self.config.panel.is_some(),
        })
    }

    /// Export diagnostics to `out` (a `.zip` the player picked in the save dialog).
    /// A name with no extension at all gets `.zip` (some save dialogs return the typed name as is);
    /// any other extension is refused.
    pub async fn diagnostics_export(&self, out: PathBuf, os: String) -> Result<PathBuf> {
        let out = if out.extension().is_none() && out.file_name().is_some() {
            out.with_extension("zip")
        } else {
            out
        };
        if !out
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        {
            return Err(LauncherError::disk(
                ErrorCode::DiagnosticsPathInvalid,
                "Diagnostics are saved as a .zip file",
            )
            .with_fix("Pick a file name that ends in .zip"));
        }
        let channel =
            Channel::effective(self.settings_get().await.update_channel, self.config.build);
        let info = SystemInfo {
            launcher_version: self.config.launcher_version.clone(),
            build: format!("{:?}", self.config.build).to_lowercase(),
            update_channel: format!("{channel:?}").to_lowercase(),
            os,
            arch: std::env::consts::ARCH.into(),
            total_memory_mb: self.config.system_memory_mb,
        };
        diagnostics::export(&self.config.dirs, &info, &out).await?;
        *self.last_export.lock().unwrap_or_else(|e| e.into_inner()) = Some(out.clone());
        Ok(out)
    }

    /// The file the last export wrote, for "Show in folder" (never a path from the webview).
    pub fn diagnostics_last(&self) -> Option<PathBuf> {
        self.last_export
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
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

    /// The update channel to follow, read without the warn-once path: startup reads this before
    /// the UI listens, so a damaged file's warning stays for the UI's first `settings_get`.
    pub async fn update_channel(&self) -> Channel {
        let (settings, _) = self.settings.lock().await.load().await;
        Channel::effective(settings.update_channel, self.config.build)
    }

    pub async fn settings_set(&self, settings: Settings) -> Result<Settings> {
        self.settings.lock().await.save(&settings).await?;
        Ok(settings)
    }

    /// The instances in the player's order: ones not in it yet on top (A-Z), then the stored order.
    /// Damaged ones stay last. The stored order is rewritten when this changes it, keeping damaged
    /// instances' ids in their places so a repaired one comes back where it was. An order file
    /// that can't be read right now is left alone.
    pub async fn instances_list(&self) -> Result<InstanceListing> {
        let order = self.order.lock().await;
        let mut listing = self.instances.list_all().await?;
        let Some(stored) = order.load().await else {
            apply_order(&mut listing.instances, &[]);
            return Ok(listing);
        };
        let ordered = apply_order(&mut listing.instances, &stored);
        let ordered = keep_in_place(&ordered, &stored, &damaged_ids(&listing));
        if ordered != stored {
            // Only a convenience: a failed save shows the same listing next time.
            let _ = order.save(&ordered).await;
        }
        Ok(listing)
    }

    /// Save the player's drag order and return the listing in it. Damaged instances (which the
    /// player can't drag) keep their stored places.
    pub async fn instance_reorder(&self, ids: Vec<String>) -> Result<InstanceListing> {
        ids.iter().try_for_each(|id| valid_id(id))?;
        let order = self.order.lock().await;
        let mut listing = self.instances.list_all().await?;
        let stored = order.load().await.unwrap_or_default();
        let ordered = apply_order(&mut listing.instances, &ids);
        order
            .save(&keep_in_place(&ordered, &stored, &damaged_ids(&listing)))
            .await?;
        Ok(listing)
    }

    /// The memory a new instance gets. A hand-edited settings file can hold any number; clamp
    /// so create + update can't half-succeed and leave an instance behind.
    pub(crate) async fn default_memory_mb(&self) -> u32 {
        self.settings_get()
            .await
            .default_memory_mb
            .clamp(MIN_MEMORY_MB, MAX_MEMORY_MB)
    }

    pub async fn instance_create(&self, request: NewInstance) -> Result<Instance> {
        let memory = self.default_memory_mb().await;
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
    /// Where an instance comes from (`source`), its server link (`server`) and its pictures
    /// (`art`, `art_seed`, `icon`, `banner_focus`) change only through their own commands, so a stale copy from
    /// the UI can't undo a relink or a picture. Server instances
    /// always follow their server (`Auto`).
    pub async fn instance_update(&self, instance: Instance) -> Result<Instance> {
        let current = self.instances.load(&instance.id).await?;
        if self.is_busy(&instance.id)
            && (current.game != instance.game || current.java != instance.java)
        {
            return Err(busy(&instance.id));
        }
        // An install in flight was planned for this game; it must not land in another one.
        if self.is_installing(&instance.id)
            && (current.game != instance.game || current.java != instance.java)
        {
            return Err(crate::browse::content_installing());
        }
        let mut instance = instance;
        instance.source = current.source.clone();
        instance.server = current.server.clone();
        instance.art = current.art;
        instance.art_seed = current.art_seed.clone();
        instance.icon = current.icon;
        instance.banner_focus = current.banner_focus;
        // Only going back and "Update again" set or lift the hold.
        instance.update_hold = current.update_hold.clone();
        if matches!(current.source, SourceRef::Perland { .. }) {
            instance.update_policy = UpdatePolicy::Auto;
        }
        self.instances.update(&instance).await
    }

    /// Refused while the game runs or a download writes into the instance. A rename that moves
    /// the folder changes the id, so everything keyed by the old id follows (or is dropped).
    pub async fn instance_rename(&self, id: &str, name: &str) -> Result<Instance> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        let order = self.order.lock().await;
        let renamed = self.instances.rename(id, name).await?;
        if renamed.id != id {
            // The rename already happened; an unreadable order or a failed save only loses the
            // place in the list.
            if let Some(mut ids) = order.load().await {
                rename_in_order(&mut ids, id, &renamed.id);
                let _ = order.save(&ids).await;
            }
            let logs = self.config.dirs.logs();
            let _ = tokio::fs::rename(
                logs.join(format!("{id}-latest.log")),
                logs.join(format!("{}-latest.log", renamed.id)),
            )
            .await;
            self.logs.rename(id, &renamed.id);
            self.manual.lock().await.remove(id);
            self.plans
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .retain(|_, plan| !plan.targets(id));
        }
        Ok(renamed)
    }

    /// Time played in every listed instance, zeros included.
    pub async fn playtime_list(&self) -> Result<Vec<PlaytimeEntry>> {
        let mut out = Vec::new();
        for instance in self.instances.list().await? {
            let played = playtime::read(&self.instances.perland_dir(&instance.id)).await;
            out.push(PlaytimeEntry {
                id: instance.id,
                seconds: played.seconds,
                last_played: played.last_played,
            });
        }
        Ok(out)
    }

    pub async fn instance_delete(&self, id: &str) -> Result<()> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        self.instances.delete(id).await
    }

    pub async fn catalog_minecraft(&self) -> Result<Vec<McVersion>> {
        catalog::minecraft_versions(&self.meta).await
    }

    pub async fn catalog_loaders(&self, kind: LoaderKind, mc: &str) -> Result<Vec<LoaderVersion>> {
        catalog::loader_versions(&self.meta, kind, mc).await
    }

    /// The accounts the player may see: outside dev builds, offline ones are left out.
    pub async fn accounts_list(&self) -> Result<AccountsFile> {
        let file = self.accounts.lock().await.list().await?;
        Ok(visible(file, self.config.build))
    }

    /// The active account as the player sees it (never an offline one outside dev builds).
    pub(crate) async fn active_account(&self) -> Result<Option<launcher_core::accounts::Account>> {
        let file = self.accounts_list().await?;
        Ok(file
            .active
            .and_then(|uuid| file.accounts.into_iter().find(|a| a.uuid == uuid)))
    }

    pub async fn account_add_offline(&self, name: &str) -> Result<AccountsFile> {
        let store = self.accounts.lock().await;
        store.add_offline(name, self.config.build).await?;
        Ok(visible(store.list().await?, self.config.build))
    }

    /// Only an account the player may see can become active (never an offline one outside
    /// dev builds).
    pub async fn account_set_active(&self, uuid: &str) -> Result<AccountsFile> {
        let store = self.accounts.lock().await;
        let shown = visible(store.list().await?, self.config.build);
        if !shown.accounts.iter().any(|a| a.uuid == uuid) {
            return Err(LauncherError::auth(
                ErrorCode::NoAccount,
                "That account isn't on this computer anymore",
            ));
        }
        let file = store.set_active(uuid).await?;
        Ok(visible(file, self.config.build))
    }

    /// Forget an account, with its Microsoft sign-in.
    pub async fn account_remove(&self, uuid: &str) -> Result<AccountsFile> {
        let file = self.accounts.lock().await.remove(uuid).await?;
        self.forget_microsoft(uuid).await;
        Ok(visible(file, self.config.build))
    }

    async fn game_dir(&self, id: &str) -> Result<PathBuf> {
        self.instances.load(id).await?;
        Ok(self.instances.game_dir(id))
    }

    pub async fn mods_list(&self, id: &str) -> Result<Vec<LocalMod>> {
        mods::list(&self.game_dir(id).await?).await
    }

    /// A mod's icon as a `data:` URL (the CSP allows no file URLs), or `None` for the placeholder:
    /// the jar's own, else its Modrinth project's when Modrinth knows the jar.
    pub async fn mod_icon(&self, id: &str, file: &str) -> Result<Option<String>> {
        let dir = self.game_dir(id).await?;
        let cache = self.config.dirs.mod_icons();
        if let Some(png) = mod_icons::icon(&dir, &cache, file).await? {
            return Ok(Some(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(png)
            )));
        }
        let browser = self.runner.browser();
        let lookup = |sha1| browser.project_icon_url(sha1);
        // Offline, rate limited or refused: the placeholder now, and asked again next time.
        let url = match mod_icons::project_icon_url(&dir, &cache, file, lookup).await {
            Ok(Some(url)) => url,
            Ok(None) => return Ok(None),
            Err(error) => {
                tracing::debug!(code = ?error.code, "no Modrinth icon for a mod");
                return Ok(None);
            }
        };
        Ok(browser.image(url, ImageKind::Icon).await.ok().map(|image| {
            format!(
                "data:{};base64,{}",
                image.mime,
                STANDARD.encode(image.bytes)
            )
        }))
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

    /// Refused while the game runs, an install writes into the instance or another process
    /// holds it. The restore point keeps the jar, so the removal can be undone.
    pub async fn mod_remove(&self, id: &str, file: &str) -> Result<Vec<LocalMod>> {
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        self.refuse_managed(id, file).await?;
        let dir = self.game_dir(id).await?;
        let _lock = lock_or_busy(&self.instances.perland_dir(id), id)?;
        self.before_removal(id, "mods", file).await;
        mods::remove(&dir, file).await?;
        mods::list(&dir).await
    }

    /// Keep a restore point before `file` leaves `folder` (the caller holds the lock).
    async fn before_removal(&self, id: &str, folder: &str, file: &str) {
        restore::before_change(
            &self.instances,
            id,
            &[folder.to_string()],
            Change::Removed {
                name: file.to_string(),
            },
            ChangeCtx {
                during_run: false,
                now: now_secs(),
            },
        )
        .await;
    }

    /// The resource packs or shaders in the instance. Those the pack's lockfile owns are flagged
    /// `managed` (compared as the file system does: case-blind).
    pub async fn content_list(&self, id: &str, kind: ContentKind) -> Result<Vec<ContentFile>> {
        let dir = self.game_dir(id).await?;
        let folder = kind.folder().unwrap_or_default();
        let owned: HashSet<String> = Managed::load(&self.instances.perland_dir(id))
            .await
            .map(|m| m.files.keys().map(|k| path_key(k)).collect())
            .unwrap_or_default();
        Ok(content::list(&dir, kind)
            .await?
            .into_iter()
            .map(|p| ContentFile {
                managed: owned.contains(&path_key(&format!("{folder}/{}", p.file_name))),
                file_name: p.file_name,
                size: p.size,
                folder: p.folder,
            })
            .collect())
    }

    pub async fn content_remove(
        &self,
        id: &str,
        kind: ContentKind,
        file: &str,
    ) -> Result<Vec<ContentFile>> {
        // The game holds its pack zips open; deleting one fails (or sticks) while it runs.
        if self.is_busy(id) {
            return Err(busy(id));
        }
        if self.is_installing(id) {
            return Err(crate::browse::content_installing());
        }
        let folder = kind.folder().unwrap_or_default();
        self.refuse_managed_in(id, folder, file).await?;
        let dir = self.game_dir(id).await?;
        let _lock = lock_or_busy(&self.instances.perland_dir(id), id)?;
        self.before_removal(id, folder, file).await;
        content::remove(&dir, kind, file).await?;
        self.content_list(id, kind).await
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
            OpenTarget::ResourcePacks { id } => {
                self.existing_instance_dir(&id).await?;
                self.instances.game_dir(&id).join("resourcepacks")
            }
            OpenTarget::ShaderPacks { id } => {
                self.existing_instance_dir(&id).await?;
                self.instances.game_dir(&id).join("shaderpacks")
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
    /// from the webview. With `minecraft`, say how it fits that version.
    pub async fn java_test(&self, path: PathBuf, minecraft: Option<String>) -> Result<JavaFound> {
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
        let info = java::probe_info(&path).await?;
        let required = match minecraft {
            Some(mc) => self.java_required(&mc).await,
            None => None,
        };
        let install = java::JavaInstall {
            path,
            major: info.major,
            version: info.version,
            vendor: info.vendor,
        };
        Ok(JavaFound::new(install, required))
    }

    /// The Java `minecraft` asks for and every Java on this computer, for the instance settings.
    pub async fn java_overview(&self, minecraft: String) -> Result<JavaOverview> {
        let (required, installs) = tokio::join!(
            self.java_required(&minecraft),
            java::detect_installed(Platform::current())
        );
        Ok(JavaOverview::new(required, installs))
    }

    /// Unknown (not an error) when the version JSON can't be had, e.g. offline before it was cached.
    pub(crate) async fn java_required(&self, minecraft: &str) -> Option<u32> {
        java::required_for(&self.meta, minecraft).await.ok()
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
        core_with_secrets(
            build,
            Arc::new(launcher_core::perland::MemoryStore::default()),
        )
    }

    /// `core`, with the Perland token kept in `secret`.
    pub(crate) fn core_with_secrets(
        build: BuildKind,
        secret: Arc<dyn SecretStore>,
    ) -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        core_with(build, secret, None)
    }

    /// `core`, with the Perland panel at `panel`.
    pub(crate) fn core_with_panel(
        panel: &str,
    ) -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        core_with(
            BuildKind::Dev,
            Arc::new(launcher_core::perland::MemoryStore::default()),
            launcher_core::perland::panel_url(panel),
        )
    }

    fn core_with(
        build: BuildKind,
        secret: Arc<dyn SecretStore>,
        panel: Option<launcher_core::invite::Url>,
    ) -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        core_with_msa(
            build,
            secret,
            panel,
            Arc::new(launcher_core::msa::secrets::MemorySecrets::default()),
        )
    }

    /// `core`, with Microsoft refresh tokens kept in `msa_secrets`.
    pub(crate) fn core_with_msa_secrets(
        build: BuildKind,
        msa_secrets: Arc<dyn KeyedSecrets>,
    ) -> (
        Arc<TestCore>,
        Arc<FakeRunner>,
        Arc<RecordingHost>,
        tempfile::TempDir,
    ) {
        core_with_msa(
            build,
            Arc::new(launcher_core::perland::MemoryStore::default()),
            None,
            msa_secrets,
        )
    }

    fn core_with_msa(
        build: BuildKind,
        secret: Arc<dyn SecretStore>,
        panel: Option<launcher_core::invite::Url>,
        msa_secrets: Arc<dyn KeyedSecrets>,
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
        let core = AppCore::with_secret_store(
            AppConfig {
                dirs,
                build,
                launcher_version: "0.1.0".into(),
                system_memory_mb: 16384,
                downloads_dir: Some(tmp.path().join("Downloads")),
                panel,
            },
            meta,
            runner.clone(),
            host.clone(),
            secret,
            msa_secrets,
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
    async fn app_info_says_whether_a_panel_is_configured() {
        let (without, _, _, _t1) = core(BuildKind::Dev);
        assert!(!without.app_info().await.unwrap().panel);
        let (with, _, _, _t2) = core_with_panel("https://panel.example.com");
        assert!(with.app_info().await.unwrap().panel);
    }

    #[tokio::test]
    async fn app_info_reflects_build_and_offline_gate() {
        let (dev, _, _, _t1) = core(BuildKind::Dev);
        let info = dev.app_info().await.unwrap();
        assert!(info.offline_allowed);
        assert_eq!(info.build, Build::Dev);
        assert_eq!(info.system_memory_mb, 16384);
        for kind in [BuildKind::Release, BuildKind::Beta] {
            let (locked, _, _, _t2) = core(kind);
            assert!(!locked.app_info().await.unwrap().offline_allowed);
            assert_eq!(
                locked.account_add_offline("Steve").await.unwrap_err().code,
                ErrorCode::OfflineLocked
            );
        }
        let (beta, _, _, _t3) = core(BuildKind::Beta);
        assert_eq!(beta.app_info().await.unwrap().build, Build::Beta);
    }

    #[tokio::test]
    async fn outside_dev_offline_accounts_on_disk_are_hidden() {
        let (dev, _, _, tmp) = core(BuildKind::Dev);
        dev.account_add_offline("Steve").await.unwrap();
        let (beta, _, _, _t) = core(BuildKind::Beta);
        // Same file, other build.
        std::fs::copy(
            tmp.path().join("accounts.json"),
            _t.path().join("accounts.json"),
        )
        .unwrap();
        let shown = beta.accounts_list().await.unwrap();
        assert!(shown.accounts.is_empty());
        assert_eq!(shown.active, None);
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
    async fn the_startup_channel_read_leaves_the_settings_warning_for_the_ui() {
        let (core, _, host, tmp) = core(BuildKind::Beta);
        std::fs::write(tmp.path().join("settings.json"), "{nope").unwrap();
        assert_eq!(core.update_channel().await, Channel::Beta);
        assert!(host.warnings().is_empty());
        core.settings_get().await;
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
    async fn mod_icon_is_a_png_data_url_from_the_jar() {
        use std::io::Write;
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let inst = core.instance_create(vanilla("Icons")).await.unwrap();
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        w.start_file("fabric.mod.json", opts).unwrap();
        w.write_all(br#"{"icon":"icon.png"}"#).unwrap();
        w.start_file("icon.png", opts).unwrap();
        w.write_all(b"\x89PNG\r\n\x1a\n").unwrap();
        let src = tmp.path().join("sodium.jar");
        std::fs::write(&src, w.finish().unwrap().into_inner()).unwrap();
        core.mods_add(&inst.id, vec![src]).await.unwrap();
        let icon = core.mod_icon(&inst.id, "sodium.jar").await.unwrap();
        assert_eq!(icon.as_deref(), Some("data:image/png;base64,iVBORw0KGgo="));
        assert!(tmp.path().join("cache/mod-icons").is_dir());
        assert_eq!(core.mod_icon(&inst.id, "gone.jar").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_jar_without_an_icon_gets_its_modrinth_projects_icon() {
        use launcher_core::{browse::images::Image, download::hash::sha1_hex};
        let (core, runner, _, tmp) = core(BuildKind::Dev);
        let inst = core.instance_create(vanilla("Icons")).await.unwrap();
        let (known, unknown) = (tmp.path().join("known.jar"), tmp.path().join("mine.jar"));
        std::fs::write(&known, b"known jar").unwrap();
        std::fs::write(&unknown, b"my own jar").unwrap();
        core.mods_add(&inst.id, vec![known, unknown]).await.unwrap();
        runner.browser.icon_urls.lock().unwrap().insert(
            sha1_hex(b"known jar"),
            "https://cdn.modrinth.com/data/k/icon.webp".into(),
        );
        *runner.browser.image.lock().unwrap() = Some(Image {
            bytes: b"RIFF".to_vec(),
            mime: "image/webp",
        });
        for _ in 0..2 {
            let icon = core.mod_icon(&inst.id, "known.jar").await.unwrap();
            assert_eq!(icon.as_deref(), Some("data:image/webp;base64,UklGRg=="));
            assert_eq!(core.mod_icon(&inst.id, "mine.jar").await.unwrap(), None);
        }
        assert_eq!(
            runner.browser.icon_lookups.lock().unwrap().len(),
            2,
            "each jar is looked up once"
        );
        // An icon the image cache refuses is the placeholder, not an error.
        *runner.browser.image.lock().unwrap() = None;
        assert_eq!(core.mod_icon(&inst.id, "known.jar").await.unwrap(), None);
    }

    #[tokio::test]
    async fn a_failed_modrinth_lookup_is_the_placeholder() {
        let (core, runner, _, tmp) = core(BuildKind::Dev);
        let inst = core.instance_create(vanilla("Icons")).await.unwrap();
        let src = tmp.path().join("a.jar");
        std::fs::write(&src, b"jar").unwrap();
        core.mods_add(&inst.id, vec![src]).await.unwrap();
        runner
            .browser
            .icon_lookup_fails
            .store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(core.mod_icon(&inst.id, "a.jar").await.unwrap(), None);
        assert_eq!(
            core.mod_icon(&inst.id, "../a.jar").await.unwrap_err().code,
            ErrorCode::ModNameInvalid
        );
    }

    fn put_pack(core: &TestCore, id: &str, folder: &str, name: &str) {
        let dir = core.instances.game_dir(id).join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(name), b"zip").unwrap();
    }

    #[tokio::test]
    async fn folder_opens_and_creates_the_pack_folders() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        let game = core.instances.game_dir(&pack.id);
        let shaders = core
            .folder(OpenTarget::ShaderPacks {
                id: pack.id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(shaders, game.join("shaderpacks"));
        assert!(shaders.is_dir());
        let resources = core
            .folder(OpenTarget::ResourcePacks {
                id: pack.id.clone(),
            })
            .await
            .unwrap();
        assert_eq!(resources, game.join("resourcepacks"));
        assert!(resources.is_dir());
        assert_eq!(
            core.folder(OpenTarget::ShaderPacks { id: "ghost".into() })
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceNotFound
        );
    }

    #[tokio::test]
    async fn content_list_marks_pack_files_managed_whatever_their_case() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        put_pack(&core, &pack.id, "resourcepacks", "faithful.zip");
        put_pack(&core, &pack.id, "resourcepacks", "Mine.zip");
        std::fs::create_dir_all(
            core.instances
                .game_dir(&pack.id)
                .join("resourcepacks/Loose Pack"),
        )
        .unwrap();
        crate::testing::managed_for(&["resourcepacks/Faithful.zip", "shaderpacks/Mine.zip"])
            .save(&core.instances.perland_dir(&pack.id))
            .await
            .unwrap();
        let list = core
            .content_list(&pack.id, ContentKind::ResourcePack)
            .await
            .unwrap();
        let view: Vec<(&str, bool, bool)> = list
            .iter()
            .map(|f| (f.file_name.as_str(), f.folder, f.managed))
            .collect();
        assert_eq!(
            view,
            vec![
                ("faithful.zip", false, true),
                ("Loose Pack", true, false),
                ("Mine.zip", false, false),
            ]
        );
        assert_eq!(list[0].size, 3);
        assert_eq!(
            core.content_list("ghost", ContentKind::Shader)
                .await
                .unwrap_err()
                .code,
            ErrorCode::InstanceNotFound
        );
    }

    #[tokio::test]
    async fn content_remove_deletes_a_players_pack_and_returns_the_list() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        put_pack(&core, &pack.id, "shaderpacks", "Old.zip");
        put_pack(&core, &pack.id, "shaderpacks", "New.zip");
        let list = core
            .content_remove(&pack.id, ContentKind::Shader, "Old.zip")
            .await
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].file_name, "New.zip");
        assert!(!list[0].managed);
    }

    #[tokio::test]
    async fn content_remove_refuses_managed_files_folders_and_running_installs() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        let pack = core.instance_create(vanilla("Pack")).await.unwrap();
        put_pack(&core, &pack.id, "resourcepacks", "Faithful.zip");
        std::fs::create_dir_all(
            core.instances
                .game_dir(&pack.id)
                .join("resourcepacks/Folder"),
        )
        .unwrap();
        crate::testing::managed_for(&["resourcepacks/faithful.zip"])
            .save(&core.instances.perland_dir(&pack.id))
            .await
            .unwrap();
        let kind = ContentKind::ResourcePack;
        let managed = core
            .content_remove(&pack.id, kind, "Faithful.zip")
            .await
            .unwrap_err();
        assert_eq!(managed.code, ErrorCode::ModManaged);
        assert_eq!(managed.params["file"], "Faithful.zip");
        assert_eq!(
            core.content_remove(&pack.id, kind, "Folder")
                .await
                .unwrap_err()
                .code,
            ErrorCode::ModNameInvalid
        );
        core.installing.lock().unwrap().insert(pack.id.clone());
        assert_eq!(
            core.content_remove(&pack.id, kind, "Faithful.zip")
                .await
                .unwrap_err()
                .code,
            ErrorCode::ContentInstalling
        );
        assert!(core
            .instances
            .game_dir(&pack.id)
            .join("resourcepacks/Faithful.zip")
            .is_file());
    }

    async fn listed(core: &TestCore) -> Vec<String> {
        core.instances_list()
            .await
            .unwrap()
            .instances
            .into_iter()
            .map(|i| i.id)
            .collect()
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[tokio::test]
    async fn listing_puts_new_instances_on_top() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        core.instance_create(vanilla("B")).await.unwrap();
        assert_eq!(listed(&core).await, ids(&["a", "b"]));
        core.instance_create(vanilla("C")).await.unwrap();
        assert_eq!(listed(&core).await, ids(&["c", "a", "b"]));
    }

    #[tokio::test]
    async fn reorder_is_kept_and_survives_a_settings_save() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        for name in ["A", "B", "C"] {
            core.instance_create(vanilla(name)).await.unwrap();
        }
        let listing = core.instance_reorder(ids(&["b", "c", "a"])).await.unwrap();
        let got: Vec<_> = listing.instances.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(got, ["b", "c", "a"]);
        core.settings_set(Settings {
            language: Some(launcher_core::settings::Lang::Es),
            ..Settings::default()
        })
        .await
        .unwrap();
        assert_eq!(listed(&core).await, ids(&["b", "c", "a"]));
    }

    #[tokio::test]
    async fn a_repaired_instance_comes_back_in_its_old_place() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        for name in ["A", "B", "C"] {
            core.instance_create(vanilla(name)).await.unwrap();
        }
        core.instance_reorder(ids(&["c", "a", "b"])).await.unwrap();
        let json = core.instances.instance_dir("a").join("instance.json");
        let good = std::fs::read(&json).unwrap();
        std::fs::write(&json, b"{oops").unwrap();
        let listing = core.instances_list().await.unwrap();
        assert_eq!(listing.damaged[0].id, "a");
        assert_eq!(listed(&core).await, ids(&["c", "b"]));
        // A drag while it's damaged doesn't lose its place either.
        core.instance_reorder(ids(&["b", "c"])).await.unwrap();
        // A new instance meanwhile still lands on top.
        core.instance_create(vanilla("D")).await.unwrap();
        assert_eq!(listed(&core).await, ids(&["d", "b", "c"]));
        std::fs::write(&json, good).unwrap();
        assert_eq!(listed(&core).await, ids(&["d", "b", "c", "a"]));
    }

    #[tokio::test]
    async fn rename_keeps_the_place_in_the_order() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        core.instance_create(vanilla("B")).await.unwrap();
        core.instance_reorder(ids(&["b", "a"])).await.unwrap();
        let renamed = core.instance_rename("a", "Zed").await.unwrap();
        assert_eq!(renamed.id, "zed");
        assert_eq!(listed(&core).await, ids(&["b", "zed"]));
    }

    #[tokio::test]
    async fn rename_moves_the_latest_log() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        let logs = tmp.path().join("logs");
        std::fs::create_dir_all(&logs).unwrap();
        std::fs::write(logs.join("a-latest.log"), "hello").unwrap();
        core.logs.push("a", &["line".to_string()]);
        core.instance_rename("a", "Zed").await.unwrap();
        assert!(!logs.join("a-latest.log").exists());
        assert_eq!(
            std::fs::read_to_string(logs.join("zed-latest.log")).unwrap(),
            "hello"
        );
        assert_eq!(core.logs.tail("zed"), ["line"]);
        assert!(core.logs.tail("a").is_empty());
        // No log yet: nothing to move, and the rename still succeeds.
        core.instance_create(vanilla("B")).await.unwrap();
        assert_eq!(core.instance_rename("b", "Yak").await.unwrap().id, "yak");
    }

    #[tokio::test]
    async fn playtime_follows_a_rename() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        core.instance_create(vanilla("B")).await.unwrap();
        launcher_core::playtime::add(&core.instances.perland_dir("a"), 120, 1_700_000_000)
            .await
            .unwrap();
        core.instance_rename("a", "Zed").await.unwrap();
        let list = core.playtime_list().await.unwrap();
        assert_eq!(
            list,
            vec![
                PlaytimeEntry {
                    id: "b".into(),
                    seconds: 0,
                    last_played: 0,
                },
                PlaytimeEntry {
                    id: "zed".into(),
                    seconds: 120,
                    last_played: 1_700_000_000,
                },
            ]
        );
        core.instance_delete("zed").await.unwrap();
        assert_eq!(core.playtime_list().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rename_drops_state_keyed_by_the_old_id() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        core.manual.lock().await.insert(
            "a".into(),
            ManualState {
                total: 1,
                remaining: Vec::new(),
                scanner: Scanner::default(),
            },
        );
        core.instance_rename("a", "Zed").await.unwrap();
        assert!(core.manual.lock().await.is_empty());
    }

    #[tokio::test]
    async fn rename_refused_while_installing() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        core.installing.lock().unwrap().insert("a".into());
        let err = core.instance_rename("a", "Zed").await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ContentInstalling);
        assert_eq!(core.instances.load("a").await.unwrap().name, "A");
    }

    #[tokio::test]
    async fn reorder_rejects_bad_ids() {
        let (core, _, _, _t) = core(BuildKind::Dev);
        core.instance_create(vanilla("A")).await.unwrap();
        for bad in ["../x", "", "A b"] {
            assert_eq!(
                core.instance_reorder(ids(&["a", bad]))
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::InstanceIdInvalid,
                "{bad}"
            );
        }
        assert!(!core.config.dirs.instance_order().exists());
    }

    #[tokio::test]
    async fn update_keeps_the_art_the_disk_has() {
        use launcher_core::instances::ArtMode;
        let (core, _, _, _t) = core(BuildKind::Dev);
        let mut instance = core.instance_create(vanilla("A")).await.unwrap();
        let mut stored = core.instances.load("a").await.unwrap();
        stored.art_seed = Some("seed-1".into());
        stored.art = ArtMode::Generated;
        core.instances.update(&stored).await.unwrap();
        // A stale copy from the UI doesn't undo the art.
        instance.art_seed = None;
        instance.art = ArtMode::Auto;
        instance.java.memory_mb += 512;
        core.instance_update(instance).await.unwrap();
        let after = core.instances.load("a").await.unwrap();
        assert_eq!(after.art_seed.as_deref(), Some("seed-1"));
        assert_eq!(after.art, ArtMode::Generated);
        assert_eq!(after.java.memory_mb, stored.java.memory_mb + 512);
    }

    #[tokio::test]
    async fn update_keeps_the_icon_from_disk() {
        use launcher_core::instances::IconMode;
        let (core, _, _, _t) = core(BuildKind::Dev);
        let mut instance = core.instance_create(vanilla("A")).await.unwrap();
        let mut stored = core.instances.load("a").await.unwrap();
        stored.icon = Some(IconMode::Generated);
        core.instances.update(&stored).await.unwrap();
        // A stale copy from the UI doesn't undo the icon.
        instance.icon = None;
        instance.java.memory_mb += 512;
        core.instance_update(instance).await.unwrap();
        let after = core.instances.load("a").await.unwrap();
        assert_eq!(after.icon, Some(IconMode::Generated));
        assert_eq!(after.java.memory_mb, stored.java.memory_mb + 512);
    }

    #[tokio::test]
    async fn update_keeps_the_banner_focus_from_disk() {
        use launcher_core::instances::Focus;
        let (core, _, _, _t) = core(BuildKind::Dev);
        let mut instance = core.instance_create(vanilla("A")).await.unwrap();
        let mut stored = core.instances.load("a").await.unwrap();
        stored.banner_focus = Some(Focus { x: 20, y: 80 });
        core.instances.update(&stored).await.unwrap();
        // A stale copy from the UI doesn't undo the position.
        instance.banner_focus = None;
        instance.java.memory_mb += 512;
        core.instance_update(instance).await.unwrap();
        let after = core.instances.load("a").await.unwrap();
        assert_eq!(after.banner_focus, Some(Focus { x: 20, y: 80 }));
        assert_eq!(after.java.memory_mb, stored.java.memory_mb + 512);
    }

    #[tokio::test]
    async fn an_install_or_content_install_blocks_updates() {
        let (core, _, _, _t) = core(BuildKind::Beta);
        assert!(!core.update_blocked());
        let permit = core.install_slot.clone().try_acquire_owned().unwrap();
        assert!(core.update_blocked());
        drop(permit);
        core.installing.lock().unwrap().insert("asalith".into());
        assert!(core.update_blocked());
    }

    #[tokio::test]
    async fn a_running_import_blocks_updates() {
        let (core, _, _, _t) = core(BuildKind::Beta);
        assert!(!core.update_blocked());
        *core.import_slot.lock().unwrap() = Some(Cancel::default());
        assert!(core.update_blocked());
        *core.import_slot.lock().unwrap() = None;
        assert!(!core.update_blocked());
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
            core.java_test(tmp.path().join("nope"), None)
                .await
                .unwrap_err()
                .code,
            ErrorCode::JavaTestFailed
        );
    }

    #[tokio::test]
    async fn diagnostics_export_writes_a_zip_and_remembers_it() {
        let (core, _, _, tmp) = core(BuildKind::Beta);
        let out = tmp.path().join("perland-diagnostics.zip");
        assert_eq!(
            core.diagnostics_export(out.clone(), "Windows 11".into())
                .await
                .unwrap(),
            out
        );
        assert!(out.is_file());
        assert_eq!(core.diagnostics_last(), Some(out));
    }

    #[tokio::test]
    async fn diagnostics_must_be_a_zip() {
        let (core, _, _, tmp) = core(BuildKind::Beta);
        let err = core
            .diagnostics_export(tmp.path().join("notes.txt"), "Windows 11".into())
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::DiagnosticsPathInvalid);
        assert_eq!(core.diagnostics_last(), None);
    }

    #[tokio::test]
    async fn diagnostics_without_an_extension_get_zip_appended() {
        // Some save dialogs (GTK) return the typed name as is.
        let (core, _, _, tmp) = core(BuildKind::Beta);
        let picked = tmp.path().join("perland-diagnostics");
        let out = tmp.path().join("perland-diagnostics.zip");
        assert_eq!(
            core.diagnostics_export(picked.clone(), "Linux".into())
                .await
                .unwrap(),
            out
        );
        assert!(out.is_file() && !picked.exists());
        assert_eq!(core.diagnostics_last(), Some(out));
    }
}
