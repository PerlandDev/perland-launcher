//! Mojang's official Java runtimes: the same builds the vanilla launcher uses.
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use futures_util::StreamExt;
use serde::Deserialize;

use crate::{
    download::{DownloadRequest, Downloader, Hashes},
    error::{ErrorCode, LauncherError, Result},
    events::{EventSink, TaskId},
    fsutil::{safe_join, write_atomic},
    meta::{
        loaders::valid_component,
        mojang::{Artifact, VersionJson},
        MetaClient,
    },
    platform::{Os, Platform},
};

pub const RUNTIME_INDEX_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

#[derive(Debug, Deserialize)]
#[serde(transparent)]
pub struct RuntimeIndex(pub HashMap<String, HashMap<String, Vec<RuntimeEntry>>>);

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeEntry {
    pub manifest: ManifestRef,
    pub version: RuntimeVersion,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestRef {
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeVersion {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RuntimeManifest {
    pub files: BTreeMap<String, RuntimeFile>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum RuntimeFile {
    File {
        #[serde(default)]
        executable: bool,
        downloads: RuntimeDownloads,
    },
    Directory,
    Link {
        target: String,
    },
}

#[derive(Debug, Deserialize)]
pub struct RuntimeDownloads {
    pub raw: Artifact,
}

pub struct RuntimePlan {
    pub downloads: Vec<DownloadRequest>,
    pub executables: Vec<PathBuf>,
    pub links: Vec<(PathBuf, String)>,
}

pub fn component_for(version: &VersionJson) -> String {
    version
        .java_version
        .as_ref()
        .map(|j| j.component.clone())
        .unwrap_or_else(|| "jre-legacy".to_string())
}

pub fn select_runtime<'a>(
    index: &'a RuntimeIndex,
    platform: Platform,
    component: &str,
) -> Result<&'a RuntimeEntry> {
    platform
        .java_runtime_key()
        .and_then(|key| index.0.get(key))
        .and_then(|components| components.get(component))
        .and_then(|entries| entries.first())
        .ok_or_else(|| {
            LauncherError::java(
                ErrorCode::JavaUnavailable,
                format!("There's no official Java ({component}) for this computer"),
            )
            .with_param("component", component)
            .with_fix("Choose a Java installation in the instance settings")
        })
}

pub fn plan_runtime(manifest: &RuntimeManifest, dest: &Path) -> Result<RuntimePlan> {
    let mut plan = RuntimePlan {
        downloads: vec![],
        executables: vec![],
        links: vec![],
    };
    for (rel, file) in &manifest.files {
        let path = safe_join(dest, rel).ok_or_else(|| {
            LauncherError::integrity(
                ErrorCode::UnsafeGameData,
                "The Java download list contains an unsafe path",
            )
            .with_detail(rel)
        })?;
        match file {
            RuntimeFile::File {
                executable,
                downloads,
            } => {
                let raw = &downloads.raw;
                plan.downloads.push(DownloadRequest::new(
                    raw.url.clone(),
                    path.clone(),
                    Hashes::sha1(&raw.sha1),
                    Some(raw.size),
                ));
                if *executable {
                    plan.executables.push(path);
                }
            }
            RuntimeFile::Directory => {}
            RuntimeFile::Link { target } => plan.links.push((path, target.clone())),
        }
    }
    Ok(plan)
}

pub fn java_executable(runtime_dir: &Path, platform: Platform) -> PathBuf {
    match platform.os {
        Os::Windows => runtime_dir.join("bin").join("javaw.exe"),
        _ => runtime_dir.join("bin").join("java"),
    }
}

/// Make sure Mojang's runtime `component` is installed and return its java executable.
pub async fn ensure_runtime(
    meta: &MetaClient,
    downloader: &Downloader,
    java_root: &Path,
    platform: Platform,
    component: &str,
    sink: &EventSink,
) -> Result<PathBuf> {
    // `component` comes from the version JSON and becomes a folder name.
    if !valid_component(component) {
        return Err(LauncherError::integrity(
            ErrorCode::UnsafeGameData,
            "Minecraft's version data names an unsafe Java runtime",
        )
        .with_detail(component));
    }
    let index: RuntimeIndex = meta.get_json(RUNTIME_INDEX_URL).await?;
    let entry = select_runtime(&index, platform, component)?;
    let dir = java_root.join(component);
    let marker = dir.join(".perland-runtime");
    let exe = java_executable(&dir, platform);
    let installed = tokio::fs::read_to_string(&marker)
        .await
        .map(|s| s.trim() == entry.manifest.sha1)
        .unwrap_or(false);
    if installed && exe.is_file() {
        return Ok(exe);
    }
    let manifest: RuntimeManifest = meta
        .get_verified_json(&entry.manifest.url, &entry.manifest.sha1)
        .await?;
    let plan = plan_runtime(&manifest, &dir)?;
    downloader
        .fetch_all(
            plan.downloads,
            &TaskId::new("download_java").with("version", &entry.version.name),
            sink,
        )
        .await?;
    finish_unix(&plan.executables, &plan.links)?;
    write_atomic(&marker, entry.manifest.sha1.as_bytes()).await?;
    Ok(exe)
}

#[cfg(unix)]
fn finish_unix(executables: &[PathBuf], links: &[(PathBuf, String)]) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for exe in executables {
        std::fs::set_permissions(exe, std::fs::Permissions::from_mode(0o755))?;
    }
    for (link, target) in links {
        let _ = std::fs::remove_file(link);
        if let Some(parent) = link.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::os::unix::fs::symlink(target, link)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn finish_unix(_executables: &[PathBuf], _links: &[(PathBuf, String)]) -> Result<()> {
    Ok(())
}

/// Major version from `java -version` output: `"17.0.15"` → 17, `"1.8.0_202"` → 8.
pub fn parse_java_version(output: &str) -> Option<u32> {
    let start = output.find('"')? + 1;
    let rest = &output[start..];
    let version = &rest[..rest.find('"')?];
    let mut parts = version.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok()
    } else {
        Some(first)
    }
}

/// Run `<path> -version` and return its major version. Backs the "Test" button for a custom Java.
pub async fn probe(path: &Path) -> Result<u32> {
    let failed = || {
        LauncherError::java(
            ErrorCode::JavaTestFailed,
            "That doesn't look like a working Java",
        )
        .with_fix("Pick the java executable inside a Java installation's bin folder")
        .with_detail(path.display())
    };
    let mut command = tokio::process::Command::new(path);
    command
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let output = tokio::time::timeout(Duration::from_secs(10), command.output())
        .await
        .map_err(|_| failed())?
        .map_err(|_| failed())?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stderr),
        String::from_utf8_lossy(&output.stdout)
    );
    parse_java_version(&text).ok_or_else(failed)
}

/// The Java major version a version asks for; versions without `javaVersion` predate Java 9.
pub fn required_major(version: &VersionJson) -> u32 {
    version.java_version.as_ref().map_or(8, |j| j.major_version)
}

/// Mojang's runtime for `version`, or, when Mojang has none for this computer, an installed
/// Java of exactly the major version the game asks for.
pub async fn ensure_java(
    meta: &MetaClient,
    downloader: &Downloader,
    java_root: &Path,
    platform: Platform,
    version: &VersionJson,
    sink: &EventSink,
) -> Result<PathBuf> {
    match ensure_runtime(
        meta,
        downloader,
        java_root,
        platform,
        &component_for(version),
        sink,
    )
    .await
    {
        Err(e) if e.code == ErrorCode::JavaUnavailable => {
            let installs = detect_installed(platform).await;
            installed_fallback(e, &installs, required_major(version))
        }
        other => other,
    }
}

/// The installed Java with exactly `major`, else the original error. Only an exact match:
/// older Forge breaks on newer Java, and older Java can't run newer Minecraft.
pub fn installed_fallback(
    err: LauncherError,
    installs: &[JavaInstall],
    major: u32,
) -> Result<PathBuf> {
    installs
        .iter()
        .find(|j| j.major == major)
        .map(|j| j.path.clone())
        .ok_or(err)
}

/// A Java found on this computer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaInstall {
    pub path: PathBuf,
    pub major: u32,
}

/// Folders under Program Files that hold one folder per installed Java.
const WINDOWS_VENDORS: &[&str] = &[
    "Java",
    "Eclipse Adoptium",
    "Microsoft",
    "Zulu",
    "BellSoft",
    "Amazon Corretto",
    "Semeru",
    "Eclipse Foundation",
];

/// Java executables that exist on this computer: `JAVA_HOME`, then `PATH`, then the usual
/// install folders. Nothing is run here; each path appears once.
pub fn candidate_paths(platform: Platform, env: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    let exe = match platform.os {
        Os::Windows => "javaw.exe",
        _ => "java",
    };
    let mut out = Vec::new();
    if let Some(home) = env("JAVA_HOME") {
        out.push(PathBuf::from(home).join("bin").join(exe));
    }
    if let Some(path) = env("PATH") {
        out.extend(std::env::split_paths(&path).map(|dir| dir.join(exe)));
    }
    let mut roots: Vec<PathBuf> = Vec::new();
    match platform.os {
        Os::Windows => {
            for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
                if let Some(pf) = env(key) {
                    roots.extend(WINDOWS_VENDORS.iter().map(|v| PathBuf::from(&pf).join(v)));
                }
            }
            if let Some(profile) = env("USERPROFILE") {
                roots.push(PathBuf::from(profile).join(".jdks"));
            }
        }
        _ => {
            roots.extend(["/usr/lib/jvm", "/usr/lib64/jvm", "/opt/java"].map(PathBuf::from));
            if let Some(home) = env("HOME") {
                let home = PathBuf::from(home);
                roots.push(home.join(".jdks"));
                roots.push(home.join(".sdkman/candidates/java"));
            }
        }
    }
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        let mut homes: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        homes.sort();
        out.extend(homes.into_iter().map(|h| h.join("bin").join(exe)));
    }
    let mut seen = HashSet::new();
    out.into_iter()
        .filter(|p| p.is_file() && seen.insert(p.clone()))
        .collect()
}

/// Every working Java on this computer, newest major version first. Each candidate is run once
/// (`-version`), at most four at a time.
pub async fn detect_installed(platform: Platform) -> Vec<JavaInstall> {
    let candidates = tokio::task::spawn_blocking(move || {
        candidate_paths(platform, &|key| std::env::var_os(key))
    })
    .await
    .unwrap_or_default();
    let mut found: Vec<JavaInstall> = futures_util::stream::iter(candidates)
        .map(|path| async move {
            probe(&path)
                .await
                .ok()
                .map(|major| JavaInstall { path, major })
        })
        .buffered(4)
        .filter_map(|j| async move { j })
        .collect()
        .await;
    found.sort_by_key(|j| std::cmp::Reverse(j.major));
    found
}

/// Mojang's runtime starts the game with `javaw.exe` (no console). Installer tools run better
/// with the `java.exe` next to it, whose output is captured the normal way.
pub fn console_java(java: &Path) -> PathBuf {
    let is_javaw = java
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case("javaw.exe"));
    if is_javaw {
        let sibling = java.with_file_name("java.exe");
        if sibling.is_file() {
            return sibling;
        }
    }
    java.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::Downloader,
        error::LauncherError,
        meta::MetaClient,
        test_support::{fixture, version, WINDOWS_X64},
        ErrorCategory,
    };

    #[test]
    fn picks_component_from_version() {
        assert_eq!(component_for(&version("1.20.1.json")), "java-runtime-gamma");
        assert_eq!(component_for(&version("1.12.2.json")), "jre-legacy");
    }

    #[test]
    fn selects_runtime_for_platform() {
        let index: RuntimeIndex = serde_json::from_str(&fixture("java-all.json")).unwrap();
        let entry = select_runtime(&index, WINDOWS_X64, "java-runtime-gamma").unwrap();
        assert_eq!(entry.version.name, "17.0.15");
        assert_eq!(
            entry.manifest.sha1,
            "89ce85ccb518c62e18b4b58d63399ba2d9611426"
        );
        let err = select_runtime(&index, WINDOWS_X64, "java-runtime-nope").unwrap_err();
        assert_eq!(err.category, ErrorCategory::Java);
        assert!(err.fix.is_some());
    }

    #[test]
    fn plans_runtime_files() {
        let manifest: RuntimeManifest =
            serde_json::from_str(&fixture("jre17-windows-x64.json")).unwrap();
        let dest = Path::new("/j/java-runtime-gamma");
        let plan = plan_runtime(&manifest, dest).unwrap();
        assert_eq!(plan.downloads.len(), 403);
        let javaw = plan
            .downloads
            .iter()
            .find(|d| d.dest == dest.join("bin/javaw.exe"))
            .unwrap();
        assert_eq!(
            javaw.hashes.sha1.as_deref(),
            Some("503664a377eb2e63a5f74b273299f622fca2bf95")
        );
        assert_eq!(javaw.size, Some(39424));
        assert!(plan.executables.contains(&dest.join("bin/javaw.exe")));
        assert!(plan.links.is_empty());
    }

    #[test]
    fn rejects_manifest_paths_that_escape_the_install_dir() {
        let manifest: RuntimeManifest = serde_json::from_str(
            r#"{"files":{"../evil.dll":{"type":"file","executable":false,"downloads":{"raw":{"sha1":"00","size":1,"url":"https://example.invalid/x"}}}}}"#,
        )
        .unwrap();
        let err = plan_runtime(&manifest, Path::new("/j/x")).err().unwrap();
        assert_eq!(err.category, ErrorCategory::Integrity);
        let link: RuntimeManifest =
            serde_json::from_str(r#"{"files":{"a/../../l":{"type":"link","target":"x"}}}"#)
                .unwrap();
        assert!(plan_runtime(&link, Path::new("/j/x")).is_err());
    }

    #[test]
    fn executable_path_per_os() {
        assert_eq!(
            java_executable(Path::new("/j/x"), WINDOWS_X64),
            Path::new("/j/x/bin/javaw.exe")
        );
        assert_eq!(
            java_executable(Path::new("/j/x"), crate::test_support::LINUX_X64),
            Path::new("/j/x/bin/java")
        );
    }

    #[test]
    fn parses_java_version_output() {
        assert_eq!(
            parse_java_version("openjdk version \"17.0.15\" 2025-04-15"),
            Some(17)
        );
        assert_eq!(
            parse_java_version("java version \"1.8.0_202\"\nJava(TM) SE Runtime"),
            Some(8)
        );
        assert_eq!(
            parse_java_version("openjdk version \"25.0.3\" 2026-04-21 LTS"),
            Some(25)
        );
        assert_eq!(parse_java_version("not java"), None);
    }

    #[tokio::test]
    async fn probe_rejects_a_missing_java() {
        let tmp = tempfile::tempdir().unwrap();
        let err = probe(&tmp.path().join("nope")).await.unwrap_err();
        assert_eq!(err.code, crate::ErrorCode::JavaTestFailed);
    }

    #[tokio::test]
    async fn probe_reads_the_java_on_path() {
        // Skipped where no java is installed.
        if std::process::Command::new("java")
            .arg("-version")
            .output()
            .is_err()
        {
            return;
        }
        let major = probe(std::path::Path::new("java")).await.unwrap();
        assert!(major >= 8, "{major}");
    }

    #[tokio::test]
    async fn unsafe_runtime_component_is_refused_before_any_download() {
        let tmp = tempfile::tempdir().unwrap();
        let meta = MetaClient::new(reqwest::Client::new(), tmp.path().join("meta"));
        let downloader = Downloader::new(reqwest::Client::new());
        for bad in ["../evil", "a/b", "", "..", "C:x"] {
            let err = ensure_runtime(
                &meta,
                &downloader,
                tmp.path(),
                WINDOWS_X64,
                bad,
                &crate::events::noop_sink(),
            )
            .await
            .unwrap_err();
            assert_eq!(err.code, crate::ErrorCode::UnsafeGameData, "{bad:?}");
        }
    }

    fn touch(root: &Path, rel: &str) -> PathBuf {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        p
    }

    #[test]
    fn finds_java_in_java_home_path_and_vendor_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let adoptium = touch(
            root,
            "pf/Eclipse Adoptium/jdk-17.0.9.9-hotspot/bin/javaw.exe",
        );
        let home = touch(root, "home-java/bin/javaw.exe");
        let on_path = touch(root, "tools/bin/javaw.exe");
        touch(root, "pf/Eclipse Adoptium/not-a-jdk/readme.txt");
        let path_var =
            std::env::join_paths([root.join("tools/bin"), root.join("missing")]).unwrap();
        let env = |key: &str| match key {
            "ProgramFiles" => Some(root.join("pf").into_os_string()),
            "JAVA_HOME" => Some(root.join("home-java").into_os_string()),
            "PATH" => Some(path_var.clone()),
            _ => None,
        };
        assert_eq!(
            candidate_paths(WINDOWS_X64, &env),
            vec![home, on_path, adoptium]
        );
    }

    #[test]
    fn the_same_java_is_listed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let java = touch(tmp.path(), "jdk/bin/java");
        let path_var = std::env::join_paths([tmp.path().join("jdk/bin")]).unwrap();
        let env = |key: &str| match key {
            "JAVA_HOME" => Some(tmp.path().join("jdk").into_os_string()),
            "PATH" => Some(path_var.clone()),
            _ => None,
        };
        let found: Vec<PathBuf> = candidate_paths(crate::test_support::LINUX_X64, &env)
            .into_iter()
            .filter(|p| p.starts_with(tmp.path()))
            .collect();
        assert_eq!(found, vec![java]);
    }

    #[test]
    fn installed_java_is_used_only_for_the_exact_major_version() {
        let installs = vec![
            JavaInstall {
                path: "/j21/bin/java".into(),
                major: 21,
            },
            JavaInstall {
                path: "/j17/bin/java".into(),
                major: 17,
            },
        ];
        let unavailable = || LauncherError::java(ErrorCode::JavaUnavailable, "none");
        assert_eq!(
            installed_fallback(unavailable(), &installs, 17).unwrap(),
            PathBuf::from("/j17/bin/java")
        );
        assert_eq!(
            installed_fallback(unavailable(), &installs, 8)
                .unwrap_err()
                .code,
            ErrorCode::JavaUnavailable
        );
    }

    #[test]
    fn required_major_comes_from_the_version() {
        assert_eq!(required_major(&version("1.20.1.json")), 17);
        assert_eq!(required_major(&version("1.12.2.json")), 8);
    }

    #[test]
    fn console_java_prefers_java_exe_next_to_javaw() {
        let tmp = tempfile::tempdir().unwrap();
        let javaw = touch(tmp.path(), "bin/javaw.exe");
        assert_eq!(console_java(&javaw), javaw, "no java.exe yet");
        let java = touch(tmp.path(), "bin/java.exe");
        assert_eq!(console_java(&javaw), java);
        let linux = tmp.path().join("bin/java");
        assert_eq!(console_java(&linux), linux);
    }
}
