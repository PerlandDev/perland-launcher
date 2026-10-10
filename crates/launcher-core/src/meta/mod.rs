pub mod loaders;
pub mod maven;
pub mod merge;
pub mod mojang;
pub mod rules;

use std::path::PathBuf;

use reqwest::header::{ETAG, IF_NONE_MATCH};
use serde::de::DeserializeOwned;

use crate::{
    download::hash::sha1_hex,
    error::{LauncherError, Result},
    fsutil::write_atomic,
};

/// Fetches upstream metadata and keeps the last good copy on disk, so the launcher works offline.
///
/// A fresh response only replaces the cached copy after it passes validation (it parses, or its
/// checksum matches), so a captive-portal page or truncated body can never clobber a good copy.
#[derive(Clone)]
pub struct MetaClient {
    http: reqwest::Client,
    dir: PathBuf,
}

impl MetaClient {
    pub fn new(http: reqwest::Client, dir: impl Into<PathBuf>) -> Self {
        Self {
            http,
            dir: dir.into(),
        }
    }

    fn body_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.body", sha1_hex(url.as_bytes())))
    }

    fn etag_path(&self, url: &str) -> PathBuf {
        self.dir.join(format!("{}.etag", sha1_hex(url.as_bytes())))
    }

    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        self.fetch(url, |_| Ok(())).await
    }

    /// Fetch `url` (revalidating with the cached ETag) and return the body. A fresh body is only
    /// written to the cache when `validate` accepts it. If it is rejected, the cache is left alone
    /// and a cached copy that passes the same validation is returned instead, else the validation error.
    async fn fetch(&self, url: &str, validate: impl Fn(&[u8]) -> Result<()>) -> Result<Vec<u8>> {
        let body_path = self.body_path(url);
        let cached = tokio::fs::read(&body_path).await.ok();
        let mut request = self.http.get(url);
        if cached.is_some() {
            if let Ok(etag) = tokio::fs::read_to_string(self.etag_path(url)).await {
                request = request.header(IF_NONE_MATCH, etag.trim());
            }
        }
        match request.send().await {
            Ok(resp) if resp.status() == reqwest::StatusCode::NOT_MODIFIED => {
                cached.ok_or_else(|| {
                    LauncherError::network(format!(
                        "{url} said nothing changed, but there's no saved copy"
                    ))
                })
            }
            Ok(resp) if resp.status().is_success() => {
                let etag = resp
                    .headers()
                    .get(ETAG)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_owned);
                let bytes = resp.bytes().await.map_err(|e| {
                    LauncherError::network(format!("Downloading {url} was interrupted"))
                        .with_detail(e)
                })?;
                if let Err(invalid) = validate(&bytes) {
                    return match cached {
                        Some(c) if validate(&c).is_ok() => Ok(c),
                        _ => Err(invalid),
                    };
                }
                // Caching is best-effort: a body that arrived and validated is returned even if
                // it can't be saved. The ETag is only kept alongside a body that was saved.
                let saved = write_atomic(&body_path, &bytes).await.is_ok();
                match etag {
                    Some(tag) if saved => {
                        let _ = write_atomic(&self.etag_path(url), tag.as_bytes()).await;
                    }
                    _ => {
                        let _ = tokio::fs::remove_file(self.etag_path(url)).await;
                    }
                }
                Ok(bytes.to_vec())
            }
            Ok(resp) => match cached {
                Some(c) => Ok(c),
                None => {
                    let status = resp.status();
                    let message = format!("{url} answered {status}");
                    Err(if status.is_client_error() {
                        LauncherError::loader(message).with_fix("Check that this version exists")
                    } else {
                        LauncherError::network(message).with_fix("Try again in a few minutes")
                    })
                }
            },
            Err(e) => match cached {
                Some(c) => Ok(c),
                None => Err(LauncherError::network(
                    "You seem to be offline, and this hasn't been downloaded before",
                )
                .with_fix("Connect to the internet and try again")
                .with_detail(e)),
            },
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self
            .fetch(url, |b| parse_json::<T>(url, b).map(|_| ()))
            .await?;
        parse_json(url, &bytes)
    }

    /// For content-addressed URLs (Mojang packages): a cached copy with the right sha1 is used without any network access.
    pub async fn get_verified_json<T: DeserializeOwned>(&self, url: &str, sha1: &str) -> Result<T> {
        if let Ok(cached) = tokio::fs::read(self.body_path(url)).await {
            if sha1_hex(&cached).eq_ignore_ascii_case(sha1) {
                return parse_json(url, &cached);
            }
        }
        let verify = |b: &[u8]| {
            if sha1_hex(b).eq_ignore_ascii_case(sha1) {
                Ok(())
            } else {
                Err(
                    LauncherError::integrity(format!("Data from {url} didn't match its checksum"))
                        .with_fix("Try again in a minute"),
                )
            }
        };
        let bytes = self.fetch(url, verify).await?;
        // A cached fallback (offline / server error) bypasses fetch's validation of fresh bodies.
        verify(&bytes)?;
        parse_json(url, &bytes)
    }
}

pub fn parse_json<T: DeserializeOwned>(url: &str, bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes)
        .map_err(|e| LauncherError::loader(format!("Unexpected data from {url}")).with_detail(e))
}
