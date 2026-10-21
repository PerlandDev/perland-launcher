pub mod cache;
pub mod hash;

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

pub use hash::Hashes;

use crate::{
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
};

/// One file to put on disk, with every URL it can come from.
#[derive(Debug, Clone, PartialEq)]
pub struct DownloadRequest {
    pub urls: Vec<String>,
    pub dest: PathBuf,
    pub hashes: Hashes,
    pub size: Option<u64>,
}

impl DownloadRequest {
    pub fn new(
        url: impl Into<String>,
        dest: impl Into<PathBuf>,
        hashes: Hashes,
        size: Option<u64>,
    ) -> Self {
        Self {
            urls: vec![url.into()],
            dest: dest.into(),
            hashes,
            size,
        }
    }
}

/// How an already-present destination is judged. `Size` is fast; `Hash` reads the file (used by "Repair").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExistingCheck {
    Size,
    Hash,
}

#[derive(Clone)]
pub struct Downloader {
    client: reqwest::Client,
    max_concurrent: usize,
    retries: u32,
    backoff: Duration,
    existing: ExistingCheck,
    /// One lock per destination, shared by every clone, so two overlapping fetches never share a `.part`.
    locks: Arc<Mutex<HashMap<PathBuf, Arc<tokio::sync::Mutex<()>>>>>,
}

enum Attempt {
    Retry(LauncherError),
    NextUrl(LauncherError),
    Fatal(LauncherError),
}

impl Downloader {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            max_concurrent: 8,
            retries: 3,
            backoff: Duration::from_millis(500),
            existing: ExistingCheck::Size,
            locks: Arc::default(),
        }
    }

    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.max_concurrent = n.max(1);
        self
    }

    pub fn existing_check(&self) -> ExistingCheck {
        self.existing
    }

    pub fn with_existing_check(mut self, check: ExistingCheck) -> Self {
        self.existing = check;
        self
    }

    async fn already_valid(&self, req: &DownloadRequest) -> bool {
        let Ok(meta) = tokio::fs::metadata(&req.dest).await else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }
        match (self.existing, req.size) {
            (ExistingCheck::Size, Some(size)) => meta.len() == size,
            _ => {
                if req.hashes.is_empty() {
                    return true;
                }
                matches!(verify_on_disk(&req.hashes, &req.dest).await, Ok(true))
            }
        }
    }

    pub async fn fetch(&self, req: &DownloadRequest) -> Result<()> {
        let key = lock_key(&req.dest);
        let dest_lock = {
            let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
            locks.entry(key.clone()).or_default().clone()
        };
        let result = {
            let _guard = dest_lock.clone().lock_owned().await;
            self.fetch_locked(req).await
        };
        // Forget the lock once nobody else holds or waits for it. Clones are only handed out
        // under the map's mutex, so the count can't change while it is checked here.
        let mut locks = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        if Arc::strong_count(&dest_lock) == 2 {
            locks.remove(&key);
        }
        result
    }

    async fn fetch_locked(&self, req: &DownloadRequest) -> Result<()> {
        if self.already_valid(req).await {
            return Ok(());
        }
        if let Some(parent) = req.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = part_path(&req.dest);
        // Libraries a loader installer builds itself have an empty URL: there is nothing to fetch.
        let urls: Vec<&String> = req.urls.iter().filter(|u| !u.trim().is_empty()).collect();
        let mut last_err = LauncherError::network(
            ErrorCode::DownloadNoUrl,
            "No download address was given for this file",
        );
        for _round in 0..2 {
            let mut downloaded = false;
            'urls: for url in &urls {
                for attempt in 0..=self.retries {
                    match self.download_to_part(url, &part).await {
                        Ok(()) => {
                            downloaded = true;
                            break 'urls;
                        }
                        Err(Attempt::Retry(e)) => {
                            last_err = e;
                            if attempt < self.retries {
                                tokio::time::sleep(self.backoff * 2u32.pow(attempt)).await;
                            }
                        }
                        Err(Attempt::NextUrl(e)) => {
                            last_err = e;
                            continue 'urls;
                        }
                        Err(Attempt::Fatal(e)) => return Err(e),
                    }
                }
            }
            if !downloaded {
                return Err(last_err);
            }
            let verified = verify_on_disk(&req.hashes, &part).await.map_err(|e| {
                LauncherError::disk(ErrorCode::DiskIo, "Couldn't check a downloaded file")
                    .with_fix("Check free disk space and that the launcher folder isn't read-only")
                    .with_detail(e)
            })?;
            if verified {
                tokio::fs::rename(&part, &req.dest).await?;
                return Ok(());
            }
            let _ = tokio::fs::remove_file(&part).await;
            last_err = LauncherError::integrity(
                ErrorCode::DownloadCorrupt,
                format!("{} was damaged while downloading", file_name(&req.dest)),
            )
            .with_param("file", file_name(&req.dest))
            .with_fix("Try again. If it keeps happening, the file on the server may be broken.");
        }
        Err(last_err)
    }

    async fn download_to_part(&self, url: &str, part: &Path) -> std::result::Result<(), Attempt> {
        let existing = tokio::fs::metadata(part)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        let mut request = self.client.get(url);
        if existing > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={existing}-"));
        }
        let response = request.send().await.map_err(|e| {
            if e.is_builder() {
                // A malformed address won't get better by retrying it.
                return Attempt::NextUrl(
                    LauncherError::network(
                        ErrorCode::DownloadNoUrl,
                        "This file's download address isn't valid",
                    )
                    .with_detail(e.without_url()),
                );
            }
            Attempt::Retry(
                LauncherError::network(
                    ErrorCode::HostUnreachable,
                    format!("Couldn't reach {}", host_of(url)),
                )
                .with_param("host", host_of(url))
                .with_fix("Check your internet connection")
                .with_detail(e.without_url()),
            )
        })?;
        let status = response.status().as_u16();
        let append = match status {
            206 => true,
            200 => false,
            416 => {
                let _ = tokio::fs::remove_file(part).await;
                return Err(Attempt::Retry(LauncherError::network(
                    ErrorCode::DownloadRestarted,
                    "The download had to restart",
                )));
            }
            429 | 500..=599 => {
                return Err(Attempt::Retry(
                    LauncherError::network(
                        ErrorCode::HostTrouble,
                        format!("{} is having trouble ({status})", host_of(url)),
                    )
                    .with_param("host", host_of(url))
                    .with_param("status", status),
                ))
            }
            _ => {
                return Err(Attempt::NextUrl(
                    LauncherError::network(
                        ErrorCode::DownloadRejected,
                        format!("{} refused the download ({status})", host_of(url)),
                    )
                    .with_param("host", host_of(url))
                    .with_param("status", status)
                    .with_detail(redact_url(url)),
                ))
            }
        };
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(append)
            .truncate(!append)
            .open(part)
            .await
            .map_err(|e| Attempt::Fatal(e.into()))?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| {
                Attempt::Retry(
                    LauncherError::network(
                        ErrorCode::ConnectionDropped,
                        "The connection dropped during a download",
                    )
                    .with_detail(e.without_url()),
                )
            })?;
            file.write_all(&chunk)
                .await
                .map_err(|e| Attempt::Fatal(e.into()))?;
        }
        file.flush().await.map_err(|e| Attempt::Fatal(e.into()))?;
        Ok(())
    }

    /// Download everything, at most `max_concurrent` at a time. Duplicate destinations are fetched once.
    pub async fn fetch_all(
        &self,
        reqs: Vec<DownloadRequest>,
        task: &TaskId,
        sink: &EventSink,
    ) -> Result<()> {
        let mut seen = HashSet::new();
        let reqs: Vec<DownloadRequest> = reqs
            .into_iter()
            .filter(|r| seen.insert(r.dest.clone()))
            .collect();
        let total = reqs.len() as u64;
        let (mut done, mut bytes) = (0u64, 0u64);
        sink(Event::Progress {
            task: task.clone(),
            done,
            total,
            bytes,
        });
        let mut results = futures_util::stream::iter(
            reqs.into_iter()
                .map(|r| async move { self.fetch(&r).await.map(|()| r.size.unwrap_or(0)) }),
        )
        .buffer_unordered(self.max_concurrent);
        while let Some(result) = results.next().await {
            bytes += result?;
            done += 1;
            sink(Event::Progress {
                task: task.clone(),
                done,
                total,
                bytes,
            });
        }
        Ok(())
    }
}

/// Hash a file on the blocking pool so big files neither sit in memory nor stall other downloads.
async fn verify_on_disk(hashes: &Hashes, path: &Path) -> std::io::Result<bool> {
    let (hashes, path) = (hashes.clone(), path.to_path_buf());
    tokio::task::spawn_blocking(move || hashes.verify_file(&path))
        .await
        .map_err(std::io::Error::other)?
}

fn part_path(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".part");
    PathBuf::from(s)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// `url` without its query, fragment and user info: fine for `detail`, never for a message.
pub fn redact_url(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut u) => {
            u.set_query(None);
            u.set_fragment(None);
            let _ = u.set_username("");
            let _ = u.set_password(None);
            u.to_string()
        }
        Err(_) => "(not a valid address)".to_string(),
    }
}

/// Just the host of `url`, for player-facing messages and params.
pub fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_owned))
        .unwrap_or_else(|| "the download server".to_string())
}

/// The key a destination is locked under: absolute and, on Windows, case-folded, so two
/// spellings of one file share one lock.
fn lock_key(dest: &Path) -> PathBuf {
    let abs = std::path::absolute(dest).unwrap_or_else(|_| dest.to_path_buf());
    if cfg!(windows) {
        PathBuf::from(abs.as_os_str().to_string_lossy().to_lowercase())
    } else {
        abs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn destination_locks_are_forgotten_after_use() {
        let dir = tempfile::tempdir().unwrap();
        let d = Downloader::new(reqwest::Client::new());
        for i in 0..5 {
            let dest = dir.path().join(format!("{i}.txt"));
            std::fs::write(&dest, "hello").unwrap();
            // Already on disk with the right size: no network needed.
            d.fetch(&DownloadRequest::new(
                "http://127.0.0.1:1/x",
                dest,
                Hashes::default(),
                Some(5),
            ))
            .await
            .unwrap();
        }
        assert!(d.locks.lock().unwrap().is_empty());
    }

    #[test]
    fn lock_keys_are_absolute() {
        assert!(lock_key(Path::new("rel/a.jar")).is_absolute());
    }

    #[cfg(windows)]
    #[test]
    fn lock_keys_ignore_case_and_separators_on_windows() {
        assert_eq!(
            lock_key(Path::new("C:/Data/A.jar")),
            lock_key(Path::new(r"c:\data\a.JAR"))
        );
    }

    #[test]
    fn redact_url_drops_secrets() {
        assert_eq!(
            redact_url("https://u:p@cdn.example.com/a/b.jar?sig=x#y"),
            "https://cdn.example.com/a/b.jar"
        );
        assert_eq!(redact_url("not a url"), "(not a valid address)");
        assert_eq!(
            host_of("https://u:p@cdn.example.com:8443/a"),
            "cdn.example.com"
        );
        assert_eq!(host_of("nonsense"), "the download server");
    }
}
