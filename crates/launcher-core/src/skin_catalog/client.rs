//! The catalog client. Errors name the Engine's host only.

use std::io::Cursor;

use image::{ImageFormat, ImageReader};
use reqwest::header::IF_NONE_MATCH;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::manifest::Manifest;
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_of, host_trouble, rate_limited, request_failed},
        InviteClient, Url,
    },
};

/// The longest manifest read, in bytes.
const MANIFEST_CAP: usize = 1 << 20;
/// The largest catalog file read, in bytes.
const FILE_CAP: usize = 64 << 10;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// What a manifest request answered.
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    /// The version asked about is still current.
    NotModified,
    Manifest(Manifest),
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<Manifest>,
}

fn unavailable(host: &str, why: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::SkinCatalogUnavailable,
        "Couldn't reach Perland's skin catalog.",
    )
    .with_param("host", host)
    .with_fix("Check your connection and try again")
    .with_detail(why)
}

/// A catalog file name: 64 lower-case hex digits.
fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// A PNG of a size the catalog uses: 64x64 (art, masks, paintings) or 48x96 (thumbnails).
fn png_size_ok(bytes: &[u8]) -> bool {
    if !bytes.starts_with(PNG_SIGNATURE) {
        return false;
    }
    let Ok(reader) =
        ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png).into_dimensions()
    else {
        return false;
    };
    matches!(reader, (64, 64) | (48, 96))
}

#[derive(Clone)]
pub struct CatalogClient {
    invites: InviteClient,
    site: Url,
}

impl std::fmt::Debug for CatalogClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogClient")
            .field("site", &self.site.as_str())
            .finish_non_exhaustive()
    }
}

impl CatalogClient {
    /// `site` should come from `public_list::site_url`.
    pub fn new(invites: InviteClient, site: Url) -> Self {
        Self { invites, site }
    }

    /// The manifest, or `NotModified` when `etag` (the manifest's `version`) is still current.
    ///
    /// A manifest of any `format_version` is returned as the Engine sent it: the caller must
    /// check `format_version` against [`FORMAT_VERSION`](super::FORMAT_VERSION).
    pub async fn manifest(&self, etag: Option<&str>) -> Result<Fetched> {
        let engine = self
            .invites
            .engine_for_site(&self.site)
            .await
            .map_err(|e| unavailable(self.site.host_str().unwrap_or("the site"), &e.message))?;
        let host = host_of(&engine);
        let url = self.route(&engine, "");
        let result = self.fetch_manifest(url, &host, etag).await;
        self.forget_if_offline(&result);
        result
    }

    /// One catalog PNG by its SHA-256 (lower-case hex), checked against its name.
    pub async fn file(&self, sha256: &str) -> Result<Vec<u8>> {
        let host = self.site.host_str().unwrap_or("the site").to_string();
        if !is_hash(sha256) {
            return Err(unavailable(&host, "not a file hash"));
        }
        let engine = self
            .invites
            .engine_for_site(&self.site)
            .await
            .map_err(|e| unavailable(&host, &e.message))?;
        let host = host_of(&engine);
        let url = self.route(&engine, &format!("/files/{sha256}"));
        let result = self.fetch_file(url, &host, sha256).await;
        self.forget_if_offline(&result);
        result
    }

    fn route(&self, engine: &Url, tail: &str) -> Url {
        Url::parse(&format!(
            "{}/api/v1/skin-catalog{tail}",
            engine.as_str().trim_end_matches('/')
        ))
        .expect("a checked Engine URL plus a fixed path is a URL")
    }

    fn forget_if_offline<T>(&self, result: &Result<T>) {
        if let Err(e) = result {
            if e.code == ErrorCode::Offline {
                self.invites
                    .forget_origin(&self.site.origin().ascii_serialization());
            }
        }
    }

    async fn get(&self, url: Url, host: &str, etag: Option<&str>) -> Result<reqwest::Response> {
        let mut request = self
            .invites
            .http()
            .get(url)
            .timeout(self.invites.timing().request_timeout);
        if let Some(version) = etag {
            request = request.header(IF_NONE_MATCH, format!("\"{version}\""));
        }
        request.send().await.map_err(|e| request_failed(host, e))
    }

    async fn fetch_manifest(&self, url: Url, host: &str, etag: Option<&str>) -> Result<Fetched> {
        let response = self.get(url, host, etag).await?;
        match response.status().as_u16() {
            304 => return Ok(Fetched::NotModified),
            200..=299 => {}
            404 | 503 => return Err(unavailable(host, "the catalog isn't served")),
            429 => return Err(rate_limited(host)),
            s => return Err(host_trouble(host, s)),
        }
        let body = read_capped(response, MANIFEST_CAP)
            .await
            .map_err(|e| request_failed(host, e))?
            .ok_or_else(|| unavailable(host, "the manifest is too large"))?;
        // The parser's own message can quote the body, so it is never shown.
        let envelope: Envelope = serde_json::from_slice(&body)
            .map_err(|_| unavailable(host, "the manifest isn't valid"))?;
        envelope
            .data
            .map(Fetched::Manifest)
            .ok_or_else(|| unavailable(host, "the manifest is empty"))
    }

    async fn fetch_file(&self, url: Url, host: &str, sha256: &str) -> Result<Vec<u8>> {
        let response = self.get(url, host, None).await?;
        match response.status().as_u16() {
            200..=299 => {}
            404 | 503 => return Err(unavailable(host, "the file isn't served")),
            429 => return Err(rate_limited(host)),
            s => return Err(host_trouble(host, s)),
        }
        let body = read_capped(response, FILE_CAP)
            .await
            .map_err(|e| request_failed(host, e))?
            .ok_or_else(|| unavailable(host, "the file is too large"))?;
        if hex::encode(Sha256::digest(&body)) != sha256 {
            return Err(unavailable(host, "the file doesn't match its name"));
        }
        if !png_size_ok(&body) {
            return Err(unavailable(host, "the file isn't a catalog image"));
        }
        Ok(body)
    }
}
