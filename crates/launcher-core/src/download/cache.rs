//! The content-addressed store for pack files: `cache/objects/<2-char prefix>/<key>`, keyed by
//! [`object_key`]. A file is downloaded once for every instance that uses it, and a sync
//! interrupted halfway resumes from here. Objects are copied out, never linked, so a player's
//! edit in one instance can't reach the cache or another instance.
//!
//! A file known only by CurseForge's murmur2 fingerprint is kept per instance: murmur2 is
//! trivially forged, so a shared key would let one pack plant a file another pack receives.
//! Renaming an instance moves its objects to the new id's scope ([`ObjectCache::rescope`]).
//! Reusing an object marks it used (its modified time), which `gc` reads.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::SystemTime,
};

use super::{hash::sha1_hex, DownloadRequest, Downloader, ExistingCheck, Hashes};
use crate::error::{ErrorCode, LauncherError, Result};

/// Makes each insert's temp name unique, so concurrent inserts of one object never share a file.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The instance a murmur2-only object belongs to: the first 12 hex digits of its id's sha1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheScope {
    tag: String,
}

impl CacheScope {
    pub fn for_instance(id: &str) -> Self {
        Self {
            tag: sha1_hex(id.as_bytes())[..12].to_string(),
        }
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }
}

/// The object key for `hashes`: [`Hashes::cache_key`] when there is a strong hash (shared by
/// every instance; the scope is ignored), else `m2-<murmur2>-<tag>` for `scope`. A murmur2-only
/// file without a scope has no key.
pub fn object_key(hashes: &Hashes, scope: Option<&CacheScope>) -> Option<String> {
    hashes.cache_key().or_else(|| {
        let (m, scope) = (hashes.murmur2?, scope?);
        Some(format!("m2-{m}-{}", scope.tag))
    })
}

#[derive(Debug, Clone)]
pub struct ObjectCache {
    dir: PathBuf,
    scope: Option<CacheScope>,
}

/// The UI words this from the `no_checksum` context (there may be no file name to show). `file`
/// is the file's own name when the caller has one.
fn no_usable_hash(file: Option<&str>) -> LauncherError {
    let e = LauncherError::pack(ErrorCode::PackInvalid, "A pack file has no usable checksum")
        .with_param("context", "no_checksum");
    match file {
        Some(name) if !name.is_empty() => e.with_param("file", name),
        _ => e,
    }
}

async fn verify(hashes: &Hashes, path: &Path) -> Result<bool> {
    let (h, p) = (hashes.clone(), path.to_path_buf());
    let ok = tokio::task::spawn_blocking(move || h.verify_file(&p))
        .await
        .map_err(std::io::Error::other)??;
    Ok(ok)
}

/// Mark an object used now, for the cleanup. Best effort: a failure only makes it look older.
async fn touch(path: &Path) {
    let path = path.to_path_buf();
    let _ = tokio::task::spawn_blocking(move || {
        std::fs::File::options()
            .write(true)
            .open(&path)
            .and_then(|f| f.set_modified(SystemTime::now()))
    })
    .await;
}

impl ObjectCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            scope: None,
        }
    }

    /// The same store, seen by one instance: murmur2-only files are kept under its scope.
    pub fn scoped(&self, scope: CacheScope) -> Self {
        Self {
            dir: self.dir.clone(),
            scope: Some(scope),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the object for `hashes` lives, or `None` when they hold no usable hash.
    pub fn path_for(&self, hashes: &Hashes) -> Option<PathBuf> {
        let key = object_key(hashes, self.scope.as_ref())?;
        Some(self.dir.join(&key[..2]).join(&key))
    }

    /// The object, only if it is present and matches every hash. Never touches the network.
    pub async fn get(&self, hashes: &Hashes) -> Option<PathBuf> {
        let path = self.path_for(hashes)?;
        if !matches!(verify(hashes, &path).await, Ok(true)) {
            return None;
        }
        touch(&path).await;
        Some(path)
    }

    /// The verified object, downloaded from `urls` when it is missing or damaged.
    pub async fn ensure(
        &self,
        downloader: &Downloader,
        urls: &[String],
        hashes: &Hashes,
        size: Option<u64>,
    ) -> Result<PathBuf> {
        let dest = self.path_for(hashes).ok_or_else(|| no_usable_hash(None))?;
        if let Some(valid) = self.get(hashes).await {
            return Ok(valid);
        }
        let req = DownloadRequest {
            urls: urls.to_vec(),
            dest: dest.clone(),
            hashes: hashes.clone(),
            size,
        };
        downloader
            .clone()
            .with_existing_check(ExistingCheck::Hash)
            .fetch(&req)
            .await?;
        Ok(dest)
    }

    /// Move the murmur2-only objects among `hashes` from `from`'s keys to `to`'s, for an
    /// instance whose id changed (a rename). Objects with a strong hash are shared and stay.
    /// Best effort: a missing or unmovable object is skipped, and is only downloaded again.
    /// Returns how many moved.
    pub async fn rescope(&self, from: &CacheScope, to: &CacheScope, hashes: &[Hashes]) -> usize {
        let mut seen = std::collections::HashSet::new();
        let mut moved = 0;
        for hashes in hashes {
            if object_key(hashes, None).is_some() {
                continue;
            }
            let (Some(old), Some(new)) =
                (object_key(hashes, Some(from)), object_key(hashes, Some(to)))
            else {
                continue;
            };
            if old == new || !seen.insert(old.clone()) {
                continue;
            }
            let src = self.dir.join(&old[..2]).join(&old);
            let dest = self.dir.join(&new[..2]).join(&new);
            // Never follows a symlink: only a regular file is moved.
            let is_file = tokio::fs::symlink_metadata(&src)
                .await
                .is_ok_and(|m| m.file_type().is_file());
            if !is_file
                || tokio::fs::create_dir_all(&self.dir.join(&new[..2]))
                    .await
                    .is_err()
            {
                continue;
            }
            if tokio::fs::rename(&src, &dest).await.is_ok() {
                moved += 1;
            }
        }
        moved
    }

    /// Copy a local file in (a manual download, an imported `.mrpack`) after checking it.
    pub async fn insert_file(&self, src: &Path, hashes: &Hashes) -> Result<PathBuf> {
        let name = src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let dest = self
            .path_for(hashes)
            .ok_or_else(|| no_usable_hash(Some(&name)))?;
        if !verify(hashes, src).await? {
            return Err(LauncherError::integrity(
                ErrorCode::DownloadCorrupt,
                format!("{name} isn't the file the pack expects"),
            )
            .with_param("file", name));
        }
        if let Some(valid) = self.get(hashes).await {
            return Ok(valid);
        }
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let n = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut tmp = dest.as_os_str().to_owned();
        tmp.push(format!(".{}.{n}.tmp", std::process::id()));
        let tmp = PathBuf::from(tmp);
        let copied = match tokio::fs::copy(src, &tmp).await {
            Ok(_) => tokio::fs::rename(&tmp, &dest).await,
            Err(e) => Err(e),
        };
        if let Err(e) = copied {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(e.into());
        }
        // A copy can keep the source's old modified time; the object is in use from now on.
        touch(&dest).await;
        Ok(dest)
    }
}
