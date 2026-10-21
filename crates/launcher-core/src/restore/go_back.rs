//! Going back to the restore point, as one all-or-nothing swap of the instance's folders
//! guarded by a journal (`restore/restoring.json`), and finishing a swap that was interrupted
//! (spec §5.5 and §5.6). Everything before the journal only writes into `restore/` and into
//! `minecraft/.<top>.perland-restore/`, so failing there changes nothing the game sees. Once the
//! journal exists, every staged folder is complete and recovery rolls forward.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};

use super::{
    point::{self, build_point, clear_scratch, load_meta, point_dir, recover_old, swap_in},
    restore_dir, top_of, PointKind, SnapFile,
};
use crate::{
    error::{restore as errors, Result},
    fsutil::{rename_with_retry, write_atomic},
    instances::{InstanceStore, UpdateHold},
    sync::{
        apply::{ensure_space, free_space},
        lockfile::{MANAGED_FILE, PACK_FILE},
    },
};

const JOURNAL_FILE: &str = "restoring.json";
const JOURNAL_VERSION: u32 = 1;
const STAGED_SUFFIX: &str = ".perland-restore";

/// What going back did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WentBack {
    /// Linked files changed in place since the point was taken: the current file was kept.
    pub kept_current: u32,
}

/// Where a test stops `go_back`, as if the launcher died right after that step. A real
/// `go_back` never stops; the variants exist outside tests only as checkpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopAfter {
    Staged,
    Journal,
    /// After swapping the folder at this index of the journal's `tops`.
    Swapped(usize),
    Bookkeeping,
    Hold,
}

/// `restore/restoring.json`: written once every folder is staged, removed when done.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Journal {
    version: u32,
    /// The folders being swapped, in order.
    tops: Vec<String>,
    /// Set on the instance (or cleared, when `None`) as part of the restore.
    hold: Option<UpdateHold>,
    /// `false` when the point had no such folder: the current one is moved aside, not replaced.
    had: BTreeMap<String, bool>,
}

fn staged_name(top: &str) -> String {
    format!(".{top}{STAGED_SUFFIX}")
}

fn is_staged_name(name: &str) -> bool {
    name.len() > STAGED_SUFFIX.len() + 1 && name.starts_with('.') && name.ends_with(STAGED_SUFFIX)
}

/// Present, without following a link.
async fn exists(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path).await.is_ok()
}

/// Caller holds the instance lock and has checked the game isn't running.
/// `hold`: set on the instance as part of the restore (the app decides it).
pub async fn go_back(
    store: &InstanceStore,
    id: &str,
    hold: Option<UpdateHold>,
    now: u64,
) -> Result<WentBack> {
    run(store, id, hold, now, None).await
}

#[cfg(test)]
async fn go_back_stopping(
    store: &InstanceStore,
    id: &str,
    hold: Option<UpdateHold>,
    now: u64,
    stop: StopAfter,
) -> Result<WentBack> {
    run(store, id, hold, now, Some(stop)).await
}

async fn run(
    store: &InstanceStore,
    id: &str,
    hold: Option<UpdateHold>,
    now: u64,
    stop: Option<StopAfter>,
) -> Result<WentBack> {
    // 1. An interrupted go-back that recovery finished already did what the player asked.
    if recover_inner(store, id).await? {
        return Ok(WentBack { kept_current: 0 });
    }
    let restore = restore_dir(&store.perland_dir(id));
    let game_dir = store.game_dir(id);
    let meta = point::read(store, id, now)
        .await
        .ok_or_else(errors::point_gone)?;
    // A linked folder (symlink or junction) was never snapshotted: leave it alone.
    let mut tops = Vec::new();
    for top in &meta.scope {
        let linked = tokio::fs::symlink_metadata(game_dir.join(top))
            .await
            .is_ok_and(|m| m.file_type().is_symlink());
        if !linked {
            tops.push(top.clone());
        }
    }

    // 2–3. The point of now, waiting in `new/`, and the staged folders.
    let prepared = async {
        clear_scratch(&restore).await;
        build_point(
            store,
            id,
            &meta.scope,
            PointKind::BeforeGoingBack,
            now,
            &restore.join("new"),
        )
        .await
        .map_err(errors::failed)?;
        stage(
            game_dir.clone(),
            point_dir(&restore).join("files"),
            tops.clone(),
            meta.files.clone(),
        )
        .await
    }
    .await;
    let (had, kept_current) = match prepared {
        Ok(prepared) => prepared,
        Err(e) => {
            discard(&game_dir, &restore).await;
            return Err(e);
        }
    };
    if stop == Some(StopAfter::Staged) {
        return Ok(WentBack { kept_current });
    }

    // 4. The journal: from here on, recovery rolls forward.
    let journal = Journal {
        version: JOURNAL_VERSION,
        tops,
        hold,
        had,
    };
    let bytes = serde_json::to_vec_pretty(&journal).map_err(errors::failed)?;
    if let Err(e) = write_atomic(&restore.join(JOURNAL_FILE), &bytes).await {
        discard(&game_dir, &restore).await;
        return Err(errors::failed(e));
    }
    if stop == Some(StopAfter::Journal) {
        return Ok(WentBack { kept_current });
    }

    finish(store, id, &journal, stop).await?;
    Ok(WentBack { kept_current })
}

/// Build `game_dir/.<top>.perland-restore/` for each of `tops` from the point's files. Linked
/// files whose size or mtime changed were edited in place: the current file is kept instead,
/// and counted. Returns which folders the point had, and that count.
async fn stage(
    game_dir: PathBuf,
    point_files: PathBuf,
    tops: Vec<String>,
    files: Vec<SnapFile>,
) -> Result<(BTreeMap<String, bool>, u32)> {
    tokio::task::spawn_blocking(move || {
        let in_tops = |f: &&SnapFile| top_of(&f.path).is_some_and(|t| tops.iter().any(|x| x == t));
        let copied: u64 = files
            .iter()
            .filter(in_tops)
            .filter(|f| !f.linked)
            .map(|f| f.size)
            .sum();
        ensure_space(copied, free_space(&game_dir))?;

        let mut had = BTreeMap::new();
        let mut kept = 0u32;
        for top in &tops {
            let staged = game_dir.join(staged_name(top));
            let _ = std::fs::remove_dir_all(&staged);
            let present = point_files.join(top).is_dir();
            had.insert(top.clone(), present);
            if !present {
                continue;
            }
            std::fs::create_dir_all(&staged).map_err(errors::failed)?;
            for f in files.iter().filter(|f| top_of(&f.path) == Some(top)) {
                let rel = &f.path[top.len() + 1..];
                let (from, to) = (point_files.join(&f.path), staged.join(rel));
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(errors::failed)?;
                }
                if f.linked && !unchanged(&from, f) {
                    kept += 1;
                    let current = game_dir.join(&f.path);
                    if current.is_file() {
                        link_or_copy(&current, &to).map_err(errors::failed)?;
                    }
                    continue;
                }
                if f.linked {
                    link_or_copy(&from, &to).map_err(errors::failed)?;
                } else {
                    std::fs::copy(&from, &to).map_err(errors::failed)?;
                }
            }
        }
        Ok((had, kept))
    })
    .await
    .map_err(errors::failed)?
}

/// The point's linked file still has the size and mtime it was taken with.
fn unchanged(path: &Path, file: &SnapFile) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return false;
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs());
    meta.is_file() && meta.len() == file.size && mtime == file.mtime
}

fn link_or_copy(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::hard_link(from, to).or_else(|_| std::fs::copy(from, to).map(|_| ()))
}

/// Steps 5–9: swap the folders, put back the bookkeeping and the hold, undo the setting aside,
/// and swap in the point of how it was before going back. Each step can be redone.
async fn finish(
    store: &InstanceStore,
    id: &str,
    journal: &Journal,
    stop: Option<StopAfter>,
) -> Result<()> {
    let restore = restore_dir(&store.perland_dir(id));
    let game_dir = store.game_dir(id);
    let aside = restore.join("aside");

    // 5. Swap each folder not swapped yet.
    for (i, top) in journal.tops.iter().enumerate() {
        let (current, staged) = (game_dir.join(top), game_dir.join(staged_name(top)));
        let had = journal.had.get(top).copied().unwrap_or(false);
        let pending = if had {
            exists(&staged).await
        } else {
            exists(&current).await
        };
        if pending {
            if exists(&current).await {
                tokio::fs::create_dir_all(&aside)
                    .await
                    .map_err(errors::failed)?;
                let _ = tokio::fs::remove_dir_all(aside.join(top)).await;
                rename_with_retry(&current, &aside.join(top))
                    .await
                    .map_err(errors::failed)?;
            }
            if had {
                rename_with_retry(&staged, &current)
                    .await
                    .map_err(errors::failed)?;
            }
        }
        if stop == Some(StopAfter::Swapped(i)) {
            return Ok(());
        }
    }

    // 6. Bookkeeping, from the point being restored.
    put_back_bookkeeping(store, id, &restore).await?;
    if stop == Some(StopAfter::Bookkeeping) {
        return Ok(());
    }

    // 7. The hold (reloaded, so a rename meanwhile is kept).
    let mut instance = store.load(id).await?;
    if instance.update_hold != journal.hold {
        instance.update_hold = journal.hold.clone();
        store.save(&instance).await?;
    }
    if stop == Some(StopAfter::Hold) {
        return Ok(());
    }

    // 8. A jar set aside that is back in `mods/` would be set aside twice by "Update again".
    let game = game_dir.clone();
    let _ = tokio::task::spawn_blocking(move || clear_set_aside(&game)).await;

    // 9. The point of before going back becomes the point; then the journal goes.
    swap_in(&restore).await.map_err(errors::failed)?;
    if let Err(e) = tokio::fs::remove_dir_all(&aside).await {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!("couldn't remove the folders set aside by going back: {e}");
        }
    }
    remove_journal(&restore).await
}

async fn remove_journal(restore: &Path) -> Result<()> {
    match tokio::fs::remove_file(restore.join(JOURNAL_FILE)).await {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(errors::failed(e)),
        _ => Ok(()),
    }
}

/// `managed.json` and `pack.json` (deleted when the point had none), and `game`, `source` and
/// `optional_choices` onto a freshly loaded instance. Reads `point/`, so it can be redone.
async fn put_back_bookkeeping(store: &InstanceStore, id: &str, restore: &Path) -> Result<()> {
    recover_old(restore).await;
    let Some(meta) = load_meta(restore).await else {
        // Only a point deleted from outside gets here. The folders are back; refusing to
        // finish would leave Play blocked for good.
        tracing::warn!("the restore point went missing while going back");
        return Ok(());
    };
    let perland = store.perland_dir(id);
    let point = point_dir(restore);
    for name in [MANAGED_FILE, PACK_FILE] {
        match tokio::fs::read(point.join(name)).await {
            Ok(bytes) => write_atomic(&perland.join(name), &bytes)
                .await
                .map_err(errors::failed)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match tokio::fs::remove_file(perland.join(name)).await {
                    Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                        return Err(errors::failed(e))
                    }
                    _ => {}
                }
            }
            Err(e) => return Err(errors::failed(e)),
        }
    }
    let mut instance = store.load(id).await?;
    instance.game = meta.game;
    instance.source = meta.source;
    instance.optional_choices = meta.optional_choices;
    store.save(&instance).await
}

/// Remove each `mods-before-link/<name>` that is the very same file as `mods/<name>`.
fn clear_set_aside(game_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(game_dir.join("mods-before-link")) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let back = game_dir.join("mods").join(entry.file_name());
        if path.is_file() && same_file::is_same_file(&path, &back).unwrap_or(false) {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!("couldn't clear a set-aside duplicate: {e}");
            }
        }
    }
}

/// Remove what an unjournalled go-back left: staged folders, `aside/`, `new/` and the like.
async fn discard(game_dir: &Path, restore: &Path) {
    if let Ok(mut entries) = tokio::fs::read_dir(game_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            if is_staged_name(&entry.file_name().to_string_lossy()) {
                let _ = tokio::fs::remove_dir_all(entry.path()).await;
            }
        }
    }
    let _ = tokio::fs::remove_dir_all(restore.join("aside")).await;
    clear_scratch(restore).await;
}

/// Finish or clean an interrupted go_back. Ok(()) when there was nothing to do.
pub async fn recover(store: &InstanceStore, id: &str) -> Result<()> {
    recover_inner(store, id).await.map(|_| ())
}

/// `true` when a journalled go-back was rolled forward.
async fn recover_inner(store: &InstanceStore, id: &str) -> Result<bool> {
    let restore = restore_dir(&store.perland_dir(id));
    let game_dir = store.game_dir(id);
    let bytes = match tokio::fs::read(restore.join(JOURNAL_FILE)).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            discard(&game_dir, &restore).await;
            return Ok(false);
        }
        Err(e) => return Err(errors::failed(e)),
    };
    let journal = serde_json::from_slice::<Journal>(&bytes)
        .ok()
        .filter(|j| j.version == JOURNAL_VERSION)
        .ok_or_else(|| errors::failed("the restore journal is damaged"))?;
    recover_old(&restore).await;
    if exists(&restore.join("new")).await {
        finish(store, id, &journal, None).await?;
    } else {
        // Step 9 swapped the point in already: only the cleanup is left.
        discard(&game_dir, &restore).await;
        remove_journal(&restore).await?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        path::{Path, PathBuf},
    };

    use super::*;
    use crate::{
        error::ErrorCode,
        instances::GameSpec,
        restore::{before_change, read, restore_dir, Change, ChangeCtx, PointKind},
        sync::lockfile::{Managed, MANAGED_FILE},
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
        fn text(&self, rel: &str) -> String {
            std::fs::read_to_string(self.game(rel)).unwrap()
        }
        fn names(&self, top: &str) -> BTreeSet<String> {
            std::fs::read_dir(self.game(top))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        }
        fn restore(&self) -> PathBuf {
            restore_dir(&self.store.perland_dir(&self.id))
        }
        async fn set_managed(&self, version: &str) {
            Managed {
                version: Managed::VERSION,
                stamp: "s".into(),
                pack_version: version.into(),
                choices: Default::default(),
                files: Default::default(),
            }
            .save(&self.store.perland_dir(&self.id))
            .await
            .unwrap();
        }
        async fn managed(&self) -> Option<String> {
            Managed::load(&self.store.perland_dir(&self.id))
                .await
                .map(|m| m.pack_version)
        }
        async fn set_game(&self, minecraft: &str) {
            let mut instance = self.store.load(&self.id).await.unwrap();
            instance.game.minecraft = minecraft.into();
            self.store.save(&instance).await.unwrap();
        }
        async fn minecraft(&self) -> String {
            self.store.load(&self.id).await.unwrap().game.minecraft
        }
        async fn snapshot(&self, tops: &[&str]) {
            let tops: Vec<String> = tops.iter().map(|t| t.to_string()).collect();
            before_change(
                &self.store,
                &self.id,
                &tops,
                Change::PackUpdate {
                    from: "1".into(),
                    to: "2".into(),
                },
                ChangeCtx {
                    during_run: false,
                    now: T0,
                },
            )
            .await;
        }

        /// mods/{a.jar}, config/c.toml = "1", managed v1, game 1.20.1, a world and options;
        /// a point of mods, config and shaderpacks (which doesn't exist yet); then the update:
        /// mods/{b.jar}, config/c.toml = "2", managed v2, game 1.20.4, shaderpacks/s.zip.
        async fn updated() -> Self {
            let fx = Fixture::new().await;
            fx.put("mods/a.jar", b"a");
            fx.put("config/c.toml", b"1");
            fx.put("saves/w/level.dat", b"world");
            fx.put("options.txt", b"opts");
            fx.set_managed("1").await;
            fx.snapshot(&["mods", "config", "shaderpacks"]).await;

            std::fs::remove_file(fx.game("mods/a.jar")).unwrap();
            fx.put("mods/b.jar", b"bb");
            fx.put("config/c.toml", b"2");
            fx.put("shaderpacks/s.zip", b"s");
            fx.set_managed("2").await;
            fx.set_game("1.20.4").await;
            fx
        }

        async fn go_back(&self, hold: Option<UpdateHold>) -> Result<WentBack> {
            go_back(&self.store, &self.id, hold, T0 + 10).await
        }

        async fn assert_before(&self) {
            assert_eq!(self.names("mods"), set(&["a.jar"]));
            assert_eq!(self.text("config/c.toml"), "1");
            assert!(!self.game("shaderpacks").exists());
            assert_eq!(self.managed().await.as_deref(), Some("1"));
            assert_eq!(self.minecraft().await, "1.20.1");
            self.assert_untouched();
        }

        async fn assert_after(&self) {
            assert_eq!(self.names("mods"), set(&["b.jar"]));
            assert_eq!(self.text("config/c.toml"), "2");
            assert_eq!(self.names("shaderpacks"), set(&["s.zip"]));
            assert_eq!(self.managed().await.as_deref(), Some("2"));
            assert_eq!(self.minecraft().await, "1.20.4");
            self.assert_untouched();
        }

        fn assert_untouched(&self) {
            assert_eq!(self.text("saves/w/level.dat"), "world");
            assert_eq!(self.text("options.txt"), "opts");
        }

        fn assert_no_leftovers(&self) {
            for entry in std::fs::read_dir(self.store.game_dir(&self.id)).unwrap() {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                assert!(!name.ends_with(".perland-restore"), "{name}");
            }
            for name in ["aside", "new", "old", "restoring.json"] {
                assert!(!self.restore().join(name).exists(), "{name}");
            }
        }
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    fn exists(path: &Path) -> bool {
        path.exists()
    }

    #[tokio::test]
    async fn go_back_restores_folders_and_bookkeeping() {
        let fx = Fixture::updated().await;
        let went = fx.go_back(None).await.unwrap();
        assert_eq!(went.kept_current, 0);
        fx.assert_before().await;
        fx.assert_no_leftovers();
    }

    #[tokio::test]
    async fn go_back_keeps_after_state_as_point() {
        let fx = Fixture::updated().await;
        fx.go_back(None).await.unwrap();
        let meta = read(&fx.store, &fx.id, T0 + 10).await.unwrap();
        assert_eq!(meta.kind, PointKind::BeforeGoingBack);
        assert_eq!(meta.game.minecraft, "1.20.4");
        assert_eq!(meta.pack_version.as_deref(), Some("2"));
        assert!(exists(&fx.restore().join("point/files/mods/b.jar")));
        assert!(meta.files.iter().any(|f| f.path == "mods/b.jar"));
    }

    #[tokio::test]
    async fn go_back_twice_returns_to_after_state() {
        let fx = Fixture::updated().await;
        fx.go_back(None).await.unwrap();
        fx.go_back(None).await.unwrap();
        fx.assert_after().await;
        fx.assert_no_leftovers();
        let meta = read(&fx.store, &fx.id, T0 + 10).await.unwrap();
        assert_eq!(meta.kind, PointKind::BeforeGoingBack);
        assert_eq!(meta.game.minecraft, "1.20.1");
    }

    #[tokio::test]
    async fn go_back_removes_folder_that_did_not_exist() {
        let fx = Fixture::updated().await;
        assert!(fx.game("shaderpacks").exists());
        fx.go_back(None).await.unwrap();
        assert!(!fx.game("shaderpacks").exists());
    }

    #[tokio::test]
    async fn go_back_keeps_a_folder_that_was_empty() {
        let fx = Fixture::new().await;
        std::fs::create_dir_all(fx.game("shaderpacks")).unwrap();
        fx.snapshot(&["shaderpacks"]).await;
        fx.put("shaderpacks/s.zip", b"s");
        fx.go_back(None).await.unwrap();
        assert!(fx.game("shaderpacks").is_dir());
        assert!(fx.names("shaderpacks").is_empty());
    }

    #[tokio::test]
    async fn go_back_sets_and_clears_hold() {
        let fx = Fixture::updated().await;
        let hold = UpdateHold {
            since: 5,
            version: "1.0".into(),
        };
        fx.go_back(Some(hold.clone())).await.unwrap();
        let instance = fx.store.load(&fx.id).await.unwrap();
        assert_eq!(instance.update_hold, Some(hold));

        fx.go_back(None).await.unwrap();
        let instance = fx.store.load(&fx.id).await.unwrap();
        assert_eq!(instance.update_hold, None);
    }

    #[tokio::test]
    async fn go_back_keeps_a_rename_made_meanwhile() {
        let fx = Fixture::updated().await;
        let mut instance = fx.store.load(&fx.id).await.unwrap();
        instance.name = "Test Renamed".into();
        fx.store.save(&instance).await.unwrap();

        fx.go_back(None).await.unwrap();
        let instance = fx.store.load(&fx.id).await.unwrap();
        assert_eq!(instance.name, "Test Renamed");
        assert_eq!(instance.game.minecraft, "1.20.1");
    }

    #[tokio::test]
    async fn go_back_without_point_is_point_gone() {
        let fx = Fixture::new().await;
        fx.put("mods/a.jar", b"a");
        let err = fx.go_back(None).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::RestorePointGone);
        assert_eq!(fx.names("mods"), set(&["a.jar"]));
    }

    #[tokio::test]
    async fn go_back_deletes_bookkeeping_the_point_did_not_have() {
        let fx = Fixture::new().await;
        fx.put("mods/a.jar", b"a");
        fx.snapshot(&["mods"]).await;
        fx.set_managed("2").await;
        fx.go_back(None).await.unwrap();
        assert!(!fx.store.perland_dir(&fx.id).join(MANAGED_FILE).exists());
    }

    #[tokio::test]
    async fn in_place_edited_jar_is_kept_and_counted() {
        let fx = Fixture::new().await;
        fx.put("mods/a.jar", b"a");
        fx.put("mods/keep.jar", b"k");
        fx.snapshot(&["mods"]).await;
        // Same inode, which the point shares: the point's copy changes too.
        std::fs::write(fx.game("mods/a.jar"), b"edited").unwrap();
        std::fs::remove_file(fx.game("mods/keep.jar")).unwrap();

        let went = fx.go_back(None).await.unwrap();
        assert_eq!(went.kept_current, 1);
        assert_eq!(fx.text("mods/a.jar"), "edited");
        assert_eq!(fx.text("mods/keep.jar"), "k");
    }

    #[tokio::test]
    async fn mods_before_link_duplicate_is_cleared() {
        let fx = Fixture::new().await;
        fx.put("mods/p.jar", b"p");
        fx.put("mods-before-link/other.jar", b"o");
        std::fs::hard_link(fx.game("mods/p.jar"), fx.game("mods-before-link/p.jar")).unwrap();
        fx.snapshot(&["mods"]).await;
        // The update set p.jar aside.
        std::fs::remove_file(fx.game("mods/p.jar")).unwrap();

        fx.go_back(None).await.unwrap();
        assert!(!fx.game("mods-before-link/p.jar").exists());
        assert!(fx.game("mods-before-link/other.jar").exists());
        assert!(fx.game("mods/p.jar").exists());
    }

    #[tokio::test]
    async fn go_back_after_an_interrupted_one_finishes_it_only() {
        let fx = Fixture::updated().await;
        go_back_stopping(&fx.store, &fx.id, None, T0 + 10, StopAfter::Journal)
            .await
            .unwrap();
        fx.go_back(None).await.unwrap();
        fx.assert_before().await;
        fx.assert_no_leftovers();
    }

    #[tokio::test]
    async fn recover_without_anything_to_do_is_ok() {
        let fx = Fixture::new().await;
        recover(&fx.store, &fx.id).await.unwrap();
    }

    #[tokio::test]
    async fn recover_rolls_forward_after_each_step() {
        let stops = [
            StopAfter::Staged,
            StopAfter::Journal,
            StopAfter::Swapped(0),
            StopAfter::Swapped(1),
            StopAfter::Swapped(2),
            StopAfter::Bookkeeping,
            StopAfter::Hold,
        ];
        for stop in stops {
            let fx = Fixture::updated().await;
            let hold = UpdateHold {
                since: 7,
                version: "1".into(),
            };
            go_back_stopping(&fx.store, &fx.id, Some(hold.clone()), T0 + 10, stop)
                .await
                .unwrap();
            recover(&fx.store, &fx.id).await.unwrap();

            let instance = fx.store.load(&fx.id).await.unwrap();
            let meta = read(&fx.store, &fx.id, T0 + 10).await.unwrap();
            if stop == StopAfter::Staged {
                fx.assert_after().await;
                assert_eq!(instance.update_hold, None, "{stop:?}");
                assert_eq!(meta.kind, PointKind::BeforeChange, "{stop:?}");
            } else {
                fx.assert_before().await;
                assert_eq!(instance.update_hold, Some(hold), "{stop:?}");
                assert_eq!(meta.kind, PointKind::BeforeGoingBack, "{stop:?}");
            }
            fx.assert_no_leftovers();
        }
    }
}
