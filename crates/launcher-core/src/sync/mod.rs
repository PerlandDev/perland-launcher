//! Bringing an instance's files in line with its pack (core spec §3). `sources` says what the
//! pack is; this module decides (`plan`), touches the disk (`apply`), and records (`lockfile`).
pub(crate) mod apply;
pub mod lockfile;
pub mod plan;

use std::{
    collections::BTreeMap,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
    time::SystemTime,
};

use futures_util::StreamExt;
use serde::Serialize;

use crate::{
    download::{cache::ObjectCache, hash::sha1_file, Hashes},
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
    install::Installer,
    instances::{GameSpec, InstanceStore},
    locks::InstanceLock,
    sources::{mrpack, DesiredState, FileSource, PackFile, SourceConfig, Sources},
};
use lockfile::{save_pack, Managed, ManagedFile};
use plan::{Changes, Planned};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyncOutcome {
    Done(SyncReport),
    /// Nothing was applied: these files must be downloaded by hand first (see `import_manual`).
    NeedsManual(Vec<ManualFile>),
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

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

pub struct Syncer {
    pub game: Arc<dyn GameInstall>,
    pub sources: Sources,
    /// The launcher's data folder; free space is measured here.
    pub root: PathBuf,
}

impl Syncer {
    pub fn new(game: Arc<dyn GameInstall>, sources: Sources, root: impl Into<PathBuf>) -> Self {
        Self {
            game,
            sources,
            root: root.into(),
        }
    }

    /// The production wiring: the installer's metadata client and downloader, the object cache
    /// under its data folder, and the real upstream addresses.
    pub fn for_installer(installer: Arc<Installer>, http: reqwest::Client) -> Self {
        let sources = Sources {
            http,
            meta: installer.meta.clone(),
            downloader: installer.downloader.clone(),
            cache: ObjectCache::new(installer.dirs.objects()),
            config: SourceConfig::default(),
        };
        let root = installer.dirs.root().to_path_buf();
        Self::new(installer, sources, root)
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
        let mut instance = store.load(id).await?;
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
                return Ok(SyncOutcome::Done(SyncReport {
                    up_to_date: true,
                    ..SyncReport::default()
                }));
            }
        }

        let changes = plan::changes(&wanted, managed.as_ref(), &desired.pack_version);
        let disk = apply::survey(&game_dir, &wanted, managed.as_ref()).await?;
        let plan = plan::plan(&wanted, managed.as_ref(), &disk);

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

        let staging = perland.join("staging");
        let _ = tokio::fs::remove_dir_all(&staging).await;
        tokio::fs::create_dir_all(&staging).await?;
        let archive = match &desired.archive {
            Some(h) => Some(self.sources.cache.get(h).await.ok_or_else(archive_gone)?),
            None => None,
        };
        let staged = match self
            .stage_all(
                &plan.write,
                archive.as_deref(),
                &staging,
                &desired.name,
                sink,
            )
            .await?
        {
            Ok(staged) => staged,
            Err(blocked) => return Ok(SyncOutcome::NeedsManual(blocked)),
        };

        if game_changed {
            self.game.install_game(&new_game, sink).await?;
        }

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

        if game_changed {
            instance.game = new_game;
            store.save(&instance).await?;
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
        save_pack(&perland, desired).await?;
        Managed {
            version: Managed::VERSION,
            stamp: desired.stamp.clone(),
            pack_version: desired.pack_version.clone(),
            choices,
            files,
        }
        .save(&perland)
        .await?;
        let _ = tokio::fs::remove_dir_all(&staging).await;
        Ok(SyncOutcome::Done(SyncReport {
            up_to_date: false,
            changes,
            kept_modified: plan.kept_modified,
            game_changed,
        }))
    }

    /// Stage every file. The outer error stops the sync; the inner `Err` lists blocked files.
    async fn stage_all(
        &self,
        writes: &[Planned],
        archive: Option<&Path>,
        staging: &Path,
        pack_name: &str,
        sink: &EventSink,
    ) -> Result<std::result::Result<BTreeMap<String, StagedFile>, Vec<ManualFile>>> {
        let task = TaskId::new("sync_pack").with("name", pack_name);
        let total = writes.len() as u64;
        let (mut done, mut bytes) = (0u64, 0u64);
        sink(Event::Progress {
            task: task.clone(),
            done,
            total,
            bytes,
        });
        let mut results = futures_util::stream::iter(
            writes
                .iter()
                .map(|p| self.stage_one(&p.file, archive, staging)),
        )
        .buffer_unordered(STAGE_CONCURRENCY);
        let mut staged = BTreeMap::new();
        let mut blocked = Vec::new();
        while let Some(result) = results.next().await {
            match result {
                Ok((path, file)) => {
                    bytes += file.size;
                    staged.insert(path, file);
                }
                Err(StageError::Blocked(m)) => blocked.push(*m),
                Err(StageError::Failed(e)) => return Err(e),
            }
            done += 1;
            sink(Event::Progress {
                task: task.clone(),
                done,
                total,
                bytes,
            });
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
