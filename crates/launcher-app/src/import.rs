//! Import a whole instance from another launcher (#38): preview what the picked folder holds,
//! copy its game folder into a staging folder, then make it a plain instance. The picked folder
//! is only ever read, and a failed or cancelled import leaves nothing behind.
use std::{
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Mutex,
    time::Instant,
};

use launcher_core::{
    events::TaskId,
    import,
    instances::{validate_name, GameSpec, Instance},
    sync::Cancel,
    LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{ImportPreview, ImportRequest},
    events::{AppEvent, Host, ProgressThrottle, PROGRESS_INTERVAL},
    runner::Runner,
};

/// The progress task key; the UI keeps this progress off Play.
const TASK: &str = "import_instance";
/// Under the data root, next to `instances/` (so the final move is a rename on the same disk)
/// but outside it, so a half-copied import is never listed.
const STAGING: &str = "import-staging";

/// Frees the import slot when the import ends, however it ends.
struct SlotGuard<'a>(&'a Mutex<Option<Cancel>>);

impl Drop for SlotGuard<'_> {
    fn drop(&mut self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// What the picked folder holds, to prefill the import form. `path` comes back as picked.
    pub async fn import_inspect(&self, path: PathBuf) -> Result<ImportPreview> {
        let (detected, size_bytes) = import::inspect(&path, self.config.dirs.root()).await?;
        Ok(ImportPreview {
            path,
            name: detected.name,
            minecraft: detected.minecraft,
            loader: detected.loader,
            size_bytes,
        })
    }

    /// Copy the picked folder's game files into a new plain instance. One import at a time; a
    /// failed or cancelled one leaves no instance and no staged files behind.
    pub async fn import_instance(&self, request: ImportRequest) -> Result<Instance> {
        // Before anything is copied: a bad name would only fail after the whole copy.
        let name = validate_name(&request.name)?;
        let (cancel, _slot) = self.import_begin()?;
        // The folder is read again here: nothing about it is taken from the UI but its path.
        let (detected, _) = import::inspect(&request.path, self.config.dirs.root()).await?;
        let staging_root = self.config.dirs.root().join(STAGING);
        clear_staging(&staging_root).await?;
        let staging = free_slot(&staging_root).await?;
        let result = self
            .import_staged(&staging, &detected, &cancel, &name, request)
            .await;
        if result.is_err() {
            let _ = tokio::fs::remove_dir_all(&staging).await;
        }
        // Only goes when empty; a leftover that couldn't be removed waits for the next import.
        let _ = tokio::fs::remove_dir(&staging_root).await;
        result
    }

    /// Stop the running import before its next file. Nothing happens when none runs.
    pub fn import_cancel(&self) {
        if let Some(cancel) = self
            .import_slot
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            cancel.cancel();
        }
    }

    fn import_begin(&self) -> Result<(Cancel, SlotGuard<'_>)> {
        let mut slot = self.import_slot.lock().unwrap_or_else(|e| e.into_inner());
        if slot.is_some() {
            return Err(import::busy());
        }
        let cancel = Cancel::default();
        *slot = Some(cancel.clone());
        Ok((cancel, SlotGuard(&self.import_slot)))
    }

    /// Copy into `staging`, then create the instance and move the copy in as its game folder.
    async fn import_staged(
        &self,
        staging: &Path,
        detected: &import::Detected,
        cancel: &Cancel,
        name: &str,
        request: ImportRequest,
    ) -> Result<Instance> {
        let throttle = Mutex::new(ProgressThrottle::new(PROGRESS_INTERVAL));
        let host = self.host.clone();
        let progress = move |done: u64, total: u64| {
            let admit = throttle.lock().unwrap_or_else(|e| e.into_inner()).admit(
                TASK,
                done,
                total,
                Instant::now(),
            );
            if admit {
                host.emit(AppEvent::Progress {
                    instance: None,
                    task: TaskId::new(TASK),
                    done,
                    total,
                    bytes: done,
                });
            }
        };
        import::copy_game_dir(
            &detected.game_dir,
            staging,
            detected.skip_root_meta,
            cancel,
            &progress,
        )
        .await?;
        // A cancel after the last file still counts: the player asked before the instance exists.
        if cancel.is_cancelled() {
            return Err(import::cancelled());
        }
        let memory = self.default_memory_mb().await;
        let instance = self
            .instances
            .create(
                name,
                GameSpec {
                    minecraft: request.minecraft,
                    loader: request.loader,
                },
            )
            .await?;
        match self.import_place(instance, staging, memory).await {
            Ok(instance) => Ok(instance),
            Err((id, e)) => {
                // No half-made instance: it goes with everything moved into it.
                let _ = self.instances.delete(&id).await;
                Err(e)
            }
        }
    }

    /// Swap the new instance's empty game folder for the staged copy, and give it the default
    /// memory. On failure, the error comes back with the instance id to remove.
    async fn import_place(
        &self,
        mut instance: Instance,
        staging: &Path,
        memory: u32,
    ) -> std::result::Result<Instance, (String, LauncherError)> {
        let id = instance.id.clone();
        let fail = |e: LauncherError| (id.clone(), e);
        #[cfg(test)]
        if self
            .import_fail_move
            .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(fail(std::io::Error::other("injected move failure").into()));
        }
        let game = self.instances.game_dir(&instance.id);
        tokio::fs::remove_dir(&game)
            .await
            .map_err(|e| fail(e.into()))?;
        tokio::fs::rename(staging, &game)
            .await
            .map_err(|e| fail(e.into()))?;
        if instance.java.memory_mb != memory {
            instance.java.memory_mb = memory;
            instance = self.instances.update(&instance).await.map_err(fail)?;
        }
        Ok(instance)
    }
}

/// Remove what a crashed import left in `import-staging/`. Only its folders: a file there is
/// unexpected and fails the import rather than being deleted.
async fn clear_staging(staging_root: &Path) -> Result<()> {
    let mut entries = match tokio::fs::read_dir(staging_root).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await.is_ok_and(|t| t.is_dir()) {
            // A file still open somewhere (Windows) keeps that folder until next time.
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
    Ok(())
}

/// The first `import-staging/<n>/` that doesn't exist yet (its parent is created).
async fn free_slot(staging_root: &Path) -> Result<PathBuf> {
    tokio::fs::create_dir_all(staging_root).await?;
    let mut n = 1u32;
    loop {
        let dir = staging_root.join(n.to_string());
        if !tokio::fs::try_exists(&dir).await? {
            return Ok(dir);
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc, Arc, Mutex,
        },
        time::Duration,
    };

    use launcher_core::{
        accounts::BuildKind,
        instances::{GameSpec, LoaderSpec, SourceRef},
        meta::loaders::LoaderKind,
        ErrorCode,
    };

    use super::*;
    use crate::{
        app::tests::{core, TestCore},
        events::AppEvent,
        testing::RecordingHost,
    };

    /// How long a test waits on the other import before giving up.
    const WAIT: Duration = Duration::from_secs(5);

    fn put(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// A Fabric instance in another launcher's folder layout, outside the launcher's data root.
    fn prism(outside: &Path) -> PathBuf {
        let dir = outside.join("My Pack");
        put(&dir, "instance.cfg", "name=My Pack\n");
        put(
            &dir,
            "mmc-pack.json",
            r#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},
                {"uid":"net.fabricmc.fabric-loader","version":"0.15.11"}]}"#,
        );
        put(&dir, ".minecraft/mods/a.jar", "jar");
        put(&dir, ".minecraft/XaeroWaypoints/x.txt", "xy");
        dir
    }

    fn fabric() -> Option<LoaderSpec> {
        Some(LoaderSpec {
            kind: LoaderKind::Fabric,
            version: "0.15.11".into(),
        })
    }

    fn request(path: &Path, name: &str) -> ImportRequest {
        ImportRequest {
            path: path.to_path_buf(),
            name: name.into(),
            minecraft: "1.20.1".into(),
            loader: fabric(),
        }
    }

    /// The import progress events: (done, total, bytes).
    fn progress(host: &RecordingHost) -> Vec<(u64, u64, u64)> {
        host.events
            .lock()
            .unwrap()
            .iter()
            .filter_map(|e| match e {
                AppEvent::Progress {
                    instance: None,
                    task,
                    done,
                    total,
                    bytes,
                } if task.key == "import_instance" => Some((*done, *total, *bytes)),
                _ => None,
            })
            .collect()
    }

    /// Nothing is left in the staging folder (or it is gone).
    fn staging_is_empty(root: &Path) -> bool {
        match std::fs::read_dir(root.join("import-staging")) {
            Ok(mut entries) => entries.next().is_none(),
            Err(_) => !root.join("import-staging").is_dir(),
        }
    }

    fn instance_folders(core: &TestCore) -> usize {
        std::fs::read_dir(core.config.dirs.instances())
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    /// Calls `on_import_progress` with every import progress event.
    fn on_import_progress(host: &RecordingHost, hook: impl Fn() + Send + Sync + 'static) {
        *host.on_emit.lock().unwrap() = Some(Arc::new(move |event| {
            if matches!(event, AppEvent::Progress { task, .. } if task.key == "import_instance") {
                hook();
            }
        }));
    }

    #[tokio::test]
    async fn inspect_previews_the_picked_folder() {
        let (core, _, _, _tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());
        let preview = core.import_inspect(dir.join(".minecraft")).await.unwrap();
        assert_eq!(
            preview,
            ImportPreview {
                path: dir.join(".minecraft"),
                name: Some("My Pack".into()),
                minecraft: Some("1.20.1".into()),
                loader: fabric(),
                size_bytes: 5,
            }
        );
    }

    #[tokio::test]
    async fn import_creates_a_plain_instance_with_the_copied_files() {
        let (core, _, host, tmp) = core(BuildKind::Dev);
        let settings = core.settings_get().await;
        core.settings_set(launcher_core::settings::Settings {
            default_memory_mb: 3072,
            ..settings
        })
        .await
        .unwrap();
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());

        let instance = core
            .import_instance(request(&dir, "  Imported  "))
            .await
            .unwrap();

        assert_eq!(instance.name, "Imported");
        assert_eq!(
            instance.game,
            GameSpec {
                minecraft: "1.20.1".into(),
                loader: fabric(),
            }
        );
        assert_eq!(instance.source, SourceRef::None);
        assert_eq!(instance.java.memory_mb, 3072);
        assert_eq!(core.instances.load(&instance.id).await.unwrap(), instance);
        let game = core.instances.game_dir(&instance.id);
        assert_eq!(std::fs::read(game.join("mods/a.jar")).unwrap(), b"jar");
        assert_eq!(
            std::fs::read(game.join("XaeroWaypoints/x.txt")).unwrap(),
            b"xy"
        );
        let mods = core.mods_list(&instance.id).await.unwrap();
        assert_eq!(
            mods.iter()
                .map(|m| m.file_name.as_str())
                .collect::<Vec<_>>(),
            ["a.jar"]
        );
        let events = progress(&host);
        assert!(!events.is_empty());
        assert_eq!(events.last(), Some(&(5, 5, 5)));
        assert!(staging_is_empty(tmp.path()));
        assert!(
            dir.join(".minecraft/mods/a.jar").is_file(),
            "the source is left alone"
        );
    }

    #[tokio::test]
    async fn cancelled_import_leaves_nothing() {
        let (core, _, host, tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());
        let weak = Arc::downgrade(&core);
        on_import_progress(&host, move || {
            if let Some(core) = weak.upgrade() {
                core.import_cancel();
            }
        });

        let err = core
            .import_instance(request(&dir, "Imported"))
            .await
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::ImportCancelled);
        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert_eq!(instance_folders(&core), 0);
        assert!(staging_is_empty(tmp.path()));
        *host.on_emit.lock().unwrap() = None;
        core.import_instance(request(&dir, "Imported"))
            .await
            .expect("the slot is free again");
    }

    #[tokio::test]
    async fn failed_import_leaves_nothing() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());
        std::fs::write(tmp.path().join("import-staging"), "in the way").unwrap();

        assert!(core
            .import_instance(request(&dir, "Imported"))
            .await
            .is_err());

        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert_eq!(instance_folders(&core), 0);
        std::fs::remove_file(tmp.path().join("import-staging")).unwrap();
        core.import_instance(request(&dir, "Imported"))
            .await
            .expect("the slot is free again");
    }

    #[tokio::test]
    async fn failed_move_removes_the_new_instance() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());
        core.import_fail_move.store(true, Ordering::SeqCst);

        assert!(core
            .import_instance(request(&dir, "Imported"))
            .await
            .is_err());

        assert!(core.instances_list().await.unwrap().instances.is_empty());
        assert_eq!(instance_folders(&core), 0, "no half-made instance");
        assert!(staging_is_empty(tmp.path()));
    }

    #[tokio::test]
    async fn leftovers_from_a_crash_are_removed() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());
        put(tmp.path(), "import-staging/1/mods/old.jar", "old");
        put(tmp.path(), "import-staging/7/saves/w/level.dat", "w");

        let instance = core
            .import_instance(request(&dir, "Imported"))
            .await
            .unwrap();

        assert!(staging_is_empty(tmp.path()));
        let mods = core.mods_list(&instance.id).await.unwrap();
        assert_eq!(mods.len(), 1, "{mods:?}");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn second_import_while_one_runs_is_refused() {
        let (core, _, host, _tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let held = AtomicBool::new(false);
        on_import_progress(&host, move || {
            if !held.swap(true, Ordering::SeqCst) {
                started_tx.send(()).unwrap();
                let _ = release_rx.lock().unwrap().recv_timeout(WAIT);
            }
        });

        let first = {
            let (core, request) = (core.clone(), request(&dir, "First"));
            tokio::spawn(async move { core.import_instance(request).await })
        };
        tokio::task::spawn_blocking(move || started_rx.recv_timeout(WAIT))
            .await
            .unwrap()
            .expect("the first import reported progress");
        let err = core
            .import_instance(request(&dir, "Second"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportBusy);
        release_tx.send(()).unwrap();

        let first = first.await.unwrap().unwrap();
        assert_eq!(first.name, "First");
        assert_eq!(core.instances_list().await.unwrap().instances.len(), 1);
    }

    #[tokio::test]
    async fn name_is_validated() {
        let (core, _, host, tmp) = core(BuildKind::Dev);
        let outside = tempfile::tempdir().unwrap();
        let dir = prism(outside.path());

        let err = core
            .import_instance(request(&dir, "   "))
            .await
            .unwrap_err();

        assert_eq!(err.code, ErrorCode::NameInvalid);
        assert!(progress(&host).is_empty(), "nothing was copied");
        assert!(!tmp.path().join("import-staging").exists());
        assert_eq!(instance_folders(&core), 0);
    }

    #[tokio::test]
    async fn import_refuses_what_inspect_refuses() {
        let (core, _, _, tmp) = core(BuildKind::Dev);
        let err = core
            .import_instance(request(tmp.path(), "Mine"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportInsideLauncher);
        assert_eq!(instance_folders(&core), 0);
    }
}
