//! Taking, extending, sealing, reading and pruning the instance's restore point (see the module
//! docs of `restore`). Callers hold the instance lock. Nothing here may fail the change that
//! asked for a point: problems are logged and the change goes ahead without one.
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use super::{
    restore_dir,
    snapshot::{self, LinkMode},
    Change, PointKind, PointMeta, SnapFile, COPY_CAP, MAX_AGE_SECS,
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::{rename_with_retry, write_atomic},
    instances::InstanceStore,
    sync::{
        apply::{free_space, not_enough_space, SPACE_MARGIN},
        lockfile::{Managed, MANAGED_FILE, PACK_FILE},
    },
};

/// Param marking the error of a point that would copy more than the cap.
const OVER_CAP: &str = "over_cap";

/// The point couldn't be built because it would copy more than the cap (not for lack of room).
pub(crate) fn is_over_cap(e: &LauncherError) -> bool {
    e.params.contains_key(OVER_CAP)
}

const META_FILE: &str = "point.json";
const META_VERSION: u32 = 1;

/// How a change reached `before_change`.
pub struct ChangeCtx {
    /// The game is running (a Browse install while playing).
    pub during_run: bool,
    /// Unix seconds.
    pub now: u64,
}

fn internal(message: impl Into<String>) -> LauncherError {
    LauncherError::disk(ErrorCode::Internal, message)
}

fn io_err(e: std::io::Error) -> LauncherError {
    internal("Couldn't keep a restore point").with_detail(e)
}

pub(crate) fn point_dir(restore: &Path) -> PathBuf {
    restore.join("point")
}

/// Best effort: never fails the change. `tops` are filtered with `in_scope`.
pub async fn before_change(
    store: &InstanceStore,
    id: &str,
    tops: &[String],
    change: Change,
    ctx: ChangeCtx,
) {
    before_change_capped(store, id, tops, change, ctx, COPY_CAP).await;
}

async fn before_change_capped(
    store: &InstanceStore,
    id: &str,
    tops: &[String],
    change: Change,
    ctx: ChangeCtx,
    cap: u64,
) {
    let tops: Vec<String> = tops
        .iter()
        .filter(|t| super::in_scope(t))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if let Err(e) = take(store, id, &tops, change, &ctx, cap).await {
        // The change goes ahead uncovered: an older point would restore a mix, so it goes too.
        discard(store, id).await;
        tracing::warn!(folders = ?tops, "no restore point for this change: {e}");
    }
}

async fn take(
    store: &InstanceStore,
    id: &str,
    tops: &[String],
    change: Change,
    ctx: &ChangeCtx,
    cap: u64,
) -> Result<()> {
    let perland = store.perland_dir(id);
    let restore = restore_dir(&perland);
    let existing = read_meta(&restore, ctx.now).await;

    // An existing point that this change joins.
    if let Some(mut meta) = existing
        .clone()
        .filter(|m| m.kind == PointKind::BeforeChange && (!m.sealed || ctx.during_run))
    {
        let missing: Vec<String> = tops
            .iter()
            .filter(|t| !meta.scope.contains(t))
            .cloned()
            .collect();
        if !missing.is_empty() {
            if let Err(e) = extend(store, id, &restore, &mut meta, &missing, cap).await {
                // The point can't cover this change, so it would restore a mix. Drop it.
                let _ = tokio::fs::remove_dir_all(point_dir(&restore)).await;
                return Err(e);
            }
        }
        push_change(&mut meta, change, ctx.now);
        return write_meta(&point_dir(&restore), &meta).await;
    }

    // Nothing in a folder a point holds is changing: nothing to take.
    if tops.is_empty() {
        return Ok(());
    }
    clear_scratch(&restore).await;
    let new = restore.join("new");
    let mut meta =
        build_point_capped(store, id, tops, PointKind::BeforeChange, ctx.now, &new, cap).await?;
    meta.sealed = ctx.during_run;
    push_change(&mut meta, change, ctx.now);
    write_meta(&new, &meta).await?;
    swap_in(&restore).await
}

fn push_change(meta: &mut PointMeta, change: Change, now: u64) {
    if meta.changes.last() != Some(&change) {
        meta.changes.push(change);
    }
    meta.last_change_at = now;
    meta.clean_run_since = false;
}

/// Snapshot `missing` folders into the unsealed point, via `new-<top>/`.
async fn extend(
    store: &InstanceStore,
    id: &str,
    restore: &Path,
    meta: &mut PointMeta,
    missing: &[String],
    cap: u64,
) -> Result<()> {
    let perland = store.perland_dir(id);
    let game_dir = store.game_dir(id);
    let held: u64 = meta
        .files
        .iter()
        .filter(|f| !f.linked)
        .map(|f| f.size)
        .sum();
    for top in missing {
        let scratch = restore.join(format!("new-{top}"));
        let _ = tokio::fs::remove_dir_all(&scratch).await;
        let files = snapshot_tops(
            &perland,
            &game_dir,
            std::slice::from_ref(top),
            &scratch,
            held,
            cap,
        )
        .await?;
        let files_dir = point_dir(restore).join("files");
        tokio::fs::create_dir_all(&files_dir)
            .await
            .map_err(io_err)?;
        let target = files_dir.join(top);
        // Nothing to keep if the folder doesn't exist yet: the scope still names it.
        if tokio::fs::try_exists(scratch.join(top))
            .await
            .map_err(io_err)?
        {
            let _ = tokio::fs::remove_dir_all(&target).await;
            rename_with_retry(&scratch.join(top), &target)
                .await
                .map_err(io_err)?;
        }
        let _ = tokio::fs::remove_dir_all(&scratch).await;
        meta.files.extend(files);
        meta.scope.push(top.clone());
    }
    meta.scope.sort();
    Ok(())
}

/// Check there is room, then snapshot each of `tops` into `dest/<top>`: hard links first, and if
/// one fails, copies of everything. `already` is what the point copies already.
async fn snapshot_tops(
    perland: &Path,
    game_dir: &Path,
    tops: &[String],
    dest: &Path,
    already: u64,
    cap: u64,
) -> Result<Vec<SnapFile>> {
    let (perland, game_dir, dest, tops) = (
        perland.to_path_buf(),
        game_dir.to_path_buf(),
        dest.to_path_buf(),
        tops.to_vec(),
    );
    tokio::task::spawn_blocking(move || {
        for mode in [LinkMode::Auto, LinkMode::Copy] {
            let mut copied = already;
            for top in &tops {
                copied = copied.saturating_add(snapshot::copy_size(&game_dir, top, mode)?);
            }
            if copied > cap {
                return Err(
                    internal("A restore point would be too large").with_param(OVER_CAP, "true")
                );
            }
            std::fs::create_dir_all(&dest)?;
            let needed = copied.saturating_add(SPACE_MARGIN);
            let free = free_space(&perland);
            if free < needed {
                return Err(not_enough_space(needed, free));
            }
            let mut files = Vec::new();
            let mut failed = false;
            for top in &tops {
                match snapshot::snapshot_folder(&game_dir, top, &dest, mode)? {
                    Ok(done) => files.extend(done),
                    Err(snapshot::LinkFailed) => {
                        failed = true;
                        break;
                    }
                }
            }
            if !failed {
                return Ok(files);
            }
            // Wipe what the links left and go again, copying.
            for top in &tops {
                let _ = std::fs::remove_dir_all(dest.join(top));
            }
        }
        Err(internal("Couldn't copy files for a restore point"))
    })
    .await
    .map_err(|e| internal("Couldn't keep a restore point").with_detail(e))?
}

/// Build a point for `tops` in `into` (emptied first): the folders' files, copies of
/// `managed.json` and `pack.json`, and `point.json`. `go_back` builds its own point with it (with `COPY_CAP`).
pub(crate) async fn build_point_capped(
    store: &InstanceStore,
    id: &str,
    tops: &[String],
    kind: PointKind,
    now: u64,
    into: &Path,
    cap: u64,
) -> Result<PointMeta> {
    let instance = store.load(id).await?;
    let perland = store.perland_dir(id);
    let _ = tokio::fs::remove_dir_all(into).await;
    tokio::fs::create_dir_all(into).await.map_err(io_err)?;

    let files = snapshot_tops(
        &perland,
        &store.game_dir(id),
        tops,
        &into.join("files"),
        0,
        cap,
    )
    .await?;
    for name in [MANAGED_FILE, PACK_FILE] {
        match tokio::fs::copy(perland.join(name), into.join(name)).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_err(e)),
        }
    }
    let mut scope = tops.to_vec();
    scope.sort();
    let meta = PointMeta {
        version: META_VERSION,
        created_at: now,
        kind,
        scope,
        game: instance.game,
        source: instance.source,
        optional_choices: instance.optional_choices,
        pack_version: Managed::load(&perland).await.map(|m| m.pack_version),
        changes: Vec::new(),
        sealed: false,
        last_change_at: now,
        clean_run_since: false,
        files,
    };
    write_meta(into, &meta).await?;
    Ok(meta)
}

/// `new/` becomes `point/`, going through `old/` so an interruption leaves one or the other.
pub(crate) async fn swap_in(restore_dir: &Path) -> Result<()> {
    let (new, point, old) = (
        restore_dir.join("new"),
        point_dir(restore_dir),
        restore_dir.join("old"),
    );
    recover_old(restore_dir).await;
    let _ = tokio::fs::remove_dir_all(&old).await;
    let had_point = tokio::fs::try_exists(&point).await.map_err(io_err)?;
    if had_point {
        rename_with_retry(&point, &old).await.map_err(io_err)?;
    }
    if let Err(e) = rename_with_retry(&new, &point).await {
        if had_point {
            // Put the previous point back: it is the only copy.
            let _ = rename_with_retry(&old, &point).await;
        }
        return Err(io_err(e));
    }
    let _ = tokio::fs::remove_dir_all(&old).await;
    Ok(())
}

/// An interrupted swap can leave the previous point in `old/` and no `point/`: put it back.
pub(crate) async fn recover_old(restore: &Path) {
    let (point, old) = (point_dir(restore), restore.join("old"));
    if !tokio::fs::try_exists(&point).await.unwrap_or(true)
        && tokio::fs::try_exists(&old).await.unwrap_or(false)
    {
        let _ = rename_with_retry(&old, &point).await;
    }
}

/// Remove leftovers of an interrupted build: `new/`, `new-<top>/` and `old/` (after putting
/// back a previous point that only `old/` still holds).
pub(crate) async fn clear_scratch(restore: &Path) {
    recover_old(restore).await;
    let Ok(mut entries) = tokio::fs::read_dir(restore).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "new" || name == "old" || name.starts_with("new-") {
            let _ = tokio::fs::remove_dir_all(entry.path()).await;
        }
    }
}

async fn write_meta(dir: &Path, meta: &PointMeta) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(meta)
        .map_err(|e| internal("Couldn't keep a restore point").with_detail(e))?;
    write_atomic(&dir.join(META_FILE), &bytes).await
}

/// The point as stored, with no age check. A point folder that can't be read is removed.
pub(crate) async fn load_meta(restore: &Path) -> Option<PointMeta> {
    recover_old(restore).await;
    let dir = point_dir(restore);
    if !tokio::fs::try_exists(&dir).await.unwrap_or(false) {
        return None;
    }
    let meta = tokio::fs::read(dir.join(META_FILE))
        .await
        .ok()
        .and_then(|bytes| serde_json::from_slice::<PointMeta>(&bytes).ok())
        .filter(|m| m.version == META_VERSION);
    if meta.is_none() {
        let _ = tokio::fs::remove_dir_all(&dir).await;
    }
    meta
}

async fn read_meta(restore: &Path, now: u64) -> Option<PointMeta> {
    let meta = load_meta(restore).await?;
    if now.saturating_sub(meta.created_at) > MAX_AGE_SECS {
        let _ = tokio::fs::remove_dir_all(point_dir(restore)).await;
        return None;
    }
    Some(meta)
}

/// Remove the instance's point and any leftovers of building one. Best effort. For a change
/// that goes ahead without a point, or a change of source, which an older point would undo
/// only in part.
pub async fn discard(store: &InstanceStore, id: &str) {
    let restore = restore_dir(&store.perland_dir(id));
    clear_scratch(&restore).await;
    match tokio::fs::remove_dir_all(point_dir(&restore)).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            tracing::warn!("couldn't remove the restore point: {e}");
        }
        _ => {}
    }
}

/// Append `change` without snapshotting anything (a resumed sync).
pub async fn note_change(store: &InstanceStore, id: &str, change: Change, now: u64) {
    let restore = restore_dir(&store.perland_dir(id));
    let Some(mut meta) = read_meta(&restore, now).await else {
        return;
    };
    push_change(&mut meta, change, now);
    if let Err(e) = write_meta(&point_dir(&restore), &meta).await {
        tracing::warn!("couldn't note a change on the restore point: {e}");
    }
}

/// The instance's restore point. One older than `MAX_AGE_SECS`, or damaged, is deleted first.
pub async fn read(store: &InstanceStore, id: &str, now: u64) -> Option<PointMeta> {
    read_meta(&restore_dir(&store.perland_dir(id)), now).await
}

/// Bytes the point alone holds on disk (0 without a point).
pub async fn held_size(store: &InstanceStore, id: &str) -> u64 {
    let restore = restore_dir(&store.perland_dir(id));
    let Some(meta) = load_meta(&restore).await else {
        return 0;
    };
    let files_dir = point_dir(&restore).join("files");
    let game_dir = store.game_dir(id);
    tokio::task::spawn_blocking(move || snapshot::held_bytes(&files_dir, &game_dir, &meta.files))
        .await
        .unwrap_or(0)
}

/// The game launched: later changes start a new point.
pub async fn seal(store: &InstanceStore, id: &str) {
    update(store, id, |meta| meta.sealed = true).await;
}

/// A run that began at `started_at` ended. A clean one that began after the last change
/// marks the point as tested.
pub async fn run_ended(store: &InstanceStore, id: &str, started_at: u64, crashed: bool) {
    update(store, id, |meta| {
        if !crashed && started_at >= meta.last_change_at {
            meta.clean_run_since = true;
        }
    })
    .await;
}

async fn update(store: &InstanceStore, id: &str, f: impl FnOnce(&mut PointMeta)) {
    let restore = restore_dir(&store.perland_dir(id));
    let Some(mut meta) = load_meta(&restore).await else {
        return;
    };
    f(&mut meta);
    if let Err(e) = write_meta(&point_dir(&restore), &meta).await {
        tracing::warn!("couldn't update the restore point: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        instances::{GameSpec, InstanceStore},
        restore::MAX_AGE_SECS,
    };

    const T0: u64 = 1_000_000;

    struct Fixture {
        _tmp: tempfile::TempDir,
        store: InstanceStore,
        id: String,
    }

    impl Fixture {
        async fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let store = InstanceStore::new(tmp.path());
            let game = GameSpec {
                minecraft: "1.20.1".into(),
                loader: None,
            };
            let id = store.create("Test", game).await.unwrap().id;
            Fixture {
                _tmp: tmp,
                store,
                id,
            }
        }
        fn game(&self, rel: &str) -> PathBuf {
            self.store.game_dir(&self.id).join(rel)
        }
        fn put(&self, rel: &str, bytes: &[u8]) {
            let path = self.game(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        fn restore(&self) -> PathBuf {
            restore_dir(&self.store.perland_dir(&self.id))
        }
        fn point_file(&self, rel: &str) -> PathBuf {
            self.restore().join("point/files").join(rel)
        }
        async fn change(&self, tops: &[&str], name: &str, during_run: bool, now: u64) {
            let tops: Vec<String> = tops.iter().map(|t| t.to_string()).collect();
            before_change(
                &self.store,
                &self.id,
                &tops,
                installed(name),
                ChangeCtx { during_run, now },
            )
            .await;
        }
        async fn read(&self) -> Option<PointMeta> {
            read(&self.store, &self.id, T0).await
        }
    }

    fn installed(name: &str) -> Change {
        Change::Installed { name: name.into() }
    }

    #[tokio::test]
    async fn first_change_makes_unsealed_point() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;

        let meta = fx.read().await.unwrap();
        assert_eq!(meta.kind, PointKind::BeforeChange);
        assert!(!meta.sealed);
        assert_eq!(meta.scope, ["mods"]);
        assert_eq!(meta.changes, [installed("Sodium")]);
        assert_eq!(meta.game.minecraft, "1.20.1");
        assert_eq!(
            std::fs::read(fx.point_file("mods/old.jar")).unwrap(),
            b"old"
        );
    }

    #[tokio::test]
    async fn second_change_before_play_appends() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        fx.put("mods/sodium.jar", b"new");
        fx.change(&["mods"], "Lithium", false, T0 + 5).await;

        let meta = fx.read().await.unwrap();
        assert_eq!(meta.changes, [installed("Sodium"), installed("Lithium")]);
        assert_eq!(meta.last_change_at, T0 + 5);
        assert!(fx.point_file("mods/old.jar").exists());
        assert!(!fx.point_file("mods/sodium.jar").exists());
    }

    #[tokio::test]
    async fn repeated_change_is_not_listed_twice() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        fx.change(&["mods"], "Sodium", false, T0 + 1).await;
        assert_eq!(fx.read().await.unwrap().changes, [installed("Sodium")]);
    }

    #[tokio::test]
    async fn new_top_is_added_to_unsealed_point() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.put("config/a.toml", b"cfg");
        fx.change(&["mods"], "Sodium", false, T0).await;
        fx.change(&["config"], "Lithium", false, T0 + 1).await;

        let meta = fx.read().await.unwrap();
        assert_eq!(meta.scope, ["config", "mods"]);
        assert_eq!(
            std::fs::read(fx.point_file("config/a.toml")).unwrap(),
            b"cfg"
        );
        assert!(meta.files.iter().any(|f| f.path == "config/a.toml"));
        assert!(!fx.restore().join("new-config").exists());
    }

    #[tokio::test]
    async fn sealed_point_is_replaced() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        seal(&fx.store, &fx.id).await;
        fx.change(&["mods"], "Lithium", false, T0 + 10).await;

        let meta = fx.read().await.unwrap();
        assert!(!meta.sealed);
        assert_eq!(meta.changes, [installed("Lithium")]);
        assert_eq!(meta.created_at, T0 + 10);
        assert!(!fx.restore().join("old").exists());
    }

    #[tokio::test]
    async fn change_during_run_appends_to_sealed_point() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        seal(&fx.store, &fx.id).await;
        fx.change(&["mods"], "Lithium", true, T0 + 20).await;

        let meta = fx.read().await.unwrap();
        assert!(meta.sealed);
        assert_eq!(meta.changes, [installed("Sodium"), installed("Lithium")]);
        assert_eq!(meta.last_change_at, T0 + 20);

        run_ended(&fx.store, &fx.id, T0 + 10, false).await;
        assert!(!fx.read().await.unwrap().clean_run_since);
    }

    #[tokio::test]
    async fn change_during_run_without_point_makes_sealed_one() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", true, T0).await;
        assert!(fx.read().await.unwrap().sealed);
    }

    #[tokio::test]
    async fn clean_run_marks_point() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        seal(&fx.store, &fx.id).await;

        run_ended(&fx.store, &fx.id, T0 + 1, true).await;
        assert!(!fx.read().await.unwrap().clean_run_since);
        run_ended(&fx.store, &fx.id, T0 + 1, false).await;
        assert!(fx.read().await.unwrap().clean_run_since);
    }

    #[tokio::test]
    async fn denylist_never_snapshotted() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.put("saves/world/level.dat", b"world");
        fx.change(&["saves", "mods"], "Sodium", false, T0).await;

        assert_eq!(fx.read().await.unwrap().scope, ["mods"]);
        assert!(!fx.restore().join("point/files/saves").exists());
    }

    #[tokio::test]
    async fn old_point_is_pruned() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;

        assert!(read(&fx.store, &fx.id, T0 + MAX_AGE_SECS).await.is_some());
        assert!(read(&fx.store, &fx.id, T0 + MAX_AGE_SECS + 1)
            .await
            .is_none());
        assert!(!fx.restore().join("point").exists());
    }

    #[tokio::test]
    async fn damaged_point_reads_none() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        std::fs::write(fx.restore().join("point/point.json"), b"{ garbage").unwrap();

        assert!(fx.read().await.is_none());
        assert!(!fx.restore().join("point").exists());
    }

    #[tokio::test]
    async fn copy_over_cap_makes_no_point() {
        let fx = Fixture::new().await;
        fx.put("config/big.toml", b"0123456789");
        before_change_capped(
            &fx.store,
            &fx.id,
            &["config".to_string()],
            installed("Sodium"),
            ChangeCtx {
                during_run: false,
                now: T0,
            },
            4,
        )
        .await;

        assert!(fx.read().await.is_none());
        assert!(!fx.restore().join("new").exists());
    }

    /// The change goes ahead without a point, so the sealed one before it would restore a mix.
    #[tokio::test]
    async fn failing_to_replace_a_sealed_point_drops_it() {
        let fx = Fixture::new().await;
        fx.put("config/big.toml", b"0123456789");
        before_change_capped(
            &fx.store,
            &fx.id,
            &["mods".to_string()],
            installed("Sodium"),
            ChangeCtx {
                during_run: false,
                now: T0,
            },
            4,
        )
        .await;
        assert!(fx.read().await.is_some());
        seal(&fx.store, &fx.id).await;

        before_change_capped(
            &fx.store,
            &fx.id,
            &["config".to_string()],
            installed("Lithium"),
            ChangeCtx {
                during_run: false,
                now: T0 + 1,
            },
            4,
        )
        .await;
        assert!(fx.read().await.is_none());
        assert!(!fx.restore().join("point").exists());
        assert!(!fx.restore().join("new").exists());
    }

    #[tokio::test]
    async fn discard_removes_the_point_and_leftovers() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        std::fs::create_dir_all(fx.restore().join("new-config")).unwrap();
        crate::restore::discard(&fx.store, &fx.id).await;
        assert!(fx.read().await.is_none());
        assert!(!fx.restore().join("point").exists());
        assert!(!fx.restore().join("new-config").exists());
    }

    #[tokio::test]
    async fn leftover_new_dir_is_cleared() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        std::fs::create_dir_all(fx.restore().join("new/files/mods")).unwrap();
        std::fs::write(fx.restore().join("new/files/mods/stale.jar"), b"x").unwrap();
        fx.change(&["mods"], "Sodium", false, T0).await;

        assert!(fx.read().await.is_some());
        assert!(!fx.point_file("mods/stale.jar").exists());
        assert!(!fx.restore().join("new").exists());
    }

    #[tokio::test]
    async fn stale_old_dir_is_the_point_when_point_is_missing() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        // An interrupted swap: point/ renamed to old/, new/ never arrived.
        std::fs::rename(fx.restore().join("point"), fx.restore().join("old")).unwrap();
        assert_eq!(fx.read().await.unwrap().changes, [installed("Sodium")]);

        std::fs::rename(fx.restore().join("point"), fx.restore().join("old")).unwrap();
        fx.change(&["mods"], "Lithium", false, T0 + 1).await;
        let meta = fx.read().await.unwrap();
        assert_eq!(meta.changes, [installed("Sodium"), installed("Lithium")]);
        assert!(!fx.restore().join("old").exists());
    }

    #[tokio::test]
    async fn failed_swap_keeps_previous_point() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        // No new/ to swap in: the second rename fails.
        assert!(swap_in(&fx.restore()).await.is_err());
        assert_eq!(fx.read().await.unwrap().changes, [installed("Sodium")]);
    }

    #[tokio::test]
    async fn going_back_point_is_replaced_by_before_change() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        let mut meta = fx.read().await.unwrap();
        meta.kind = PointKind::BeforeGoingBack;
        write_meta(&fx.restore().join("point"), &meta)
            .await
            .unwrap();
        fx.change(&["mods"], "Lithium", false, T0 + 1).await;

        let meta = fx.read().await.unwrap();
        assert_eq!(meta.kind, PointKind::BeforeChange);
        assert_eq!(meta.changes, [installed("Lithium")]);
    }

    #[tokio::test]
    async fn point_keeps_managed_and_pack_json() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        let perland = fx.store.perland_dir(&fx.id);
        let managed = Managed {
            version: Managed::VERSION,
            stamp: "s".into(),
            pack_version: "1.4.1".into(),
            choices: Default::default(),
            files: Default::default(),
        };
        managed.save(&perland).await.unwrap();
        std::fs::write(perland.join(PACK_FILE), b"{\"pack\":true}").unwrap();
        fx.change(&["mods"], "Sodium", false, T0).await;

        let meta = fx.read().await.unwrap();
        assert_eq!(meta.pack_version.as_deref(), Some("1.4.1"));
        assert!(fx.restore().join("point/managed.json").exists());
        assert_eq!(
            std::fs::read(fx.restore().join("point/pack.json")).unwrap(),
            b"{\"pack\":true}"
        );
    }

    #[tokio::test]
    async fn note_change_appends_without_snapshot() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.change(&["mods"], "Sodium", false, T0).await;
        note_change(&fx.store, &fx.id, installed("Lithium"), T0 + 3).await;
        note_change(&fx.store, &fx.id, installed("Lithium"), T0 + 4).await;

        let meta = fx.read().await.unwrap();
        assert_eq!(meta.changes, [installed("Sodium"), installed("Lithium")]);
        assert_eq!(meta.last_change_at, T0 + 4);
    }

    #[tokio::test]
    async fn held_size_counts_what_the_game_dropped() {
        let fx = Fixture::new().await;
        fx.put("mods/old.jar", b"old");
        fx.put("config/a.toml", b"cf");
        fx.change(&["mods", "config"], "Sodium", false, T0).await;
        assert_eq!(held_size(&fx.store, &fx.id).await, 2);
        std::fs::remove_file(fx.game("mods/old.jar")).unwrap();
        assert_eq!(held_size(&fx.store, &fx.id).await, 5);
    }
}
