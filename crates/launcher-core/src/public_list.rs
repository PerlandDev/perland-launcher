//! Perland's public server list: the Engine's `GET /api/v1/public-servers`, found through the
//! Perland site's `config.json` the way an invite finds its Engine. The list is public, but
//! every server's token is the credential of its invite: nothing here prints one, and every
//! error names the Engine's host only.

use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    http::read_capped,
    instances::ServerStatus,
    invite::{
        client::{
            check_engine_url, host_of, host_trouble, rate_limited, request_failed,
            server_list_closed, unexpected,
        },
        invite_route,
        view::modrinth_ref,
        InviteAddress, InviteClient, InviteGame, InviteLink, InviteModrinth, InvitePack,
        InvitePlayers, Url,
    },
    sources::perland::joinable_game,
};

/// The site whose `config.json` names the Engine.
pub const DEFAULT_SITE: &str = "https://play.perland.online";
/// Servers per page. The Engine's default, sent explicitly.
pub const PAGE_SIZE: u32 = 50;

/// The longest list page read, in bytes.
const PAGE_CAP: usize = 512 << 10;
/// The largest icon kept, in bytes.
const ICON_CAP: usize = 1 << 20;
const ICON_CACHE: usize = 256;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// A Perland site: https, or http on this computer; no user info, query or fragment. Reduced
/// to its origin (any path is dropped).
pub fn site_url(raw: &str) -> Option<Url> {
    let mut url = check_engine_url(raw)?;
    url.set_path("/");
    Some(url)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListSort {
    /// Most players first (the Engine's default).
    #[default]
    Players,
    Newest,
    Name,
}

impl ListSort {
    fn as_str(self) -> &'static str {
        match self {
            ListSort::Players => "players",
            ListSort::Newest => "newest",
            ListSort::Name => "name",
        }
    }
}

/// What the player asked for. Every filter is the Engine's: the launcher filters nothing itself.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ListQuery {
    pub sort: ListSort,
    pub modded: Option<bool>,
    pub minecraft: Option<String>,
    pub has_room: bool,
    pub category: Option<String>,
    pub language: Option<String>,
    pub region: Option<String>,
    pub offset: u32,
}

impl ListQuery {
    /// The query string's pairs: only what is set (the default sort and `hasRoom=false` are
    /// left out), then `limit` and `offset`, which are always sent.
    pub fn pairs(&self) -> Vec<(&'static str, String)> {
        let mut pairs = Vec::new();
        if self.sort != ListSort::default() {
            pairs.push(("sort", self.sort.as_str().to_string()));
        }
        if let Some(modded) = self.modded {
            pairs.push(("modded", modded.to_string()));
        }
        let text = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_string)
        };
        if let Some(v) = text(&self.minecraft) {
            pairs.push(("minecraft", v));
        }
        if self.has_room {
            pairs.push(("hasRoom", "true".to_string()));
        }
        if let Some(v) = text(&self.category) {
            pairs.push(("category", v));
        }
        if let Some(v) = text(&self.language) {
            pairs.push(("language", v));
        }
        if let Some(v) = text(&self.region) {
            pairs.push(("region", v));
        }
        pairs.push(("limit", PAGE_SIZE.to_string()));
        pairs.push(("offset", self.offset.to_string()));
        pairs
    }
}

/// A serde mirror of the Engine's `publiclistdtos` server: unknown fields are ignored and every
/// field the Engine may omit has a default. `token` is the invite's credential.
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PublicServer {
    pub token: String,
    /// Absent without the Engine's `INVITE_BASE_URL`.
    pub invite_url: Option<String>,
    /// The server's own name, which never changes.
    pub name: String,
    /// The name the team gives players; absent on Engines before display names.
    pub display_name: Option<String>,
    /// Raw text that may hold `§` colour codes.
    pub motd: String,
    pub icon_version: Option<String>,
    pub address: InviteAddress,
    pub game: InviteGame,
    pub modded: bool,
    /// `online | starting | sleeping`; see [`PublicServer::status`].
    pub status: String,
    pub players: Option<InvitePlayers>,
    pub whitelisted: bool,
    pub pack: Option<InvitePack>,
    /// The Modrinth modpack version the server runs. Read when present: today's Engine
    /// doesn't send it in the list.
    #[serde(deserialize_with = "modrinth_ref")]
    pub modrinth: Option<InviteModrinth>,
    pub categories: Vec<String>,
    pub language: String,
    pub region: Option<String>,
    pub listed_at: Option<String>,
}

// Manual Debug: the token is a credential.
impl fmt::Debug for PublicServer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicServer")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

impl PublicServer {
    /// The Engine's contract: a state the launcher doesn't know counts as sleeping.
    pub fn status(&self) -> ServerStatus {
        match ServerStatus::parse(&self.status) {
            Some(ServerStatus::Online) => ServerStatus::Online,
            Some(ServerStatus::Starting) => ServerStatus::Starting,
            _ => ServerStatus::Sleeping,
        }
    }

    /// Can Play join it? The same rule an invite link is held to.
    pub fn joinable(&self) -> bool {
        joinable_game(
            "stack",
            &self.game,
            self.pack.is_some() || self.modrinth.is_some(),
        )
    }
}

/// What the whole listed set offers, for the filter menus.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Facets {
    pub categories: Vec<String>,
    pub languages: Vec<String>,
    pub minecraft_versions: Vec<String>,
    pub regions: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ListPage {
    pub servers: Vec<PublicServer>,
    pub total: u32,
    pub facets: Facets,
}

/// A page and the Engine it came from.
#[derive(Clone, Debug)]
pub struct FetchedPage {
    pub engine: Url,
    pub page: ListPage,
}

#[derive(Deserialize)]
struct Envelope {
    data: Option<ListPage>,
}

type IconKey = (String, String);
type IconCache = VecDeque<(IconKey, Vec<u8>)>;

#[derive(Clone)]
pub struct PublicListClient {
    invites: InviteClient,
    site: Url,
    /// (token, icon version) → PNG, oldest first. Memory only.
    icons: Arc<Mutex<IconCache>>,
}

// Manual Debug: the icon cache keys hold tokens.
impl fmt::Debug for PublicListClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicListClient")
            .field("site", &self.site.as_str())
            .finish_non_exhaustive()
    }
}

impl PublicListClient {
    /// `site` should come from [`site_url`].
    pub fn new(invites: InviteClient, site: Url) -> Self {
        Self {
            invites,
            site,
            icons: Arc::default(),
        }
    }

    pub fn site(&self) -> &Url {
        &self.site
    }

    /// One page of the list. A closed list (404, 503, or a site naming no Engine) is
    /// `server_list_closed`.
    pub async fn page(&self, query: &ListQuery) -> Result<FetchedPage> {
        let engine = self.invites.engine_for_site(&self.site).await?;
        let host = host_of(&engine);
        let result = self.fetch(&engine, &host, query).await;
        if let Err(e) = &result {
            // The Engine may have moved: ask the site again next time.
            if e.code == crate::ErrorCode::Offline {
                self.invites
                    .forget_origin(&self.site.origin().ascii_serialization());
            }
        }
        result.map(|page| FetchedPage { engine, page })
    }

    async fn fetch(&self, engine: &Url, host: &str, query: &ListQuery) -> Result<ListPage> {
        let mut url = Url::parse(&format!(
            "{}/api/v1/public-servers",
            engine.as_str().trim_end_matches('/')
        ))
        .expect("a checked Engine URL plus a fixed path is a URL");
        url.query_pairs_mut().extend_pairs(query.pairs());
        let response = self
            .invites
            .http()
            .get(url)
            .timeout(self.invites.timing().request_timeout)
            .send()
            .await
            .map_err(|e| request_failed(host, e))?;
        match response.status().as_u16() {
            200..=299 => {}
            404 | 503 => return Err(server_list_closed(host)),
            429 => return Err(rate_limited(host)),
            400 => return Err(unexpected(host, "the server list refused the filters")),
            s => return Err(host_trouble(host, s)),
        }
        let body = read_capped(response, PAGE_CAP)
            .await
            .map_err(|e| request_failed(host, e))?
            .ok_or_else(|| unexpected(host, "the server list is too large"))?;
        // The parser's own message can quote the body, so it is never shown.
        let envelope: Envelope = serde_json::from_slice(&body)
            .map_err(|_| unexpected(host, "the server list isn't valid"))?;
        envelope
            .data
            .ok_or_else(|| unexpected(host, "the server list is empty"))
    }

    /// The invite for `token` on this site, if `token` is shaped like one.
    fn site_link(&self, token: &str) -> Option<InviteLink> {
        let origin = self.site.origin().ascii_serialization();
        InviteLink::parse(&format!("{origin}/{token}")).ok()
    }

    /// A server's icon PNG (signature checked, size capped), or `None`: no icon, or anything
    /// going wrong. Never an error. Kept per (token, icon version).
    pub async fn icon(&self, token: &str, icon_version: &str) -> Option<Vec<u8>> {
        let key = (token.to_string(), icon_version.to_string());
        if let Some(png) = self.cached_icon(&key) {
            return Some(png);
        }
        let link = self.site_link(token)?;
        let engine = self.invites.engine_for_site(&self.site).await.ok()?;
        let response = self
            .invites
            .http()
            .get(invite_route(&engine, &link, "/icon"))
            .timeout(self.invites.timing().request_timeout)
            .send()
            .await
            .ok()?;
        if !response.status().is_success() {
            return None;
        }
        let body = read_capped(response, ICON_CAP).await.ok()??;
        if !body.starts_with(PNG_SIGNATURE) {
            return None;
        }
        let mut icons = self.icons.lock().unwrap_or_else(|e| e.into_inner());
        icons.retain(|(k, _)| *k != key);
        if icons.len() >= ICON_CACHE {
            icons.pop_front();
        }
        icons.push_back((key, body.clone()));
        Some(body)
    }

    fn cached_icon(&self, key: &IconKey) -> Option<Vec<u8>> {
        self.icons
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, png)| png.clone())
    }

    /// The invite Play joins: the server's own `inviteUrl` when it is one, else the site's
    /// `<site>/<token>`, else `None`.
    pub fn invite_for(&self, server: &PublicServer) -> Option<InviteLink> {
        server
            .invite_url
            .as_deref()
            .and_then(|u| InviteLink::parse(u).ok())
            .or_else(|| self.site_link(&server.token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_server_reads_modrinth() {
        let server: PublicServer = serde_json::from_value(serde_json::json!({
            "token": "A".repeat(43), "name": "pack", "modded": true,
            "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric" },
            "modrinth": { "projectId": "1KVo5zza", "versionId": "Yp8wLY1P" }
        }))
        .unwrap();
        assert_eq!(
            server.modrinth,
            Some(InviteModrinth {
                project_id: "1KVo5zza".into(),
                version_id: "Yp8wLY1P".into(),
            })
        );
        assert!(server.joinable());
        let bad: PublicServer = serde_json::from_value(serde_json::json!({
            "token": "A".repeat(43), "name": "pack", "modded": true,
            "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric" },
            "modrinth": { "projectId": "1KVo5zza", "versionId": "no/pe" }
        }))
        .unwrap();
        assert_eq!(bad.modrinth, None);
        assert!(!bad.joinable());
    }

    #[test]
    fn default_query_sends_the_page_only() {
        assert_eq!(
            ListQuery::default().pairs(),
            [("limit", "50".to_string()), ("offset", "0".to_string())]
        );
    }

    #[test]
    fn blank_filters_are_not_sent() {
        let query = ListQuery {
            minecraft: Some("  ".into()),
            category: Some(String::new()),
            ..ListQuery::default()
        };
        assert_eq!(query.pairs().len(), 2);
    }

    #[test]
    fn has_room_false_is_left_out_and_modded_false_is_not() {
        let query = ListQuery {
            modded: Some(false),
            ..ListQuery::default()
        };
        assert!(query.pairs().contains(&("modded", "false".to_string())));
        assert!(!query.pairs().iter().any(|(k, _)| *k == "hasRoom"));
    }
}
