//! A Perland server's public news: the Engine's `GET /api/v1/invites/<token>/news` and the
//! pictures under `/news/<post>/image`, found through the invite's site the way the invite
//! view is. The token is the credential: nothing here prints one, and every error names the
//! Engine's host only.
//!
//! News is a bonus, never a blocker. "No news here" (a 404, or a 503 with or without
//! `news_disabled`) is `Ok(None)`, and the Engine's per-IP limit is shared by every server, so
//! one 429 quiets the whole client for a while.

use std::{
    collections::{HashMap, VecDeque},
    fmt,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;

use crate::{
    error::Result,
    http::read_capped,
    invite::{
        client::{host_of, host_trouble, rate_limited, request_failed, unexpected},
        invite_route, InviteClient, InviteLink,
    },
    ErrorCode,
};

/// The longest news page read, in bytes.
const PAGE_CAP: usize = 256 << 10;
/// The largest picture kept, in bytes.
const IMAGE_CAP: usize = 2 << 20;
const IMAGE_CACHE: usize = 64;
/// The Engine's `limit` range.
const MIN_LIMIT: u8 = 1;
const MAX_LIMIT: u8 = 50;
/// Invites whose pages are remembered, before expired ones are dropped.
const SLOTS: usize = 256;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const JPEG_SIGNATURE: &[u8] = b"\xFF\xD8\xFF";

/// A serde mirror of the Engine's public `PostDTO`: unknown fields are ignored and every field
/// the Engine may omit has a default. Times stay RFC3339 text. `kind` is the Engine's `type`
/// as sent; deciding what an unknown one means is the app's job.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsPost {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    /// Plain text with newlines: never HTML.
    pub body: String,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub image_version: Option<String>,
    pub modpack: Option<NewsModpack>,
    pub created_at: String,
}

/// A modpack post's change. `changes` is absent when the Engine could not read the pack.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewsModpack {
    pub changes: Option<NewsChanges>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NewsChanges {
    pub added: NameList,
    pub removed: NameList,
    pub updated: NameList,
}

/// Some mods: how many, and the first few by name.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct NameList {
    pub count: u32,
    pub names: Vec<String>,
}

/// A page of posts, newest first. `next_before` is absent on the last page.
#[derive(Clone, Debug, Default)]
pub struct NewsPage {
    pub posts: Vec<NewsPost>,
    pub next_before: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
struct RawPage {
    posts: Vec<NewsPost>,
    next_before: Option<String>,
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<RawPage>,
}

/// What one invite's first page answered, and when.
struct Memo {
    at: Instant,
    limit: u8,
    page: Option<NewsPage>,
}

/// Held across a request: callers that arrive meanwhile wait and share its answer.
type Slot = Arc<tokio::sync::Mutex<Option<Memo>>>;
/// (canonical invite, post id, image version).
type ImageKey = (String, String, String);
type ImageCache = VecDeque<(ImageKey, Vec<u8>)>;

#[derive(Clone)]
pub struct NewsClient {
    invites: InviteClient,
    /// How long a first page is remembered.
    memo: Duration,
    /// How long after a 429 the Engine is left alone.
    backoff: Duration,
    /// Canonical invite → its slot. Memory only.
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    /// When the Engine last answered 429, and its host. One for the whole client: the limit
    /// is per IP, across every server.
    limited: Arc<Mutex<Option<(Instant, String)>>>,
    /// Oldest first. Memory only.
    images: Arc<Mutex<ImageCache>>,
}

// Manual Debug: the slot and cache keys hold tokens.
impl fmt::Debug for NewsClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NewsClient")
            .field("memo", &self.memo)
            .field("backoff", &self.backoff)
            .finish_non_exhaustive()
    }
}

/// `post_id` is a UUID (any case), so it is safe in a path.
fn is_uuid(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        })
}

impl NewsClient {
    /// Pages are remembered for 60 s, and a 429 quiets the client for 60 s.
    pub fn new(invites: InviteClient) -> Self {
        Self {
            invites,
            memo: Duration::from_secs(60),
            backoff: Duration::from_secs(60),
            slots: Arc::default(),
            limited: Arc::default(),
            images: Arc::default(),
        }
    }

    /// Other times for `memo` and `backoff`, so tests stay quick.
    pub fn with_timing(mut self, memo: Duration, backoff: Duration) -> Self {
        self.memo = memo;
        self.backoff = backoff;
        self
    }

    /// One page of a server's news, newest first. `limit` is clamped to 1-50. `before` is the
    /// previous page's `next_before`; it is sent only when it is ASCII digits, but a `before`
    /// that is set always makes this a later page (never remembered, and told about a limit).
    ///
    /// `Ok(None)` is "no news here": a 404, a 503, or a first page asked while the Engine is
    /// being left alone after a 429. A later page in that time is `invite_rate_limited`.
    pub async fn page(
        &self,
        link: &InviteLink,
        limit: u8,
        before: Option<&str>,
    ) -> Result<Option<NewsPage>> {
        let limit = limit.clamp(MIN_LIMIT, MAX_LIMIT);
        let later = before.is_some();
        let sent = before.filter(|b| !b.is_empty() && b.bytes().all(|c| c.is_ascii_digit()));
        let slot = self.slot(&link.canonical());
        let mut memo = slot.lock().await;
        if !later {
            if let Some(m) = memo.as_ref() {
                if m.limit == limit && m.at.elapsed() < self.memo {
                    return Ok(m.page.clone());
                }
            }
        }
        if let Some(host) = self.backed_off() {
            return if later {
                Err(rate_limited(&host))
            } else {
                Ok(None)
            };
        }
        match self.fetch(link, limit, sent).await {
            Ok(page) => {
                if !later {
                    *memo = Some(Memo {
                        at: Instant::now(),
                        limit,
                        page: page.clone(),
                    });
                }
                Ok(page)
            }
            Err(e) if e.code == ErrorCode::InviteRateLimited && !later => Ok(None),
            Err(e) => {
                if e.code == ErrorCode::Offline {
                    // The Engine may have moved: ask the site again next time.
                    self.invites.forget_origin(&link.origin());
                }
                Err(e)
            }
        }
    }

    async fn fetch(
        &self,
        link: &InviteLink,
        limit: u8,
        before: Option<&str>,
    ) -> Result<Option<NewsPage>> {
        let engine = self.invites.engine_for(link).await?;
        let host = host_of(&engine);
        let mut url = invite_route(&engine, link, "/news");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("limit", &limit.to_string());
            if let Some(before) = before {
                query.append_pair("before", before);
            }
        }
        let response = self
            .invites
            .http()
            .get(url)
            .timeout(self.invites.timing().request_timeout)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        match response.status().as_u16() {
            200..=299 => {}
            404 | 503 => return Ok(None),
            429 => {
                *self.limited.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((Instant::now(), host.clone()));
                return Err(rate_limited(&host));
            }
            400 => return Err(unexpected(&host, "the news request was refused")),
            s => return Err(host_trouble(&host, s)),
        }
        let body = read_capped(response, PAGE_CAP)
            .await
            .map_err(|e| request_failed(&host, e))?
            .ok_or_else(|| unexpected(&host, "the news page is too large"))?;
        // The parser's own message can quote the body, so it is never shown.
        let envelope: Envelope = serde_json::from_slice(&body)
            .map_err(|_| unexpected(&host, "the news page isn't valid"))?;
        let raw = envelope
            .data
            .ok_or_else(|| unexpected(&host, "the news page is empty"))?;
        Ok(Some(NewsPage {
            posts: raw.posts,
            next_before: raw.next_before.filter(|b| !b.is_empty()),
        }))
    }

    /// The Engine's host while it is being left alone after a 429.
    fn backed_off(&self) -> Option<String> {
        let mut limited = self.limited.lock().unwrap_or_else(|e| e.into_inner());
        match limited.as_ref() {
            Some((at, host)) if at.elapsed() < self.backoff => Some(host.clone()),
            Some(_) => {
                *limited = None;
                None
            }
            None => None,
        }
    }

    fn slot(&self, canonical: &str) -> Slot {
        let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(slot) = slots.get(canonical) {
            return slot.clone();
        }
        if slots.len() >= SLOTS {
            // Drop what is idle and stale; a slot someone holds or waits on stays.
            slots.retain(|_, slot| {
                Arc::strong_count(slot) > 1
                    || slot.try_lock().map_or(true, |memo| {
                        memo.as_ref().is_some_and(|m| m.at.elapsed() < self.memo)
                    })
            });
        }
        slots.entry(canonical.to_string()).or_default().clone()
    }

    /// A post's picture (PNG or JPEG signature checked, at most 2 MiB), or `None`: no picture,
    /// or anything going wrong. Never an error, and a `post_id` that isn't a UUID asks nobody.
    /// Kept per (invite, post, version).
    pub async fn image(
        &self,
        link: &InviteLink,
        post_id: &str,
        image_version: &str,
    ) -> Option<Vec<u8>> {
        if !is_uuid(post_id) {
            return None;
        }
        let post_id = post_id.to_ascii_lowercase();
        let key = (link.canonical(), post_id, image_version.to_string());
        if let Some(bytes) = self.cached_image(&key) {
            return Some(bytes);
        }
        let engine = self.invites.engine_for(link).await.ok()?;
        let mut url = invite_route(&engine, link, &format!("/news/{}/image", key.1));
        if !image_version.is_empty() {
            url.query_pairs_mut().append_pair("v", image_version);
        }
        let response = self
            .invites
            .http()
            .get(url)
            .timeout(self.invites.timing().request_timeout)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body = read_capped(response, IMAGE_CAP).await.ok()??;
        if !(body.starts_with(PNG_SIGNATURE) || body.starts_with(JPEG_SIGNATURE)) {
            return None;
        }
        let mut images = self.images.lock().unwrap_or_else(|e| e.into_inner());
        images.retain(|(k, _)| *k != key);
        if images.len() >= IMAGE_CACHE {
            images.pop_front();
        }
        images.push_back((key, body.clone()));
        Some(body)
    }

    fn cached_image(&self, key: &ImageKey) -> Option<Vec<u8>> {
        self.images
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, bytes)| bytes.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuids_are_checked_by_shape() {
        assert!(is_uuid("6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5e"));
        assert!(is_uuid("6F1C2D3E-4B5A-4C6D-8E7F-901A2B3C4D5E"));
        for bad in [
            "",
            "../x",
            "6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5",
            "6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5ee",
            "6f1c2d3e_4b5a-4c6d-8e7f-901a2b3c4d5e",
            "6f1c2d3e-4b5a-4c6d-8e7f-901a2b3c4d5g",
        ] {
            assert!(!is_uuid(bad), "{bad:?}");
        }
    }
}
