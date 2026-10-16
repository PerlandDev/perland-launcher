//! Bringing an instance's files in line with its pack (core spec §3). `sources` says what the
//! pack is; this module decides (`plan`), touches the disk (`apply`), and records (`lockfile`).
pub(crate) mod apply;
pub mod lockfile;
pub mod manual;
pub mod plan;

use std::{
    collections::{BTreeMap, HashMap},
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::SystemTime,
};

use futures_util::StreamExt;
use serde::Serialize;

use crate::{
    download::{cache::ObjectCache, hash::sha1_file, Hashes},
    error::{ErrorCategory, ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
    fsutil::path_key,
    install::Installer,
    instances::{GameSpec, Instance, InstanceStore, SourceRef, UpdatePolicy},
    locks::InstanceLock,
    sources::{
        art, mrpack, no_pack, DesiredState, FileMode, FileSource, PackFile, PackInput,
        SourceConfig, Sources, UpdateCheck,
    },
};
use lockfile::{load_pack, save_pack, Managed, ManagedFile};
use plan::{Changes, Planned};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// In `.perland/`: the instance was just linked to a server, and its first sync sets the jars
/// the pack doesn't install aside (link-instance spec §3.2).
const ADOPT_MARKER: &str = "adopt";
/// In the game folder: where a linked instance's old jars go. Never purged; they're the
/// player's.
pub const SET_ASIDE_DIR: &str = "mods-before-link";

/// Mark a never-synced instance as linked: its first sync sets old jars aside.
pub async fn mark_adopt(perland_dir: &Path) -> Result<()> {
    tokio::fs::create_dir_all(perland_dir).await?;
    tokio::fs::write(perland_dir.join(ADOPT_MARKER), b"").await?;
    Ok(())
}

/// Installs a Minecraft and loader version. The real one is `Installer`; tests use a fake.
pub trait GameInstall: Send + Sync {
    fn install_game<'a>(
        &'a self,
        game: &'a GameSpec,
        sink: &'a EventSink,
    ) -> BoxFuture<'a, Result<()>>;
}

impl GameInstall for Installer {
    fn install_game<'a>(
        &'a self,
        game: &'a GameSpec,
        sink: &'a EventSink,
    ) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move {
            let loader = game.loader.as_ref().map(|l| (l.kind, l.version.as_str()));
            let resolved = self.resolve(&game.minecraft, loader).await?;
            self.install(&resolved, sink).await
        })
    }
}

/// A file its author blocks other launchers from downloading: the player gets it by hand.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualFile {
    pub path: String,
    pub name: String,
    pub file_name: String,
    pub page_url: String,
    pub hashes: Hashes,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    /// Nothing needed doing.
    pub up_to_date: bool,
    pub changes: Changes,
    /// Override files the player changed, kept instead of updated ("Kept your version of …").
    pub kept_modified: Vec<String>,
    pub game_changed: bool,
    /// A linked instance's first sync: the jars it moved to `SET_ASIDE_DIR` (old paths).
    pub set_aside: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Done(SyncReport),
    /// Nothing was applied: these files must be downloaded by hand first (see `import_manual`).
    NeedsManual(Vec<ManualFile>),
}

/// What `ensure` did, and whether the update check failed along the way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ensured {
    pub outcome: SyncOutcome,
    /// The update check (or resolving the update) failed, so the installed version was used.
    pub warning: Option<LauncherError>,
}

/// How many pack files are fetched or copied at once.
const STAGE_CONCURRENCY: usize = 8;

struct StagedFile {
    sha1: String,
    size: u64,
}

enum StageError {
    Blocked(Box<ManualFile>),
    Failed(LauncherError),
}

impl From<LauncherError> for StageError {
    fn from(e: LauncherError) -> Self {
        StageError::Failed(e)
    }
}

fn archive_gone() -> LauncherError {
    LauncherError::pack(ErrorCode::PackNotFound, "The pack's archive is gone")
        .with_fix("Import the pack again")
}

/// Resolving an update failed: the installed version plays, so this is only a warning.
fn update_check_failed(e: LauncherError) -> LauncherError {
    LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
        .with_fix("You can still play the version you have")
        .with_detail(e.message)
}

/// Stops a sync before its next file and before the swap. Once the swap starts the sync
/// finishes, so a cancel never leaves an instance half-updated. A wait (a server pack the
/// Engine is still building) ends at once.
#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<CancelState>);

#[derive(Debug, Default)]
struct CancelState {
    flag: AtomicBool,
    wake: tokio::sync::Notify,
}

impl Cancel {
    pub fn cancel(&self) {
        self.0.flag.store(true, Ordering::SeqCst);
        self.0.wake.notify_waiters();
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.flag.load(Ordering::SeqCst)
    }
    /// Done once `cancel` was called (at once if it already was).
    pub async fn cancelled(&self) {
        loop {
            let woken = self.0.wake.notified();
            tokio::pin!(woken);
            // Registered before the flag is read, so a cancel in between still wakes it.
            woken.as_mut().enable();
            if self.is_cancelled() {
                return;
            }
            woken.await;
        }
    }
}

pub(crate) fn sync_cancelled() -> LauncherError {
    LauncherError::pack(ErrorCode::SyncCancelled, "The update was cancelled")
}

/// Downloading the update failed before anything moved: the installed version plays instead.
fn update_download_failed(installed: &str, e: LauncherError) -> LauncherError {
    LauncherError::network(
        ErrorCode::UpdateDownloadFailed,
        format!("Couldn't download the update. You still have {installed}"),
    )
    .with_param("version", installed)
    .with_fix("Try updating again later")
    .with_detail(e.message)
}

/// An update the player can take: where it comes from, the pack it is, and what it changes.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingUpdate {
    /// What the instance's source becomes once the update is installed.
    pub source: SourceRef,
    pub desired: DesiredState,
    /// What the update adds, updates and removes; edits the player keeps are not "updated".
    pub changes: Changes,
    /// Override files the player edited that the update keeps as they are.
    pub kept: Vec<String>,
}

/// Override files the player edited that a sync to `wanted` would keep. Only
/// `ReplaceUnlessModified` files are hashed, never the mods, so a preview stays quick.
async fn kept_edits(
    game_dir: &Path,
    wanted: &[&PackFile],
    managed: Option<&Managed>,
) -> Vec<String> {
    let overrides: Vec<&PackFile> = wanted
        .iter()
        .copied()
        .filter(|f| f.mode == FileMode::ReplaceUnlessModified)
        .collect();
    match apply::survey(game_dir, &overrides, None, None).await {
        Ok(disk) => plan::plan(&overrides, managed, &disk).kept_modified,
        Err(_) => Vec::new(),
    }
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

pub struct Syncer {
    pub game: Arc<dyn GameInstall>,
    pub sources: Sources,
    /// The launcher's data folder; free space is measured here.
    pub root: PathBuf,
    /// Per instance: the last update `find_update` resolved (offered stamp, source, pack), so
    /// the banner and Play don't resolve the same version's metafiles twice.
    resolved: std::sync::Mutex<HashMap<String, (String, SourceRef, DesiredState)>>,
}

impl Syncer {
    pub fn new(game: Arc<dyn GameInstall>, sources: Sources, root: impl Into<PathBuf>) -> Self {
        Self {
            game,
            sources,
            root: root.into(),
            resolved: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// A sync changed what's installed: the next update check resolves afresh.
    fn forget_update(&self, id: &str) {
        self.resolved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(id);
    }

    /// The production wiring: the installer's metadata client and downloader, the object cache
    /// under its data folder, and the real upstream addresses.
    pub fn for_installer(installer: Arc<Installer>, http: reqwest::Client) -> Self {
        let sources = Sources {
            invites: crate::invite::InviteClient::new(http.clone()),
            http,
            meta: installer.meta.clone(),
            downloader: installer.downloader.clone(),
            cache: ObjectCache::new(installer.dirs.objects()),
            config: SourceConfig::default(),
        };
        let root = installer.dirs.root().to_path_buf();
        Self::new(installer, sources, root)
    }

    /// Copy files a scan found into the object cache (the player's copies stay where they
    /// are); the next sync uses them.
    pub async fn import_found(&self, found: &[(ManualFile, PathBuf)]) -> Result<()> {
        for (file, path) in found {
            self.sources.cache.insert_file(path, &file.hashes).await?;
        }
        Ok(())
    }

    /// Scan `dir` once and import every blocked file found there. Returns how many were found.
    pub async fn import_manual(&self, dir: &Path, files: &[ManualFile]) -> Result<usize> {
        let found = manual::scan(dir, files).await?;
        self.import_found(&found).await?;
        Ok(found.len())
    }

    /// A new instance for a pack, named after it, with its game version and its preferred update
    /// policy (else "ask"). Its files arrive with the first sync of the returned pack.
    pub async fn add_pack(
        &self,
        store: &InstanceStore,
        input: &PackInput,
    ) -> Result<(Instance, DesiredState)> {
        let (source, desired) = self.sources.open(input).await?;
        let name = if desired.name.trim().is_empty() {
            "Pack"
        } else {
            desired.name.as_str()
        };
        let mut instance = store
            .create(
                name,
                GameSpec {
                    minecraft: desired.minecraft.clone(),
                    loader: desired.loader.clone(),
                },
            )
            .await?;
        instance.source = source;
        instance.update_policy = desired.about.policy.unwrap_or(UpdatePolicy::Ask);
        store.save(&instance).await?;
        art::fetch(
            &self.sources.http,
            &desired.about.art,
            &store.perland_dir(&instance.id).join("art"),
        )
        .await;
        Ok((instance, desired))
    }

    /// The pack version installed in `id` (`pack.json`), if it was ever synced.
    pub async fn installed(&self, store: &InstanceStore, id: &str) -> Option<DesiredState> {
        load_pack(&store.perland_dir(id)).await
    }

    /// Is there an update? `UpToDate` for a pinned instance (it never updates) and for one that
    /// was never synced (its first sync installs the newest version anyway).
    pub async fn check(&self, store: &InstanceStore, instance: &Instance) -> UpdateCheck {
        if instance.update_policy == UpdatePolicy::Pinned {
            return UpdateCheck::UpToDate;
        }
        match self.installed(store, &instance.id).await {
            Some(installed) => self.sources.check(&instance.source, &installed).await,
            None => UpdateCheck::UpToDate,
        }
    }

    /// What syncing to `desired` would change: for the app's "Update ready" dialog.
    pub async fn preview(
        &self,
        store: &InstanceStore,
        instance: &Instance,
        desired: &DesiredState,
    ) -> Changes {
        let managed = Managed::load(&store.perland_dir(&instance.id)).await;
        let wanted = plan::wanted(desired, &instance.optional_choices);
        plan::changes(&wanted, managed.as_ref(), &desired.pack_version)
    }

    /// The update the player could take now, with what it would change, or `None` (pinned,
    /// never synced, up to date). `Err` is the failed check: a warning, after which the
    /// installed version plays.
    pub async fn find_update(
        &self,
        store: &InstanceStore,
        instance: &Instance,
    ) -> Result<Option<PendingUpdate>> {
        self.find_update_with(store, instance, &crate::events::noop_sink())
            .await
    }

    /// `find_update`, telling `sink` about long waits (a server pack still being built).
    pub async fn find_update_with(
        &self,
        store: &InstanceStore,
        instance: &Instance,
        sink: &EventSink,
    ) -> Result<Option<PendingUpdate>> {
        if instance.update_policy == UpdatePolicy::Pinned {
            return Ok(None);
        }
        let Some(installed) = self.installed(store, &instance.id).await else {
            return Ok(None);
        };
        let (source, stamp) = match self.sources.check(&instance.source, &installed).await {
            UpdateCheck::UpToDate => return Ok(None),
            UpdateCheck::Unavailable(e) => return Err(e),
            UpdateCheck::Available { source, stamp, .. } => (source, stamp),
        };
        // Only the expensive `resolve` is remembered; `changes` and `kept` below are recomputed
        // from the disk and the current optional choices on every call.
        let remembered = self
            .resolved
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&instance.id)
            .filter(|(offered, known, _)| *offered == stamp && *known == source)
            .map(|(_, _, desired)| desired.clone());
        let desired = match remembered {
            Some(desired) => desired,
            None => {
                let desired = self
                    .sources
                    .resolve_with(&source, sink, &Cancel::default())
                    .await
                    .map_err(update_check_failed)?;
                self.resolved
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(
                        instance.id.clone(),
                        (stamp, source.clone(), desired.clone()),
                    );
                desired
            }
        };
        let managed = Managed::load(&store.perland_dir(&instance.id)).await;
        let wanted = plan::wanted(&desired, &instance.optional_choices);
        let kept = kept_edits(&store.game_dir(&instance.id), &wanted, managed.as_ref()).await;
        let changes = plan::changes_except(&wanted, managed.as_ref(), &desired.pack_version, &kept);
        Ok(Some(PendingUpdate {
            source,
            desired,
            changes,
            kept,
        }))
    }

    /// Bring a pack instance up to date before playing, following its update policy (core spec
    /// §3.1). Pinned stays on the installed version. Auto and ask take updates (the app asks
    /// first, with `find_update`). A failed check plays what's installed, with a warning.
    pub async fn ensure(
        &self,
        store: &InstanceStore,
        id: &str,
        sink: &EventSink,
    ) -> Result<Ensured> {
        // Loading validates `id` before the lock creates anything under it.
        let instance = store.load(id).await?;
        let lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        let (update, check_warning) = match self.find_update_with(store, &instance, sink).await {
            Ok(update) => (update, None),
            Err(e) => (None, Some(e)),
        };
        let ensured = self
            .ensure_locked(&lock, store, id, update, sink, &Cancel::default())
            .await?;
        Ok(Ensured {
            warning: check_warning.or(ensured.warning),
            ..ensured
        })
    }

    /// `ensure` for a caller that holds the instance's lock and already chose: `accept` is the
    /// update to install, `None` keeps the installed version. A never-synced instance installs
    /// what its source names now. If downloading an accepted update fails, nothing has moved
    /// yet, so the installed version is synced instead, with an `update_download_failed` warning.
    pub async fn ensure_locked(
        &self,
        _lock: &InstanceLock,
        store: &InstanceStore,
        id: &str,
        accept: Option<PendingUpdate>,
        sink: &EventSink,
        cancel: &Cancel,
    ) -> Result<Ensured> {
        let instance = store.load(id).await?;
        if instance.source == SourceRef::None {
            return Err(no_pack());
        }
        let Some(installed) = self.installed(store, id).await else {
            let desired = self
                .sources
                .resolve_with(&instance.source, sink, cancel)
                .await?;
            let outcome = self.sync_inner(store, id, &desired, sink, cancel).await?;
            self.after_sync(store, id, &desired, &outcome, None).await?;
            return Ok(Ensured {
                outcome,
                warning: None,
            });
        };
        let Some(update) = accept else {
            let outcome = self.sync_inner(store, id, &installed, sink, cancel).await?;
            return Ok(Ensured {
                outcome,
                warning: None,
            });
        };
        match self
            .sync_inner(store, id, &update.desired, sink, cancel)
            .await
        {
            Ok(outcome) => {
                let new_source = (update.source != instance.source).then_some(update.source);
                self.after_sync(store, id, &update.desired, &outcome, new_source)
                    .await?;
                Ok(Ensured {
                    outcome,
                    warning: None,
                })
            }
            // Network failures happen only while staging or installing the loader, before
            // anything in the game folder moves.
            Err(e) if e.category == ErrorCategory::Network => {
                // A Stop that raced the failed download is a Stop, not a reason to fall back.
                if cancel.is_cancelled() {
                    return Err(sync_cancelled());
                }
                let warning = update_download_failed(&installed.pack_version, e);
                let outcome = self.sync_inner(store, id, &installed, sink, cancel).await?;
                Ok(Ensured {
                    outcome,
                    warning: Some(warning),
                })
            }
            Err(e) => Err(e),
        }
    }

    /// After a newly installed version: name it in the instance's source, and fetch its art.
    async fn after_sync(
        &self,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        outcome: &SyncOutcome,
        new_source: Option<SourceRef>,
    ) -> Result<()> {
        let SyncOutcome::Done(report) = outcome else {
            return Ok(());
        };
        if let Some(source) = new_source {
            // Only now: a source that names a version whose sync never finished would hide
            // the update from the next check.
            let mut fresh = store.load(id).await?;
            fresh.source = source;
            store.save(&fresh).await?;
        }
        if !report.up_to_date {
            art::fetch(
                &self.sources.http,
                &desired.about.art,
                &store.perland_dir(id).join("art"),
            )
            .await;
        }
        Ok(())
    }

    /// Make instance `id` match `desired`. Fails with `instance_locked` while anyone else holds
    /// the instance's lock.
    pub async fn sync(
        &self,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        sink: &EventSink,
    ) -> Result<SyncOutcome> {
        // Loading validates `id` before the lock creates anything under it.
        store.load(id).await?;
        let lock = InstanceLock::try_acquire(&store.perland_dir(id))?;
        self.sync_locked(&lock, store, id, desired, sink).await
    }

    /// `sync` for a caller that already holds the instance's lock.
    pub async fn sync_locked(
        &self,
        _lock: &InstanceLock,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        sink: &EventSink,
    ) -> Result<SyncOutcome> {
        self.sync_inner(store, id, desired, sink, &Cancel::default())
            .await
    }

    async fn sync_inner(
        &self,
        store: &InstanceStore,
        id: &str,
        desired: &DesiredState,
        sink: &EventSink,
        cancel: &Cancel,
    ) -> Result<SyncOutcome> {
        let instance = store.load(id).await?;
        let game_dir = store.game_dir(id);
        let perland = store.perland_dir(id);
        let now = SystemTime::now();
        apply::purge_trash(&perland.join("trash"), now, apply::TRASH_MAX_AGE).await;
        desired.check_paths()?;

        let managed = Managed::load(&perland).await;
        let choices = plan::effective_choices(desired, &instance.optional_choices);
        let wanted = plan::wanted(desired, &instance.optional_choices);
        let new_game = GameSpec {
            minecraft: desired.minecraft.clone(),
            loader: desired.loader.clone(),
        };
        let game_changed = instance.game != new_game;
        if let Some(m) = &managed {
            if !game_changed
                && m.stamp == desired.stamp
                && m.choices == choices
                && apply::quick_check(&game_dir, m)
            {
                self.forget_update(id);
                return Ok(SyncOutcome::Done(SyncReport {
                    up_to_date: true,
                    ..SyncReport::default()
                }));
            }
        }

        // Hashing every file takes a while on a big pack; say so (a first install has nothing to
        // check, so it stays quiet).
        let progress: Option<apply::SurveyProgress> = managed.is_some().then(|| {
            let (sink, task) = (
                sink.clone(),
                TaskId::new("check_pack").with("name", &desired.name),
            );
            Arc::new(move |done: u64, total: u64| {
                sink(Event::Progress {
                    task: task.clone(),
                    done,
                    total,
                    bytes: 0,
                })
            }) as apply::SurveyProgress
        });
        let disk = apply::survey(&game_dir, &wanted, managed.as_ref(), progress).await?;
        let plan = plan::plan(&wanted, managed.as_ref(), &disk);
        // Only a never-synced instance: once sync has written a lockfile, every jar it doesn't
        // know is the player's addition.
        let adopt = perland.join(ADOPT_MARKER);
        let set_aside = if managed.is_none() && adopt.exists() {
            apply::unwanted_jars(&game_dir, &wanted).await?
        } else {
            Vec::new()
        };
        let changes = plan::changes_except(
            &wanted,
            managed.as_ref(),
            &desired.pack_version,
            &plan.kept_modified,
        );

        // A leftover from an earlier attempt must not count against free space.
        let staging = perland.join("staging");
        let _ = tokio::fs::remove_dir_all(&staging).await;

        // Staged copies of everything written, plus downloads that aren't cached yet.
        let mut needed = 0u64;
        for p in &plan.write {
            let size = p.file.size.unwrap_or(0);
            needed += size;
            let cached = self
                .sources
                .cache
                .path_for(&p.file.hashes)
                .is_some_and(|o| o.exists());
            if matches!(p.file.source, FileSource::Remote { .. }) && !cached {
                needed += size;
            }
        }
        apply::ensure_space(needed, apply::free_space(&self.root))?;

        tokio::fs::create_dir_all(&staging).await?;
        // Only a sync that extracts from the .mrpack needs it; `stage_one` reports it gone.
        let needs_archive = plan
            .write
            .iter()
            .any(|p| matches!(p.file.source, FileSource::Archive { .. }));
        let archive = match (&desired.archive, needs_archive) {
            (Some(h), true) => Some(self.sources.cache.get(h).await.ok_or_else(archive_gone)?),
            _ => None,
        };
        let staged = match self
            .stage_all(
                &plan.write,
                archive.as_deref(),
                &staging,
                &desired.name,
                if managed.is_none() {
                    "install_pack"
                } else {
                    "sync_pack"
                },
                sink,
                cancel,
            )
            .await?
        {
            Ok(staged) => staged,
            Err(blocked) => return Ok(SyncOutcome::NeedsManual(blocked)),
        };

        if game_changed {
            self.game.install_game(&new_game, sink).await?;
        }

        // Every byte of the new version is in the cache now. Recording it before the swap means a
        // swap that stops halfway resumes toward it, even offline, instead of toward the old
        // version (whose leftovers would sit next to the new files as "additions").
        if cancel.is_cancelled() {
            return Err(sync_cancelled());
        }
        save_pack(&perland, desired).await?;
        let trash_dir = apply::trash_folder(&perland, now);
        for p in &plan.write {
            if p.replaces {
                apply::move_to_trash(&game_dir, &trash_dir, &p.file.path).await?;
            }
            apply::place(&staging.join(&p.file.path), &game_dir.join(&p.file.path)).await?;
        }
        for path in &plan.trash {
            apply::move_to_trash(&game_dir, &trash_dir, path).await?;
        }
        for alias in &plan.case_aliases {
            let key = path_key(alias);
            let Some(file) = wanted.iter().find(|f| path_key(&f.path) == key) else {
                continue;
            };
            if apply::is_separate(&game_dir, alias, &file.path) {
                apply::move_to_trash(&game_dir, &trash_dir, alias).await?;
            }
        }
        for path in &set_aside {
            apply::move_aside(&game_dir, path).await?;
        }

        if game_changed {
            // Reload: a rename or settings change made meanwhile must not be reverted.
            let mut fresh = store.load(id).await?;
            fresh.game = new_game;
            store.save(&fresh).await?;
        }

        let mut files = BTreeMap::new();
        for k in &plan.keep {
            files.insert(
                k.file.path.clone(),
                ManagedFile {
                    name: k.file.name.clone(),
                    sha1: k.sha1.clone(),
                    size: k.size,
                    mode: k.file.mode,
                    declared: k.file.hashes.clone(),
                },
            );
        }
        for p in &plan.write {
            let s = &staged[&p.file.path];
            files.insert(
                p.file.path.clone(),
                ManagedFile {
                    name: p.file.name.clone(),
                    sha1: s.sha1.clone(),
                    size: s.size,
                    mode: p.file.mode,
                    declared: p.file.hashes.clone(),
                },
            );
        }
        Managed {
            version: Managed::VERSION,
            stamp: desired.stamp.clone(),
            pack_version: desired.pack_version.clone(),
            choices,
            files,
        }
        .save(&perland)
        .await?;
        let _ = tokio::fs::remove_file(&adopt).await;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        self.forget_update(id);
        Ok(SyncOutcome::Done(SyncReport {
            up_to_date: false,
            changes,
            kept_modified: plan.kept_modified,
            game_changed,
            set_aside,
        }))
    }

    /// Stage every file. The outer error stops the sync; the inner `Err` lists blocked files.
    #[allow(clippy::too_many_arguments)]
    async fn stage_all(
        &self,
        writes: &[Planned],
        archive: Option<&Path>,
        staging: &Path,
        pack_name: &str,
        task_key: &str,
        sink: &EventSink,
        cancel: &Cancel,
    ) -> Result<std::result::Result<BTreeMap<String, StagedFile>, Vec<ManualFile>>> {
        let task = TaskId::new(task_key).with("name", pack_name);
        let total = writes.len() as u64;
        let (mut done, mut bytes) = (0u64, 0u64);
        sink(Event::Progress {
            task: task.clone(),
            done,
            total,
            bytes,
        });
        // After a failure no new file is started, but the ones in flight are awaited: nothing
        // may still be writing into staging once this returns and the lock is released.
        let failed = AtomicBool::new(false);
        let failed = &failed;
        // Collected first: a closure taking `&Planned` inside the stream makes this future
        // fail to prove `Send` (rust-lang/rust#110338), and callers box it as a `Send` future.
        let stages: Vec<_> = writes
            .iter()
            .map(|p| async move {
                if failed.load(Ordering::SeqCst) || cancel.is_cancelled() {
                    return None;
                }
                let result = self.stage_one(&p.file, archive, staging).await;
                if matches!(result, Err(StageError::Failed(_))) {
                    failed.store(true, Ordering::SeqCst);
                }
                Some(result)
            })
            .collect();
        let mut results = futures_util::stream::iter(stages).buffer_unordered(STAGE_CONCURRENCY);
        let mut staged = BTreeMap::new();
        let mut blocked = Vec::new();
        let mut first_error = None;
        while let Some(result) = results.next().await {
            let Some(result) = result else { continue };
            match result {
                Ok((path, file)) => {
                    bytes += file.size;
                    staged.insert(path, file);
                }
                Err(StageError::Blocked(m)) => blocked.push(*m),
                Err(StageError::Failed(e)) => {
                    first_error.get_or_insert(e);
                }
            }
            done += 1;
            sink(Event::Progress {
                task: task.clone(),
                done,
                total,
                bytes,
            });
        }
        if let Some(e) = first_error {
            return Err(e);
        }
        if cancel.is_cancelled() {
            return Err(sync_cancelled());
        }
        if blocked.is_empty() {
            Ok(Ok(staged))
        } else {
            blocked.sort_by(|a, b| a.path.cmp(&b.path));
            Ok(Err(blocked))
        }
    }

    async fn stage_one(
        &self,
        file: &PackFile,
        archive: Option<&Path>,
        staging: &Path,
    ) -> std::result::Result<(String, StagedFile), StageError> {
        let dest = staging.join(&file.path);
        match &file.source {
            FileSource::Remote { urls } => {
                let object = match self
                    .sources
                    .cache
                    .ensure(&self.sources.downloader, urls, &file.hashes, file.size)
                    .await
                {
                    Ok(object) => object,
                    Err(e) if e.code == ErrorCode::DownloadRejected && file.manual.is_some() => {
                        let manual = file.manual.as_ref().expect("checked above");
                        return Err(StageError::Blocked(Box::new(ManualFile {
                            path: file.path.clone(),
                            name: file.name.clone(),
                            file_name: manual.file_name.clone(),
                            page_url: manual.page_url.clone(),
                            hashes: file.hashes.clone(),
                        })));
                    }
                    Err(e) => return Err(e.into()),
                };
                if let Some(parent) = dest.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(LauncherError::from)?;
                }
                tokio::fs::copy(&object, &dest)
                    .await
                    .map_err(LauncherError::from)?;
            }
            FileSource::Archive { entry } => {
                let archive = archive.ok_or_else(archive_gone)?.to_path_buf();
                let (entry, to) = (entry.clone(), dest.clone());
                let sha1 = file.hashes.sha1.clone().unwrap_or_default();
                tokio::task::spawn_blocking(move || {
                    mrpack::extract_entry(&archive, &entry, &to, &sha1)
                })
                .await
                .map_err(join_error)??;
            }
        }
        let sha1 = match &file.hashes.sha1 {
            Some(s) => s.to_ascii_lowercase(),
            None => {
                let d = dest.clone();
                tokio::task::spawn_blocking(move || sha1_file(&d))
                    .await
                    .map_err(join_error)?
                    .map_err(LauncherError::from)?
            }
        };
        let size = tokio::fs::metadata(&dest)
            .await
            .map_err(LauncherError::from)?
            .len();
        Ok((file.path.clone(), StagedFile { sha1, size }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::hash::sha1_hex,
        sources::{FileMode, FileSource, Side},
    };

    fn file(path: &str, mode: FileMode, body: &[u8]) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.into(),
            source: FileSource::Remote { urls: vec![] },
            hashes: Hashes::sha1(sha1_hex(body)),
            size: None,
            side: Side::Both,
            optional: None,
            mode,
            manual: None,
        }
    }

    #[tokio::test]
    async fn kept_edits_hashes_only_override_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::create_dir_all(game.join("config")).unwrap();
        std::fs::write(game.join("config/c.toml"), b"mine").unwrap();
        std::fs::create_dir_all(game.join("mods/a.jar")).unwrap(); // a folder: surveying it fails
        let old = file("config/c.toml", FileMode::ReplaceUnlessModified, b"old");
        let managed = Managed {
            version: Managed::VERSION,
            stamp: "s1".into(),
            pack_version: "1.0.0".into(),
            choices: BTreeMap::new(),
            files: BTreeMap::from([(
                "config/c.toml".to_string(),
                lockfile::ManagedFile {
                    name: "c".into(),
                    sha1: sha1_hex(b"old"),
                    size: 3,
                    mode: FileMode::ReplaceUnlessModified,
                    declared: old.hashes.clone(),
                },
            )]),
        };
        let new = file("config/c.toml", FileMode::ReplaceUnlessModified, b"new");
        let jar = file("mods/a.jar", FileMode::Replace, b"jar");
        let kept = kept_edits(game, &[&new, &jar], Some(&managed)).await;
        assert_eq!(kept, vec!["config/c.toml".to_string()]);
    }
}
