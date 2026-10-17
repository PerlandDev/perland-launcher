//! A mod's own icon, read from its jar: Fabric's `fabric.mod.json` `icon`, Quilt's
//! `quilt.mod.json` `quilt_loader.metadata.icon`, or Forge's and NeoForge's `logoFile`. PNG only,
//! under a size cap, never outside the jar. Each answer (an icon, or none) is cached on disk by the
//! jar's fingerprint, so the mod list reopens a jar only when it changes. A jar without one can
//! still have its Modrinth project's icon: the project is looked up once per jar, by its sha1.
use std::{
    future::Future,
    io::{Read, Seek},
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::{
    download::hash::{sha1_file, sha256_hex},
    error::Result,
    fsutil::{normalize_rel, write_atomic},
    mods,
};

/// The largest icon (or metadata file) read from a jar.
pub const MAX_ICON_BYTES: u64 = 1 << 20;
/// The mod list draws icons at 40 CSS pixels; this covers a 2x screen.
const TARGET_PX: u32 = 80;
/// The cache stays under this, oldest answers dropped first.
pub const MAX_CACHE_BYTES: u64 = 32 << 20;
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";
/// Cached "this jar has no icon", so a jar without one isn't reopened either.
const NONE_EXT: &str = "none";
/// Cached Modrinth answer for a jar: its project's icon URL, or empty for none.
const MODRINTH_EXT: &str = "modrinth";
const MAX_URL_BYTES: u64 = 4 << 10;

/// The icon PNG inside a mod jar, or `None` when it has none or it isn't a usable PNG. A jar built
/// for several loaders is asked as each in turn.
pub fn read_icon<R: Read + Seek>(reader: R) -> Option<Vec<u8>> {
    let mut jar = zip::ZipArchive::new(reader).ok()?;
    let named = [
        read_json(&mut jar, "fabric.mod.json").and_then(|v| pick(v.get("icon")?)),
        read_json(&mut jar, "quilt.mod.json")
            .and_then(|v| pick(v.get("quilt_loader")?.get("metadata")?.get("icon")?)),
        logo_file(&mut jar, "META-INF/neoforge.mods.toml"),
        logo_file(&mut jar, "META-INF/mods.toml"),
    ];
    named.into_iter().flatten().find_map(|path| {
        let path = normalize_rel(path.strip_prefix('/').unwrap_or(&path))?;
        read_capped(&mut jar, &path).filter(|bytes| bytes.starts_with(PNG))
    })
}

/// One entry's bytes, or `None` past [`MAX_ICON_BYTES`] (whatever the entry claims its size is).
fn read_capped<R: Read + Seek>(jar: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let entry = jar.by_name(name).ok()?;
    if !entry.is_file() || entry.size() > MAX_ICON_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_ICON_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= MAX_ICON_BYTES).then_some(bytes)
}

fn read_json<R: Read + Seek>(
    jar: &mut zip::ZipArchive<R>,
    name: &str,
) -> Option<serde_json::Value> {
    serde_json::from_slice(&read_capped(jar, name)?).ok()
}

/// Forge's `logoFile`: the first mod's, else the file's own.
fn logo_file<R: Read + Seek>(jar: &mut zip::ZipArchive<R>, name: &str) -> Option<String> {
    let text = String::from_utf8(read_capped(jar, name)?).ok()?;
    let table = toml::from_str::<toml::Table>(&text).ok()?;
    let first_mod = table
        .get("mods")
        .and_then(|m| m.as_array())
        .and_then(|m| m.first());
    first_mod
        .and_then(|m| m.get("logoFile"))
        .or_else(|| table.get("logoFile"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
}

/// Fabric and Quilt name one icon, or one per pixel size: the smallest that covers the slot,
/// else the largest.
fn pick(icon: &serde_json::Value) -> Option<String> {
    if let Some(path) = icon.as_str() {
        return Some(path.to_string());
    }
    let sizes: Vec<(u32, &str)> = icon
        .as_object()?
        .iter()
        .filter_map(|(size, path)| Some((size.parse().ok()?, path.as_str()?)))
        .collect();
    let covering = sizes.iter().filter(|(size, _)| *size >= TARGET_PX).min();
    covering
        .or_else(|| sizes.iter().max())
        .map(|(_, path)| path.to_string())
}

/// A mod's icon from the instance's `mods/` folder, through the cache in `cache_dir`. `None` when
/// the jar has none, isn't there anymore, or can't be read.
pub async fn icon(game_dir: &Path, cache_dir: &Path, file_name: &str) -> Result<Option<Vec<u8>>> {
    let Some((jar, key)) = locate(game_dir, file_name).await? else {
        return Ok(None);
    };
    let found = cache_dir.join(format!("{key}.png"));
    let missing = cache_dir.join(format!("{key}.{NONE_EXT}"));
    if let Some(bytes) = read_cached(&found).await {
        return Ok(Some(bytes));
    }
    if tokio::fs::try_exists(&missing).await.unwrap_or(false) {
        return Ok(None);
    }
    let icon = tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(jar).ok()?;
        read_icon(std::io::BufReader::new(file))
    })
    .await
    .ok()
    .flatten();
    // The cache only saves work: an icon that can't be saved is still shown.
    let saved = match &icon {
        Some(bytes) => write_atomic(&found, bytes).await,
        None => write_atomic(&missing, &[]).await,
    };
    if saved.is_ok() {
        prune(cache_dir, MAX_CACHE_BYTES).await;
    }
    Ok(icon)
}

/// The icon URL of the Modrinth project the jar belongs to, from `lookup` (given the jar's sha1)
/// once per jar and from the cache after that. `None` when the jar is gone or Modrinth knows no
/// icon for it. A failed lookup is not remembered, so it is asked again next time.
pub async fn project_icon_url<F, Fut>(
    game_dir: &Path,
    cache_dir: &Path,
    file_name: &str,
    lookup: F,
) -> Result<Option<String>>
where
    F: FnOnce(String) -> Fut,
    Fut: Future<Output = Result<Option<String>>>,
{
    let Some((jar, key)) = locate(game_dir, file_name).await? else {
        return Ok(None);
    };
    let answer = cache_dir.join(format!("{key}.{MODRINTH_EXT}"));
    if let Some(known) = read_url(&answer).await {
        return Ok(known);
    }
    let sha1 = tokio::task::spawn_blocking(move || sha1_file(&jar))
        .await
        .map_err(std::io::Error::other)??;
    let url = lookup(sha1).await?;
    if write_atomic(&answer, url.as_deref().unwrap_or("").as_bytes())
        .await
        .is_ok()
    {
        prune(cache_dir, MAX_CACHE_BYTES).await;
    }
    Ok(url)
}

/// A remembered Modrinth answer: `Some(None)` for "no icon", `None` when there is none to read.
async fn read_url(path: &Path) -> Option<Option<String>> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    if !meta.is_file() || meta.len() > MAX_URL_BYTES {
        return None;
    }
    let text = String::from_utf8(tokio::fs::read(path).await.ok()?).ok()?;
    Some((!text.is_empty()).then_some(text))
}

/// The jar `file_name` names and its cache key, or `None` when it isn't there.
async fn locate(game_dir: &Path, file_name: &str) -> Result<Option<(PathBuf, String)>> {
    let (jar, base) = mods::jar_path(game_dir, file_name)?;
    Ok(match tokio::fs::metadata(&jar).await {
        Ok(meta) if meta.is_file() => {
            let key = fingerprint(&base, &meta);
            Some((jar, key))
        }
        _ => None,
    })
}

/// Names the jar as it is now, without reading it all: its name (on or off alike), size and
/// modified time. Any change to the jar gives a new key.
fn fingerprint(base: &str, meta: &std::fs::Metadata) -> String {
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos());
    sha256_hex(format!("{base}\0{}\0{modified}", meta.len()).as_bytes())
}

/// A cached icon, checked again: the disk is the player's.
async fn read_cached(path: &Path) -> Option<Vec<u8>> {
    let meta = tokio::fs::metadata(path).await.ok()?;
    if !meta.is_file() || meta.len() > MAX_ICON_BYTES {
        return None;
    }
    let bytes = tokio::fs::read(path).await.ok()?;
    bytes.starts_with(PNG).then_some(bytes)
}

/// Delete the oldest answers until the folder fits `limit`. Best effort.
async fn prune(dir: &Path, limit: u64) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
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
        if total <= limit {
            break;
        }
        if tokio::fs::remove_file(&path).await.is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use super::*;

    fn png(tag: u8) -> Vec<u8> {
        let mut bytes = PNG.to_vec();
        bytes.extend_from_slice(&[0, 0, 0, 13, tag]);
        bytes
    }

    fn jar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            w.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(bytes).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn read(entries: &[(&str, &[u8])]) -> Option<Vec<u8>> {
        read_icon(Cursor::new(jar_bytes(entries)))
    }

    #[test]
    fn fabric_icon_as_a_string() {
        let icon = png(1);
        let got = read(&[
            (
                "fabric.mod.json",
                br#"{"id":"sodium","icon":"assets/sodium/icon.png"}"#,
            ),
            ("assets/sodium/icon.png", &icon),
        ]);
        assert_eq!(got, Some(icon));
    }

    #[test]
    fn fabric_icon_map_picks_the_smallest_size_that_covers_the_slot() {
        let got = read(&[
            (
                "fabric.mod.json",
                br#"{"icon":{"16":"s.png","128":"m.png","512":"l.png","64":"x.png"}}"#,
            ),
            ("s.png", &png(16)),
            ("x.png", &png(64)),
            ("m.png", &png(128)),
            ("l.png", &png(255)),
        ]);
        assert_eq!(got, Some(png(128)));
    }

    #[test]
    fn fabric_icon_map_falls_back_to_the_largest_when_all_are_small() {
        let got = read(&[
            (
                "fabric.mod.json",
                br#"{"icon":{"16":"s.png","32":"m.png"}}"#,
            ),
            ("s.png", &png(16)),
            ("m.png", &png(32)),
        ]);
        assert_eq!(got, Some(png(32)));
    }

    #[test]
    fn quilt_icon() {
        let got = read(&[
            (
                "quilt.mod.json",
                br#"{"quilt_loader":{"metadata":{"icon":"/assets/q/icon.png"}}}"#,
            ),
            ("assets/q/icon.png", &png(2)),
        ]);
        assert_eq!(got, Some(png(2)));
    }

    #[test]
    fn forge_and_neoforge_logo_file() {
        let forge = read(&[
            (
                "META-INF/mods.toml",
                b"modLoader=\"javafml\"\n[[mods]]\nmodId=\"jei\"\nlogoFile=\"jei.png\"\n",
            ),
            ("jei.png", &png(3)),
        ]);
        assert_eq!(forge, Some(png(3)));
        let neo = read(&[
            (
                "META-INF/neoforge.mods.toml",
                b"logoFile=\"top.png\"\n[[mods]]\nmodId=\"x\"\n",
            ),
            ("top.png", &png(4)),
        ]);
        assert_eq!(neo, Some(png(4)), "a top-level logoFile counts too");
    }

    #[test]
    fn only_png_is_accepted() {
        let got = read(&[
            ("fabric.mod.json", br#"{"icon":"icon.png"}"#),
            ("icon.png", b"GIF89a not a png"),
        ]);
        assert_eq!(got, None);
    }

    #[test]
    fn oversized_icons_are_refused() {
        let mut big = PNG.to_vec();
        big.resize(MAX_ICON_BYTES as usize + 1, 0);
        let got = read(&[
            ("fabric.mod.json", br#"{"icon":"icon.png"}"#),
            ("icon.png", &big),
        ]);
        assert_eq!(got, None);
    }

    #[test]
    fn paths_outside_the_jar_are_refused() {
        for path in [
            "../icon.png",
            "a/../../icon.png",
            "C:/icon.png",
            "a\\..\\..\\b.png",
        ] {
            let meta = format!(r#"{{"icon":"{}"}}"#, path.replace('\\', "\\\\"));
            // The jar even holds an entry by that name: the name itself is refused.
            let got = read(&[("fabric.mod.json", meta.as_bytes()), (path, &png(5))]);
            assert_eq!(got, None, "{path}");
        }
    }

    #[test]
    fn garbage_is_no_icon() {
        assert_eq!(read_icon(Cursor::new(b"not a zip".to_vec())), None);
        assert_eq!(read(&[("fabric.mod.json", b"{not json")]), None);
        assert_eq!(read(&[("fabric.mod.json", br#"{"icon":42}"#)]), None);
        assert_eq!(read(&[("META-INF/mods.toml", b"[[[")]), None);
        assert_eq!(
            read(&[("fabric.mod.json", br#"{"icon":"missing.png"}"#)]),
            None
        );
        assert_eq!(read(&[("readme.txt", b"hi")]), None);
    }

    fn put_jar(game: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let dir = game.join("mods");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[tokio::test]
    async fn icons_are_cached_by_the_jar_fingerprint() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        let with = jar_bytes(&[
            ("fabric.mod.json", br#"{"icon":"i.png"}"#),
            ("i.png", &png(7)),
        ]);
        let jar = put_jar(&game, "a.jar", &with);
        assert_eq!(icon(&game, &cache, "a.jar").await.unwrap(), Some(png(7)));
        // A cached answer is used without opening the jar again: break the jar but keep its
        // fingerprint (size and modified time), and the icon still comes back.
        let modified = std::fs::metadata(&jar).unwrap().modified().unwrap();
        let mut broken = with.clone();
        broken.fill(0);
        std::fs::write(&jar, &broken).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&jar)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_eq!(icon(&game, &cache, "a.jar").await.unwrap(), Some(png(7)));
        // Turning the mod off keeps the fingerprint, so the icon follows it.
        std::fs::rename(&jar, game.join("mods/a.jar.disabled")).unwrap();
        assert_eq!(
            icon(&game, &cache, "a.jar.disabled").await.unwrap(),
            Some(png(7))
        );
    }

    #[tokio::test]
    async fn a_jar_without_an_icon_is_remembered_as_none() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        put_jar(&game, "plain.jar", &jar_bytes(&[("readme.txt", b"hi")]));
        assert_eq!(icon(&game, &cache, "plain.jar").await.unwrap(), None);
        let names: Vec<_> = std::fs::read_dir(&cache)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names.len(), 1);
        assert!(names[0].ends_with(".none"), "{names:?}");
    }

    #[tokio::test]
    async fn a_tampered_cache_entry_is_not_served() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        put_jar(
            &game,
            "a.jar",
            &jar_bytes(&[
                ("fabric.mod.json", br#"{"icon":"i.png"}"#),
                ("i.png", &png(8)),
            ]),
        );
        icon(&game, &cache, "a.jar").await.unwrap();
        let cached = std::fs::read_dir(&cache).unwrap().next().unwrap().unwrap();
        std::fs::write(cached.path(), b"<svg/>").unwrap();
        assert_eq!(
            icon(&game, &cache, "a.jar").await.unwrap(),
            Some(png(8)),
            "read again from the jar"
        );
    }

    #[tokio::test]
    async fn missing_jars_and_bad_names() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        assert_eq!(icon(&game, &cache, "gone.jar").await.unwrap(), None);
        for bad in ["../x.jar", "a/b.jar", "notes.txt", ""] {
            assert_eq!(
                icon(&game, &cache, bad).await.unwrap_err().code,
                crate::ErrorCode::ModNameInvalid,
                "{bad}"
            );
        }
    }

    #[tokio::test]
    async fn prune_drops_the_oldest_answers_first() {
        let tmp = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        for (i, name) in ["old.png", "mid.png", "new.png"].iter().enumerate() {
            let path = tmp.path().join(name);
            std::fs::write(&path, [0u8; 10]).unwrap();
            std::fs::File::options()
                .write(true)
                .open(&path)
                .unwrap()
                .set_modified(now - std::time::Duration::from_secs(100 - i as u64 * 10))
                .unwrap();
        }
        prune(tmp.path(), 20).await;
        assert!(!tmp.path().join("old.png").exists());
        assert!(tmp.path().join("mid.png").exists());
        assert!(tmp.path().join("new.png").exists());
    }

    #[tokio::test]
    async fn the_projects_icon_url_is_looked_up_once_per_jar() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        put_jar(&game, "a.jar", b"jar bytes");
        put_jar(&game, "b.jar", b"other bytes");
        let asked = std::sync::Mutex::new(Vec::new());
        let lookup = |answer: Option<&'static str>| {
            let asked = &asked;
            move |sha1: String| async move {
                asked.lock().unwrap().push(sha1);
                Ok(answer.map(str::to_string))
            }
        };
        let url = "https://cdn.modrinth.com/data/a/icon.png";
        for _ in 0..2 {
            let got = project_icon_url(&game, &cache, "a.jar", lookup(Some(url))).await;
            assert_eq!(got.unwrap().as_deref(), Some(url));
            let none = project_icon_url(&game, &cache, "b.jar", lookup(None)).await;
            assert_eq!(none.unwrap(), None);
        }
        assert_eq!(
            *asked.lock().unwrap(),
            vec![
                crate::download::hash::sha1_hex(b"jar bytes"),
                crate::download::hash::sha1_hex(b"other bytes")
            ],
            "each answer, none included, is remembered"
        );
        // Turned off, the jar keeps its answer.
        std::fs::rename(game.join("mods/a.jar"), game.join("mods/a.jar.disabled")).unwrap();
        let got = project_icon_url(&game, &cache, "a.jar.disabled", lookup(None)).await;
        assert_eq!(got.unwrap().as_deref(), Some(url));
        assert_eq!(asked.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn a_failed_lookup_is_asked_again_next_time() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        put_jar(&game, "a.jar", b"jar");
        let offline = |_: String| async { Err(crate::browse::api::not_found()) };
        assert!(project_icon_url(&game, &cache, "a.jar", offline)
            .await
            .is_err());
        let online = |_: String| async { Ok(Some("https://cdn.modrinth.com/i.png".to_string())) };
        let got = project_icon_url(&game, &cache, "a.jar", online)
            .await
            .unwrap();
        assert_eq!(got.as_deref(), Some("https://cdn.modrinth.com/i.png"));
    }

    #[tokio::test]
    async fn a_missing_jar_is_never_looked_up() {
        let tmp = tempfile::tempdir().unwrap();
        let (game, cache) = (tmp.path().join("game"), tmp.path().join("cache"));
        let never = |_: String| async { panic!("looked up a missing jar") };
        assert_eq!(
            project_icon_url(&game, &cache, "gone.jar", never)
                .await
                .unwrap(),
            None
        );
    }
}
