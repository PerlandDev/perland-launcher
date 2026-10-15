//! Cleanup of the object cache (`cache/objects`) and `cache/incoming`, at most once a day.
//!
//! The roots are what instances still reference: an imported `.mrpack` (`instance.json`), the
//! installed pack (`.perland/pack.json`: its files and archive) and what sync wrote
//! (`.perland/managed.json`). An object that is no root and went unused (its modified time,
//! which every reuse refreshes) for [`UNUSED_FOR`] is deleted; leftover partial downloads and
//! incoming packs go after [`LEFTOVER_AGE`]. If any record can't be read, nothing is deleted.
//! Best effort throughout: what can't be deleted is counted and skipped. Symlinks are never
//! followed, and nothing outside those two folders is touched.
use std::{
    collections::HashSet,
    fs,
    io::ErrorKind,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{de::DeserializeOwned, Deserialize};

use super::{
    cache::{object_key, CacheScope},
    Hashes,
};
use crate::{
    error::Result,
    instances::SourceRef,
    paths::LauncherDirs,
    sources::DesiredState,
    sync::lockfile::{Managed, MANAGED_FILE, PACK_FILE},
};

/// When the last cleanup ran (unix seconds), inside `cache/objects`.
const STAMP_FILE: &str = ".gc-stamp";

/// An object nothing references is deleted once it has gone unused this long.
pub const UNUSED_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// Leftover `.part`/`.tmp` files and incoming downloads are deleted after this long.
pub const LEFTOVER_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// The cleanup runs at most this often.
pub const RUN_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Why a cleanup deleted nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GcSkip {
    /// The last cleanup was less than [`RUN_EVERY`] ago.
    RanRecently,
    /// This many instance records exist but couldn't be read: what they reference is unknown.
    UnreadableRecords(usize),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GcReport {
    pub removed: usize,
    pub freed_bytes: u64,
    /// Files that should have gone but couldn't be deleted.
    pub failed: usize,
    pub skipped: Option<GcSkip>,
}

/// Clean the cache as of `now`, unless that was done less than [`RUN_EVERY`] ago. Logs one
/// line of counts.
pub async fn collect(dirs: &LauncherDirs, now: SystemTime) -> Result<GcReport> {
    let dirs = dirs.clone();
    let report = tokio::task::spawn_blocking(move || collect_blocking(&dirs, now))
        .await
        .map_err(std::io::Error::other)?;
    tracing::info!(
        removed = report.removed,
        freed_bytes = report.freed_bytes,
        failed = report.failed,
        skipped = ?report.skipped,
        "object cache cleanup"
    );
    Ok(report)
}

fn collect_blocking(dirs: &LauncherDirs, now: SystemTime) -> GcReport {
    let objects = dirs.objects();
    let stamp = objects.join(STAMP_FILE);
    if ran_recently(&stamp, now) {
        return skipped(GcSkip::RanRecently);
    }
    let roots = match roots(&dirs.instances()) {
        Ok(roots) => roots,
        Err(unreadable) => return skipped(GcSkip::UnreadableRecords(unreadable)),
    };
    let mut report = GcReport::default();
    sweep_objects(&objects, &roots, now, &mut report);
    sweep_incoming(&objects.with_file_name("incoming"), now, &mut report);
    if fs::create_dir_all(&objects).is_ok() {
        let _ = fs::write(&stamp, unix_secs(now).to_string());
    }
    report
}

fn skipped(why: GcSkip) -> GcReport {
    GcReport {
        skipped: Some(why),
        ..GcReport::default()
    }
}

fn unix_secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// A stamp from the future (the clock went back) doesn't count.
fn ran_recently(stamp: &Path, now: SystemTime) -> bool {
    let Some(secs) = fs::read_to_string(stamp)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
    else {
        return false;
    };
    let last = UNIX_EPOCH + Duration::from_secs(secs);
    now.duration_since(last).is_ok_and(|ago| ago < RUN_EVERY)
}

/// How long ago `meta`'s file was last modified; `None` when unknown or in the future.
fn age(meta: &fs::Metadata, now: SystemTime) -> Option<Duration> {
    now.duration_since(meta.modified().ok()?).ok()
}

/// `instance.json` holds more, but only the source matters here.
#[derive(Deserialize)]
struct InstanceSource {
    source: SourceRef,
}

/// Every object key an instance references, or how many records couldn't be read.
fn roots(instances: &Path) -> std::result::Result<HashSet<String>, usize> {
    let mut keys = HashSet::new();
    let entries = match fs::read_dir(instances) {
        Ok(rd) => rd,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(keys),
        Err(_) => return Err(1),
    };
    let mut unreadable = 0;
    for entry in entries {
        let Ok(entry) = entry else {
            unreadable += 1;
            continue;
        };
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().into_owned();
        let scope = CacheScope::for_instance(&id);
        let mut add = |hashes: &Hashes| {
            if let Some(key) = object_key(hashes, Some(&scope)) {
                keys.insert(key);
            }
        };
        let perland = dir.join(".perland");
        match read_record::<InstanceSource>(&dir.join("instance.json")) {
            Ok(Some(InstanceSource {
                source: SourceRef::Mrpack { sha1, .. },
            })) => add(&Hashes::sha1(sha1)),
            Ok(_) => {}
            Err(()) => unreadable += 1,
        }
        match read_record::<DesiredState>(&perland.join(PACK_FILE)) {
            Ok(Some(pack)) => {
                pack.files.iter().for_each(|f| add(&f.hashes));
                if let Some(archive) = &pack.archive {
                    add(archive);
                }
            }
            Ok(None) => {}
            Err(()) => unreadable += 1,
        }
        match read_record::<Managed>(&perland.join(MANAGED_FILE)) {
            Ok(Some(managed)) => managed.files.values().for_each(|f| add(&f.declared)),
            Ok(None) => {}
            Err(()) => unreadable += 1,
        }
    }
    if unreadable > 0 {
        Err(unreadable)
    } else {
        Ok(keys)
    }
}

/// `Ok(None)` when the file doesn't exist; `Err` when it exists but can't be read or parsed.
fn read_record<T: DeserializeOwned>(path: &Path) -> std::result::Result<Option<T>, ()> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|_| ()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(_) => Err(()),
    }
}

/// The key the cache used before murmur2-only objects were kept per instance: `m2-<digits>`.
fn is_global_murmur2(name: &str) -> bool {
    name.strip_prefix("m2-")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

fn sweep_objects(objects: &Path, roots: &HashSet<String>, now: SystemTime, report: &mut GcReport) {
    let Ok(entries) = fs::read_dir(objects) else {
        return;
    };
    for entry in entries.flatten() {
        // Not followed: a symlinked prefix folder is left alone.
        if !entry.file_type().is_ok_and(|t| t.is_dir()) {
            continue;
        }
        let prefix = entry.path();
        sweep_files(&prefix, now, report, |name, age| {
            if name.ends_with(".part") || name.ends_with(".tmp") {
                age.is_some_and(|a| a > LEFTOVER_AGE)
            } else if is_global_murmur2(name) {
                true
            } else {
                !roots.contains(name) && age.is_some_and(|a| a > UNUSED_FOR)
            }
        });
        if fs::read_dir(&prefix).is_ok_and(|mut d| d.next().is_none()) {
            let _ = fs::remove_dir(&prefix);
        }
    }
}

fn sweep_incoming(incoming: &Path, now: SystemTime, report: &mut GcReport) {
    sweep_files(incoming, now, report, |_, age| {
        age.is_some_and(|a| a > LEFTOVER_AGE)
    });
}

/// Delete the regular files directly in `dir` that `doomed(name, age)` picks.
fn sweep_files(
    dir: &Path,
    now: SystemTime,
    report: &mut GcReport,
    doomed: impl Fn(&str, Option<Duration>) -> bool,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        // `DirEntry` metadata doesn't follow symlinks: a link is never a regular file here.
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.file_type().is_file() {
            continue;
        }
        let name = entry.file_name();
        if !doomed(&name.to_string_lossy(), age(&meta, now)) {
            continue;
        }
        match fs::remove_file(entry.path()) {
            Ok(()) => {
                report.removed += 1;
                report.freed_bytes += meta.len();
            }
            Err(_) => report.failed += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, path::PathBuf};

    use super::*;
    use crate::{
        download::{
            cache::{object_key, CacheScope},
            hash::sha1_hex,
            Hashes,
        },
        sources::{DesiredState, FileMode, FileSource, PackAbout, PackFile, Side},
        sync::lockfile::{Managed, ManagedFile},
    };

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    struct Env {
        _tmp: tempfile::TempDir,
        dirs: LauncherDirs,
        now: SystemTime,
    }

    impl Env {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let dirs = LauncherDirs::new(tmp.path());
            Self {
                _tmp: tmp,
                dirs,
                now: SystemTime::now(),
            }
        }

        /// A file under `cache/objects/<name[..2]>/`, last modified `age` before `now`.
        fn object(&self, name: &str, age: Duration) -> PathBuf {
            let path = self.dirs.objects().join(&name[..2]).join(name);
            self.file(&path, age)
        }

        fn incoming(&self, name: &str, age: Duration) -> PathBuf {
            let path = self.dirs.root().join("cache/incoming").join(name);
            self.file(&path, age)
        }

        fn file(&self, path: &std::path::Path, age: Duration) -> PathBuf {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"bytes").unwrap();
            std::fs::File::options()
                .write(true)
                .open(path)
                .unwrap()
                .set_modified(self.now - age)
                .unwrap();
            path.to_path_buf()
        }

        fn record(&self, id: &str, rel: &str, bytes: &[u8]) {
            let path = self.dirs.instances().join(id).join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }

        async fn collect_at(&self, now: SystemTime) -> GcReport {
            collect(&self.dirs, now).await.unwrap()
        }

        async fn collect(&self) -> GcReport {
            self.collect_at(self.now).await
        }
    }

    fn key(body: &str) -> String {
        sha1_hex(body.as_bytes())
    }

    fn pack_file(hashes: Hashes) -> PackFile {
        PackFile {
            path: "mods/a.jar".into(),
            name: "a".into(),
            source: FileSource::Remote { urls: vec![] },
            hashes,
            size: None,
            side: Side::Both,
            optional: None,
            mode: FileMode::Replace,
            manual: None,
        }
    }

    fn pack_json(files: Vec<Hashes>, archive: Option<Hashes>) -> Vec<u8> {
        serde_json::to_vec(&DesiredState {
            name: "Tiny".into(),
            pack_version: "1.0.0".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files: files.into_iter().map(pack_file).collect(),
            stamp: "s1".into(),
            about: PackAbout::default(),
            archive,
        })
        .unwrap()
    }

    fn managed_json(declared: Hashes) -> Vec<u8> {
        serde_json::to_vec(&Managed {
            version: Managed::VERSION,
            stamp: "s1".into(),
            pack_version: "1.0.0".into(),
            choices: BTreeMap::new(),
            files: BTreeMap::from([(
                "mods/a.jar".to_string(),
                ManagedFile {
                    name: "a".into(),
                    sha1: String::new(),
                    size: 5,
                    mode: FileMode::Replace,
                    declared,
                },
            )]),
        })
        .unwrap()
    }

    fn mrpack_instance_json(sha1: &str) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "id": "a",
            "name": "A",
            "source": { "kind": "mrpack", "name": "a.mrpack", "sha1": sha1 },
            "game": { "minecraft": "1.20.1" },
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn referenced_objects_stay_and_long_unused_ones_go() {
        let env = Env::new();
        let old = 31 * DAY;
        let (file, archive, declared, mrpack) =
            (key("file"), key("archive"), key("declared"), key("mrpack"));
        env.record("a", "instance.json", &mrpack_instance_json(&mrpack));
        env.record(
            "a",
            ".perland/pack.json",
            &pack_json(vec![Hashes::sha1(&file)], Some(Hashes::sha1(&archive))),
        );
        env.record(
            "a",
            ".perland/managed.json",
            &managed_json(Hashes::sha1(&declared)),
        );
        let kept: Vec<_> = [&file, &archive, &declared, &mrpack]
            .into_iter()
            .map(|k| env.object(k, old))
            .collect();
        let unused = env.object(&key("unused"), old);
        let recent = env.object(&key("recent"), 29 * DAY);

        let report = env.collect().await;
        for path in &kept {
            assert!(path.exists(), "{path:?} is referenced");
        }
        assert!(!unused.exists());
        assert!(
            !unused.parent().unwrap().exists(),
            "its empty folder goes too"
        );
        assert!(recent.exists(), "used within 30 days");
        assert_eq!(report.removed, 1);
        assert_eq!(report.freed_bytes, 5);
        assert_eq!(report.skipped, None);
    }

    #[tokio::test]
    async fn murmur2_only_roots_keep_their_instances_object() {
        let env = Env::new();
        let m2 = Hashes {
            murmur2: Some(5),
            ..Default::default()
        };
        env.record(
            "a",
            ".perland/pack.json",
            &pack_json(vec![m2.clone()], None),
        );
        let a = object_key(&m2, Some(&CacheScope::for_instance("a"))).unwrap();
        let b = object_key(&m2, Some(&CacheScope::for_instance("b"))).unwrap();
        let (a, b) = (env.object(&a, 31 * DAY), env.object(&b, 31 * DAY));
        env.collect().await;
        assert!(a.exists());
        assert!(!b.exists(), "nothing in instance b references it");
    }

    #[tokio::test]
    async fn global_murmur2_objects_go_at_any_age() {
        let env = Env::new();
        let legacy = env.object("m2-123", Duration::ZERO);
        let scoped = env.object("m2-123-0123456789ab", Duration::ZERO);
        let report = env.collect().await;
        assert!(
            !legacy.exists(),
            "no lookup uses the old global key any more"
        );
        assert!(scoped.exists());
        assert_eq!(report.removed, 1);
    }

    #[tokio::test]
    async fn leftovers_go_after_a_day() {
        let env = Env::new();
        let k = key("x");
        let old_part = env.object(&format!("{k}.part"), 2 * DAY);
        let old_tmp = env.object(&format!("{k}.123.4.tmp"), 2 * DAY);
        let new_part = env.object(&format!("{}.part", key("y")), Duration::from_secs(60));
        let old_in = env.incoming("perland-a.mrpack", 2 * DAY);
        let new_in = env.incoming("perland-b.mrpack", Duration::from_secs(60));
        let report = env.collect().await;
        assert!(!old_part.exists() && !old_tmp.exists() && !old_in.exists());
        assert!(
            new_part.exists() && new_in.exists(),
            "maybe still being written"
        );
        assert_eq!(report.removed, 3);
    }

    #[tokio::test]
    async fn a_damaged_record_deletes_nothing() {
        let env = Env::new();
        env.record("a", ".perland/pack.json", b"{ not json");
        let unused = env.object(&key("unused"), 31 * DAY);
        let part = env.object(&format!("{}.part", key("x")), 2 * DAY);
        for _ in 0..2 {
            let report = env.collect().await;
            assert_eq!(report.skipped, Some(GcSkip::UnreadableRecords(1)));
            assert_eq!(report.removed, 0);
        }
        assert!(unused.exists() && part.exists());
    }

    #[tokio::test]
    async fn runs_at_most_once_a_day() {
        let env = Env::new();
        env.object(&key("one"), 31 * DAY);
        assert_eq!(env.collect().await.removed, 1);
        let two = env.object(&key("two"), 31 * DAY);
        let soon = env.collect_at(env.now + Duration::from_secs(60 * 60)).await;
        assert_eq!(soon.skipped, Some(GcSkip::RanRecently));
        assert!(two.exists());
        let tomorrow = env
            .collect_at(env.now + DAY + Duration::from_secs(60))
            .await;
        assert_eq!(tomorrow.skipped, None);
        assert!(!two.exists());
    }
}
