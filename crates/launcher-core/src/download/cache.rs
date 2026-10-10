//! The content-addressed store for pack files: `cache/objects/<2-char prefix>/<key>`, keyed by
//! [`Hashes::cache_key`]. A file is downloaded once for every instance that uses it, and a sync
//! interrupted halfway resumes from here. Objects are copied out, never linked, so a player's
//! edit in one instance can't reach the cache or another instance.
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{DownloadRequest, Downloader, ExistingCheck, Hashes};
use crate::error::{ErrorCode, LauncherError, Result};

/// Makes each insert's temp name unique, so concurrent inserts of one object never share a file.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone)]
pub struct ObjectCache {
    dir: PathBuf,
}

fn no_usable_hash() -> LauncherError {
    LauncherError::pack(ErrorCode::PackInvalid, "A pack file has no usable checksum")
        .with_param("file", "checksum")
}

async fn verify(hashes: &Hashes, path: &Path) -> Result<bool> {
    let (h, p) = (hashes.clone(), path.to_path_buf());
    let ok = tokio::task::spawn_blocking(move || h.verify_file(&p))
        .await
        .map_err(std::io::Error::other)??;
    Ok(ok)
}

impl ObjectCache {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the object for `hashes` lives, or `None` when they hold no usable hash.
    pub fn path_for(&self, hashes: &Hashes) -> Option<PathBuf> {
        let key = hashes.cache_key()?;
        Some(self.dir.join(&key[..2]).join(&key))
    }

    /// The object, only if it is present and matches every hash. Never touches the network.
    pub async fn get(&self, hashes: &Hashes) -> Option<PathBuf> {
        let path = self.path_for(hashes)?;
        matches!(verify(hashes, &path).await, Ok(true)).then_some(path)
    }

    /// The verified object, downloaded from `urls` when it is missing or damaged.
    pub async fn ensure(
        &self,
        downloader: &Downloader,
        urls: &[String],
        hashes: &Hashes,
        size: Option<u64>,
    ) -> Result<PathBuf> {
        let dest = self.path_for(hashes).ok_or_else(no_usable_hash)?;
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

    /// Copy a local file in (a manual download, an imported `.mrpack`) after checking it.
    pub async fn insert_file(&self, src: &Path, hashes: &Hashes) -> Result<PathBuf> {
        let dest = self.path_for(hashes).ok_or_else(no_usable_hash)?;
        if !verify(hashes, src).await? {
            let name = src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
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
        Ok(dest)
    }
}
