//! Project icons and gallery images for Browse (spec §3.7). The app's CSP allows no remote
//! images, so the core fetches them from Modrinth's CDN only, under a size cap, keeps only real
//! images, and caches them on disk, pruned oldest first.
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};

use serde::{Deserialize, Serialize};
use tokio::sync::Semaphore;

use crate::{
    download::hash::sha256_hex,
    error::{ErrorCode, LauncherError, Result},
    fsutil::write_atomic,
    sources::art::sniff,
};

pub const MAX_ICON_BYTES: usize = 1 << 20;
pub const MAX_GALLERY_BYTES: usize = 5 << 20;
pub const MAX_CACHE_BYTES: u64 = 100 << 20;
const AT_ONCE: usize = 6;
const EXTENSIONS: [&str; 5] = ["png", "jpg", "webp", "gif", "svg"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageKind {
    Icon,
    Gallery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

/// The 3b pack-art sniffer, plus GIF (many mod icons are animated).
pub fn sniff_image(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some("gif");
    }
    sniff(bytes)
}

fn mime(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        _ => "image/svg+xml",
    }
}

fn refused(detail: impl std::fmt::Display) -> LauncherError {
    LauncherError::network(ErrorCode::ImageRefused, "That image can't be shown").with_detail(detail)
}

pub struct ImageCache {
    http: reqwest::Client,
    dir: PathBuf,
    origin: Option<reqwest::Url>,
    icon_limit: usize,
    gallery_limit: usize,
    total_limit: u64,
    permits: Arc<Semaphore>,
}

impl ImageCache {
    /// Images only from `origin` (`https://cdn.modrinth.com`; a wiremock server in tests). `http`
    /// must be built by `http::client_without_redirects`, or a CDN redirect could lead off-origin.
    pub fn new(http: reqwest::Client, dir: impl Into<PathBuf>, origin: &str) -> Self {
        Self {
            http,
            dir: dir.into(),
            origin: reqwest::Url::parse(origin).ok(),
            icon_limit: MAX_ICON_BYTES,
            gallery_limit: MAX_GALLERY_BYTES,
            total_limit: MAX_CACHE_BYTES,
            permits: Arc::new(Semaphore::new(AT_ONCE)),
        }
    }

    pub fn with_limits(mut self, icon: usize, gallery: usize, total: u64) -> Self {
        self.icon_limit = icon;
        self.gallery_limit = gallery;
        self.total_limit = total;
        self
    }

    /// Same scheme, host and port as the origin, and no user info.
    pub fn allows(&self, raw: &str) -> bool {
        let (Some(origin), Ok(url)) = (&self.origin, reqwest::Url::parse(raw)) else {
            return false;
        };
        url.scheme() == origin.scheme()
            && url.host_str() == origin.host_str()
            && url.port_or_known_default() == origin.port_or_known_default()
            && url.username().is_empty()
            && url.password().is_none()
    }

    fn limit(&self, kind: ImageKind) -> usize {
        match kind {
            ImageKind::Icon => self.icon_limit,
            ImageKind::Gallery => self.gallery_limit,
        }
    }

    pub async fn get(&self, raw: &str, kind: ImageKind) -> Result<Image> {
        if !self.allows(raw) {
            return Err(refused("not from the image server"));
        }
        let limit = self.limit(kind);
        let stem = sha256_hex(raw.as_bytes());
        for ext in EXTENSIONS {
            let path = self.dir.join(format!("{stem}.{ext}"));
            if let Ok(bytes) = tokio::fs::read(&path).await {
                if bytes.len() <= limit {
                    return Ok(Image {
                        bytes,
                        mime: mime(ext),
                    });
                }
            }
        }
        let _permit = self.permits.acquire().await.map_err(refused)?;
        let bytes = self.download(raw, limit).await?;
        let ext = sniff_image(&bytes).ok_or_else(|| refused("not an image"))?;
        if write_atomic(&self.dir.join(format!("{stem}.{ext}")), &bytes)
            .await
            .is_ok()
        {
            self.prune().await;
        }
        Ok(Image {
            bytes,
            mime: mime(ext),
        })
    }

    async fn download(&self, raw: &str, limit: usize) -> Result<Vec<u8>> {
        let mut response = self
            .http
            .get(raw)
            .send()
            .await
            .map_err(|e| refused(e.without_url()))?;
        if !response.status().is_success() {
            return Err(refused(response.status()));
        }
        if response.content_length().is_some_and(|n| n > limit as u64) {
            return Err(refused("too big"));
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|e| refused(e.without_url()))?
        {
            body.extend_from_slice(&chunk);
            if body.len() > limit {
                return Err(refused("too big"));
            }
        }
        Ok(body)
    }

    /// Delete the least recently written images until the folder fits the cap. Best effort.
    async fn prune(&self) {
        let Ok(mut entries) = tokio::fs::read_dir(&self.dir).await else {
            return;
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        while let Ok(Some(entry)) = entries.next_entry().await {
            if let Ok(meta) = entry.metadata().await {
                if meta.is_file() {
                    let when = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    files.push((when, meta.len(), entry.path()));
                }
            }
        }
        let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
        files.sort_by_key(|(when, _, _)| *when);
        for (_, len, path) in files {
            if total <= self.total_limit {
                break;
            }
            if remove(&path).await {
                total -= len;
            }
        }
    }
}

async fn remove(path: &Path) -> bool {
    tokio::fs::remove_file(path).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_cdn_origin_is_allowed() {
        let cache = ImageCache::new(reqwest::Client::new(), "unused", "https://cdn.modrinth.com");
        assert!(cache.allows("https://cdn.modrinth.com/data/AANobbMI/icon.png"));
        for bad in [
            "http://cdn.modrinth.com/data/x.png",
            "https://cdn.modrinth.com.evil.example/x.png",
            "https://user@cdn.modrinth.com/x.png",
            "https://cdn.modrinth.com:8443/x.png",
            "https://i.imgur.com/x.png",
            "data:image/png;base64,AAAA",
            "not a url",
        ] {
            assert!(!cache.allows(bad), "{bad}");
        }
    }

    #[test]
    fn gif_is_an_image_too() {
        assert_eq!(sniff_image(b"GIF89a\x01\x00"), Some("gif"));
        assert_eq!(sniff_image(b"GIF87a\x01\x00"), Some("gif"));
        assert_eq!(sniff_image(b"\x89PNG\r\n\x1a\n\0"), Some("png"));
        assert_eq!(sniff_image(b"<html>"), None);
    }
}
