//! The catalog client. Errors name the Engine's host only.

use std::io::Cursor;

use image::{ImageFormat, ImageReader};
use reqwest::header::IF_NONE_MATCH;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::manifest::{is_hash, Manifest, FORMAT_VERSION, SHEET_COLUMNS};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    http::read_capped,
    invite::{
        client::{host_of, host_trouble, rate_limited, request_failed},
        InviteClient, Url,
    },
};

/// The longest manifest read, in bytes.
const MANIFEST_CAP: usize = 8 << 20;
/// The most distinct files a manifest may name.
const FILES_CAP: usize = 4000;
/// The largest catalog file read, in bytes.
const FILE_CAP: usize = 64 << 10;
/// The largest thumbnail sheet read, in bytes.
const SHEET_CAP: usize = 2 << 20;
/// The most rows a thumbnail sheet may have.
const SHEET_ROWS: u32 = 200;
/// The size of a thumbnail, and so of a sheet's cell.
const THUMB: (u32, u32) = (48, 96);
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// What a manifest request answered.
// One per request and never kept in a collection, so the manifest isn't boxed.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    /// The version asked about is still current.
    NotModified,
    Manifest(Manifest),
    /// The Engine's catalog is a newer format than this launcher reads.
    TooNew,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

/// Only the format of a manifest, read first so a newer format is recognised even when the rest of it changed shape.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FormatProbe {
    format_version: u32,
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

/// What a catalog file is asked for as, which decides the sizes it may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// Art, a mask or a painting (64x64), or a thumbnail (48x96): at most 64 KiB.
    Image,
    /// A thumbnail sheet of `columns` cells per row: `columns` thumbnails wide and 1 to 200
    /// whole thumbnails high, at most 2 MiB.
    Sheet { columns: u32 },
}

impl Kind {
    fn cap(self) -> usize {
        match self {
            Kind::Image => FILE_CAP,
            Kind::Sheet { .. } => SHEET_CAP,
        }
    }

    fn fits(self, (w, h): (u32, u32)) -> bool {
        match self {
            Kind::Image => (w, h) == (64, 64) || (w, h) == THUMB,
            Kind::Sheet { columns } => {
                w == columns * THUMB.0
                    && h % THUMB.1 == 0
                    && (1..=SHEET_ROWS).contains(&(h / THUMB.1))
            }
        }
    }
}

/// The width and height a PNG's header gives.
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(PNG_SIGNATURE) {
        return None;
    }
    ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png)
        .into_dimensions()
        .ok()
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

    /// The manifest, or `NotModified` when `etag` (the manifest's `version`) is still current, or
    /// `TooNew` when its `formatVersion` is newer than [`FORMAT_VERSION`]. A manifest naming more
    /// than 4000 distinct files, or failing [`Manifest::check`], is refused.
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

    /// One catalog PNG (art, a mask, a painting or a thumbnail) by its SHA-256 (lower-case
    /// hex), checked against its name.
    pub async fn file(&self, sha256: &str) -> Result<Vec<u8>> {
        self.fetch(sha256, Kind::Image).await
    }

    /// A thumbnail sheet by its SHA-256, for a hash the manifest names as a sheet of `columns`
    /// cells per row. Checked against its name; it must be `columns` thumbnails wide and 1 to
    /// 200 whole thumbnails high.
    pub async fn sheet(&self, sha256: &str, columns: u32) -> Result<Vec<u8>> {
        if !(1..=SHEET_COLUMNS).contains(&columns) {
            let host = self.site.host_str().unwrap_or("the site");
            return Err(unavailable(host, "not a sheet's columns"));
        }
        self.fetch(sha256, Kind::Sheet { columns }).await
    }

    async fn fetch(&self, sha256: &str, kind: Kind) -> Result<Vec<u8>> {
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
        let result = self.fetch_file(url, &host, sha256, kind).await;
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
        let probe: Envelope<FormatProbe> = serde_json::from_slice(&body)
            .map_err(|_| unavailable(host, "the manifest isn't valid"))?;
        match probe.data {
            None => return Err(unavailable(host, "the manifest is empty")),
            Some(p) if p.format_version > FORMAT_VERSION => return Ok(Fetched::TooNew),
            Some(_) => {}
        }
        let envelope: Envelope<Manifest> = serde_json::from_slice(&body)
            .map_err(|_| unavailable(host, "the manifest isn't valid"))?;
        let manifest = envelope
            .data
            .ok_or_else(|| unavailable(host, "the manifest is empty"))?;
        if manifest.hashes().len() > FILES_CAP {
            return Err(unavailable(host, "the manifest names too many files"));
        }
        manifest.check().map_err(|why| unavailable(host, why))?;
        Ok(Fetched::Manifest(manifest))
    }

    async fn fetch_file(&self, url: Url, host: &str, sha256: &str, kind: Kind) -> Result<Vec<u8>> {
        let response = self.get(url, host, None).await?;
        match response.status().as_u16() {
            200..=299 => {}
            404 | 503 => return Err(unavailable(host, "the file isn't served")),
            429 => return Err(rate_limited(host)),
            s => return Err(host_trouble(host, s)),
        }
        let body = read_capped(response, kind.cap())
            .await
            .map_err(|e| request_failed(host, e))?
            .ok_or_else(|| unavailable(host, "the file is too large"))?;
        if hex::encode(Sha256::digest(&body)) != sha256 {
            return Err(unavailable(host, "the file doesn't match its name"));
        }
        if !png_size(&body).is_some_and(|size| kind.fits(size)) {
            return Err(unavailable(host, "the file isn't a catalog image"));
        }
        Ok(body)
    }
}
