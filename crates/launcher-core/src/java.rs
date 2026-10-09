//! Mojang's official Java runtimes: the same builds the vanilla launcher uses.
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
};

use serde::Deserialize;

use crate::{
    download::{DownloadRequest, Downloader, Hashes},
    error::{LauncherError, Result},
    events::EventSink,
    fsutil::{safe_join, write_atomic},
    meta::{
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
            LauncherError::java(format!(
                "There's no official Java ({component}) for this computer"
            ))
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
            LauncherError::integrity("The Java download list contains an unsafe path")
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
            &format!("Downloading Java {}", entry.version.name),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
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
}
