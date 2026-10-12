//! Perland server invites as a pack source (join-by-link spec §2.1).
//! - A modded stack resolves to the invite's `.mrpack`, built from the commit the server runs.
//! - A stack without a modpack resolves to vanilla at the server's Minecraft version.
//! - Networks, and modded stacks without a modpack, are refused (`invite_unsupported`).
//!
//! Every error names the host only: the token is part of every invite URL.
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
};

use sha1::Digest;

use super::{game_from, invalid, mrpack, DesiredState, PackAbout, PackArt, Sources, UpdateCheck};
use crate::{
    download::{hash::sha1_hex, DownloadRequest, Hashes},
    error::{ErrorCode, LauncherError, Result},
    events::{Event, EventSink, TaskId},
    instances::{LoaderSpec, ServerAddress, SourceRef, UpdatePolicy, MAX_NAME_CHARS},
    invite::{
        client::{
            gone, host_of, host_trouble, offline, rate_limited, request_failed, status_error,
        },
        invite_route, non_empty, EngineView, InviteLink, InviteView, Url, PACK_READY,
        PACK_UNAVAILABLE,
    },
    sync::{sync_cancelled, Cancel},
};

/// The stamp of a stack without a modpack: `vanilla:<minecraft version>`.
pub const VANILLA_PREFIX: &str = "vanilla:";

fn game_version(view: &InviteView) -> Option<&str> {
    non_empty(&view.game.version)
}

/// What the update check compares: the pinned commit, or `vanilla:<version>`. `None` for a
/// modded stack on an Engine that doesn't send the commit yet.
pub fn stamp_of(view: &InviteView) -> Option<String> {
    match &view.pack {
        Some(pack) => non_empty(&pack.commit).map(str::to_string),
        None => game_version(view).map(|v| format!("{VANILLA_PREFIX}{v}")),
    }
}

/// The version players see: the pack's, else the server's Minecraft version.
pub fn version_of(view: &InviteView) -> String {
    view.pack
        .as_ref()
        .and_then(|p| non_empty(&p.version))
        .or_else(|| game_version(view))
        .unwrap_or_default()
        .to_string()
}

/// Refuse what the launcher can't join yet: networks; modded stacks without a modpack (their
/// mods can't be installed, and vanilla wouldn't get in); and stacks with neither a modpack nor
/// a Minecraft version. The error carries the address so the player can still join by hand.
/// The address may be the view's own display text, so the error is scrubbed of `link`'s token.
pub fn supported(link: &InviteLink, ev: &EngineView) -> Result<()> {
    refused(ev).map_err(|e| scrub(e, link.token()))
}

fn refused(ev: &EngineView) -> Result<()> {
    let view = &ev.view;
    // `vanilla` names no loader, as in `loader_from`.
    let modded_without_pack = view.pack.is_none()
        && non_empty(&view.game.loader).is_some_and(|l| !l.eq_ignore_ascii_case("vanilla"));
    if !view.is_network()
        && !modded_without_pack
        && (view.pack.is_some() || game_version(view).is_some())
    {
        return Ok(());
    }
    let address = ServerAddress::from_view(&view.address, &ev.engine)
        .map(|a| a.display())
        .or_else(|| non_empty(&view.address.display).map(|d| d.chars().take(260).collect()))
        .unwrap_or_else(|| host_of(&ev.engine));
    Err(LauncherError::pack(
        ErrorCode::InviteUnsupported,
        format!(
            "This server can't be joined from the launcher yet. Join with the address {address}."
        ),
    )
    .with_param("address", address))
}

/// A Minecraft version id the launcher can look up and use as a folder name.
fn valid_version(v: &str) -> bool {
    (1..=64).contains(&v.len())
        && !v.contains("..")
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+ ".contains(&b))
}

fn unusable_version() -> LauncherError {
    invalid("the server's invite", "no usable Minecraft version")
}

fn display_name(view: &InviteView) -> String {
    view.name.trim().chars().take(MAX_NAME_CHARS).collect()
}

fn source_of(link: &InviteLink, ev: &EngineView) -> SourceRef {
    SourceRef::Perland {
        invite: link.canonical(),
        engine: ev.engine.to_string(),
    }
}

fn about(link: &InviteLink, ev: &EngineView) -> PackAbout {
    PackAbout {
        description: non_empty(&ev.view.note).map(str::to_string),
        art: PackArt {
            // Fetched by `art::fetch`, which never fails and reports nothing, so the token in
            // this URL can't leave it.
            icon: Some(invite_route(&ev.engine, link, "/icon").to_string()),
            banner: None,
        },
        policy: Some(UpdatePolicy::Auto),
    }
}

/// The loader the view names, if any. `vanilla`, or a loader without a version, means none.
fn loader_from(view: &InviteView, minecraft: &str) -> Result<Option<LoaderSpec>> {
    let Some(pack) = &view.pack else {
        return Ok(None);
    };
    let (Some(kind), Some(version)) = (non_empty(&pack.loader), non_empty(&pack.loader_version))
    else {
        return Ok(None);
    };
    if kind.eq_ignore_ascii_case("vanilla") {
        return Ok(None);
    }
    let versions = BTreeMap::from([
        ("minecraft".to_string(), minecraft.to_string()),
        (kind.to_ascii_lowercase(), version.to_string()),
    ]);
    Ok(game_from(&versions)?.1)
}

/// Replace the token wherever it appears in an error. The downloader's details keep URL paths,
/// and the token is in the path.
pub(crate) fn scrub(mut e: LauncherError, token: &str) -> LauncherError {
    let clean = |s: &str| s.replace(token, "<invite>");
    e.message = clean(&e.message);
    e.fix = e.fix.map(|f| clean(&f));
    e.detail = e.detail.map(|d| clean(&d));
    for value in e.params.values_mut() {
        *value = clean(value);
    }
    e
}

fn pack_unavailable(reason: Option<String>) -> LauncherError {
    let e = LauncherError::pack(
        ErrorCode::ServerPackUnavailable,
        "The server's modpack isn't available right now",
    )
    .with_fix("Try again later, or let the server's owner know");
    match reason {
        Some(r) => e.with_detail(r),
        None => e,
    }
}

fn pack_pending() -> LauncherError {
    LauncherError::pack(
        ErrorCode::ServerPackPending,
        "The server is still preparing the modpack. Try again in a minute.",
    )
}

/// A failed check: a dead link stays `invite_gone` (the app shows its plate); anything else is
/// the usual "Couldn't check for updates" warning.
fn check_failed(e: LauncherError) -> LauncherError {
    if e.code == ErrorCode::InviteGone {
        return e;
    }
    LauncherError::network(ErrorCode::UpdateCheckFailed, "Couldn't check for updates")
        .with_fix("You can still play the version you have")
        .with_detail(e.message)
}

fn join_error(e: tokio::task::JoinError) -> LauncherError {
    std::io::Error::other(e).into()
}

/// A new server instance's source, and the pack as the view describes it. Nothing is
/// downloaded (`files` is empty and `stamp` is blank). The first sync resolves the real pack
/// (3b §4.1 decision 4), so a pack that is still building never holds up Add.
pub async fn open(src: &Sources, link: &InviteLink) -> Result<(SourceRef, DesiredState)> {
    open_inner(src, link)
        .await
        .map_err(|e| scrub(e, link.token()))
}

async fn open_inner(src: &Sources, link: &InviteLink) -> Result<(SourceRef, DesiredState)> {
    let ev = src.invites.view(link).await?;
    supported(link, &ev)?;
    let view = &ev.view;
    let minecraft = view
        .pack
        .as_ref()
        .map(|p| p.minecraft.trim())
        .filter(|m| !m.is_empty())
        .or_else(|| game_version(view))
        .filter(|m| valid_version(m))
        .ok_or_else(unusable_version)?
        .to_string();
    let desired = DesiredState {
        name: display_name(view),
        pack_version: version_of(view),
        loader: loader_from(view, &minecraft)?,
        minecraft,
        files: Vec::new(),
        stamp: String::new(),
        about: about(link, &ev),
        archive: None,
    };
    Ok((source_of(link, &ev), desired))
}

/// `resolve` for a saved source: seeds the Engine it last used, so no `config.json` is needed.
pub async fn resolve_ref(
    src: &Sources,
    invite: &str,
    engine: &str,
    sink: &EventSink,
    cancel: &Cancel,
) -> Result<DesiredState> {
    let link = InviteLink::parse(invite)?;
    src.invites.remember_engine(&link, engine);
    resolve(src, &link, sink, cancel).await
}

/// Exactly what the server runs now. `sink` hears `server_pack_wait` while the Engine is still
/// building the pack; `cancel` ends that wait (`sync_cancelled`), and a cancelled resolve never
/// starts the pack download.
pub async fn resolve(
    src: &Sources,
    link: &InviteLink,
    sink: &EventSink,
    cancel: &Cancel,
) -> Result<DesiredState> {
    resolve_inner(src, link, sink, cancel)
        .await
        .map_err(|e| scrub(e, link.token()))
}

async fn resolve_inner(
    src: &Sources,
    link: &InviteLink,
    sink: &EventSink,
    cancel: &Cancel,
) -> Result<DesiredState> {
    let ev = until_cancelled(cancel, src.invites.view(link)).await?;
    supported(link, &ev)?;
    let ev = wait_for_pack(src, link, ev, sink, cancel).await?;
    let Some(pack) = ev.view.pack.clone() else {
        return vanilla(link, &ev);
    };
    if cancel.is_cancelled() {
        return Err(sync_cancelled());
    }
    let (path, hashes, stamp) = download_pack(src, link, &ev, non_empty(&pack.commit)).await?;
    let mut desired = tokio::task::spawn_blocking(move || {
        mrpack::read_archive(&path, stamp, hashes, mrpack::Limits::default())
    })
    .await
    .map_err(join_error)??;
    let name = display_name(&ev.view);
    if !name.is_empty() {
        desired.name = name;
    }
    if let Some(version) = non_empty(&pack.version) {
        desired.pack_version = version.to_string();
    }
    desired.about = about(link, &ev);
    Ok(desired)
}

/// A stack without a modpack: vanilla at the server's Minecraft version. When the server moves
/// to a new version, the stamp changes and sync installs it.
fn vanilla(link: &InviteLink, ev: &EngineView) -> Result<DesiredState> {
    let version = game_version(&ev.view)
        .filter(|v| valid_version(v))
        .ok_or_else(unusable_version)?
        .to_string();
    Ok(DesiredState {
        name: display_name(&ev.view),
        pack_version: version.clone(),
        minecraft: version.clone(),
        loader: None,
        files: Vec::new(),
        stamp: format!("{VANILLA_PREFIX}{version}"),
        about: about(link, ev),
        archive: None,
    })
}

/// `work`, unless `cancel` comes first: then `sync_cancelled` at once.
async fn until_cancelled<T>(
    cancel: &Cancel,
    work: impl std::future::Future<Output = Result<T>>,
) -> Result<T> {
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(sync_cancelled()),
        out = work => out,
    }
}

/// While the Engine builds the pack, poll the view every `pack_poll_every` for up to
/// `pack_poll_for`, telling `sink` (`server_pack_wait`). A Stop ends the wait at once,
/// during the sleep or a poll.
async fn wait_for_pack(
    src: &Sources,
    link: &InviteLink,
    mut ev: EngineView,
    sink: &EventSink,
    cancel: &Cancel,
) -> Result<EngineView> {
    let timing = src.invites.timing();
    let deadline = tokio::time::Instant::now() + timing.pack_poll_for;
    loop {
        let state = ev
            .view
            .pack
            .as_ref()
            .map_or(PACK_READY, |p| p.state.as_str());
        match state {
            PACK_READY => return Ok(ev),
            PACK_UNAVAILABLE => return Err(pack_unavailable(None)),
            _ => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(pack_pending());
        }
        sink(Event::Progress {
            task: TaskId::new("server_pack_wait").with("name", display_name(&ev.view)),
            done: 0,
            total: 0,
            bytes: 0,
        });
        until_cancelled(cancel, async {
            tokio::time::sleep(timing.pack_poll_every).await;
            Ok(())
        })
        .await?;
        ev = until_cancelled(cancel, src.invites.view_fresh(link)).await?;
        supported(link, &ev)?;
    }
}

/// Download the pack into the object cache. Returns where it is, its hashes and its stamp:
/// the pack's ETag, else `commit` (the Engine's ETag is `"<commit>"`).
async fn download_pack(
    src: &Sources,
    link: &InviteLink,
    ev: &EngineView,
    commit: Option<&str>,
) -> Result<(PathBuf, Hashes, String)> {
    let host = host_of(&ev.engine);
    let url = invite_route(&ev.engine, link, "/pack");
    let stamp = match (head_pack(src, &url, &host, None).await, commit) {
        (Ok(PackHead::Tag(Some(tag))), _) => tag,
        // A failed HEAD is left to the GET below, which reports the Engine's reason.
        (_, Some(commit)) => commit.to_string(),
        (Err(e), None) => return Err(e),
        (Ok(_), None) => return Err(pack_unavailable(Some("no ETag".into()))),
    };
    // Named by stamp (the pack's ETag, else the commit), so an interrupted download resumes
    // with Range and a finished one is reused: one stamp is always one file.
    let dest = incoming_path(src, &stamp);
    let request = DownloadRequest::new(url.as_str(), &dest, Hashes::default(), None);
    if let Err(e) = src.downloader.fetch(&request).await {
        return Err(pack_download_error(src, url, &host, e).await);
    }
    let hashes = hash_file(&dest).await?;
    let cached = src.cache.insert_file(&dest, &hashes).await?;
    let _ = tokio::fs::remove_file(&dest).await;
    Ok((cached, hashes, stamp))
}

/// What `HEAD /pack` answered.
enum PackHead {
    /// 304: the pack still has the ETag the request named.
    NotModified,
    /// Success, with the pack's ETag when it sent one.
    Tag(Option<String>),
}

/// `HEAD /pack`, with `If-None-Match: "<stamp>"` when `if_none_match` is given. The one place
/// the pack's ETag is read: the installed stamp, and the update check without a commit.
async fn head_pack(
    src: &Sources,
    url: &Url,
    host: &str,
    if_none_match: Option<&str>,
) -> Result<PackHead> {
    let mut request = src.invites.http().head(url.clone());
    if let Some(stamp) = if_none_match {
        request = request.header(reqwest::header::IF_NONE_MATCH, format!("\"{stamp}\""));
    }
    let response = request.send().await.map_err(|e| request_failed(host, e))?;
    if response.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok(PackHead::NotModified);
    }
    if let Some(e) = pack_status_error(response.status(), host) {
        return Err(e);
    }
    Ok(PackHead::Tag(etag_of(response.headers())))
}

/// An ETag without its quotes or `W/`.
pub(crate) fn etag_of(headers: &reqwest::header::HeaderMap) -> Option<String> {
    let raw = headers.get(reqwest::header::ETAG)?.to_str().ok()?.trim();
    let raw = raw.strip_prefix("W/").unwrap_or(raw);
    let tag = raw.trim_matches('"');
    (!tag.is_empty() && tag.len() <= 128).then(|| tag.to_string())
}

/// `/pack`'s own failures (a failed build) first, then the usual invite answers.
fn pack_status_error(status: reqwest::StatusCode, host: &str) -> Option<LauncherError> {
    match status.as_u16() {
        s @ (400 | 413 | 502) => Some(pack_unavailable(Some(format!("status {s}")))),
        _ => status_error(status, host),
    }
}

/// What a downloader failure on `/pack` means for the player. A failed build (502, 413 or 400)
/// carries the Engine's `data.code`, read with one small extra request.
async fn pack_download_error(
    src: &Sources,
    url: Url,
    host: &str,
    e: LauncherError,
) -> LauncherError {
    let status: Option<u16> = e.params.get("status").and_then(|s| s.parse().ok());
    match (e.code, status) {
        (_, Some(404)) => gone(host),
        (_, Some(429)) => rate_limited(host),
        (_, Some(s @ (400 | 413 | 502))) => {
            let reason = src.invites.error_code(url).await;
            pack_unavailable(Some(reason.unwrap_or_else(|| format!("status {s}"))))
        }
        (ErrorCode::HostUnreachable, _) => offline(host, "the pack download couldn't connect"),
        (ErrorCode::DownloadRejected | ErrorCode::HostTrouble, Some(s)) => host_trouble(host, s),
        _ => e,
    }
}

/// `cache/incoming/perland-<stamp>.mrpack`, next to the object store.
fn incoming_path(src: &Sources, stamp: &str) -> PathBuf {
    let plain = (1..=80).contains(&stamp.len())
        && !stamp.starts_with('.')
        && stamp
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b));
    let name = if plain {
        stamp.to_string()
    } else {
        sha1_hex(stamp.as_bytes())
    };
    src.cache
        .dir()
        .with_file_name("incoming")
        .join(format!("perland-{name}.mrpack"))
}

/// sha1 and sha512 of a file, read once on the blocking pool.
async fn hash_file(path: &Path) -> Result<Hashes> {
    let p = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> std::io::Result<Hashes> {
        let mut file = std::fs::File::open(&p)?;
        let (mut s1, mut s512) = (sha1::Sha1::new(), sha2::Sha512::new());
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            s1.update(&buf[..n]);
            s512.update(&buf[..n]);
        }
        Ok(Hashes {
            sha1: Some(hex::encode(s1.finalize())),
            sha512: Some(hex::encode(s512.finalize())),
            ..Hashes::default()
        })
    })
    .await
    .map_err(join_error)?
    .map_err(LauncherError::from)
}

/// The cheap check against what is installed, using the shared view. It never fails: a failure
/// is `Unavailable`, and the installed version plays.
pub async fn check(
    src: &Sources,
    invite: &str,
    engine: &str,
    installed: &DesiredState,
) -> UpdateCheck {
    let link = match InviteLink::parse(invite) {
        Ok(link) => link,
        Err(e) => return UpdateCheck::Unavailable(e),
    };
    src.invites.remember_engine(&link, engine);
    match check_inner(src, &link, installed).await {
        Ok(check) => check,
        Err(e) => UpdateCheck::Unavailable(check_failed(scrub(e, link.token()))),
    }
}

async fn check_inner(
    src: &Sources,
    link: &InviteLink,
    installed: &DesiredState,
) -> Result<UpdateCheck> {
    let ev = src.invites.view(link).await?;
    supported(link, &ev)?;
    let stamp = match stamp_of(&ev.view) {
        Some(stamp) => stamp,
        None => {
            // An Engine without `pack.commit`: ask whether the pack's ETag moved on.
            let url = invite_route(&ev.engine, link, "/pack");
            match head_pack(src, &url, &host_of(&ev.engine), Some(&installed.stamp)).await? {
                PackHead::NotModified | PackHead::Tag(None) => return Ok(UpdateCheck::UpToDate),
                PackHead::Tag(Some(tag)) => tag,
            }
        }
    };
    if stamp == installed.stamp {
        return Ok(UpdateCheck::UpToDate);
    }
    Ok(UpdateCheck::Available {
        source: source_of(link, &ev),
        stamp,
        version: version_of(&ev.view),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "b4sE64url_-b4sE64url_-b4sE64url_-b4sE64url_";

    #[test]
    fn scrub_takes_the_token_out_of_every_field() {
        let url = format!("https://api.example.com/api/v1/invites/{TOKEN}/pack");
        let e = LauncherError::network(ErrorCode::ConnectionDropped, format!("Lost {url}"))
            .with_fix(format!("Open {url} again"))
            .with_detail(format!("GET {url}: reset"))
            .with_param("url", &url)
            .with_param("file", format!("perland-{TOKEN}.mrpack"))
            .with_param("host", "api.example.com");
        let clean = scrub(e, TOKEN);
        let text = format!("{clean:?} {}", serde_json::to_string(&clean).unwrap());
        assert!(!text.contains(TOKEN), "{text}");
        let path = "https://api.example.com/api/v1/invites/<invite>/pack";
        assert_eq!(clean.message, format!("Lost {path}"));
        assert_eq!(clean.fix, Some(format!("Open {path} again")));
        assert_eq!(clean.detail, Some(format!("GET {path}: reset")));
        assert_eq!(clean.params["url"], path);
        assert_eq!(clean.params["file"], "perland-<invite>.mrpack");
        assert_eq!(clean.params["host"], "api.example.com");
        assert_eq!(clean.code, ErrorCode::ConnectionDropped);
    }
}
