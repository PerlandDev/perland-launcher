pub mod hash;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    time::Duration,
};

use futures_util::StreamExt;
use tokio::io::AsyncWriteExt;

pub use hash::Hashes;

use crate::{
    error::{LauncherError, Result},
    events::{Event, EventSink},
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
                match tokio::fs::read(&req.dest).await {
                    Ok(data) => req.hashes.verify(&data),
                    Err(_) => false,
                }
            }
        }
    }

    pub async fn fetch(&self, req: &DownloadRequest) -> Result<()> {
        if self.already_valid(req).await {
            return Ok(());
        }
        if let Some(parent) = req.dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let part = part_path(&req.dest);
        let mut last_err = LauncherError::network("No download address was given for this file");
        for _round in 0..2 {
            let mut downloaded = false;
            'urls: for url in &req.urls {
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
            let data = tokio::fs::read(&part).await?;
            if req.hashes.verify(&data) {
                tokio::fs::rename(&part, &req.dest).await?;
                return Ok(());
            }
            let _ = tokio::fs::remove_file(&part).await;
            last_err = LauncherError::integrity(format!(
                "{} was damaged while downloading",
                file_name(&req.dest)
            ))
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
            Attempt::Retry(
                LauncherError::network(format!("Couldn't reach {}", host_of(url)))
                    .with_fix("Check your internet connection")
                    .with_detail(e),
            )
        })?;
        let status = response.status().as_u16();
        let append = match status {
            206 => true,
            200 => false,
            416 => {
                let _ = tokio::fs::remove_file(part).await;
                return Err(Attempt::Retry(LauncherError::network(
                    "The download had to restart",
                )));
            }
            429 | 500..=599 => {
                return Err(Attempt::Retry(LauncherError::network(format!(
                    "{} is having trouble ({status})",
                    host_of(url)
                ))))
            }
            _ => {
                return Err(Attempt::NextUrl(LauncherError::network(format!(
                    "{url} answered {status}"
                ))))
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
                    LauncherError::network("The connection dropped during a download")
                        .with_detail(e),
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
        task: &str,
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
            task: task.to_string(),
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
                task: task.to_string(),
                done,
                total,
                bytes,
            });
        }
        Ok(())
    }
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

fn host_of(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or(url)
}
