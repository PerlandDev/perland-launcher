//! Shapes the commands take and return that aren't core types.
use std::{fmt, path::PathBuf};

use launcher_core::{
    accounts::BuildKind,
    browse::{
        compat::ContentKind,
        plan::{ItemReason, PlanItem, PlanSkip},
        search::SortIndex,
    },
    instances::{accent_of, Instance, LoaderSpec, ServerAddress, ServerStatus},
    invite::{non_empty, shown_invite, EngineView, InviteLink, InvitePlayers},
    java::{JavaFit, JavaInstall, TEMURIN},
    meta::loaders::LoaderKind,
    mods::LocalMod,
    perland::{CreateAllowance, Deletion, Disk, FreeLimits},
    public_list::Facets,
    sync::{ManualFile, PendingUpdate, SyncReport},
    LauncherError,
};
use serde::{Deserialize, Serialize};

use crate::events::RunStateKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Build {
    Dev,
    Beta,
    Release,
}

impl From<BuildKind> for Build {
    fn from(kind: BuildKind) -> Self {
        match kind {
            BuildKind::Dev => Build::Dev,
            BuildKind::Beta => Build::Beta,
            BuildKind::Release => Build::Release,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub build: Build,
    pub offline_allowed: bool,
    pub system_memory_mb: u64,
    pub data_dir: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
}

/// What a folder from another launcher holds, to prefill the import form. `path` is the folder
/// the player picked (never the resolved game folder); the parts not detected are `None`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub path: PathBuf,
    pub name: Option<String>,
    pub minecraft: Option<String>,
    pub loader: Option<LoaderSpec>,
    /// Bytes the import copies.
    pub size_bytes: u64,
    /// The memory the instance gets, in MB, when the other launcher kept one of its own.
    pub memory_mb: Option<u32>,
    /// What the other launcher had, when it was outside what this launcher allows and
    /// `memory_mb` is the nearest allowed amount instead.
    pub memory_wanted_mb: Option<u32>,
    /// The Java arguments the instance gets: the other launcher's own for it, less the ones
    /// this launcher sets itself. `None` when none are left.
    pub java_args: Option<String>,
    /// Time played the other launcher counted, added to the instance's. `None` when there is
    /// none, or another instance already counts this folder's.
    pub playtime_seconds: Option<u64>,
}

/// Import the picked folder as a new instance with this name and game. The folder is inspected
/// again, so nothing about it is taken from the UI but its path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRequest {
    pub path: PathBuf,
    pub name: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
}

/// Time played in one instance on this computer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaytimeEntry {
    pub id: String,
    pub seconds: u64,
    /// Seconds since the Unix epoch when its last game ended (0: never played).
    pub last_played: u64,
}

/// Folders the UI may ask to open. Never a raw path, so the webview can't open arbitrary places.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OpenTarget {
    Instance { id: String },
    Mods { id: String },
    ResourcePacks { id: String },
    ShaderPacks { id: String },
    Data,
    Logs,
    Downloads,
}

/// A resource pack or shader in an instance, as the Mods & packs screen lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentFile {
    pub file_name: String,
    pub size: u64,
    pub folder: bool,
    /// Came with the pack: the player can't remove it (sync would put it back).
    pub managed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RunSnapshot {
    pub instance: String,
    pub state: RunStateKind,
    pub update: Option<PackUpdate>,
}

/// An update the player can take, as the "Update ready" dialog and banner show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackUpdate {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
    /// Files the player edited that the update keeps as they are ("Kept your version of N files").
    pub kept: Vec<String>,
}

impl From<&PendingUpdate> for PackUpdate {
    fn from(u: &PendingUpdate) -> Self {
        Self {
            from_version: u.changes.from_version.clone(),
            to_version: u.changes.to_version.clone(),
            added: u.changes.added.clone(),
            updated: u.changes.updated.clone(),
            removed: u.changes.removed.clone(),
            kept: u.kept.clone(),
        }
    }
}

/// What a finished sync changed: "Updated to 1.0.4", "Kept your version of N files".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
    pub kept_modified: Vec<String>,
    /// The player's files the pack's own took the place of, moved to `mods-before-link/`.
    pub set_aside: Vec<String>,
}

impl From<&SyncReport> for SyncSummary {
    fn from(r: &SyncReport) -> Self {
        Self {
            from_version: r.changes.from_version.clone(),
            to_version: r.changes.to_version.clone(),
            added: r.changes.added.len(),
            updated: r.changes.updated.len(),
            removed: r.changes.removed.len(),
            kept_modified: r.kept_modified.clone(),
            set_aside: r.set_aside.clone(),
        }
    }
}

/// For the banner: the installed version, and the update (or why there's no answer).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackStatus {
    pub installed_version: Option<String>,
    pub update: Option<PackUpdate>,
    pub check_error: Option<LauncherError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalChoice {
    pub enabled: bool,
    pub description: Option<String>,
}

/// A mod that comes with the pack. Read-only, except an optional one's toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackMod {
    pub path: String,
    pub name: String,
    pub optional: Option<OptionalChoice>,
}

/// A pack instance's Mods screen: the pack's mods, and the player's own jars.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackContents {
    pub managed: Vec<PackMod>,
    pub additions: Vec<LocalMod>,
}

/// "Download these manually": how many of the blocked files were found so far.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualProgress {
    pub found: usize,
    pub total: usize,
    pub remaining: Vec<ManualFile>,
}

/// An instance's pictures as `data:` URLs (the CSP allows no remote images): its icon and its
/// background. `None` falls back to the generated art.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceArtData {
    pub icon: Option<String>,
    pub banner: Option<String>,
}

/// What the player picked in "Change image…": one of the generated landscapes (by seed), the
/// pack's own art, or a file of their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ArtChoice {
    Generated { seed: String },
    Pack,
    Upload { path: PathBuf },
}

/// What the player picked for the icon in "Change image…": the generated monogram, the pack's
/// own icon, or a file of their own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum IconChoice {
    Generated,
    Pack,
    Upload { path: PathBuf },
}

/// What the New-instance screen hands over. A link is never a file path, and a file is only
/// ever a `.mrpack`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PackAddRequest {
    Link { url: String },
    File { path: PathBuf },
}

// Manual Debug: a link may be an invite, and its token is a credential.
impl fmt::Debug for PackAddRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackAddRequest::Link { url } => {
                let shown = match InviteLink::parse(url) {
                    Ok(_) => shown_invite(url),
                    Err(_) => url.clone(),
                };
                f.debug_struct("Link").field("url", &shown).finish()
            }
            PackAddRequest::File { path } => f.debug_struct("File").field("path", path).finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPlayers {
    pub online: u32,
    /// Absent for networks.
    pub max: Option<u32>,
}

impl From<&InvitePlayers> for ServerPlayers {
    fn from(p: &InvitePlayers) -> Self {
        Self {
            online: p.online,
            max: p.max,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerGame {
    pub version: Option<String>,
    pub loader: Option<String>,
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerPack {
    pub name: String,
    pub version: Option<String>,
    pub mod_count: u32,
    /// `ready | pending | unavailable`.
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerTeam {
    pub display_name: String,
    /// `cyan | blue | violet | magenta | other`: a small mark, never a second lit element.
    pub accent: Option<String>,
}

/// What Join server shows before anything is written. Holds no token: the host is enough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinPreview {
    /// The invite's host ("From play.example.com").
    pub host: String,
    pub name: String,
    /// May hold `§` colour codes.
    pub motd: String,
    /// `stack` or `network`.
    pub kind: String,
    pub status: Option<ServerStatus>,
    pub players: Option<ServerPlayers>,
    pub game: ServerGame,
    pub pack: Option<ServerPack>,
    pub whitelisted: bool,
    pub team: Option<ServerTeam>,
    pub address: Option<ServerAddress>,
    /// The icon as a `data:image/png` URL (the CSP allows no remote images).
    pub icon: Option<String>,
    /// The instance that already joins this server, if any: the screen offers Open.
    pub existing: Option<String>,
}

/// A server instance's live status, for the pill, the address line and the dead-link plate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatusView {
    /// `None` when unknown (a dead link).
    pub status: Option<ServerStatus>,
    pub players: Option<ServerPlayers>,
    pub whitelisted: bool,
    pub pack_state: Option<String>,
    pub gone: bool,
    /// The last known address.
    pub address: Option<ServerAddress>,
}

fn owned(s: &Option<String>) -> Option<String> {
    non_empty(s).map(str::to_string)
}

impl JoinPreview {
    pub fn new(
        link: &InviteLink,
        ev: &EngineView,
        icon: Option<String>,
        existing: Option<String>,
    ) -> Self {
        let view = &ev.view;
        let display_name = view.team.display_name.trim().to_string();
        let accent = accent_of(&view.team.accent);
        let team = (!display_name.is_empty() || accent.is_some()).then_some(ServerTeam {
            display_name,
            accent,
        });
        JoinPreview {
            host: link.host().to_string(),
            name: view.name.trim().to_string(),
            motd: view.motd.chars().take(512).collect(),
            kind: view.kind.clone(),
            status: view.status(),
            players: view.players.as_ref().map(ServerPlayers::from),
            game: ServerGame {
                version: owned(&view.game.version),
                loader: owned(&view.game.loader),
                loader_version: owned(&view.game.loader_version),
            },
            pack: view.pack.as_ref().map(|p| ServerPack {
                name: p.name.trim().to_string(),
                version: owned(&p.version),
                mod_count: p.mod_count,
                state: p.state.clone(),
            }),
            whitelisted: view.whitelisted,
            team,
            address: ServerAddress::from_view(&view.address, &ev.engine),
            icon,
            existing,
        }
    }
}

/// One page of Perland's public server list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicServerPage {
    pub servers: Vec<PublicServerCard>,
    /// How many servers the Engine lists for these filters, over all pages.
    pub total: u32,
    pub facets: Facets,
}

/// A listed server. `token` is its invite's credential: it reaches the window (it keys the icon
/// request) and nothing else.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicServerCard {
    pub token: String,
    /// The canonical invite Play joins, if the server has one.
    pub invite: Option<String>,
    /// Can Play join it? False for a modded server without a modpack.
    pub joinable: bool,
    /// `host:port`, the form players type.
    pub address: Option<String>,
    pub name: String,
    /// Present only when the server has an icon.
    pub icon_version: Option<String>,
    pub game: ServerGame,
    pub modded: bool,
    pub status: ServerStatus,
    pub players: Option<ServerPlayers>,
    pub whitelisted: bool,
    pub pack: Option<ServerPack>,
    pub categories: Vec<String>,
    pub language: String,
    pub region: Option<String>,
    /// The instance that already joins this server, if any.
    pub instance: Option<String>,
}

// Manual Debug: the token is a credential.
impl fmt::Debug for PublicServerCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PublicServerCard")
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}

/// The signed-in player's servers: the ones they own, were invited to, or follow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YourServersPage {
    pub servers: Vec<YourServerCard>,
}

/// A server of Your servers: the public list's card, whose `status` may also be `offline`,
/// plus how the player relates to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YourServerCard {
    #[serde(flatten)]
    pub card: PublicServerCard,
    /// `owner`, `invited` or `followed`.
    pub relation: String,
    /// Whether the player follows it (an owner or invitee can too).
    pub following: bool,
}

/// The player's free server (spec "Create a server for friends" §5 and §6.2): the limits every
/// free server has, whether they may create one now, and theirs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeServerPage {
    pub limits: FreeLimits,
    /// `reason` is the Engine's word: `email_required`, `free_server_exists`,
    /// `too_many_free_servers` (with `retryAfter` seconds) or `free_servers_full`.
    pub create: CreateAllowance,
    pub server: Option<FreeServerCard>,
}

/// The owner's free server as a card, built from the free-server view. `status` folds what the
/// card's pill doesn't know (`creating`, `deleting`, `stopped`) and `freeStatus` keeps the
/// Engine's word. `joinable` needs an invite, a status other than `creating`, `deleting` and
/// `stopped`, and a disk within its limit.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FreeServerCard {
    #[serde(flatten)]
    pub card: PublicServerCard,
    /// `vanilla` or `modrinth`.
    pub kind: String,
    pub memory_mb: u32,
    pub max_players: u32,
    /// `online`, `starting`, `sleeping`, `offline`, `stopped`, `creating`, `deleting`, or a
    /// newer Engine's word.
    pub free_status: String,
    /// When an unused server is deleted; `warned` once the owner was told.
    pub deletion: Option<Deletion>,
    pub disk: Disk,
    /// The invite link to share (holds the invite's credential).
    pub invite_url: Option<String>,
    /// `perland://join?invite=…`, for friends who have the launcher.
    pub join_url: Option<String>,
}

// Manual Debug: the invite URLs hold the invite's credential.
impl fmt::Debug for FreeServerCard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FreeServerCard")
            .field("name", &self.card.name)
            .field("free_status", &self.free_status)
            .finish_non_exhaustive()
    }
}

/// One page of a server's news, newest first, and the marker of what the player has seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerNews {
    pub posts: Vec<NewsPostView>,
    /// Pass it back as `before` for the next page; absent on the last.
    pub next_before: Option<String>,
    /// The `createdAt` of the newest post the player has seen on this server, if any.
    pub seen: Option<String>,
}

/// A news post. Times are the Engine's RFC3339 text; `body` is plain text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewsPostView {
    pub id: String,
    /// `update`, `event` or `modpack`; anything else the Engine sends is an `update`.
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub body: String,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub image_version: Option<String>,
    /// A modpack post's changes; none when the Engine could not read the pack.
    pub modpack: Option<NewsChangesView>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NewsChangesView {
    pub added: NameListView,
    pub removed: NameListView,
    pub updated: NameListView,
}

/// Some mods: how many, and the first few by name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NameListView {
    pub count: u32,
    pub names: Vec<String>,
}

/// A Java the player can choose, and how it fits the game (unknown when the version's needs are).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaFound {
    pub path: PathBuf,
    pub major: u32,
    pub version: String,
    pub vendor: Option<String>,
    pub fit: Option<JavaFit>,
    /// Adoptium's Temurin: suggested as the alternative to Automatic.
    pub recommended: bool,
}

impl JavaFound {
    pub fn new(install: JavaInstall, required: Option<u32>) -> Self {
        Self {
            fit: required.map(|r| JavaFit::of(r, install.major)),
            recommended: install.vendor.as_deref() == Some(TEMURIN),
            path: install.path,
            major: install.major,
            version: install.version,
            vendor: install.vendor,
        }
    }
}

/// The Java an instance's game asks for (when it could be looked up) and every Java on this
/// computer: the ones that match first, Temurin first among equals, then newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JavaOverview {
    pub required: Option<u32>,
    pub installs: Vec<JavaFound>,
}

impl JavaOverview {
    pub fn new(required: Option<u32>, installs: Vec<JavaInstall>) -> Self {
        let mut installs: Vec<JavaFound> = installs
            .into_iter()
            .map(|i| JavaFound::new(i, required))
            .collect();
        installs.sort_by_key(|j| {
            (
                j.fit != Some(JavaFit::Matches),
                !j.recommended,
                std::cmp::Reverse(j.major),
            )
        });
        Self { required, installs }
    }
}

/// Browse's search box and filters. With `instance`, its game decides `minecraft` and `loader`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub kind: ContentKind,
    #[serde(default)]
    pub instance: Option<String>,
    #[serde(default)]
    pub minecraft: Option<String>,
    #[serde(default)]
    pub loader: Option<LoaderKind>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub sort: SortIndex,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanTargetRequest {
    Instance { id: String },
    New,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePlanRequest {
    pub project: String,
    #[serde(default)]
    pub version: Option<String>,
    pub target: PlanTargetRequest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanTargetView {
    Instance {
        id: String,
        name: String,
        minecraft: String,
        loader: Option<LoaderKind>,
    },
    New {
        name: String,
        minecraft: String,
        loader: Option<LoaderKind>,
    },
}

/// One thing an install adds, as the confirm dialog shows it. URLs and hashes stay in the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItemView {
    pub project_id: String,
    pub title: String,
    pub kind: ContentKind,
    pub version_number: String,
    pub file_name: String,
    pub size: u64,
    pub reason: ItemReason,
}

impl From<&PlanItem> for PlanItemView {
    fn from(item: &PlanItem) -> Self {
        Self {
            project_id: item.project_id.clone(),
            title: item.title.clone(),
            kind: item.kind,
            version_number: item.version_number.clone(),
            file_name: item.file.file_name.clone(),
            size: item.file.size,
            reason: item.reason.clone(),
        }
    }
}

/// A stored install plan; the webview installs it by `plan_id` only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlanView {
    pub plan_id: String,
    pub target: PlanTargetView,
    pub items: Vec<PlanItemView>,
    pub skipped: Vec<PlanSkip>,
    pub total_size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    /// The instance the files went into (a new one for a "New instance" plan).
    pub instance: Instance,
    pub added: Vec<String>,
}

/// The Perland account card: whether the Engine has player accounts on, the ways to sign in it
/// offers (`discord`, `email`), and the signed-in player. Never carries the token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerlandStatus {
    /// False hides the card and everything Perland-account related.
    pub enabled: bool,
    pub methods: Vec<String>,
    pub account: Option<PerlandAccount>,
}

/// The signed-in Perland player, as the launcher shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerlandAccount {
    /// The display name, else the email, else `""` (the UI then says "Perland player").
    pub name: String,
    /// The display name the player picked, never the email; `""` without one (email-only
    /// players). Prefills a free server's name.
    pub display_name: String,
    pub email: Option<String>,
    /// Whether a Discord account is linked.
    pub discord: bool,
    /// The sign-in that made this session created the player.
    pub new_player: bool,
    /// The linked Minecraft account's name, as it was when linked.
    pub minecraft: Option<String>,
}

/// A Minecraft link code, typed in the game as `/perland link <code>`, and its lifetime in
/// seconds.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkCodeView {
    pub code: String,
    pub expires_in: u64,
}

// Manual Debug: the code links an account to whoever types it first.
impl fmt::Debug for LinkCodeView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LinkCodeView")
            .field("expires_in", &self.expires_in)
            .finish_non_exhaustive()
    }
}

/// The signed-in player's totals on the servers whose invite is on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PerlandStats {
    pub servers: Vec<ServerStatsView>,
}

/// A player's totals on one server. `token` is the server's invite credential, which matches it
/// to a card (`PublicServerCard.token`); `instance` is the instance that joins it, if any.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatsView {
    pub token: String,
    pub instance: Option<String>,
    pub name: String,
    pub playtime_seconds: u64,
    pub advancements: u32,
}

// Manual Debug: the token is a credential.
impl fmt::Debug for ServerStatsView {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServerStatsView")
            .field("instance", &self.instance)
            .field("name", &self.name)
            .field("playtime_seconds", &self.playtime_seconds)
            .field("advancements", &self.advancements)
            .finish_non_exhaustive()
    }
}

/// An email code is on its way: seconds until another may be asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmailCodeSent {
    pub resend_in: u64,
}

/// Where a Discord sign-in is: waiting for the browser, or redeeming the code it brought back.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PerlandSignInPhase {
    Waiting,
    Exchanging,
}

#[cfg(test)]
mod tests {
    use super::*;

    use launcher_core::{
        instances::SourceRef,
        sources::{DesiredState, PackAbout},
        sync::{plan::Changes, PendingUpdate, SyncReport},
    };

    fn changes() -> Changes {
        Changes {
            from_version: Some("1.0.3".into()),
            to_version: "1.0.4".into(),
            added: vec!["JEI".into(), "Create".into()],
            updated: vec!["Sodium".into()],
            removed: vec!["Zoomify".into()],
        }
    }

    #[test]
    fn an_update_and_a_summary_keep_names_and_counts() {
        let pending = PendingUpdate {
            source: SourceRef::Packwiz {
                url: "https://example.com/pack.toml".into(),
            },
            desired: DesiredState {
                name: "Sky".into(),
                pack_version: "1.0.4".into(),
                minecraft: "1.20.1".into(),
                loader: None,
                files: vec![],
                stamp: "s2".into(),
                about: PackAbout::default(),
                archive: None,
            },
            changes: changes(),
            kept: vec!["config/a.toml".into()],
        };
        let update = PackUpdate::from(&pending);
        assert_eq!(update.added, vec!["JEI".to_string(), "Create".to_string()]);
        assert_eq!(update.kept, vec!["config/a.toml".to_string()]);
        let json = serde_json::to_value(&update).unwrap();
        assert_eq!(json["fromVersion"], "1.0.3");
        assert_eq!(json["toVersion"], "1.0.4");

        let summary = SyncSummary::from(&SyncReport {
            up_to_date: false,
            changes: changes(),
            kept_modified: vec!["options.txt".into()],
            game_changed: false,
            set_aside: vec!["mods/old.jar".into()],
        });
        assert_eq!((summary.added, summary.updated, summary.removed), (2, 1, 1));
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["keptModified"][0], "options.txt");
        assert_eq!(json["setAside"][0], "mods/old.jar");
    }

    #[test]
    fn pack_add_requests_are_tagged_by_kind() {
        let link: PackAddRequest =
            serde_json::from_str(r#"{"kind":"link","url":"https://x.example/pack.toml"}"#).unwrap();
        assert_eq!(
            link,
            PackAddRequest::Link {
                url: "https://x.example/pack.toml".into()
            }
        );
        let file: PackAddRequest =
            serde_json::from_str(r#"{"kind":"file","path":"C:/Downloads/Sky.mrpack"}"#).unwrap();
        assert_eq!(
            file,
            PackAddRequest::File {
                path: "C:/Downloads/Sky.mrpack".into()
            }
        );
        let downloads: OpenTarget = serde_json::from_str(r#"{"kind":"downloads"}"#).unwrap();
        assert_eq!(downloads, OpenTarget::Downloads);
    }

    #[test]
    fn a_pack_add_request_never_prints_an_invite_token() {
        let token = "Z".repeat(43);
        let invite = PackAddRequest::Link {
            url: format!("https://play.example.com/{token}"),
        };
        let shown = format!("{invite:?}");
        assert!(!shown.contains(&token), "{shown}");
        assert!(
            shown.contains("https://play.example.com/<invite>"),
            "{shown}"
        );

        let packwiz = PackAddRequest::Link {
            url: "https://x.example/pack.toml".into(),
        };
        assert_eq!(
            format!("{packwiz:?}"),
            r#"Link { url: "https://x.example/pack.toml" }"#
        );
        let file = PackAddRequest::File {
            path: "C:/Downloads/Sky.mrpack".into(),
        };
        assert_eq!(
            format!("{file:?}"),
            r#"File { path: "C:/Downloads/Sky.mrpack" }"#
        );
    }

    #[test]
    fn open_target_round_trips_tagged() {
        let t: OpenTarget = serde_json::from_str(r#"{"kind":"mods","id":"pack"}"#).unwrap();
        assert_eq!(t, OpenTarget::Mods { id: "pack".into() });
        let packs: OpenTarget =
            serde_json::from_str(r#"{"kind":"resource_packs","id":"x"}"#).unwrap();
        assert_eq!(packs, OpenTarget::ResourcePacks { id: "x".into() });
        let shaders: OpenTarget =
            serde_json::from_str(r#"{"kind":"shader_packs","id":"x"}"#).unwrap();
        assert_eq!(shaders, OpenTarget::ShaderPacks { id: "x".into() });
        let d: OpenTarget = serde_json::from_str(r#"{"kind":"data"}"#).unwrap();
        assert_eq!(d, OpenTarget::Data);
    }

    #[test]
    fn app_info_is_camel_case() {
        let info = AppInfo {
            version: "0.1.0".into(),
            build: Build::Dev,
            offline_allowed: true,
            system_memory_mb: 16384,
            data_dir: "C:/x".into(),
        };
        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["offlineAllowed"], true);
        assert_eq!(json["build"], "dev");
        assert_eq!(json["systemMemoryMb"], 16384);
    }

    #[test]
    fn browse_requests_read_their_wire_shapes() {
        let search: SearchRequest = serde_json::from_str(
            r#"{"query":"sodium","kind":"resource_pack","instance":null,"minecraft":"1.20.1","loader":"fabric","categories":["adventure"],"sort":"downloads","offset":20}"#,
        )
        .unwrap();
        assert_eq!(search.kind, ContentKind::ResourcePack);
        assert_eq!(search.sort, SortIndex::Downloads);
        let plan: BrowsePlanRequest = serde_json::from_str(
            r#"{"project":"sodium","version":null,"target":{"kind":"instance","id":"asalith"}}"#,
        )
        .unwrap();
        assert_eq!(
            plan.target,
            PlanTargetRequest::Instance {
                id: "asalith".into()
            }
        );
        let new: BrowsePlanRequest =
            serde_json::from_str(r#"{"project":"x","version":"v1","target":{"kind":"new"}}"#)
                .unwrap();
        assert_eq!(new.target, PlanTargetRequest::New);
    }

    #[test]
    fn plan_views_hide_urls_and_hashes() {
        let plan = crate::testing::sample_plan(false);
        let view = PlanItemView::from(&plan.items[1]);
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["reason"]["kind"], "dependency");
        assert_eq!(json["reason"]["of"], "Sodium");
        assert_eq!(json["fileName"], "fabric-api.jar");
        assert!(json.get("url").is_none() && json.get("hashes").is_none());
    }

    #[test]
    fn link_code_and_server_stats_debug_hide_their_secrets() {
        let code = LinkCodeView {
            code: "ABCD-EFGH".into(),
            expires_in: 600,
        };
        let shown = format!("{code:?}");
        assert!(!shown.contains("ABCD") && shown.contains("600"));
        let stats = PerlandStats {
            servers: vec![ServerStatsView {
                token: "t".repeat(43),
                instance: Some("asalith".into()),
                name: "Asalith".into(),
                playtime_seconds: 60,
                advancements: 1,
            }],
        };
        let shown = format!("{stats:?}");
        assert!(!shown.contains(&"t".repeat(43)) && shown.contains("Asalith"));
    }

    fn install(path: &str, major: u32) -> JavaInstall {
        JavaInstall {
            path: path.into(),
            major,
            version: format!("{major}.0.1"),
            vendor: Some("Eclipse Temurin".into()),
        }
    }

    #[test]
    fn javas_that_fit_the_game_come_first() {
        let installs = vec![install("/j21", 21), install("/j17", 17), install("/j8", 8)];
        let overview = JavaOverview::new(Some(17), installs.clone());
        let order: Vec<_> = overview.installs.iter().map(|j| (j.major, j.fit)).collect();
        assert_eq!(
            order,
            vec![
                (17, Some(JavaFit::Matches)),
                (21, Some(JavaFit::Newer)),
                (8, Some(JavaFit::TooOld)),
            ]
        );
        assert_eq!(
            overview.installs[0].vendor.as_deref(),
            Some("Eclipse Temurin")
        );
        // Without knowing what the game needs, nothing is judged and newest stays first.
        let unknown = JavaOverview::new(None, installs);
        assert!(unknown.installs.iter().all(|j| j.fit.is_none()));
        assert_eq!(unknown.installs[0].major, 21);
    }

    #[test]
    fn temurin_is_the_suggested_alternative_among_equals() {
        let zulu = JavaInstall {
            vendor: Some("Azul Zulu".into()),
            ..install("/zulu17", 17)
        };
        let overview = JavaOverview::new(
            Some(17),
            vec![zulu, install("/temurin17", 17), install("/temurin21", 21)],
        );
        let order: Vec<_> = overview
            .installs
            .iter()
            .map(|j| (j.path.to_str().unwrap(), j.recommended))
            .collect();
        // Fit still decides first; Temurin only leads among Javas that fit equally well.
        assert_eq!(
            order,
            vec![
                ("/temurin17", true),
                ("/zulu17", false),
                ("/temurin21", true)
            ]
        );
    }
}
