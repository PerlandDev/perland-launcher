//! Bring an instance over from another launcher: work out what the picked folder is
//! ([`inspect`]) and copy the player's files out of it ([`copy_game_dir`]). The source folder
//! is only ever read.

mod detect;

use std::{
    ffi::OsStr,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use crate::{
    error::{ErrorCode, LauncherError, Result},
    sync::Cancel,
};

pub use detect::Detected;

/// Top-level entries of a game folder that are never copied. The launcher installs its own
/// `assets`, `libraries` and `versions`; the rest are caches or another launcher's accounts.
const SKIP_ALWAYS: &[&str] = &[
    "assets",
    "libraries",
    "versions",
    "natives",
    ".fabric",
    ".quilt",
    ".cache",
    "logs",
    "launcher_profiles.json",
    "launcher_accounts.json",
    "launcher_accounts_microsoft_store.json",
];

/// The other launcher's own files, also skipped when the game folder is the picked folder.
const SKIP_ROOT_META: &[&str] = &[
    "instance.cfg",
    "mmc-pack.json",
    "minecraftinstance.json",
    "profile.json",
    "instance.json",
    ".curseclient",
];

/// What the picked folder is, and how many bytes [`copy_game_dir`] would copy from its game
/// folder. Refuses the launcher's own data folder (and anything inside or around it) and a
/// folder that isn't a Minecraft instance.
pub async fn inspect(picked: &Path, launcher_root: &Path) -> Result<(Detected, u64)> {
    // Links are never followed: a picked link (a linked `.minecraft`, say) would import
    // whatever it points at.
    if is_link(picked).await {
        return Err(not_instance());
    }
    let picked = match tokio::fs::canonicalize(picked).await {
        Ok(path) if tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_dir()) => path,
        _ => return Err(not_instance()),
    };
    let root = tokio::fs::canonicalize(launcher_root)
        .await
        .unwrap_or_else(|_| launcher_root.to_path_buf());
    if picked.starts_with(&root) || root.starts_with(&picked) {
        return Err(inside_launcher());
    }
    let detected = detect::detect(&picked).await.ok_or_else(not_instance)?;
    // A Prism `.minecraft` that is a link: the instance's files live somewhere else.
    if is_link(&detected.game_dir).await {
        return Err(not_instance());
    }
    let plan = plan(&detected.game_dir, detected.skip_root_meta).await?;
    Ok((detected, plan.total))
}

/// Copy the game folder `src` into `dst` (created, with its parents), leaving out the
/// launcher files [`inspect`] doesn't count and never following links. `progress(done, total)`
/// reports cumulative bytes: once at the start and after every file, so the last call has
/// `done == total`. Checks `cancel` before every file. On failure `dst` keeps whatever was
/// copied so far; the caller copies into a staging folder and removes it.
pub async fn copy_game_dir(
    src: &Path,
    dst: &Path,
    skip_root_meta: bool,
    cancel: &Cancel,
    progress: &(dyn Fn(u64, u64) + Sync),
) -> Result<()> {
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let plan = plan(src, skip_root_meta).await?;
    for dir in std::iter::once(dst.to_path_buf()).chain(plan.dirs.iter().map(|d| dst.join(d))) {
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| io_failed(&dir, e))?;
    }
    let mut done = 0;
    progress(done, plan.total);
    for (rel, size) in &plan.files {
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        tokio::fs::copy(src.join(rel), dst.join(rel))
            .await
            .map_err(|e| io_failed(&src.join(rel), e))?;
        done += size;
        progress(done, plan.total);
    }
    Ok(())
}

/// What a copy of a game folder brings: the folders to create and the files with their sizes,
/// relative to the game folder. [`inspect`]'s size and [`copy_game_dir`]'s total both come
/// from here, so they always agree.
#[derive(Debug, Default)]
struct Plan {
    dirs: Vec<PathBuf>,
    files: Vec<(PathBuf, u64)>,
    total: u64,
}

async fn plan(src: &Path, skip_root_meta: bool) -> Result<Plan> {
    let mut plan = Plan::default();
    // A game folder that is itself a link isn't followed ([`inspect`] refuses one).
    if is_link(src).await {
        return Ok(plan);
    }
    let mut pending = vec![PathBuf::new()];
    while let Some(rel) = pending.pop() {
        let dir = src.join(&rel);
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            // A Prism instance that was never launched has no game folder yet.
            Err(e) if rel.as_os_str().is_empty() && e.kind() == ErrorKind::NotFound => {
                return Ok(plan)
            }
            Err(e) => return Err(io_failed(&dir, e)),
        };
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await.map_err(|e| io_failed(&dir, e))? {
            names.push(entry.file_name());
        }
        names.sort();
        for name in names {
            if rel.as_os_str().is_empty() && skipped_at_top(&name, skip_root_meta) {
                continue;
            }
            let child = rel.join(&name);
            let path = src.join(&child);
            let meta = tokio::fs::symlink_metadata(&path)
                .await
                .map_err(|e| io_failed(&path, e))?;
            // Links (and Windows junctions) are neither followed nor copied, nor is anything
            // that isn't a plain file or folder.
            if meta.is_dir() {
                plan.dirs.push(child.clone());
                pending.push(child);
            } else if meta.is_file() && !is_world_lock(&name) {
                plan.total += meta.len();
                plan.files.push((child, meta.len()));
            }
        }
    }
    Ok(plan)
}

/// Case-insensitive, as the folders it guards against are on Windows and macOS.
fn skipped_at_top(name: &OsStr, skip_root_meta: bool) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    let listed = |list: &[&str]| list.iter().any(|s| s.eq_ignore_ascii_case(name));
    listed(SKIP_ALWAYS) || (skip_root_meta && listed(SKIP_ROOT_META))
}

/// A world's `session.lock`: the game makes a new one, and on Windows it stays locked while
/// the other launcher's game runs, which would fail the copy.
fn is_world_lock(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|n| n.eq_ignore_ascii_case("session.lock"))
}

async fn is_link(path: &Path) -> bool {
    tokio::fs::symlink_metadata(path)
        .await
        .is_ok_and(|m| m.file_type().is_symlink())
}

fn io_failed(path: &Path, e: std::io::Error) -> LauncherError {
    let detail = format!("{}: {e}", path.display());
    LauncherError::from(e).with_detail(detail)
}

pub fn not_instance() -> LauncherError {
    LauncherError::disk(
        ErrorCode::ImportNotInstance,
        "This folder doesn't look like a Minecraft instance.",
    )
    .with_fix("Pick the instance's own folder: the one with mods, saves or config inside.")
}

pub fn inside_launcher() -> LauncherError {
    LauncherError::disk(
        ErrorCode::ImportInsideLauncher,
        "That folder belongs to Perland Launcher itself.",
    )
    .with_fix("Pick a folder from your other launcher.")
}

/// A second import while one is running. Raised by the app, which owns the running import.
pub fn busy() -> LauncherError {
    LauncherError::disk(ErrorCode::ImportBusy, "Another import is still running.")
        .with_fix("Wait for it to finish, or cancel it.")
}

pub fn cancelled() -> LauncherError {
    LauncherError::disk(ErrorCode::ImportCancelled, "The import was cancelled.")
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::{instances::LoaderSpec, meta::loaders::LoaderKind};

    fn put(root: &Path, rel: &str, body: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn loader(kind: LoaderKind, version: &str) -> Option<LoaderSpec> {
        Some(LoaderSpec {
            kind,
            version: version.into(),
        })
    }

    /// A temp folder holding the launcher's data root (`root/`) next to the folders a test picks.
    struct Fixture {
        tmp: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            std::fs::create_dir_all(tmp.path().join("root/instances")).unwrap();
            Self { tmp }
        }
        fn root(&self) -> PathBuf {
            self.tmp.path().join("root")
        }
        fn dir(&self, rel: &str) -> PathBuf {
            let dir = self.tmp.path().join(rel);
            std::fs::create_dir_all(&dir).unwrap();
            dir
        }
        async fn inspect(&self, picked: &Path) -> Result<(Detected, u64)> {
            inspect(picked, &self.root()).await
        }
    }

    /// Every file under `dir`, relative and with `/` separators, with its bytes' SHA-256.
    fn files(dir: &Path) -> BTreeMap<String, String> {
        fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let meta = std::fs::symlink_metadata(&path).unwrap();
                if meta.is_dir() {
                    walk(base, &path, out);
                } else if meta.is_file() {
                    let rel = path.strip_prefix(base).unwrap().to_string_lossy();
                    let hash = hex::encode(Sha256::digest(std::fs::read(&path).unwrap()));
                    out.insert(rel.replace('\\', "/"), hash);
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(dir, dir, &mut out);
        out
    }

    fn prism(fx: &Fixture, components: &str) -> PathBuf {
        let dir = fx.dir("prism/My Pack");
        put(
            &dir,
            "instance.cfg",
            "[General]\nInstanceType=OneSix\nname=My Pack\n",
        );
        put(
            &dir,
            "mmc-pack.json",
            &format!(r#"{{"components":[{components}],"formatVersion":1}}"#),
        );
        put(&dir, ".minecraft/mods/a.jar", "jar");
        dir
    }

    const PRISM_FABRIC: &str = r#"{"uid":"net.minecraft","version":"1.20.1"},
        {"uid":"net.fabricmc.intermediary","version":"1.20.1"},
        {"uid":"net.fabricmc.fabric-loader","version":"0.15.11"}"#;

    #[tokio::test]
    async fn detects_prism() {
        let fx = Fixture::new();
        let dir = prism(&fx, PRISM_FABRIC);
        let (d, size) = fx.inspect(&dir).await.unwrap();
        assert_eq!(d.name.as_deref(), Some("My Pack"));
        assert_eq!(d.minecraft.as_deref(), Some("1.20.1"));
        assert_eq!(d.loader, loader(LoaderKind::Fabric, "0.15.11"));
        assert!(d.game_dir.ends_with(".minecraft"), "{:?}", d.game_dir);
        assert!(!d.skip_root_meta);
        assert_eq!(size, 3);
    }

    #[tokio::test]
    async fn detects_prism_from_the_inner_folder() {
        let fx = Fixture::new();
        let dir = prism(&fx, PRISM_FABRIC);
        let (outer, _) = fx.inspect(&dir).await.unwrap();
        let (inner, size) = fx.inspect(&dir.join(".minecraft")).await.unwrap();
        assert_eq!(inner, outer);
        assert_eq!(size, 3);
    }

    #[tokio::test]
    async fn detects_prism_minecraft_folder_without_a_dot() {
        let fx = Fixture::new();
        let dir = fx.dir("multimc/Old");
        put(&dir, "instance.cfg", "name=Old\n");
        put(
            &dir,
            "mmc-pack.json",
            r#"{"components":[{"uid":"net.minecraft","version":"1.12.2"},
                {"uid":"net.minecraftforge","version":"14.23.5.2860"}]}"#,
        );
        put(&dir, "minecraft/options.txt", "o");
        let (d, _) = fx.inspect(&dir).await.unwrap();
        assert!(d.game_dir.ends_with("minecraft"), "{:?}", d.game_dir);
        assert_eq!(d.loader, loader(LoaderKind::Forge, "14.23.5.2860"));
    }

    #[tokio::test]
    async fn unknown_loader_is_left_for_the_player() {
        let fx = Fixture::new();
        let dir = prism(
            &fx,
            r#"{"uid":"net.minecraft","version":"1.12.2"},
               {"uid":"com.mumfrey.liteloader","version":"1.12.2-SNAPSHOT"}"#,
        );
        let (d, _) = fx.inspect(&dir).await.unwrap();
        assert_eq!(d.minecraft.as_deref(), Some("1.12.2"));
        assert_eq!(d.loader, None);
    }

    #[tokio::test]
    async fn malformed_metadata_is_left_undetected() {
        let fx = Fixture::new();
        let dir = fx.dir("prism/Broken");
        put(&dir, "instance.cfg", "name=Broken\n");
        put(&dir, "mmc-pack.json", "{ not json");
        put(&dir, ".minecraft/options.txt", "o");
        let (d, _) = fx.inspect(&dir).await.unwrap();
        assert_eq!(d.name.as_deref(), Some("Broken"));
        assert_eq!((d.minecraft, d.loader), (None, None));
    }

    #[tokio::test]
    async fn detects_curseforge() {
        for (base, expected) in [
            ("forge-47.2.0", loader(LoaderKind::Forge, "47.2.0")),
            (
                "neoforge-20.4.80-beta",
                loader(LoaderKind::NeoForge, "20.4.80-beta"),
            ),
            (
                "fabric-0.15.11-1.20.1",
                loader(LoaderKind::Fabric, "0.15.11"),
            ),
            ("quilt-0.26.0-1.20.1", loader(LoaderKind::Quilt, "0.26.0")),
            ("liteloader-1.0", None),
        ] {
            let fx = Fixture::new();
            let dir = fx.dir("curseforge/Instances/CF");
            put(
                &dir,
                "minecraftinstance.json",
                &format!(
                    r#"{{"name":"CF","gameVersion":"1.20.1","baseModLoader":{{"name":"{base}"}}}}"#
                ),
            );
            put(&dir, "mods/a.jar", "jar");
            let (d, size) = fx.inspect(&dir).await.unwrap();
            assert_eq!(d.name.as_deref(), Some("CF"));
            assert_eq!(d.minecraft.as_deref(), Some("1.20.1"));
            assert_eq!(d.loader, expected, "{base}");
            assert_eq!(d.game_dir, dir.canonicalize().unwrap());
            assert!(d.skip_root_meta);
            assert_eq!(size, 3, "the metadata file isn't counted");
        }
    }

    #[tokio::test]
    async fn detects_modrinth_profile() {
        for (loader_json, expected) in [
            (
                r#""loader":"quilt","loader_version":{"id":"0.26.0"}"#,
                loader(LoaderKind::Quilt, "0.26.0"),
            ),
            (
                r#""loader":"forge","loader_version":{"id":"1.21.1-52.0.1"}"#,
                loader(LoaderKind::Forge, "52.0.1"),
            ),
            (r#""loader":"vanilla","loader_version":null"#, None),
        ] {
            let fx = Fixture::new();
            let dir = fx.dir("modrinth/profiles/MR");
            put(
                &dir,
                "profile.json",
                &format!(r#"{{"metadata":{{"name":"MR","game_version":"1.21.1",{loader_json}}}}}"#),
            );
            let (d, _) = fx.inspect(&dir).await.unwrap();
            assert_eq!(d.name.as_deref(), Some("MR"));
            assert_eq!(d.minecraft.as_deref(), Some("1.21.1"));
            assert_eq!(d.loader, expected, "{loader_json}");
            assert!(d.skip_root_meta);
        }
    }

    #[tokio::test]
    async fn detects_atlauncher() {
        let fx = Fixture::new();
        let dir = fx.dir("atlauncher/instances/AT");
        put(
            &dir,
            "instance.json",
            r#"{"id":"1.19.2","launcher":{"name":"AT","loaderVersion":{"type":"Forge","version":"43.2.0"}}}"#,
        );
        let (d, _) = fx.inspect(&dir).await.unwrap();
        assert_eq!(d.name.as_deref(), Some("AT"));
        assert_eq!(d.minecraft.as_deref(), Some("1.19.2"));
        assert_eq!(d.loader, loader(LoaderKind::Forge, "43.2.0"));
        assert_eq!(d.game_dir, dir.canonicalize().unwrap());
        assert!(d.skip_root_meta);
    }

    #[tokio::test]
    async fn plain_folder_is_accepted_undetected() {
        let fx = Fixture::new();
        let dir = fx.dir("dot-minecraft");
        put(&dir, "saves/w/level.dat", "lvl");
        let (d, size) = fx.inspect(&dir).await.unwrap();
        assert_eq!((d.name, d.minecraft, d.loader), (None, None, None));
        assert_eq!(d.game_dir, dir.canonicalize().unwrap());
        assert!(d.skip_root_meta);
        assert_eq!(size, 3);
    }

    #[tokio::test]
    async fn empty_folder_is_not_an_instance() {
        let fx = Fixture::new();
        let dir = fx.dir("empty");
        put(&dir, "notes.txt", "n");
        let err = fx.inspect(&dir).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportNotInstance);
        let err = fx
            .inspect(&fx.tmp.path().join("missing"))
            .await
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportNotInstance);
        let err = fx.inspect(&dir.join("notes.txt")).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportNotInstance);
    }

    #[tokio::test]
    async fn launcher_root_is_refused() {
        let fx = Fixture::new();
        put(&fx.root(), "instances/x/minecraft/mods/a.jar", "jar");
        put(&fx.root(), "instances/x/minecraft/options.txt", "o");
        let other = fx.dir("other");
        put(&other, "options.txt", "o");
        for picked in [
            fx.root(),
            fx.root().join("instances/x/minecraft"),
            fx.tmp.path().to_path_buf(),
            other.join("..").join("root"),
        ] {
            let err = fx.inspect(&picked).await.unwrap_err();
            assert_eq!(err.code, ErrorCode::ImportInsideLauncher, "{picked:?}");
        }
        assert!(fx.inspect(&other).await.is_ok());
    }

    #[tokio::test]
    async fn copy_skips_launcher_files_and_keeps_player_data() {
        let fx = Fixture::new();
        let src = fx.dir("src");
        for rel in [
            "mods/a.jar",
            "mods/.index/a.pw.toml",
            "saves/w/level.dat",
            "XaeroWaypoints/x.txt",
            "options.txt",
            "assets/x",
            "libraries/x",
            "versions/x",
            "logs/latest.log",
            ".fabric/x",
            "launcher_profiles.json",
            "instance.cfg",
            "saves/w/session.lock",
            "config/Session.LOCK",
        ] {
            put(&src, rel, rel);
        }
        let (d, size) = fx.inspect(&src).await.unwrap();
        assert!(d.skip_root_meta);
        let dst = fx.tmp.path().join("staging/1");
        let last = Mutex::new((u64::MAX, u64::MAX));
        copy_game_dir(
            &d.game_dir,
            &dst,
            d.skip_root_meta,
            &Cancel::default(),
            &|done, total| *last.lock().unwrap() = (done, total),
        )
        .await
        .unwrap();
        let copied = files(&dst);
        assert_eq!(
            copied.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "XaeroWaypoints/x.txt",
                "mods/.index/a.pw.toml",
                "mods/a.jar",
                "options.txt",
                "saves/w/level.dat",
            ]
        );
        let bytes: u64 = copied.keys().map(|rel| rel.len() as u64).sum();
        assert_eq!(size, bytes);
        assert_eq!(*last.lock().unwrap(), (bytes, bytes));
    }

    #[tokio::test]
    async fn copy_keeps_root_meta_names_below_the_top_level_or_when_asked() {
        let fx = Fixture::new();
        let src = fx.dir("src");
        put(&src, "instance.json", "i");
        put(&src, "config/profile.json", "p");
        let dst = fx.tmp.path().join("dst");
        copy_game_dir(&src, &dst, false, &Cancel::default(), &|_, _| {})
            .await
            .unwrap();
        assert_eq!(files(&dst).len(), 2);
        let dst = fx.tmp.path().join("dst2");
        copy_game_dir(&src, &dst, true, &Cancel::default(), &|_, _| {})
            .await
            .unwrap();
        assert_eq!(
            files(&dst).keys().collect::<Vec<_>>(),
            ["config/profile.json"]
        );
    }

    #[tokio::test]
    async fn copy_of_nothing_reports_zero_of_zero() {
        let fx = Fixture::new();
        let src = fx.dir("src");
        let dst = fx.tmp.path().join("a/b/dst");
        let calls = Mutex::new(Vec::new());
        copy_game_dir(&src, &dst, true, &Cancel::default(), &|done, total| {
            calls.lock().unwrap().push((done, total))
        })
        .await
        .unwrap();
        assert!(dst.is_dir());
        assert_eq!(calls.lock().unwrap().last(), Some(&(0, 0)));
    }

    #[tokio::test]
    async fn copy_leaves_the_source_untouched() {
        let fx = Fixture::new();
        let dir = prism(&fx, PRISM_FABRIC);
        put(&dir, ".minecraft/saves/w/level.dat", "lvl");
        put(&dir, ".minecraft/logs/latest.log", "log");
        let before = files(&dir);
        let (d, _) = fx.inspect(&dir).await.unwrap();
        let dst = fx.tmp.path().join("dst");
        copy_game_dir(
            &d.game_dir,
            &dst,
            d.skip_root_meta,
            &Cancel::default(),
            &|_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(files(&dir), before);
        assert_eq!(files(&dst).len(), 2);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn copy_skips_symlinks() {
        let fx = Fixture::new();
        let outside = fx.dir("outside");
        put(&outside, "w/level.dat", "secret");
        let src = fx.dir("src");
        put(&src, "options.txt", "o");
        put(&src, "mods/real.jar", "jar");
        std::os::unix::fs::symlink(&outside, src.join("saves")).unwrap();
        std::os::unix::fs::symlink(outside.join("w/level.dat"), src.join("mods/link.jar")).unwrap();
        let (d, size) = fx.inspect(&src).await.unwrap();
        assert_eq!(size, 4);
        let dst = fx.tmp.path().join("dst");
        copy_game_dir(
            &d.game_dir,
            &dst,
            d.skip_root_meta,
            &Cancel::default(),
            &|_, _| {},
        )
        .await
        .unwrap();
        assert_eq!(
            files(&dst).keys().collect::<Vec<_>>(),
            ["mods/real.jar", "options.txt"]
        );
        assert!(std::fs::symlink_metadata(dst.join("saves")).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_linked_game_folder_is_refused() {
        let fx = Fixture::new();
        let outside = fx.dir("outside");
        put(&outside, "options.txt", "secret");
        let dir = fx.dir("prism/Linked");
        put(&dir, "instance.cfg", "name=Linked\n");
        put(&dir, "mmc-pack.json", "{}");
        std::os::unix::fs::symlink(&outside, dir.join(".minecraft")).unwrap();
        let err = fx.inspect(&dir).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportNotInstance);
        // Picking the linked inner folder itself doesn't import the link's target either.
        let err = fx.inspect(&dir.join(".minecraft")).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportNotInstance);
        // Nor does a copy handed a linked folder directly.
        let dst = fx.tmp.path().join("dst");
        copy_game_dir(
            &dir.join(".minecraft"),
            &dst,
            false,
            &Cancel::default(),
            &|_, _| {},
        )
        .await
        .unwrap();
        assert!(files(&dst).is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_picked_link_is_refused() {
        let fx = Fixture::new();
        let real = fx.dir("real");
        put(&real, "options.txt", "o");
        let link = fx.tmp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let err = fx.inspect(&link).await.unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportNotInstance);
        assert!(fx.inspect(&real).await.is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn linked_metadata_files_are_not_markers() {
        let fx = Fixture::new();
        let elsewhere = fx.dir("elsewhere");
        put(
            &elsewhere,
            "profile.json",
            r#"{"metadata":{"name":"MR","game_version":"1.21.1","loader":"vanilla"}}"#,
        );
        put(
            &elsewhere,
            "instance.json",
            r#"{"id":"1.19.2","launcher":{"name":"AT"}}"#,
        );
        for file in ["profile.json", "instance.json"] {
            let dir = fx.dir(&format!("linked-{file}"));
            put(&dir, "mods/a.jar", "jar");
            std::os::unix::fs::symlink(elsewhere.join(file), dir.join(file)).unwrap();
            let (d, _) = fx.inspect(&dir).await.unwrap();
            assert_eq!((d.name, d.minecraft), (None, None), "{file}");
        }
    }

    /// Task 5 spawns the copy and awaits it in a command, so these futures must stay `Send`.
    #[allow(dead_code)]
    fn import_futures_are_send() {
        fn send<T: Send>(_: T) {}
        let cancel = Cancel::default();
        send(copy_game_dir(
            Path::new("a"),
            Path::new("b"),
            true,
            &cancel,
            &|_, _| {},
        ));
        send(inspect(Path::new("a"), Path::new("b")));
    }

    #[tokio::test]
    async fn copy_honours_cancel() {
        let fx = Fixture::new();
        let src = fx.dir("src");
        put(&src, "a.txt", "a");
        put(&src, "b.txt", "b");
        let dst = fx.tmp.path().join("dst");
        let cancel = Cancel::default();
        let err = copy_game_dir(&src, &dst, true, &cancel, &|done, _| {
            if done > 0 {
                cancel.cancel();
            }
        })
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportCancelled);
        assert_eq!(files(&dst).len(), 1, "stopped before the second file");

        let cancelled = Cancel::default();
        cancelled.cancel();
        let err = copy_game_dir(
            &src,
            &fx.tmp.path().join("dst2"),
            true,
            &cancelled,
            &|_, _| {},
        )
        .await
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::ImportCancelled);
    }
}
