//! The skin catalog's offline cache: the newest manifest and the PNGs it names, kept under one
//! folder. Files are named by their SHA-256 and re-verified on every read.

use std::{
    collections::BTreeSet,
    future::Future,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    client::{CatalogClient, Fetched},
    manifest::{Manifest, FORMAT_VERSION},
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
};

/// How a [`SkinCatalog::load`] ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogStatus {
    /// The Engine answered and the manifest is current.
    Fresh,
    /// The cached manifest is shown while the Engine is asked in the background: load again
    /// shortly for the answer.
    Refreshing,
    /// The Engine could not be reached; the cached manifest is shown.
    Cached,
    /// Nothing cached and the Engine could not be reached.
    Unavailable,
    /// The Engine's catalog is a newer format than this launcher reads.
    TooNew,
}

/// Where the catalog comes from. [`CatalogClient`] is the real one; tests inject a fake.
pub trait CatalogSource: Send + Sync {
    fn manifest(&self, etag: Option<&str>) -> impl Future<Output = Result<Fetched>> + Send;
    /// The PNG named by `sha256`. Implementations should verify it; callers do not trust that.
    fn file(&self, sha256: &str) -> impl Future<Output = Result<Vec<u8>>> + Send;
}

impl CatalogSource for CatalogClient {
    fn manifest(&self, etag: Option<&str>) -> impl Future<Output = Result<Fetched>> + Send {
        CatalogClient::manifest(self, etag)
    }
    fn file(&self, sha256: &str) -> impl Future<Output = Result<Vec<u8>>> + Send {
        CatalogClient::file(self, sha256)
    }
}

fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone)]
pub struct CatalogCache {
    dir: PathBuf,
}

impl CatalogCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    fn files_dir(&self) -> PathBuf {
        self.dir.join("files")
    }

    fn file_path(&self, sha256: &str) -> Option<PathBuf> {
        let ok = sha256.len() == 64
            && sha256
                .bytes()
                .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        ok.then(|| self.files_dir().join(format!("{sha256}.png")))
    }

    /// The cached manifest; a missing or damaged one is no cache.
    pub async fn manifest(&self) -> Option<Manifest> {
        let bytes = tokio::fs::read(self.manifest_path()).await.ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub async fn store_manifest(&self, m: &Manifest) -> Result<()> {
        let bytes = serde_json::to_vec(m)
            .map_err(|e| crate::error::LauncherError::from(std::io::Error::other(e.to_string())))?;
        write_atomic(&self.manifest_path(), &bytes).await
    }

    /// A cached file whose bytes still hash to its name; a mismatch is deleted.
    pub async fn file(&self, sha256: &str) -> Option<Vec<u8>> {
        let path = self.file_path(sha256)?;
        let bytes = tokio::fs::read(&path).await.ok()?;
        if sha256_hex(&bytes) == sha256 {
            Some(bytes)
        } else {
            let _ = tokio::fs::remove_file(&path).await;
            None
        }
    }

    pub async fn store_file(&self, sha256: &str, png: &[u8]) -> Result<()> {
        match self.file_path(sha256) {
            Some(path) => write_atomic(&path, png).await,
            None => Ok(()),
        }
    }

    /// Deletes every cached file not in `keep`; returns how many. Never touches the manifest.
    pub async fn prune(&self, keep: &BTreeSet<String>) -> Result<usize> {
        let mut removed = 0;
        let mut entries = match tokio::fs::read_dir(self.files_dir()).await {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(stem) = name.strip_suffix(".png") else {
                continue;
            };
            if !is_hash(stem) {
                continue;
            }
            let stem = stem.to_string();
            if !keep.contains(&stem) && tokio::fs::remove_file(entry.path()).await.is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// How long a background refresh's answer is handed to the next [`SkinCatalog::load`]; after
/// that, the next load asks the Engine again.
const REFRESH_KEPT: Duration = Duration::from_secs(30);

type Loaded = (Option<Manifest>, CatalogStatus);

/// The background refresh that [`SkinCatalog::load`] starts while it answers from the cache.
enum Refresh {
    Idle,
    Running,
    Done(Loaded, Instant),
}

/// The catalog as the launcher uses it. With a cached manifest it answers from the cache at once
/// (`Refreshing`) and asks the Engine in the background; the next load hands over that answer.
/// With nothing cached it waits for the Engine.
pub struct SkinCatalog<C = CatalogClient> {
    client: C,
    cache: CatalogCache,
    refresh: Mutex<Refresh>,
}

impl<C: CatalogSource + 'static> SkinCatalog<C> {
    pub fn new(client: C, cache: CatalogCache) -> Self {
        Self {
            client,
            cache,
            refresh: Mutex::new(Refresh::Idle),
        }
    }

    pub async fn load(self: &Arc<Self>) -> Loaded {
        // A cached manifest newer than this launcher reads (after a downgrade) is no cache.
        let mut cached_too_new = false;
        let cached = self.cache.manifest().await.filter(|m| {
            cached_too_new = m.format_version > FORMAT_VERSION;
            !cached_too_new
        });
        if cached.is_none() {
            return self.fetch(None, cached_too_new).await;
        }
        let mut refresh = self.refresh.lock().unwrap_or_else(|e| e.into_inner());
        match &*refresh {
            Refresh::Done(loaded, at) if at.elapsed() < REFRESH_KEPT => {
                let loaded = loaded.clone();
                *refresh = Refresh::Idle;
                return loaded;
            }
            Refresh::Running => return (cached, CatalogStatus::Refreshing),
            _ => {}
        }
        *refresh = Refresh::Running;
        drop(refresh);
        let this = Arc::clone(self);
        let shown = cached.clone();
        tokio::spawn(async move {
            let loaded = this.fetch(shown, false).await;
            *this.refresh.lock().unwrap_or_else(|e| e.into_inner()) =
                Refresh::Done(loaded, Instant::now());
        });
        (cached, CatalogStatus::Refreshing)
    }

    /// Asks the Engine, with `cached` as what is shown when it can't say anything newer.
    async fn fetch(&self, cached: Option<Manifest>, cached_too_new: bool) -> Loaded {
        let etag = cached.as_ref().map(|m| m.version.clone());
        let nothing = |too_new: bool| {
            (
                None,
                if too_new {
                    CatalogStatus::TooNew
                } else {
                    CatalogStatus::Unavailable
                },
            )
        };
        match self.client.manifest(etag.as_deref()).await {
            Ok(Fetched::NotModified) => match cached {
                Some(m) => (Some(m), CatalogStatus::Fresh),
                None => nothing(cached_too_new),
            },
            Ok(Fetched::TooNew) => (cached, CatalogStatus::TooNew),
            Ok(Fetched::Manifest(m)) if m.format_version > FORMAT_VERSION => {
                (cached, CatalogStatus::TooNew)
            }
            Ok(Fetched::Manifest(m)) => {
                if let Err(e) = self.cache.store_manifest(&m).await {
                    tracing::warn!("couldn't cache the skin catalog: {}", e.message);
                }
                (Some(m), CatalogStatus::Fresh)
            }
            Err(_) => match cached {
                Some(m) => (Some(m), CatalogStatus::Cached),
                None => nothing(cached_too_new),
            },
        }
    }

    /// A catalog PNG: from the cache, else fetched, verified by the client and cached.
    pub async fn file(&self, sha256: &str) -> Result<Vec<u8>> {
        if let Some(bytes) = self.cache.file(sha256).await {
            return Ok(bytes);
        }
        let bytes = self.client.file(sha256).await?;
        if sha256_hex(&bytes) != sha256 {
            return Err(LauncherError::network(
                ErrorCode::SkinCatalogUnavailable,
                "Couldn't reach Perland's skin catalog.",
            )
            .with_detail("a catalog file did not match its hash"));
        }
        if let Err(e) = self.cache.store_file(sha256, &bytes).await {
            tracing::warn!("couldn't cache a skin catalog file: {}", e.message);
        }
        Ok(bytes)
    }

    /// Deletes cached files the newest manifest and the wardrobe no longer name.
    pub async fn prune(&self, wardrobe_keep: &BTreeSet<String>) -> Result<usize> {
        let Some(m) = self.cache.manifest().await else {
            return Ok(0);
        };
        let mut keep = wardrobe_keep.clone();
        keep.extend(m.hashes());
        self.cache.prune(&keep).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct Fake {
        manifest: Mutex<Option<Result<Fetched>>>,
        files: Mutex<Vec<(String, Vec<u8>)>>,
        file_calls: Mutex<usize>,
    }

    impl Fake {
        fn new(manifest: Option<Result<Fetched>>) -> Self {
            Self {
                manifest: Mutex::new(manifest),
                files: Mutex::new(vec![]),
                file_calls: Mutex::new(0),
            }
        }
    }

    impl CatalogSource for Fake {
        async fn manifest(&self, _etag: Option<&str>) -> Result<Fetched> {
            self.manifest
                .lock()
                .unwrap()
                .take()
                .unwrap_or_else(|| Err(offline()))
        }
        async fn file(&self, sha256: &str) -> Result<Vec<u8>> {
            *self.file_calls.lock().unwrap() += 1;
            self.files
                .lock()
                .unwrap()
                .iter()
                .find(|(h, _)| h == sha256)
                .map(|(_, b)| b.clone())
                .ok_or_else(offline)
        }
    }

    fn offline() -> LauncherError {
        LauncherError::network(ErrorCode::SkinCatalogUnavailable, "offline")
    }

    fn manifest(version: &str, format: u32) -> Manifest {
        Manifest {
            format_version: format,
            version: version.into(),
            license: "CC0-1.0".into(),
            slots: vec![],
            tags: vec![],
            pieces: vec![],
            looks: vec![],
        }
    }

    fn catalog(dir: &std::path::Path, fake: Fake) -> Arc<SkinCatalog<Fake>> {
        Arc::new(SkinCatalog::new(fake, CatalogCache::new(dir)))
    }

    /// Waits for the background refresh a load started.
    async fn settle(cat: &SkinCatalog<Fake>) {
        for _ in 0..200 {
            if matches!(*cat.refresh.lock().unwrap(), Refresh::Done(..)) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("the refresh never finished");
    }

    /// The first load answers from the cache at once; the next hands over the refresh's answer.
    async fn load_twice(cat: &Arc<SkinCatalog<Fake>>) -> Loaded {
        let (m, status) = cat.load().await;
        assert_eq!(status, CatalogStatus::Refreshing);
        assert!(m.is_some());
        settle(cat).await;
        cat.load().await
    }

    #[tokio::test]
    async fn load_uses_cache_when_offline() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path());
        cache.store_manifest(&manifest("v1", 1)).await.unwrap();
        let (m, status) = load_twice(&catalog(dir.path(), Fake::new(None))).await;
        assert_eq!(status, CatalogStatus::Cached);
        assert_eq!(m.unwrap().version, "v1");
    }

    #[tokio::test]
    async fn a_cached_catalog_answers_at_once_and_refreshes_in_the_background() {
        let dir = tempfile::tempdir().unwrap();
        CatalogCache::new(dir.path())
            .store_manifest(&manifest("v1", 1))
            .await
            .unwrap();
        let fake = Fake::new(Some(Ok(Fetched::Manifest(manifest("v2", 1)))));
        let cat = catalog(dir.path(), fake);
        let (m, status) = cat.load().await;
        assert_eq!(status, CatalogStatus::Refreshing);
        assert_eq!(m.unwrap().version, "v1");
        settle(&cat).await;
        let (m, status) = cat.load().await;
        assert_eq!(status, CatalogStatus::Fresh);
        assert_eq!(m.unwrap().version, "v2");
        assert_eq!(
            CatalogCache::new(dir.path())
                .manifest()
                .await
                .unwrap()
                .version,
            "v2"
        );
        // The answer is handed over once: the next load asks again.
        let (m, status) = cat.load().await;
        assert_eq!(status, CatalogStatus::Refreshing);
        assert_eq!(m.unwrap().version, "v2");
    }

    #[tokio::test]
    async fn a_load_during_a_refresh_starts_no_second_one() {
        let dir = tempfile::tempdir().unwrap();
        CatalogCache::new(dir.path())
            .store_manifest(&manifest("v1", 1))
            .await
            .unwrap();
        let cat = catalog(dir.path(), Fake::new(None));
        *cat.refresh.lock().unwrap() = Refresh::Running;
        let (m, status) = cat.load().await;
        assert_eq!(status, CatalogStatus::Refreshing);
        assert_eq!(m.unwrap().version, "v1");
        assert!(matches!(*cat.refresh.lock().unwrap(), Refresh::Running));
    }

    #[tokio::test]
    async fn a_stale_refresh_answer_is_not_handed_over() {
        let dir = tempfile::tempdir().unwrap();
        CatalogCache::new(dir.path())
            .store_manifest(&manifest("v1", 1))
            .await
            .unwrap();
        let cat = catalog(dir.path(), Fake::new(None));
        let old = Instant::now() - REFRESH_KEPT - Duration::from_secs(1);
        *cat.refresh.lock().unwrap() =
            Refresh::Done((Some(manifest("v0", 1)), CatalogStatus::Fresh), old);
        let (m, status) = cat.load().await;
        assert_eq!(status, CatalogStatus::Refreshing);
        assert_eq!(m.unwrap().version, "v1");
    }

    #[tokio::test]
    async fn load_unavailable_without_cache() {
        let dir = tempfile::tempdir().unwrap();
        let (m, status) = catalog(dir.path(), Fake::new(None)).load().await;
        assert!(m.is_none());
        assert_eq!(status, CatalogStatus::Unavailable);
    }

    #[tokio::test]
    async fn load_keeps_cache_for_newer_format() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path());
        cache.store_manifest(&manifest("v1", 1)).await.unwrap();
        let fake = Fake::new(Some(Ok(Fetched::Manifest(manifest("v2", 2)))));
        let (m, status) = load_twice(&catalog(dir.path(), fake)).await;
        assert_eq!(status, CatalogStatus::TooNew);
        assert_eq!(m.unwrap().version, "v1");
        assert_eq!(cache.manifest().await.unwrap().version, "v1");
    }

    #[tokio::test]
    async fn load_stores_new_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let fake = Fake::new(Some(Ok(Fetched::Manifest(manifest("v2", 1)))));
        let (m, status) = catalog(dir.path(), fake).load().await;
        assert_eq!(status, CatalogStatus::Fresh);
        assert_eq!(m.unwrap().version, "v2");
        assert_eq!(
            CatalogCache::new(dir.path())
                .manifest()
                .await
                .unwrap()
                .version,
            "v2"
        );
    }

    #[tokio::test]
    async fn load_not_modified_is_fresh() {
        let dir = tempfile::tempdir().unwrap();
        CatalogCache::new(dir.path())
            .store_manifest(&manifest("v1", 1))
            .await
            .unwrap();
        let fake = Fake::new(Some(Ok(Fetched::NotModified)));
        let (m, status) = load_twice(&catalog(dir.path(), fake)).await;
        assert_eq!(status, CatalogStatus::Fresh);
        assert_eq!(m.unwrap().version, "v1");
    }

    #[tokio::test]
    async fn too_new_from_the_engine_keeps_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path());
        cache.store_manifest(&manifest("v1", 1)).await.unwrap();
        let (m, status) =
            load_twice(&catalog(dir.path(), Fake::new(Some(Ok(Fetched::TooNew))))).await;
        assert_eq!(status, CatalogStatus::TooNew);
        assert_eq!(m.unwrap().version, "v1");
        let (m, status) = catalog(
            tempfile::tempdir().unwrap().path(),
            Fake::new(Some(Ok(Fetched::TooNew))),
        )
        .load()
        .await;
        assert!(m.is_none());
        assert_eq!(status, CatalogStatus::TooNew);
    }

    #[tokio::test]
    async fn damaged_manifest_json_counts_as_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("manifest.json"), b"{ nope").unwrap();
        assert!(CatalogCache::new(dir.path()).manifest().await.is_none());
        let (m, status) = catalog(dir.path(), Fake::new(None)).load().await;
        assert!(m.is_none());
        assert_eq!(status, CatalogStatus::Unavailable);
    }

    #[tokio::test]
    async fn file_is_cached_after_first_fetch() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"png-bytes".to_vec();
        let hash = sha256_hex(&bytes);
        let fake = Fake::new(None);
        fake.files
            .lock()
            .unwrap()
            .push((hash.clone(), bytes.clone()));
        let cat = catalog(dir.path(), fake);
        assert_eq!(cat.file(&hash).await.unwrap(), bytes);
        assert_eq!(cat.file(&hash).await.unwrap(), bytes);
        assert_eq!(*cat.client.file_calls.lock().unwrap(), 1);
    }

    #[tokio::test]
    async fn corrupt_cached_file_is_refetched() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = b"good".to_vec();
        let hash = sha256_hex(&bytes);
        let cache = CatalogCache::new(dir.path());
        cache.store_file(&hash, b"bad").await.unwrap();
        assert!(cache.file(&hash).await.is_none());
        assert!(!dir
            .path()
            .join("files")
            .join(format!("{hash}.png"))
            .exists());
        cache.store_file(&hash, b"bad").await.unwrap();
        let fake = Fake::new(None);
        fake.files
            .lock()
            .unwrap()
            .push((hash.clone(), bytes.clone()));
        assert_eq!(catalog(dir.path(), fake).file(&hash).await.unwrap(), bytes);
        assert_eq!(cache.file(&hash).await.unwrap(), bytes);
    }

    #[tokio::test]
    async fn prune_keeps_wardrobe_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path());
        let (a, b) = (sha256_hex(b"a"), sha256_hex(b"b"));
        cache.store_file(&a, b"a").await.unwrap();
        cache.store_file(&b, b"b").await.unwrap();
        cache.store_manifest(&manifest("v1", 1)).await.unwrap();
        let cat = catalog(dir.path(), Fake::new(None));
        let removed = cat.prune(&BTreeSet::from([a.clone()])).await.unwrap();
        assert_eq!(removed, 1);
        assert!(cache.file(&a).await.is_some());
        assert!(cache.file(&b).await.is_none());
        assert!(cache.manifest().await.is_some());
    }

    #[tokio::test]
    async fn prune_ignores_foreign_names() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path());
        cache.store_manifest(&manifest("v1", 1)).await.unwrap();
        let files = dir.path().join("files");
        std::fs::create_dir_all(&files).unwrap();
        std::fs::write(files.join("x.tmp"), b"1").unwrap();
        std::fs::write(files.join("notes.txt"), b"1").unwrap();
        let cat = catalog(dir.path(), Fake::new(None));
        assert_eq!(cat.prune(&BTreeSet::new()).await.unwrap(), 0);
        assert!(files.join("x.tmp").exists() && files.join("notes.txt").exists());
    }

    #[tokio::test]
    async fn prune_without_manifest_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CatalogCache::new(dir.path());
        let a = sha256_hex(b"a");
        cache.store_file(&a, b"a").await.unwrap();
        let cat = catalog(dir.path(), Fake::new(None));
        assert_eq!(cat.prune(&BTreeSet::new()).await.unwrap(), 0);
        std::fs::write(dir.path().join("manifest.json"), b"{ bad").unwrap();
        assert_eq!(cat.prune(&BTreeSet::new()).await.unwrap(), 0);
        assert!(cache.file(&a).await.is_some());
    }

    #[tokio::test]
    async fn cached_manifest_newer_than_launcher_is_no_cache() {
        let dir = tempfile::tempdir().unwrap();
        CatalogCache::new(dir.path())
            .store_manifest(&manifest("v9", 2))
            .await
            .unwrap();
        let (m, status) = catalog(dir.path(), Fake::new(None)).load().await;
        assert!(m.is_none());
        assert_eq!(status, CatalogStatus::TooNew);
    }

    #[tokio::test]
    async fn fetched_file_with_wrong_hash_is_not_stored() {
        let dir = tempfile::tempdir().unwrap();
        let hash = sha256_hex(b"right");
        let fake = Fake::new(None);
        fake.files
            .lock()
            .unwrap()
            .push((hash.clone(), b"wrong".to_vec()));
        let cat = catalog(dir.path(), fake);
        assert!(cat.file(&hash).await.is_err());
        assert!(CatalogCache::new(dir.path()).file(&hash).await.is_none());
    }
}
