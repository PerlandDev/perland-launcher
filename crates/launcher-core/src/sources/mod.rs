//! Where packs come from. Every source (packwiz, `.mrpack`, Modrinth) reduces to a
//! [`DesiredState`]; `sync` never knows which one it was (core spec §2.1).
pub mod art;
pub mod curseforge;
pub mod modrinth;
pub mod mrpack;
pub mod packwiz;
pub mod perland;
pub mod sidecar;

use std::{collections::BTreeMap, fmt::Display, path::PathBuf};

use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use serde::{Deserialize, Serialize};

use crate::{
    download::{cache::ObjectCache, host_of, Downloader, Hashes},
    error::{ErrorCode, LauncherError, Result},
    events::{noop_sink, EventSink},
    fsutil::path_key,
    instances::{LoaderSpec, SourceRef, UpdatePolicy},
    invite::{InviteClient, InviteLink},
    meta::{loaders::LoaderKind, MetaClient},
    sync::Cancel,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Client,
    Server,
    Both,
}

impl Side {
    /// Only client and both-side files are installed (spec §3.2).
    pub fn on_client(self) -> bool {
        self != Side::Server
    }
}

/// What sync may do with a file that differs on disk (spec §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileMode {
    /// The pack wins.
    Replace,
    /// packwiz `preserve`: written only when the file is absent.
    Preserve,
    /// `.mrpack` overrides outside `mods/`: replaced only while the player hasn't changed it.
    ReplaceUnlessModified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileSource {
    Remote {
        urls: Vec<String>,
    },
    /// An entry inside the pack's cached `.mrpack` (see [`DesiredState::archive`]).
    Archive {
        entry: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionalFile {
    pub default: bool,
    pub description: Option<String>,
}

/// Where a player can download a file by hand when its author blocks other launchers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualDownload {
    pub page_url: String,
    pub file_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    /// Canonical relative path inside the game folder (`fsutil::normalize_rel`).
    pub path: String,
    /// What changelogs call it: the mod's name, or the file name.
    pub name: String,
    pub source: FileSource,
    /// Never empty: every source refuses files without a usable checksum.
    pub hashes: Hashes,
    pub size: Option<u64>,
    pub side: Side,
    pub optional: Option<OptionalFile>,
    pub mode: FileMode,
    pub manual: Option<ManualDownload>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackArt {
    pub icon: Option<String>,
    pub banner: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackAbout {
    pub description: Option<String>,
    pub art: PackArt,
    /// The update policy a pack asks for (`launcher.toml` `[sync] policy`); only the default.
    pub policy: Option<UpdatePolicy>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesiredState {
    pub name: String,
    pub pack_version: String,
    pub minecraft: String,
    pub loader: Option<LoaderSpec>,
    pub files: Vec<PackFile>,
    /// What the cheap update check compares: packwiz `pack.toml`'s sha256, a Modrinth version
    /// id, or a local `.mrpack`'s sha1.
    pub stamp: String,
    pub about: PackAbout,
    /// The `.mrpack` that `Archive` entries come from, by its hashes in the object cache.
    pub archive: Option<Hashes>,
}

impl DesiredState {
    /// Refuse packs where two files would land on one path on a case-insensitive disk.
    pub fn check_paths(&self) -> Result<()> {
        let mut seen = std::collections::HashSet::new();
        for f in &self.files {
            if crate::fsutil::normalize_rel(&f.path).as_deref() != Some(f.path.as_str())
                || !seen.insert(path_key(&f.path))
            {
                return Err(unsafe_path(&f.path));
            }
        }
        Ok(())
    }
}

/// The result of the cheap update check (spec §3.1).
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateCheck {
    UpToDate,
    /// A newer version. `source` is what to resolve; for Modrinth it names the new version.
    Available {
        source: SourceRef,
        stamp: String,
        version: String,
    },
    /// The check itself failed (offline, server trouble): play what's installed.
    Unavailable(LauncherError),
}

/// What a player hands the launcher to make a pack instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackInput {
    Packwiz {
        url: String,
    },
    MrpackFile {
        path: PathBuf,
    },
    Modrinth {
        project: String,
        version: Option<String>,
    },
    /// A Perland server's invite link.
    Perland {
        link: InviteLink,
    },
}

fn link_invalid() -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackLinkInvalid,
        "That doesn't look like a pack link",
    )
    .with_fix("Pack links start with https:// and usually end in pack.toml")
}

impl PackInput {
    /// `https://…/pack.toml`, `modrinth:<slug or id>[@<version id>]`, a Perland invite link, or a
    /// path ending in `.mrpack`.
    pub fn parse(input: &str) -> Result<PackInput> {
        let s = input.trim();
        if let Some(rest) = s.strip_prefix("modrinth:") {
            let (project, version) = match rest.split_once('@') {
                Some((p, v)) => (p, Some(v)),
                None => (rest, None),
            };
            if !valid_modrinth_id(project) || version.is_some_and(|v| !valid_modrinth_id(v)) {
                return Err(link_invalid());
            }
            return Ok(PackInput::Modrinth {
                project: project.to_string(),
                version: version.map(str::to_string),
            });
        }
        // Before the packwiz rule: an invite is also an https URL, but never ends in pack.toml.
        if let Ok(link) = InviteLink::parse(s) {
            return Ok(PackInput::Perland { link });
        }
        if s.starts_with("http://") || s.starts_with("https://") {
            check_pack_url(s)?;
            return Ok(PackInput::Packwiz { url: s.to_string() });
        }
        if s.to_ascii_lowercase().ends_with(".mrpack") {
            return Ok(PackInput::MrpackFile {
                path: PathBuf::from(s),
            });
        }
        Err(link_invalid())
    }
}

/// A pack link the launcher will fetch: `https://`, or `http://` on this computer (tests and
/// local pack development). Mods are code, so a pack is never fetched over plain HTTP from
/// the network.
pub fn check_pack_url(raw: &str) -> Result<reqwest::Url> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| link_invalid())?;
    match url.scheme() {
        "https" => Ok(url),
        "http" if crate::http::is_loopback(&url) => Ok(url),
        _ => Err(link_invalid()),
    }
}

/// Modrinth slugs and ids go into API paths, so only their documented characters pass.
pub fn valid_modrinth_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Upstream addresses, overridable for tests.
#[derive(Debug, Clone)]
pub struct SourceConfig {
    pub modrinth_api: String,
    pub curseforge_cdn: String,
    pub curseforge_site: String,
}

impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            modrinth_api: "https://api.modrinth.com/v2".into(),
            curseforge_cdn: "https://mediafilez.forgecdn.net".into(),
            curseforge_site: "https://www.curseforge.com".into(),
        }
    }
}

/// Everything a source needs to fetch a pack.
#[derive(Clone)]
pub struct Sources {
    /// Server invites: one per process, so every caller shares its 15 s view cache.
    pub invites: InviteClient,
    pub http: reqwest::Client,
    pub meta: MetaClient,
    pub downloader: Downloader,
    pub cache: ObjectCache,
    pub config: SourceConfig,
}

/// A `MetaClient` error from fetching the pack file at `url` (`file` is its name or path in the
/// pack), in pack terms: `MetaClient`'s own codes talk about version lists.
/// - `MetaNotFound` is any 4xx. Only 404 and 410 mean the pack is gone; a rate limit or a
///   refusal is the host having trouble, and must not say the pack doesn't exist.
/// - A missing saved copy is the host's trouble too, and stays a network failure, so an update
///   that can't be fetched still falls back to what is installed.
/// - Data that fails its checksum or can't be taken means the pack file is damaged.
pub(crate) fn pack_fetch_error(url: &str, file: &str) -> impl Fn(LauncherError) -> LauncherError {
    let (host, file) = (host_of(url), file.to_string());
    move |e| {
        let damaged = invalid(&file, "");
        fetch_error(e, &host, damaged)
    }
}

/// [`pack_fetch_error`] for Modrinth's API, whose answers aren't files in the pack.
pub(crate) fn modrinth_fetch_error(url: &str) -> impl Fn(LauncherError) -> LauncherError {
    let host = host_of(url);
    move |e| {
        let damaged = invalid_because(
            "modrinth_data",
            "Modrinth sent pack details the launcher couldn't read",
            "",
        )
        .with_fix("Try again in a few minutes");
        fetch_error(e, &host, damaged)
    }
}

fn fetch_error(e: LauncherError, host: &str, damaged: LauncherError) -> LauncherError {
    let detail = e.detail.clone().or_else(|| Some(e.message.clone()));
    let mut out = match e.code {
        ErrorCode::MetaNotFound => {
            let status = e.params.get("status").cloned().unwrap_or_default();
            if status == "404" || status == "410" {
                LauncherError::pack(ErrorCode::PackNotFound, "That pack wasn't found")
                    .with_fix("Check the link and try again")
            } else {
                host_trouble(host, &format!("{host} is having trouble ({status})"))
                    .with_param("status", &status)
            }
        }
        ErrorCode::MetaCacheMissing => host_trouble(host, &e.message),
        ErrorCode::MetaChecksum | ErrorCode::MetaUnexpected => damaged,
        _ => return e,
    };
    out.detail = detail;
    out
}

fn host_trouble(host: &str, message: &str) -> LauncherError {
    LauncherError::network(ErrorCode::HostTrouble, message)
        .with_param("host", host)
        .with_fix("Try again in a few minutes")
}

pub(crate) fn no_pack() -> LauncherError {
    LauncherError::pack(ErrorCode::NoPack, "This instance doesn't come from a pack")
}

impl Sources {
    /// Turn what the player handed over into a source and the pack it currently describes.
    pub async fn open(&self, input: &PackInput) -> Result<(SourceRef, DesiredState)> {
        match input {
            PackInput::Packwiz { url } => Ok((
                SourceRef::Packwiz { url: url.clone() },
                packwiz::resolve(self, url).await?,
            )),
            PackInput::MrpackFile { path } => mrpack::import_file(self, path).await,
            PackInput::Modrinth { project, version } => {
                modrinth::open(self, project, version.as_deref()).await
            }
            PackInput::Perland { link } => perland::open(self, link).await,
        }
    }

    /// Exactly what `source` names (for Modrinth, that version).
    pub async fn resolve(&self, source: &SourceRef) -> Result<DesiredState> {
        match source {
            SourceRef::None => Err(no_pack()),
            SourceRef::Packwiz { url } => packwiz::resolve(self, url).await,
            SourceRef::Mrpack { sha1, .. } => mrpack::resolve_cached(self, sha1).await,
            SourceRef::Modrinth {
                project_id,
                version_id,
            } => modrinth::resolve(self, project_id, version_id).await,
            SourceRef::Perland { invite, engine } => {
                perland::resolve_ref(self, invite, engine, &noop_sink(), &Cancel::default()).await
            }
        }
    }

    /// `resolve`, telling `sink` about long waits (a server pack the Engine is still building).
    /// `cancel` ends such a wait (`sync_cancelled`); other sources finish what they started.
    pub async fn resolve_with(
        &self,
        source: &SourceRef,
        sink: &EventSink,
        cancel: &Cancel,
    ) -> Result<DesiredState> {
        match source {
            SourceRef::Perland { invite, engine } => {
                perland::resolve_ref(self, invite, engine, sink, cancel).await
            }
            other => self.resolve(other).await,
        }
    }

    /// The cheap update check against what is installed (`installed.stamp` is the installed
    /// Modrinth version id), never against `source`, which may already name a newer version
    /// whose sync didn't finish. A local `.mrpack` never updates; importing a newer file does.
    pub async fn check(&self, source: &SourceRef, installed: &DesiredState) -> UpdateCheck {
        match source {
            SourceRef::None | SourceRef::Mrpack { .. } => UpdateCheck::UpToDate,
            SourceRef::Packwiz { url } => packwiz::check(self, url, &installed.stamp).await,
            SourceRef::Modrinth { project_id, .. } => {
                modrinth::check(self, project_id, &installed.stamp, &installed.minecraft).await
            }
            SourceRef::Perland { invite, engine } => {
                perland::check(self, invite, engine, installed).await
            }
        }
    }
}

/// Minecraft and the loader from a pack's version table (packwiz `[versions]`, `.mrpack`
/// `dependencies`). A `<minecraft>-` prefix on the loader version is dropped.
pub fn game_from(versions: &BTreeMap<String, String>) -> Result<(String, Option<LoaderSpec>)> {
    let minecraft = versions
        .get("minecraft")
        .ok_or_else(|| invalid("versions", "no minecraft version"))?
        .clone();
    let mut loader = None;
    for (key, version) in versions {
        let kind = match key.as_str() {
            "minecraft" => continue,
            "forge" => LoaderKind::Forge,
            "neoforge" => LoaderKind::NeoForge,
            "fabric" | "fabric-loader" => LoaderKind::Fabric,
            "quilt" | "quilt-loader" => LoaderKind::Quilt,
            other => return Err(unsupported(&format!("the {other} loader"))),
        };
        if loader.is_some() {
            return Err(unsupported("more than one loader"));
        }
        let version = version
            .strip_prefix(&format!("{minecraft}-"))
            .unwrap_or(version)
            .to_string();
        loader = Some(LoaderSpec { kind, version });
    }
    Ok((minecraft, loader))
}

pub(crate) fn unsupported(what: &str) -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackUnsupported,
        format!("This pack uses something the launcher can't handle yet: {what}"),
    )
    .with_param("what", what)
    .with_fix("Ask the pack's author, or try another pack")
}

pub(crate) fn invalid(file: &str, detail: impl Display) -> LauncherError {
    LauncherError::pack(
        ErrorCode::PackInvalid,
        format!("The pack file {file} is damaged or isn't a pack"),
    )
    .with_param("file", file)
    .with_fix("Check the link, or ask the pack's author")
    .with_detail(detail)
}

/// [`invalid`] when no single pack file is to blame: `context` (an i18next context) picks the
/// UI's sentence for what is wrong, and `message` is the English fallback.
pub(crate) fn invalid_because(context: &str, message: &str, detail: impl Display) -> LauncherError {
    LauncherError::pack(ErrorCode::PackInvalid, message)
        .with_param("context", context)
        .with_fix("Check the link, or ask the pack's author")
        .with_detail(detail)
}

pub(crate) fn unsafe_path(path: &str) -> LauncherError {
    let shown: String = path.chars().take(120).collect();
    LauncherError::pack(
        ErrorCode::PackUnsafePath,
        format!("The pack tried to write outside its folder ({shown})"),
    )
    .with_param("path", shown)
    .with_fix("Don't use this pack, and let its author know")
}

/// Characters escaped in each path segment before it is joined onto a URL.
const SEGMENT: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}');

/// A canonical relative path, percent-encoded segment by segment for `Url::join`.
pub(crate) fn encode_path(rel: &str) -> String {
    rel.split('/')
        .map(|s| utf8_percent_encode(s, SEGMENT).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_input_requires_https_except_loopback() {
        assert_eq!(
            PackInput::parse(" https://perlanddev.github.io/asalith-modpack/pack.toml ").unwrap(),
            PackInput::Packwiz {
                url: "https://perlanddev.github.io/asalith-modpack/pack.toml".into()
            }
        );
        for ok in [
            "http://127.0.0.1:8080/pack.toml",
            "http://localhost/pack.toml",
            "http://[::1]:9/pack.toml",
        ] {
            assert!(
                matches!(PackInput::parse(ok), Ok(PackInput::Packwiz { .. })),
                "{ok}"
            );
        }
        for bad in [
            "http://example.com/pack.toml",
            "ftp://example.com/pack.toml",
            "file:///C:/pack.toml",
            "javascript:alert(1)",
            "",
            "pack.toml",
            "modrinth:../x",
            "modrinth:",
        ] {
            assert_eq!(
                PackInput::parse(bad).unwrap_err().code,
                ErrorCode::PackLinkInvalid,
                "{bad:?}"
            );
        }
    }

    #[test]
    fn pack_input_reads_modrinth_and_mrpack() {
        assert_eq!(
            PackInput::parse("modrinth:fabulously-optimized").unwrap(),
            PackInput::Modrinth {
                project: "fabulously-optimized".into(),
                version: None
            }
        );
        assert_eq!(
            PackInput::parse("modrinth:1KVo5zza@2ZbcYfCj").unwrap(),
            PackInput::Modrinth {
                project: "1KVo5zza".into(),
                version: Some("2ZbcYfCj".into())
            }
        );
        assert_eq!(
            PackInput::parse("C:\\Downloads\\Pack 1.0.MRPACK").unwrap(),
            PackInput::MrpackFile {
                path: "C:\\Downloads\\Pack 1.0.MRPACK".into()
            }
        );
    }

    #[test]
    fn pack_input_reads_invite_links_before_packwiz() {
        let token = "A".repeat(43);
        let input = PackInput::parse(&format!(" https://play.example.com/{token}/ ")).unwrap();
        let PackInput::Perland { link } = &input else {
            panic!("{input:?}");
        };
        assert_eq!(
            link.canonical(),
            format!("https://play.example.com/{token}")
        );
        assert!(!format!("{input:?}").contains(&token));
        assert!(matches!(
            PackInput::parse("http://127.0.0.1:8080/pack.toml"),
            Ok(PackInput::Packwiz { .. })
        ));
    }

    #[test]
    fn pack_fetch_errors_become_pack_codes() {
        let url = "https://packs.example.com/a/index.toml";
        let map = pack_fetch_error(url, "index.toml");
        let param = |e: &LauncherError, k: &str| e.params.get(k).cloned();

        let missing = map(LauncherError::network(ErrorCode::MetaCacheMissing, "x"));
        assert_eq!(missing.code, ErrorCode::HostTrouble);
        // Still a network failure, so a pack update that can't download falls back.
        assert_eq!(missing.category, crate::error::ErrorCategory::Network);
        assert_eq!(
            param(&missing, "host").as_deref(),
            Some("packs.example.com")
        );

        for code in [ErrorCode::MetaChecksum, ErrorCode::MetaUnexpected] {
            let bad = map(LauncherError::integrity(code, "x").with_detail("why"));
            assert_eq!(bad.code, ErrorCode::PackInvalid, "{code:?}");
            assert_eq!(param(&bad, "file").as_deref(), Some("index.toml"));
            assert_eq!(bad.detail.as_deref(), Some("why"));
        }

        let gone =
            map(LauncherError::loader(ErrorCode::MetaNotFound, "x").with_param("status", 410));
        assert_eq!(gone.code, ErrorCode::PackNotFound);
        let refused =
            map(LauncherError::loader(ErrorCode::MetaNotFound, "x").with_param("status", 429));
        assert_eq!(refused.code, ErrorCode::HostTrouble);
        assert_eq!(
            param(&refused, "host").as_deref(),
            Some("packs.example.com")
        );
        assert_eq!(param(&refused, "status").as_deref(), Some("429"));

        let offline = map(LauncherError::network(ErrorCode::Offline, "x"));
        assert_eq!(
            offline.code,
            ErrorCode::Offline,
            "everything else passes through"
        );

        let modrinth = modrinth_fetch_error("https://api.modrinth.com/v2/project/x")(
            LauncherError::loader(ErrorCode::MetaUnexpected, "x"),
        );
        assert_eq!(modrinth.code, ErrorCode::PackInvalid);
        assert_eq!(
            param(&modrinth, "context").as_deref(),
            Some("modrinth_data")
        );
        assert_eq!(param(&modrinth, "file"), None);
    }

    fn versions(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn game_from_reads_loaders() {
        let (mc, loader) =
            game_from(&versions(&[("minecraft", "1.20.1"), ("forge", "47.4.22")])).unwrap();
        assert_eq!(mc, "1.20.1");
        assert_eq!(
            loader,
            Some(LoaderSpec {
                kind: LoaderKind::Forge,
                version: "47.4.22".into()
            })
        );
        let (_, prefixed) = game_from(&versions(&[
            ("minecraft", "1.20.1"),
            ("forge", "1.20.1-47.4.22"),
        ]))
        .unwrap();
        assert_eq!(prefixed.unwrap().version, "47.4.22");
        let (_, fabric) = game_from(&versions(&[
            ("minecraft", "1.20.1"),
            ("fabric-loader", "0.16.14"),
        ]))
        .unwrap();
        assert_eq!(fabric.unwrap().kind, LoaderKind::Fabric);
        assert_eq!(
            game_from(&versions(&[("minecraft", "1.20.1")])).unwrap().1,
            None
        );
        let lite = game_from(&versions(&[("minecraft", "1.12.2"), ("liteloader", "1")]));
        assert_eq!(lite.unwrap_err().code, ErrorCode::PackUnsupported);
        let two = game_from(&versions(&[
            ("minecraft", "1.20.1"),
            ("forge", "1"),
            ("fabric", "2"),
        ]));
        assert_eq!(two.unwrap_err().code, ErrorCode::PackUnsupported);
        assert_eq!(
            game_from(&versions(&[])).unwrap_err().code,
            ErrorCode::PackInvalid
        );
    }

    fn file(path: &str) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.into(),
            source: FileSource::Remote { urls: vec![] },
            hashes: Hashes::sha1("aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
            size: None,
            side: Side::Both,
            optional: None,
            mode: FileMode::Replace,
            manual: None,
        }
    }

    fn desired(files: Vec<PackFile>) -> DesiredState {
        DesiredState {
            name: "P".into(),
            pack_version: "1".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files,
            stamp: "s".into(),
            about: PackAbout::default(),
            archive: None,
        }
    }

    #[test]
    fn check_paths_refuses_case_collisions() {
        desired(vec![file("mods/a.jar"), file("mods/b.jar")])
            .check_paths()
            .unwrap();
        for bad in ["mods/../x.jar", r"mods\a.jar"] {
            assert_eq!(
                desired(vec![file(bad)]).check_paths().unwrap_err().code,
                ErrorCode::PackUnsafePath,
                "{bad}"
            );
        }
        let clash = desired(vec![file("mods/a.jar"), file("MODS/A.jar")]);
        assert_eq!(
            clash.check_paths().unwrap_err().code,
            ErrorCode::PackUnsafePath
        );
    }

    #[test]
    fn encode_path_escapes_url_syntax_per_segment() {
        assert_eq!(
            encode_path("config/a b#1?.toml"),
            "config/a%20b%231%3F.toml"
        );
    }
}
