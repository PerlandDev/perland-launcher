//! Talks to a server's Perland invite. It finds the Engine through the invite site's
//! `config.json`, reads the public view (shared by every caller for 15 s), and fetches the
//! icon. There is one per process, owned by `Sources`. Every error names the host only: the
//! token is in each request path, so no URL ever reaches a `LauncherError`.

use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;

use super::{InviteLink, InviteView, Url};
use crate::error::{ErrorCode, LauncherError, Result};

#[derive(Debug, Clone, Copy)]
pub struct InviteTiming {
    /// How long one view read is shared by every caller.
    pub view_ttl: Duration,
    /// How often a pending pack is polled, and for how long.
    pub pack_poll_every: Duration,
    pub pack_poll_for: Duration,
    /// The longest one `config.json` or view request may take, body included, so a slow
    /// Engine can't hold the shared view slot.
    pub request_timeout: Duration,
}

impl Default for InviteTiming {
    fn default() -> Self {
        Self {
            view_ttl: Duration::from_secs(15),
            pack_poll_every: Duration::from_secs(5),
            pack_poll_for: Duration::from_secs(180),
            request_timeout: Duration::from_secs(20),
        }
    }
}

/// Body size caps, in bytes. Overridable so tests stay tiny.
#[derive(Debug, Clone, Copy)]
pub struct InviteLimits {
    pub config: usize,
    pub view: usize,
    pub icon: usize,
    pub error_body: usize,
}

impl Default for InviteLimits {
    fn default() -> Self {
        Self {
            config: 4 << 10,
            view: 64 << 10,
            icon: 1 << 20,
            error_body: 4 << 10,
        }
    }
}

/// The view together with the Engine it came from (an address without a host falls back to
/// the Engine's host).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineView {
    pub engine: Url,
    pub view: InviteView,
}

type Slot = Arc<tokio::sync::Mutex<Option<(Instant, InviteView)>>>;

/// Where the launcher learned a site's Engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Learned {
    /// From an instance's saved source: it may be stale.
    Saved,
    /// From the site's `config.json`: a saved Engine never replaces it.
    Site,
}

#[derive(Default)]
struct State {
    /// Invite site origin → its Engine.
    engines: Mutex<HashMap<String, (Url, Learned)>>,
    /// (Engine, token) → the last view read. Memory only.
    views: Mutex<HashMap<(String, String), Slot>>,
}

#[derive(Clone)]
pub struct InviteClient {
    http: reqwest::Client,
    timing: InviteTiming,
    limits: InviteLimits,
    state: Arc<State>,
}

// Manual Debug: the cache keys hold tokens.
impl std::fmt::Debug for InviteClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InviteClient")
            .field("timing", &self.timing)
            .finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct SiteConfig {
    #[serde(rename = "engineUrl", default)]
    engine_url: Option<String>,
}

#[derive(Deserialize)]
struct Envelope<T> {
    data: Option<T>,
}

#[derive(Deserialize)]
struct ErrorData {
    #[serde(default)]
    code: Option<String>,
}

pub(crate) fn offline(host: &str, detail: impl Display) -> LauncherError {
    LauncherError::network(ErrorCode::Offline, "You seem to be offline")
        .with_param("host", host)
        .with_fix("Check your internet connection and try again")
        .with_detail(detail)
}

fn site_unreachable(host: &str, why: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::InviteSiteUnreachable,
        format!("Couldn't reach the server's Perland site at {host}"),
    )
    .with_param("host", host)
    .with_fix("Check the link, or try again later")
    .with_detail(why)
}

pub(crate) fn gone(host: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteGone,
        "This invite link doesn't work anymore. Ask the owner for a new one.",
    )
    .with_param("host", host)
    .with_fix("Use a new invite link")
}

pub(crate) fn rate_limited(host: &str) -> LauncherError {
    LauncherError::network(
        ErrorCode::InviteRateLimited,
        "Too many requests. Try again in a minute.",
    )
    .with_param("host", host)
}

pub(crate) fn host_trouble(host: &str, status: impl Display) -> LauncherError {
    LauncherError::network(
        ErrorCode::HostTrouble,
        format!("{host} is having trouble ({status})"),
    )
    .with_param("host", host)
    .with_param("status", status)
    .with_fix("Try again in a few minutes")
}

fn unexpected(host: &str, why: &str) -> LauncherError {
    LauncherError::loader(
        ErrorCode::MetaUnexpected,
        format!("Unexpected data from {host}"),
    )
    .with_param("host", host)
    .with_detail(why)
}

/// A request that never got an answer. `without_url` keeps the token out of the detail.
pub(crate) fn request_failed(host: &str, e: reqwest::Error) -> LauncherError {
    if e.is_redirect() {
        host_trouble(host, "redirect")
    } else {
        offline(host, e.without_url())
    }
}

/// What an invite route's status means; `None` for success.
pub(crate) fn status_error(status: reqwest::StatusCode, host: &str) -> Option<LauncherError> {
    match status.as_u16() {
        200..=299 | 304 => None,
        404 => Some(gone(host)),
        429 => Some(rate_limited(host)),
        s => Some(host_trouble(host, s)),
    }
}

/// At most `cap` bytes of the body; `None` when it is longer.
async fn read_capped(
    mut response: reqwest::Response,
    cap: usize,
) -> std::result::Result<Option<Vec<u8>>, reqwest::Error> {
    if response.content_length().is_some_and(|n| n > cap as u64) {
        return Ok(None);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        body.extend_from_slice(&chunk);
        if body.len() > cap {
            return Ok(None);
        }
    }
    Ok(Some(body))
}

/// An `engineUrl` the launcher will call: https (or http on this computer), with a host and no
/// user info, query or fragment.
pub fn check_engine_url(raw: &str) -> Option<Url> {
    let url = Url::parse(raw.trim()).ok()?;
    let scheme_ok =
        url.scheme() == "https" || (url.scheme() == "http" && crate::http::is_loopback(&url));
    (scheme_ok
        && url.host_str().is_some_and(|h| !h.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

/// `<engine>/api/v1/invites/<token><suffix>` (Engine `invite/src/lib/api.ts` `inviteUrl`).
pub fn invite_route(engine: &Url, link: &InviteLink, suffix: &str) -> Url {
    let base = engine.as_str().trim_end_matches('/');
    Url::parse(&format!("{base}/api/v1/invites/{}{suffix}", link.token()))
        .expect("a checked Engine URL plus a base64url token and a fixed suffix is a URL")
}

/// The Engine's host for error messages (never a URL: routes carry the token).
pub(crate) fn host_of(engine: &Url) -> String {
    engine.host_str().unwrap_or("the server").to_string()
}

impl InviteClient {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            timing: InviteTiming::default(),
            limits: InviteLimits::default(),
            state: Arc::default(),
        }
    }

    pub fn with_timing(mut self, timing: InviteTiming) -> Self {
        self.timing = timing;
        self
    }

    pub fn with_limits(mut self, limits: InviteLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn timing(&self) -> InviteTiming {
        self.timing
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Seed the Engine for `link`'s site from what an instance saved, so a Play needs no
    /// `config.json`. An unusable value is ignored, and a known Engine is kept: above all one
    /// the site's `config.json` named.
    pub fn remember_engine(&self, link: &InviteLink, engine: &str) {
        if let Some(url) = check_engine_url(engine) {
            self.state
                .engines
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .entry(link.origin())
                .or_insert((url, Learned::Saved));
        }
    }

    fn forget_engine(&self, link: &InviteLink) {
        self.state
            .engines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&link.origin());
    }

    /// The Engine behind `link`'s invite site: `<origin>/config.json` → `engineUrl`, cached
    /// per origin.
    pub async fn engine_for(&self, link: &InviteLink) -> Result<Url> {
        Ok(self.known_engine(link).await?.0)
    }

    async fn known_engine(&self, link: &InviteLink) -> Result<(Url, Learned)> {
        let known = self
            .state
            .engines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&link.origin())
            .cloned();
        match known {
            Some(known) => Ok(known),
            None => Ok((self.ask_site(link).await?, Learned::Site)),
        }
    }

    /// `<origin>/config.json` now, remembered as the site's own answer.
    async fn ask_site(&self, link: &InviteLink) -> Result<Url> {
        let host = link.host();
        let response = self
            .http
            .get(format!("{}/config.json", link.origin()))
            .timeout(self.timing.request_timeout)
            .send()
            .await
            .map_err(|e| {
                if e.is_redirect() {
                    site_unreachable(host, "config.json redirected somewhere unsafe")
                } else {
                    offline(host, e.without_url())
                }
            })?;
        if !response.status().is_success() {
            return Err(site_unreachable(
                host,
                &format!("config.json answered {}", response.status().as_u16()),
            ));
        }
        let body = read_capped(response, self.limits.config)
            .await
            .map_err(|e| offline(host, e.without_url()))?
            .ok_or_else(|| site_unreachable(host, "config.json is too large"))?;
        let config: SiteConfig = serde_json::from_slice(&body)
            .map_err(|_| site_unreachable(host, "config.json isn't valid JSON"))?;
        let engine = config
            .engine_url
            .as_deref()
            .and_then(check_engine_url)
            .ok_or_else(|| site_unreachable(host, "config.json has no https engineUrl"))?;
        self.state
            .engines
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(link.origin(), (engine.clone(), Learned::Site));
        Ok(engine)
    }

    /// The view, shared by every caller for `view_ttl` (status polls, `pack_status`, Play).
    pub async fn view(&self, link: &InviteLink) -> Result<EngineView> {
        self.view_inner(link, false).await
    }

    /// The view from the Engine now, skipping the shared copy, which it then replaces (the
    /// pending-pack poll).
    pub async fn view_fresh(&self, link: &InviteLink) -> Result<EngineView> {
        self.view_inner(link, true).await
    }

    /// A saved Engine that can't be reached, or no longer knows the invite, may have moved:
    /// the site's `config.json` is asked once, and a different Engine it names is read
    /// instead. A failure that stands forgets an unreachable Engine, so the next read asks again.
    async fn view_inner(&self, link: &InviteLink, fresh: bool) -> Result<EngineView> {
        let (engine, learned) = self.known_engine(link).await?;
        let result = match self.view_at(&engine, link, fresh).await {
            Err(e)
                if learned == Learned::Saved
                    && matches!(e.code, ErrorCode::Offline | ErrorCode::InviteGone) =>
            {
                match self.ask_site(link).await {
                    Ok(moved) if moved != engine => self.view_at(&moved, link, fresh).await,
                    // The same Engine, or no answer: the first error stands.
                    _ => Err(e),
                }
            }
            other => other,
        };
        if let Err(e) = &result {
            if e.code == ErrorCode::Offline {
                self.forget_engine(link);
            }
        }
        result
    }

    /// The view from `engine`, through its shared slot.
    async fn view_at(&self, engine: &Url, link: &InviteLink, fresh: bool) -> Result<EngineView> {
        let slot = {
            let mut views = self.state.views.lock().unwrap_or_else(|e| e.into_inner());
            views
                .entry((engine.to_string(), link.token().to_string()))
                .or_default()
                .clone()
        };
        // Held across the request: callers that arrive meanwhile wait and share its answer.
        let mut cached = slot.lock().await;
        if !fresh {
            if let Some((at, view)) = cached.as_ref() {
                if at.elapsed() < self.timing.view_ttl {
                    return Ok(EngineView {
                        engine: engine.clone(),
                        view: view.clone(),
                    });
                }
            }
        }
        let view = self.fetch_view(engine, link).await?;
        *cached = Some((Instant::now(), view.clone()));
        Ok(EngineView {
            engine: engine.clone(),
            view,
        })
    }

    async fn fetch_view(&self, engine: &Url, link: &InviteLink) -> Result<InviteView> {
        let host = host_of(engine);
        let response = self
            .http
            .get(invite_route(engine, link, ""))
            .timeout(self.timing.request_timeout)
            .send()
            .await
            .map_err(|e| request_failed(&host, e))?;
        if let Some(e) = status_error(response.status(), &host) {
            return Err(e);
        }
        let body = read_capped(response, self.limits.view)
            .await
            .map_err(|e| request_failed(&host, e))?
            .ok_or_else(|| unexpected(&host, "the invite view is too large"))?;
        // The parser's own message can quote the body, so it is never shown.
        let envelope: Envelope<InviteView> = serde_json::from_slice(&body)
            .map_err(|_| unexpected(&host, "the invite view isn't valid"))?;
        envelope
            .data
            .ok_or_else(|| unexpected(&host, "the invite view is empty"))
    }

    /// The server's icon PNG (PNG signature checked, size capped), or `None`: no icon, or
    /// anything going wrong. Never an error.
    pub async fn icon(&self, link: &InviteLink) -> Option<Vec<u8>> {
        let engine = self.engine_for(link).await.ok()?;
        let response = self
            .http
            .get(invite_route(&engine, link, "/icon"))
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body = read_capped(response, self.limits.icon).await.ok()??;
        body.starts_with(b"\x89PNG\r\n\x1a\n").then_some(body)
    }

    /// `<engine>/api/v1/invites/<token>/pack`: the `.mrpack` of the commit the server runs.
    pub async fn pack_url(&self, link: &InviteLink) -> Result<Url> {
        Ok(invite_route(&self.engine_for(link).await?, link, "/pack"))
    }

    /// The Engine's `data.code` from a failed pack answer (`modrinth_unavailable`, …), kept
    /// only if it is a short snake_case word.
    pub(crate) async fn error_code(&self, url: Url) -> Option<String> {
        let response = self.http.get(url).send().await.ok()?;
        let body = read_capped(response, self.limits.error_body).await.ok()??;
        let envelope: Envelope<ErrorData> = serde_json::from_slice(&body).ok()?;
        envelope.data?.code.filter(|c| {
            (1..=64).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
        })
    }
}
