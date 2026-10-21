//! Share an instance as a `.mrpack` (spec `2026-10-21-share-instance-mrpack-design.md`).
//! Only an allowlist of the game folder is ever read, so logs, screenshots, the server list and
//! everything beside the game folder (`.perland/`, `instance.json`) stay out by construction.
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use crate::{
    browse::api::Api,
    download::hash::sha1_file,
    error::{ErrorCode, LauncherError, Result},
    fsutil::{is_in_use, normalize_rel, path_key},
    instances::LoaderSpec,
    meta::loaders::LoaderKind,
};

const INDEX: &str = "modrinth.index.json";

/// What the player can choose to include.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Category {
    Mods,
    Settings,
    ResourcePacks,
    ShaderPacks,
    Worlds,
}

/// A file the player could share, relative to the game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannedFile {
    pub path: String,
    pub size: u64,
    pub category: Category,
    /// Set for files Modrinth could know: top-level mod jars, resource and shader pack zips.
    pub sha1: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Scan {
    pub files: Vec<ScannedFile>,
    /// Settings files left out because they look like they hold a secret.
    pub skipped_secrets: usize,
    /// Files left out because they vanished or couldn't be read (held by another program).
    pub skipped_unreadable: usize,
}

/// A file that vanished since it was listed, or that another program holds, is left out
/// rather than failing the whole share.
fn skippable(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::NotFound || is_in_use(e)
}

/// Folders the Settings choice covers, whole.
const SETTINGS_DIRS: [&str; 3] = ["config", "defaultconfigs", "kubejs"];
/// Files in the game folder the Settings choice covers.
const SETTINGS_FILES: [&str; 3] = ["options.txt", "optionsof.txt", "optionsshaders.txt"];
/// Settings files up to this size are read for secrets; bigger ones are data, not config.
const SECRET_SCAN_MAX: u64 = 1 << 20;
/// Lines of `options.txt` that belong to this player's screen or history, not to the pack.
const PRIVATE_OPTIONS: [&str; 5] = [
    "lastServer",
    "fullscreen",
    "fullscreenResolution",
    "overrideWidth",
    "overrideHeight",
];
/// Lines of `options.txt` that name resource packs, dropped when those stay out.
const RESOURCE_PACK_OPTIONS: [&str; 2] = ["resourcePacks", "incompatibleResourcePacks"];

pub async fn scan(game_dir: &Path) -> Result<Scan> {
    let game = game_dir.to_path_buf();
    tokio::task::spawn_blocking(move || scan_blocking(&game))
        .await
        .map_err(|e| LauncherError::from(std::io::Error::other(e)))?
}

fn scan_blocking(game: &Path) -> Result<Scan> {
    let mut scan = Scan::default();
    for (rel, meta) in children(&game.join("mods"), "mods")? {
        if meta.is_file() && rel.to_ascii_lowercase().ends_with(".jar") {
            scan.push(game, rel, meta.len(), Category::Mods, true)?;
        }
    }
    for dir in SETTINGS_DIRS {
        walk(game, dir, Category::Settings, &mut scan)?;
    }
    for name in SETTINGS_FILES {
        if let Some(meta) = file_meta(&game.join(name))? {
            scan.push(
                game,
                name.to_string(),
                meta.len(),
                Category::Settings,
                false,
            )?;
        }
    }
    walk(game, "resourcepacks", Category::ResourcePacks, &mut scan)?;
    walk(game, "shaderpacks", Category::ShaderPacks, &mut scan)?;
    walk(game, "saves", Category::Worlds, &mut scan)?;
    // Two names that differ only by case would make the reader refuse the whole pack (it
    // keeps one file per case-insensitive path), so only the first, in sorted order, goes in.
    let mut seen = HashSet::new();
    scan.files.retain(|f| seen.insert(path_key(&f.path)));
    Ok(scan)
}

/// `path`'s metadata when it is a plain file (never a link).
fn file_meta(path: &Path) -> Result<Option<std::fs::Metadata>> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => Ok(Some(meta)),
        Ok(_) => Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The entries of `dir` as paths relative to the game folder, sorted, without links and names
/// that aren't safe pack paths. A missing folder (or a link in its place) has none.
fn children(dir: &Path, rel: &str) -> Result<Vec<(String, std::fs::Metadata)>> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Ok(Vec::new()),
        Err(e) if skippable(&e) => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if skippable(&e) => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
    };
    let mut out = Vec::new();
    for entry in entries {
        let entry = entry?;
        let meta = match entry.metadata() {
            Ok(meta) => meta,
            Err(e) if skippable(&e) => continue,
            Err(e) => return Err(e.into()),
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if let Some(path) = normalize_rel(&format!("{rel}/{name}")) {
            out.push((path, meta));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn walk(game: &Path, rel: &str, category: Category, scan: &mut Scan) -> Result<()> {
    for (path, meta) in children(&game.join(rel), rel)? {
        if meta.is_dir() {
            walk(game, &path, category, scan)?;
        } else if meta.is_file() {
            if category == Category::Worlds && path.ends_with("/session.lock") {
                continue;
            }
            // Top-level pack zips are what Modrinth may know.
            let content = matches!(category, Category::ResourcePacks | Category::ShaderPacks)
                && !rel.contains('/')
                && path.to_ascii_lowercase().ends_with(".zip");
            scan.push(game, path, meta.len(), category, content)?;
        }
    }
    Ok(())
}

impl Scan {
    fn push(
        &mut self,
        game: &Path,
        path: String,
        size: u64,
        category: Category,
        content: bool,
    ) -> Result<()> {
        let full = game.join(&path);
        if category == Category::Settings {
            let name = path.rsplit('/').next().unwrap_or(&path);
            let secret = is_secret_name(name)
                || (size <= SECRET_SCAN_MAX
                    && match std::fs::read(&full) {
                        Ok(bytes) => has_secret_text(&bytes),
                        Err(e) if skippable(&e) => {
                            self.skipped_unreadable += 1;
                            return Ok(());
                        }
                        Err(e) => return Err(e.into()),
                    });
            if secret {
                self.skipped_secrets += 1;
                return Ok(());
            }
        }
        let sha1 = if content {
            match sha1_file(&full) {
                Ok(sha1) => Some(sha1),
                Err(e) if skippable(&e) => {
                    self.skipped_unreadable += 1;
                    return Ok(());
                }
                Err(e) => return Err(e.into()),
            }
        } else {
            None
        };
        self.files.push(ScannedFile {
            path,
            size,
            category,
            sha1,
        });
        Ok(())
    }
}

pub fn is_secret_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    ["token", "secret", "password", "credential", "auth"]
        .iter()
        .any(|w| lower.contains(w))
        || [".key", ".pem", ".p12"].iter().any(|e| lower.ends_with(e))
}

/// A Perland player token (`plt_` and at least 16 token characters) or a Minecraft
/// `accessToken` anywhere in the bytes.
pub fn has_secret_text(bytes: &[u8]) -> bool {
    let token_char = |b: &u8| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-';
    let plt = bytes
        .windows(20)
        .any(|w| w.starts_with(b"plt_") && w[4..].iter().all(token_char));
    plt || bytes.windows(11).any(|w| w == b"accessToken")
}

pub fn clean_options(text: &str, keep_resource_packs: bool) -> String {
    text.split_inclusive('\n')
        .filter(|line| {
            let key = line.split(':').next().unwrap_or("").trim();
            !PRIVATE_OPTIONS.contains(&key)
                && (keep_resource_packs || !RESOURCE_PACK_OPTIONS.contains(&key))
        })
        .collect()
}

/// Hosts Modrinth's format allows downloads from.
pub const ALLOWED_HOSTS: [&str; 4] = [
    "cdn.modrinth.com",
    "github.com",
    "raw.githubusercontent.com",
    "gitlab.com",
];

/// Which sides need a file, as the `.mrpack` `env` says it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Env {
    pub client: String,
    pub server: String,
}

impl Env {
    fn client_only() -> Self {
        Env {
            client: "required".into(),
            server: "unsupported".into(),
        }
    }
}

/// A file Modrinth serves, so the pack links to it instead of carrying it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Linked {
    pub sha1: String,
    pub sha512: String,
    pub size: u64,
    pub url: String,
    pub env: Option<Env>,
}

fn allowed_url(raw: &str) -> bool {
    reqwest::Url::parse(raw).is_ok_and(|url| {
        url.scheme() == "https" && url.host_str().is_some_and(|h| ALLOWED_HOSTS.contains(&h))
    })
}

/// The files Modrinth knows among `sha1s`, keyed by lowercase sha1: one `version_files` call,
/// then one `projects` call for which side each needs. A failed `projects` call only leaves
/// `env` unset; a failed `version_files` call is the error.
pub async fn lookup(api: &Api, sha1s: &[String]) -> Result<HashMap<String, Linked>> {
    let asked: BTreeSet<String> = sha1s.iter().map(|s| s.to_ascii_lowercase()).collect();
    if asked.is_empty() {
        return Ok(HashMap::new());
    }
    let asked: Vec<String> = asked.into_iter().collect();
    let versions = api.version_files(&asked).await?;
    let mut linked = HashMap::new();
    let mut projects: HashMap<String, Vec<String>> = HashMap::new();
    for (hash, version) in versions {
        let hash = hash.to_ascii_lowercase();
        if !asked.contains(&hash) {
            continue;
        }
        let Some(file) = version.files.iter().find(|f| {
            f.hashes
                .sha1
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(&hash))
        }) else {
            continue;
        };
        let Some(sha512) = file.hashes.sha512.clone() else {
            continue;
        };
        if file.size == 0 || !allowed_url(&file.url) {
            continue;
        }
        projects
            .entry(version.project_id.clone())
            .or_default()
            .push(hash.clone());
        linked.insert(
            hash.clone(),
            Linked {
                sha1: hash,
                sha512: sha512.to_ascii_lowercase(),
                size: file.size,
                url: file.url.clone(),
                env: None,
            },
        );
    }
    if projects.is_empty() {
        return Ok(linked);
    }
    let mut ids: Vec<String> = projects.keys().cloned().collect();
    ids.sort();
    match api.projects(&ids).await {
        Ok(found) => {
            for project in found {
                let env = if project.server_side == "unsupported" {
                    Some(Env::client_only())
                } else if project.client_side == "unsupported" {
                    Some(Env {
                        client: "unsupported".into(),
                        server: "required".into(),
                    })
                } else {
                    None
                };
                for hash in projects.get(&project.id).into_iter().flatten() {
                    if let Some(l) = linked.get_mut(hash) {
                        l.env = env.clone();
                    }
                }
            }
        }
        Err(e) => tracing::warn!(code = ?e.code, "share: couldn't read project sides"),
    }
    Ok(linked)
}

/// What the player picked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareChoice {
    pub mods: bool,
    pub settings: bool,
    pub resource_packs: bool,
    pub shader_packs: bool,
    pub worlds: bool,
}

impl ShareChoice {
    pub fn takes(&self, category: Category) -> bool {
        match category {
            Category::Mods => self.mods,
            Category::Settings => self.settings,
            Category::ResourcePacks => self.resource_packs,
            Category::ShaderPacks => self.shader_packs,
            Category::Worlds => self.worlds,
        }
    }
}

/// A scan plus what Modrinth said about it.
#[derive(Debug, Clone, Default)]
pub struct SharePlan {
    pub scan: Scan,
    pub linked: HashMap<String, Linked>,
}

impl SharePlan {
    fn linked_for(&self, file: &ScannedFile) -> Option<&Linked> {
        file.sha1
            .as_ref()
            .and_then(|s| self.linked.get(&s.to_ascii_lowercase()))
    }
}

/// One choice's numbers, for the dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCount {
    pub category: Category,
    /// Files; for worlds, world folders.
    pub files: usize,
    /// Files the pack links to on Modrinth.
    pub linked: usize,
    /// Bytes the pack carries itself.
    pub embedded_bytes: u64,
}

const ORDER: [Category; 5] = [
    Category::Mods,
    Category::Settings,
    Category::ResourcePacks,
    Category::ShaderPacks,
    Category::Worlds,
];

/// The numbers of each category that has anything, in the dialog's order.
pub fn counts(plan: &SharePlan) -> Vec<CategoryCount> {
    ORDER
        .iter()
        .filter_map(|&category| {
            let files: Vec<&ScannedFile> = plan
                .scan
                .files
                .iter()
                .filter(|f| f.category == category)
                .collect();
            if files.is_empty() {
                return None;
            }
            let linked = files
                .iter()
                .filter(|f| plan.linked_for(f).is_some())
                .count();
            let embedded_bytes = files
                .iter()
                .filter(|f| plan.linked_for(f).is_none())
                .map(|f| f.size)
                .sum();
            let count = if category == Category::Worlds {
                // A loose file right under saves/ is no world.
                files
                    .iter()
                    .filter(|f| f.path.split('/').count() > 2)
                    .filter_map(|f| f.path.split('/').nth(1))
                    .collect::<BTreeSet<_>>()
                    .len()
            } else {
                files.len()
            };
            Some(CategoryCount {
                category,
                files: count,
                linked,
                embedded_bytes,
            })
        })
        .collect()
}

/// What the pack says about itself.
#[derive(Debug, Clone, PartialEq)]
pub struct PackMeta {
    pub name: String,
    pub version_id: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
}

/// Size caps for what the pack carries: the reader's (`mrpack::Limits`), so the launcher never
/// writes a pack it can't open.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub entry: u64,
    pub total: u64,
}

impl Default for Limits {
    fn default() -> Self {
        let reader = crate::sources::mrpack::Limits::default();
        Self {
            entry: reader.entry,
            total: reader.total,
        }
    }
}

const SUMMARY: &str = "Shared from Perland Launcher";

fn client_only(file: &ScannedFile) -> bool {
    matches!(
        file.category,
        Category::ResourcePacks | Category::ShaderPacks
    ) || SETTINGS_FILES.contains(&file.path.as_str())
}

fn loader_key(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Forge => "forge",
        LoaderKind::NeoForge => "neoforge",
        LoaderKind::Fabric => "fabric-loader",
        LoaderKind::Quilt => "quilt-loader",
    }
}

fn too_large() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ShareTooLarge,
        "This is too big to share as one file",
    )
    .with_fix("Leave out worlds, or large files")
}

pub async fn write(
    game_dir: &Path,
    plan: &SharePlan,
    choice: ShareChoice,
    meta: PackMeta,
    out: &Path,
) -> Result<()> {
    write_with(game_dir, plan, choice, meta, out, Limits::default()).await
}

/// Write the pack to `out`, through `<out>.part` so a failure never leaves a file under the
/// real name.
pub async fn write_with(
    game_dir: &Path,
    plan: &SharePlan,
    choice: ShareChoice,
    meta: PackMeta,
    out: &Path,
    limits: Limits,
) -> Result<()> {
    let mut index_files = Vec::new();
    let mut carried = Vec::new();
    let mut total = 0u64;
    for file in plan.scan.files.iter().filter(|f| choice.takes(f.category)) {
        if let Some(linked) = plan.linked_for(file) {
            let env = if client_only(file) {
                Some(Env::client_only())
            } else {
                linked.env.clone()
            };
            let mut entry = json!({
                "path": file.path,
                "hashes": { "sha1": linked.sha1, "sha512": linked.sha512 },
                "downloads": [linked.url],
                "fileSize": linked.size,
            });
            if let Some(env) = env {
                entry["env"] = json!(env);
            }
            index_files.push(entry);
            continue;
        }
        total = total.saturating_add(file.size);
        if file.size > limits.entry || total > limits.total {
            return Err(too_large());
        }
        let prefix = if client_only(file) {
            "client-overrides"
        } else {
            "overrides"
        };
        carried.push(Carried {
            rel: file.path.clone(),
            name: format!("{prefix}/{}", file.path),
            settings: file.category == Category::Settings,
            text_only: false,
        });
    }
    write_pack(
        game_dir,
        meta,
        index_files,
        carried,
        choice.resource_packs,
        out,
    )
    .await
}

async fn write_pack(
    game_dir: &Path,
    meta: PackMeta,
    index_files: Vec<Value>,
    carried: Vec<Carried>,
    keep_rp: bool,
    out: &Path,
) -> Result<()> {
    let mut dependencies = BTreeMap::from([("minecraft".to_string(), meta.minecraft.clone())]);
    if let Some(loader) = &meta.loader {
        dependencies.insert(loader_key(loader.kind).into(), loader.version.clone());
    }
    let index = json!({
        "formatVersion": 1,
        "game": "minecraft",
        "versionId": meta.version_id,
        "name": meta.name,
        "summary": SUMMARY,
        "files": index_files,
        "dependencies": dependencies,
    });
    let index = serde_json::to_vec_pretty(&index).map_err(std::io::Error::other)?;
    let (game, out) = (game_dir.to_path_buf(), out.to_path_buf());
    tokio::task::spawn_blocking(move || write_archive(&game, &index, &carried, keep_rp, &out))
        .await
        .map_err(|e| LauncherError::from(std::io::Error::other(e)))?
}

/// How a free server reads the server pack: the Engine's own caps on overrides (one file,
/// all of them), so the launcher never uploads a pack the Engine refuses.
#[derive(Debug, Clone, Copy)]
pub struct ServerLimits {
    pub override_entry: u64,
    pub overrides_total: u64,
}

impl Default for ServerLimits {
    fn default() -> Self {
        Self {
            override_entry: 1 << 20,
            overrides_total: 8 << 20,
        }
    }
}

/// The only host a free server downloads mods from.
const SERVER_HOST: &str = "cdn.modrinth.com";
/// The folders whose files a free server takes as overrides.
const SERVER_SETTINGS_DIRS: [&str; 2] = ["config/", "defaultconfigs/"];
/// The config files a free server takes, by extension.
const SERVER_TEXT_EXTENSIONS: [&str; 11] = [
    "toml",
    "json",
    "json5",
    "properties",
    "cfg",
    "txt",
    "yml",
    "yaml",
    "snbt",
    "conf",
    "ini",
];

/// What a free server made from this instance would hold, before anything is uploaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerPackCheck {
    /// Mods in the instance.
    pub mods: usize,
    /// Mods the server loads (the side Modrinth says is not client only).
    pub server_mods: usize,
    /// The memory those mods need, by the Engine's rule.
    pub required_mb: u32,
    /// File names of mods a free server cannot download: not on Modrinth's CDN.
    pub not_on_modrinth: Vec<String>,
    /// Settings files the server gets.
    pub settings: usize,
    /// Settings files left out: not a config file, or past the size caps.
    pub settings_left_out: usize,
}

/// The Engine's memory rule for a modpack: 2048 MiB plus 24 per server mod, rounded up to 512.
pub fn required_memory_mb(server_mods: usize) -> u32 {
    let mb = 2048u64 + 24 * server_mods as u64;
    (mb.div_ceil(512) * 512).min(u32::MAX as u64) as u32
}

fn on_server_host(raw: &str) -> bool {
    reqwest::Url::parse(raw)
        .is_ok_and(|url| url.scheme() == "https" && url.host_str() == Some(SERVER_HOST))
}

fn server_setting(path: &str) -> bool {
    SERVER_SETTINGS_DIRS
        .iter()
        .any(|dir| path.len() > dir.len() && path.starts_with(dir))
        && Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| SERVER_TEXT_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// The server pack's mods and settings, as `write_server` would write them.
fn server_parts(
    plan: &SharePlan,
    limits: ServerLimits,
) -> (
    ServerPackCheck,
    Vec<(&ScannedFile, &Linked)>,
    Vec<&ScannedFile>,
) {
    let mut check = ServerPackCheck::default();
    let (mut mods, mut settings, mut total) = (Vec::new(), Vec::new(), 0u64);
    for file in &plan.scan.files {
        match file.category {
            Category::Mods => {
                check.mods += 1;
                match plan.linked_for(file).filter(|l| on_server_host(&l.url)) {
                    Some(linked) => {
                        if linked
                            .env
                            .as_ref()
                            .is_none_or(|e| e.server != "unsupported")
                        {
                            check.server_mods += 1;
                        }
                        mods.push((file, linked));
                    }
                    None => check.not_on_modrinth.push(
                        file.path
                            .rsplit('/')
                            .next()
                            .unwrap_or(&file.path)
                            .to_string(),
                    ),
                }
            }
            Category::Settings if server_setting(&file.path) => {
                if file.size > limits.override_entry || total + file.size > limits.overrides_total {
                    check.settings_left_out += 1;
                    continue;
                }
                total += file.size;
                check.settings += 1;
                settings.push(file);
            }
            Category::Settings => check.settings_left_out += 1,
            _ => {}
        }
    }
    check.not_on_modrinth.sort();
    check.required_mb = required_memory_mb(check.server_mods);
    (check, mods, settings)
}

/// What a free server made from this instance would hold. Reads nothing.
pub fn server_check(plan: &SharePlan) -> ServerPackCheck {
    server_parts(plan, ServerLimits::default()).0
}

fn not_on_modrinth(names: &[String]) -> LauncherError {
    LauncherError::pack(
        ErrorCode::FreeServerModsNotOnModrinth,
        "Free servers only run mods from Modrinth",
    )
    .with_detail(names.join(", "))
}

pub async fn write_server(
    game_dir: &Path,
    plan: &SharePlan,
    meta: PackMeta,
    out: &Path,
) -> Result<ServerPackCheck> {
    write_server_with(game_dir, plan, meta, out, ServerLimits::default()).await
}

/// Write the pack a free server is made from: the mods as Modrinth CDN entries and the config
/// files under `overrides/`, nothing else (no worlds, resource or shader packs, scripts or
/// `options.txt`). Refuses, before writing, when a mod is not on Modrinth's CDN.
pub async fn write_server_with(
    game_dir: &Path,
    plan: &SharePlan,
    meta: PackMeta,
    out: &Path,
    limits: ServerLimits,
) -> Result<ServerPackCheck> {
    let (check, mods, settings) = server_parts(plan, limits);
    if !check.not_on_modrinth.is_empty() {
        return Err(not_on_modrinth(&check.not_on_modrinth));
    }
    let index_files = mods
        .iter()
        .map(|(file, linked)| {
            let mut entry = json!({
                "path": file.path,
                "hashes": { "sha1": linked.sha1, "sha512": linked.sha512 },
                "downloads": [linked.url],
                "fileSize": linked.size,
            });
            if let Some(env) = &linked.env {
                entry["env"] = json!(env);
            }
            entry
        })
        .collect();
    let carried = settings
        .iter()
        .map(|file| Carried {
            rel: file.path.clone(),
            name: format!("overrides/{}", file.path),
            settings: true,
            text_only: true,
        })
        .collect();
    write_pack(game_dir, meta, index_files, carried, false, out).await?;
    Ok(check)
}

/// A file the pack carries: where it is, its name in the archive, and whether it is a Settings
/// file (checked for secrets again as it is written: it may have changed since the scan).
struct Carried {
    rel: String,
    name: String,
    settings: bool,
    /// Left out when it is not text (holds a NUL), as a free server refuses it.
    text_only: bool,
}

fn part_path(out: &Path) -> PathBuf {
    let mut name = out.file_name().unwrap_or_default().to_os_string();
    name.push(".part");
    out.with_file_name(name)
}

fn write_archive(
    game: &Path,
    index: &[u8],
    carried: &[Carried],
    keep_resource_packs: bool,
    out: &Path,
) -> Result<()> {
    let part = part_path(out);
    let written = (|| -> Result<()> {
        let mut zip = ZipWriter::new(File::create(&part)?);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        zip.start_file(INDEX, options).map_err(zip_error)?;
        zip.write_all(index)?;
        for file in carried {
            let mut handle = match File::open(game.join(&file.rel)) {
                Ok(handle) => handle,
                // Gone since the scan (the game deleted a temp file): leave it out.
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e.into()),
            };
            if !file.settings {
                zip.start_file(file.name.as_str(), options)
                    .map_err(zip_error)?;
                std::io::copy(&mut handle, &mut zip)?;
                continue;
            }
            let mut head = Vec::new();
            (&mut handle)
                .take(SECRET_SCAN_MAX + 1)
                .read_to_end(&mut head)?;
            if head.len() as u64 <= SECRET_SCAN_MAX && has_secret_text(&head) {
                continue;
            }
            if file.text_only && head.contains(&0) {
                continue;
            }
            zip.start_file(file.name.as_str(), options)
                .map_err(zip_error)?;
            if file.rel == SETTINGS_FILES[0] {
                let text = String::from_utf8_lossy(&head);
                zip.write_all(clean_options(&text, keep_resource_packs).as_bytes())?;
            } else {
                zip.write_all(&head)?;
                std::io::copy(&mut handle, &mut zip)?;
            }
        }
        zip.finish().map_err(zip_error)?.sync_all()?;
        std::fs::rename(&part, out)?;
        Ok(())
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    written
}

fn zip_error(e: zip::result::ZipError) -> LauncherError {
    match e {
        zip::result::ZipError::Io(e) => e.into(),
        other => std::io::Error::other(other).into(),
    }
}

/// `<name>.mrpack`, with what Windows refuses in a file name replaced by `-`.
pub fn file_name_for(name: &str) -> String {
    let clean: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '-'
            } else {
                c
            }
        })
        .collect();
    let clean = clean.trim_matches(['.', ' ']);
    let stem = if clean.is_empty() { "pack" } else { clean };
    format!("{stem}.mrpack")
}

/// Today's date in UTC, `YYYY-MM-DD`: the shared pack's version.
pub fn today_utc() -> String {
    date_of(crate::instances::unix_now())
}

/// The UTC date of a unix time (Howard Hinnant's `civil_from_days`).
fn date_of(unix: i64) -> String {
    let z = unix.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_memory_follows_the_engine() {
        for (mods, mb) in [(0, 2048), (1, 2560), (21, 2560), (22, 3072), (220, 7680)] {
            assert_eq!(required_memory_mb(mods), mb, "{mods} mods");
        }
    }

    #[test]
    fn server_settings_are_config_text_only() {
        for (path, takes) in [
            ("config/a.toml", true),
            ("config/sub/b.JSON5", true),
            ("defaultconfigs/c.snbt", true),
            ("config/", false),
            ("config/a.bin", false),
            ("config/a.jar", false),
            ("kubejs/a.json", false),
            ("options.txt", false),
        ] {
            assert_eq!(server_setting(path), takes, "{path}");
        }
    }

    fn put(root: &Path, rel: &str, body: &[u8]) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn found(scan: &Scan) -> Vec<(&str, Category, bool)> {
        scan.files
            .iter()
            .map(|f| (f.path.as_str(), f.category, f.sha1.is_some()))
            .collect()
    }

    #[tokio::test]
    async fn scan_uses_the_allowlist() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path();
        for rel in [
            "logs/latest.log",
            "crash-reports/crash.txt",
            "screenshots/a.png",
            "servers.dat",
            "usercache.json",
            "mods/off.jar.disabled",
            "mods/notes.txt",
            "saves/World/session.lock",
            "launcher_profiles.json",
        ] {
            put(game, rel, b"x");
        }
        for rel in [
            "mods/a.jar",
            "config/a.toml",
            "config/sub/b.json",
            "defaultconfigs/c.toml",
            "kubejs/startup_scripts/s.js",
            "options.txt",
            "optionsof.txt",
            "resourcepacks/r.zip",
            "resourcepacks/Folder Pack/pack.mcmeta",
            "shaderpacks/s.zip",
            "shaderpacks/s.zip.txt",
            "saves/World/level.dat",
        ] {
            put(game, rel, b"y");
        }
        let scan = scan(game).await.unwrap();
        assert_eq!(
            found(&scan),
            vec![
                ("mods/a.jar", Category::Mods, true),
                ("config/a.toml", Category::Settings, false),
                ("config/sub/b.json", Category::Settings, false),
                ("defaultconfigs/c.toml", Category::Settings, false),
                ("kubejs/startup_scripts/s.js", Category::Settings, false),
                ("options.txt", Category::Settings, false),
                ("optionsof.txt", Category::Settings, false),
                (
                    "resourcepacks/Folder Pack/pack.mcmeta",
                    Category::ResourcePacks,
                    false
                ),
                ("resourcepacks/r.zip", Category::ResourcePacks, true),
                ("shaderpacks/s.zip", Category::ShaderPacks, true),
                ("shaderpacks/s.zip.txt", Category::ShaderPacks, false),
                ("saves/World/level.dat", Category::Worlds, false),
            ]
        );
        assert_eq!(scan.files[0].size, 1);
        assert_eq!(
            scan.files[0].sha1.as_deref(),
            Some(crate::download::hash::sha1_hex(b"y").as_str())
        );
    }

    #[tokio::test]
    async fn skips_secret_files() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path();
        put(game, "config/discord_Token.json", b"{}");
        put(game, "config/x.pem", b"key");
        put(
            game,
            "config/a.toml",
            b"key = \"plt_0123456789abcdef0123\"\n",
        );
        put(game, "config/b.json", b"{\"accessToken\": \"x\"}");
        put(game, "config/ok.toml", b"plt_short = 1\n");
        let scan = scan(game).await.unwrap();
        assert_eq!(
            found(&scan),
            vec![("config/ok.toml", Category::Settings, false)]
        );
        assert_eq!(scan.skipped_secrets, 4);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn skips_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        put(outside.path(), "secret.txt", b"s");
        let game = tmp.path();
        put(game, "saves/W/level.dat", b"w");
        std::os::unix::fs::symlink(outside.path(), game.join("saves/W/link")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret.txt"), game.join("options.txt"))
            .unwrap();
        std::os::unix::fs::symlink(outside.path(), game.join("config")).unwrap();
        let scan = scan(game).await.unwrap();
        assert_eq!(
            found(&scan),
            vec![("saves/W/level.dat", Category::Worlds, false)]
        );
    }

    #[test]
    fn secret_names_and_text() {
        assert!(is_secret_name("MyAuth.json"));
        assert!(is_secret_name("api_secret.txt"));
        assert!(is_secret_name("cert.P12"));
        assert!(!is_secret_name("sodium-options.json"));
        assert!(has_secret_text(b"t=plt_abcdefghijklmnop"));
        assert!(!has_secret_text(b"t=plt_abcdefghijklmno"));
        assert!(has_secret_text(b"\"accessToken\":1"));
        assert!(!has_secret_text(b"nothing here"));
    }

    #[test]
    fn clean_options_drops_private_lines() {
        let text = "version:3465\r\nlastServer:play.example.com\r\nfullscreen:true\r\n\
                    fullscreenResolution:1920x1080\r\noverrideWidth:0\r\noverrideHeight:0\r\n\
                    resourcePacks:[\"file/r.zip\"]\r\nincompatibleResourcePacks:[]\r\nkey_key.jump:key.keyboard.space\r\n";
        assert_eq!(
            clean_options(text, true),
            "version:3465\r\nresourcePacks:[\"file/r.zip\"]\r\nincompatibleResourcePacks:[]\r\nkey_key.jump:key.keyboard.space\r\n"
        );
        assert_eq!(
            clean_options(text, false),
            "version:3465\r\nkey_key.jump:key.keyboard.space\r\n"
        );
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(file_name_for("Sky Friends"), "Sky Friends.mrpack");
        assert_eq!(file_name_for("a/b:c?"), "a-b-c-.mrpack");
        assert_eq!(file_name_for("  ..  "), "pack.mrpack");
        assert_eq!(file_name_for("Mundo ñandú"), "Mundo ñandú.mrpack");
    }

    #[test]
    fn dates_are_utc_days() {
        assert_eq!(date_of(0), "1970-01-01");
        assert_eq!(date_of(951_782_400), "2000-02-29");
        assert_eq!(date_of(1_792_540_799), "2026-10-20");
        assert_eq!(date_of(1_792_540_800), "2026-10-21");
        assert_eq!(today_utc().len(), 10);
    }

    #[test]
    fn loader_keys_match_the_reader() {
        for kind in [
            LoaderKind::Forge,
            LoaderKind::NeoForge,
            LoaderKind::Fabric,
            LoaderKind::Quilt,
        ] {
            let versions = BTreeMap::from([
                ("minecraft".to_string(), "1.20.1".to_string()),
                (loader_key(kind).to_string(), "1".to_string()),
            ]);
            let (_, loader) = crate::sources::game_from(&versions).unwrap();
            assert_eq!(loader.unwrap().kind, kind);
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn names_differing_by_case_keep_the_first() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path();
        put(game, "config/Foo.toml", b"a");
        put(game, "config/foo.toml", b"b");
        put(game, "saves/W/level.dat", b"w");
        put(game, "saves/w/level.dat", b"w");
        let scan = scan(game).await.unwrap();
        assert_eq!(
            found(&scan),
            vec![
                ("config/Foo.toml", Category::Settings, false),
                ("saves/W/level.dat", Category::Worlds, false),
            ]
        );
    }

    #[test]
    fn missing_or_held_files_are_skippable() {
        assert!(skippable(&std::io::Error::from(
            std::io::ErrorKind::NotFound
        )));
        assert!(skippable(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
        assert!(!skippable(&std::io::Error::other("disk on fire")));
    }

    #[test]
    fn a_loose_file_under_saves_is_no_world() {
        let file = |path: &str| ScannedFile {
            path: path.into(),
            size: 1,
            category: Category::Worlds,
            sha1: None,
        };
        let plan = SharePlan {
            scan: Scan {
                files: vec![file("saves/.DS_Store"), file("saves/W/level.dat")],
                ..Scan::default()
            },
            linked: HashMap::new(),
        };
        assert_eq!(counts(&plan)[0].files, 1);
    }
}
