pub mod forge;
pub mod natives;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use crate::{
    download::{DownloadRequest, Downloader, Hashes},
    error::{ErrorCategory, ErrorCode, LauncherError, Result},
    events::{EventSink, TaskId},
    fsutil::{safe_join, write_atomic},
    meta::{
        forge::{InstallProfile, OFFICIAL_REPOS},
        loaders::{self, LoaderKind},
        maven::MavenCoord,
        merge::merge,
        mojang::{
            AssetIndex, Library, VersionJson, VersionManifest, RESOURCES_URL, VERSION_MANIFEST_URL,
        },
        parse_json,
        rules::{library_applies, RuleContext},
        MetaClient,
    },
    paths::LauncherDirs,
    platform::{Arch, Platform},
};

const MOJANG_LIBRARIES: &str = "https://libraries.minecraft.net/";

/// What a Forge or NeoForge version adds on top of its merged JSON: the installer and its profile.
#[derive(Debug, Clone)]
pub struct ForgeInstall {
    pub kind: LoaderKind,
    pub loader_version: String,
    /// The installer jar, kept in the libraries folder (its maven path).
    pub installer: PathBuf,
    pub installer_sha1: String,
    pub profile: InstallProfile,
}

/// The version to launch: `json` is vanilla, or vanilla merged with a loader profile.
#[derive(Debug, Clone)]
pub struct ResolvedVersion {
    pub vanilla_id: String,
    pub json: VersionJson,
    /// Forge and NeoForge: the installer steps `install` also runs. `None` for vanilla, Fabric, Quilt.
    pub forge: Option<ForgeInstall>,
}

impl ResolvedVersion {
    /// The game jar on the classpath. Forge and NeoForge tell their launcher to ignore
    /// `${version_name}.jar`, so their copy is named after the merged id, the way the official
    /// launcher lays it out; everyone else uses the vanilla jar where it was downloaded.
    pub fn client_jar(&self, dirs: &LauncherDirs) -> PathBuf {
        match self.forge {
            Some(_) => dirs.client_jar(&self.json.id),
            None => dirs.client_jar(&self.vanilla_id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NativeJar {
    pub jar: PathBuf,
    pub exclude: Vec<String>,
}

#[derive(Debug, Default)]
pub struct LibraryPlan {
    pub downloads: Vec<DownloadRequest>,
    pub natives: Vec<NativeJar>,
    /// Libraries with no download address: the loader installer builds or bundles them.
    pub local: Vec<DownloadRequest>,
}

/// A library with no download address: a loader installer provides it, so it is never fetched.
pub fn is_local(req: &DownloadRequest) -> bool {
    req.urls.iter().all(|u| u.trim().is_empty())
}

/// Join a path taken from remote metadata onto `base`, refusing anything that could escape it.
pub(crate) fn join_remote(base: &Path, rel: &str) -> Result<PathBuf> {
    safe_join(base, rel).ok_or_else(|| {
        LauncherError::integrity(
            ErrorCode::UnsafeGameData,
            "Minecraft's file list contains an unsafe path",
        )
        .with_detail(rel)
    })
}

/// A version id becomes a folder name under the data root, so it must be a single plain path segment.
pub(crate) fn check_version_id(id: &str) -> Result<()> {
    let unsafe_id = id.is_empty() || id == "." || id == ".." || id.contains(['/', '\\', '\0', ':']);
    if unsafe_id {
        return Err(LauncherError::integrity(
            ErrorCode::UnsafeGameData,
            "Minecraft's version data contains an unsafe version name",
        )
        .with_detail(id));
    }
    Ok(())
}

/// Where a library's main jar comes from and goes to. `Ok(None)` for natives-only libraries.
pub fn library_artifact(lib: &Library, dirs: &LauncherDirs) -> Result<Option<DownloadRequest>> {
    let libraries = dirs.libraries();
    if let Some(downloads) = &lib.downloads {
        let Some(art) = downloads.artifact.as_ref() else {
            return Ok(None);
        };
        let rel = match &art.path {
            Some(path) => path.clone(),
            None => maven_path(&lib.name)?,
        };
        return Ok(Some(DownloadRequest::new(
            art.url.clone(),
            join_remote(&libraries, &rel)?,
            Hashes::sha1(&art.sha1),
            Some(art.size),
        )));
    }
    let rel = maven_path(&lib.name)?;
    let base = lib.url.as_deref().unwrap_or(MOJANG_LIBRARIES);
    let hashes = Hashes {
        sha1: lib.sha1.clone(),
        sha256: lib.sha256.clone(),
        sha512: lib.sha512.clone(),
        murmur2: None,
    };
    Ok(Some(DownloadRequest::new(
        format!("{}/{}", base.trim_end_matches('/'), rel),
        join_remote(&libraries, &rel)?,
        hashes,
        lib.size,
    )))
}

/// Some loader profiles (Quilt v3, older Fabric) publish libraries without hashes. Give each such
/// request the sha1 from the Maven `<url>.sha1` file next to it, so every jar is still verified.
/// The lookups go through the metadata cache, so they also work offline after the first install.
pub async fn fill_missing_sha1(meta: &MetaClient, reqs: &mut [DownloadRequest]) -> Result<()> {
    let lookups = reqs
        .iter_mut()
        .filter(|r| r.hashes.is_empty())
        .map(|req| async move {
            let url = req.urls.first().cloned().unwrap_or_default();
            req.hashes.sha1 = Some(published_sha1(meta, &url).await?);
            Ok::<_, LauncherError>(())
        });
    futures_util::future::try_join_all(lookups).await?;
    Ok(())
}

pub(crate) async fn published_sha1(meta: &MetaClient, url: &str) -> Result<String> {
    let unverifiable = || {
        LauncherError::integrity(
            ErrorCode::LibraryUnverifiable,
            "Couldn't verify a library download",
        )
        .with_fix("Try again later")
        .with_detail(url)
    };
    let body = match meta.get_bytes(&format!("{url}.sha1")).await {
        Ok(body) => body,
        Err(e) if e.category == ErrorCategory::Network => return Err(e),
        Err(_) => return Err(unverifiable()),
    };
    let token = std::str::from_utf8(&body)
        .ok()
        .and_then(|text| text.split_whitespace().next())
        .filter(|t| t.len() == 40 && t.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(unverifiable)?;
    Ok(token.to_ascii_lowercase())
}

fn maven_path(name: &str) -> Result<String> {
    MavenCoord::parse(name).map(|c| c.path()).ok_or_else(|| {
        LauncherError::integrity(
            ErrorCode::UnsafeGameData,
            "Minecraft's file list contains an unsafe path",
        )
        .with_detail(format!("library {name}"))
    })
}

fn native_classifier(lib: &Library, platform: Platform) -> Option<String> {
    let key = lib.natives.as_ref()?.get(platform.mojang_os_name())?;
    Some(key.replace(
        "${arch}",
        if platform.arch == Arch::X86 {
            "32"
        } else {
            "64"
        },
    ))
}

pub fn plan_libraries(
    version: &VersionJson,
    dirs: &LauncherDirs,
    ctx: &RuleContext,
) -> Result<LibraryPlan> {
    let mut plan = LibraryPlan::default();
    for lib in version.libraries.iter().filter(|l| library_applies(l, ctx)) {
        if let Some(d) = library_artifact(lib, dirs)? {
            if is_local(&d) {
                plan.local.push(d);
            } else {
                plan.downloads.push(d);
            }
        }
        let Some(classifier) = native_classifier(lib, ctx.platform) else {
            continue;
        };
        let Some(art) = lib
            .downloads
            .as_ref()
            .and_then(|d| d.classifiers.as_ref())
            .and_then(|c| c.get(&classifier))
        else {
            continue;
        };
        let rel = match &art.path {
            Some(path) => Some(path.clone()),
            None => MavenCoord::parse(&format!("{}:{classifier}", lib.name)).map(|c| c.path()),
        };
        if let Some(rel) = rel {
            let jar = join_remote(&dirs.libraries(), &rel)?;
            plan.downloads.push(DownloadRequest::new(
                art.url.clone(),
                jar.clone(),
                Hashes::sha1(&art.sha1),
                Some(art.size),
            ));
            plan.natives.push(NativeJar {
                jar,
                exclude: lib
                    .extract
                    .as_ref()
                    .map(|e| e.exclude.clone())
                    .unwrap_or_default(),
            });
        }
    }
    Ok(plan)
}

pub fn classpath(
    resolved: &ResolvedVersion,
    dirs: &LauncherDirs,
    ctx: &RuleContext,
) -> Result<Vec<PathBuf>> {
    let mut seen = HashSet::new();
    let mut cp = Vec::new();
    for lib in resolved
        .json
        .libraries
        .iter()
        .filter(|l| library_applies(l, ctx))
    {
        if let Some(d) = library_artifact(lib, dirs)? {
            if seen.insert(d.dest.clone()) {
                cp.push(d.dest);
            }
        }
    }
    cp.push(resolved.client_jar(dirs));
    Ok(cp)
}

pub fn plan_client(resolved: &ResolvedVersion, dirs: &LauncherDirs) -> Result<DownloadRequest> {
    let client = resolved
        .json
        .downloads
        .as_ref()
        .map(|d| &d.client)
        .ok_or_else(|| {
            LauncherError::loader(
                ErrorCode::NoClientDownload,
                format!("Minecraft {} has no client download", resolved.vanilla_id),
            )
            .with_param("version", &resolved.vanilla_id)
        })?;
    Ok(DownloadRequest::new(
        client.url.clone(),
        dirs.client_jar(&resolved.vanilla_id),
        Hashes::sha1(&client.sha1),
        Some(client.size),
    ))
}

/// Unique objects with well-formed hashes (exactly 40 lowercase hex characters); anything else is skipped.
pub fn plan_assets(index: &AssetIndex, dirs: &LauncherDirs) -> Vec<DownloadRequest> {
    let mut seen = HashSet::new();
    index
        .objects
        .values()
        .filter(|o| {
            o.hash.len() == 40
                && o.hash
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
        .filter(|o| seen.insert(o.hash.clone()))
        .map(|o| {
            let prefix = &o.hash[..2];
            DownloadRequest::new(
                format!("{RESOURCES_URL}/{prefix}/{}", o.hash),
                dirs.assets().join("objects").join(prefix).join(&o.hash),
                Hashes::sha1(&o.hash),
                Some(o.size),
            )
        })
        .collect()
}

pub fn plan_logging(version: &VersionJson, dirs: &LauncherDirs) -> Result<Option<DownloadRequest>> {
    let Some(file) = version
        .logging
        .as_ref()
        .and_then(|l| l.client.as_ref())
        .map(|c| &c.file)
    else {
        return Ok(None);
    };
    let dest = join_remote(&dirs.assets().join("log_configs"), &file.id)?;
    Ok(Some(DownloadRequest::new(
        file.url.clone(),
        dest,
        Hashes::sha1(&file.sha1),
        Some(file.size),
    )))
}

pub struct Installer {
    pub dirs: LauncherDirs,
    pub meta: MetaClient,
    pub downloader: Downloader,
    pub platform: Platform,
}

impl Installer {
    /// Look up the vanilla version (and loader profile) and merge them. Works offline once cached.
    pub async fn resolve(
        &self,
        mc: &str,
        loader: Option<(LoaderKind, &str)>,
    ) -> Result<ResolvedVersion> {
        let manifest: VersionManifest = self.meta.get_json(VERSION_MANIFEST_URL).await?;
        let entry = manifest.find(mc).ok_or_else(|| {
            LauncherError::loader(
                ErrorCode::MinecraftVersionNotFound,
                format!("Minecraft {mc} doesn't exist"),
            )
            .with_param("version", mc)
            .with_fix("Check the version number")
        })?;
        let vanilla: VersionJson = self.meta.get_verified_json(&entry.url, &entry.sha1).await?;
        if vanilla.id != mc {
            return Err(LauncherError::integrity(
                ErrorCode::MojangMismatch,
                "Mojang returned a different version than requested",
            )
            .with_detail(format!("requested {mc}, got {}", vanilla.id)));
        }
        check_version_id(&vanilla.id)?;
        let (json, forge) = match loader {
            Some((kind @ (LoaderKind::Forge | LoaderKind::NeoForge), version)) => {
                let (installer, sha1) = forge::fetch_installer(
                    &self.meta,
                    &self.downloader,
                    &self.dirs,
                    kind,
                    mc,
                    version,
                    OFFICIAL_REPOS,
                )
                .await?;
                let (json, install) =
                    forge::resolve_forge(vanilla.clone(), kind, mc, version, installer, sha1)
                        .await?;
                (json, Some(install))
            }
            Some((kind, version)) => (
                merge(
                    vanilla.clone(),
                    loaders::fetch_profile(&self.meta, kind, mc, version).await?,
                ),
                None,
            ),
            None => (vanilla.clone(), None),
        };
        check_version_id(&json.id)?;
        Ok(ResolvedVersion {
            vanilla_id: vanilla.id,
            json,
            forge,
        })
    }

    /// Download everything the version needs, extract natives, and save the merged JSON for offline launches.
    pub async fn install(&self, resolved: &ResolvedVersion, sink: &EventSink) -> Result<()> {
        let ctx = RuleContext::new(self.platform);
        let json = &resolved.json;
        check_version_id(&resolved.vanilla_id)?;
        check_version_id(&json.id)?;
        let index_ref = json.asset_index.as_ref().ok_or_else(|| {
            LauncherError::loader(
                ErrorCode::NoAssetIndex,
                format!("{} has no asset index", json.id),
            )
            .with_param("version", &json.id)
        })?;
        let index_path = join_remote(
            &self.dirs.assets().join("indexes"),
            &format!("{}.json", index_ref.id),
        )?;
        self.downloader
            .fetch(&DownloadRequest::new(
                index_ref.url.clone(),
                index_path.clone(),
                Hashes::sha1(&index_ref.sha1),
                Some(index_ref.size),
            ))
            .await?;
        let index: AssetIndex = parse_json(&index_ref.url, &tokio::fs::read(&index_path).await?)?;

        let mut libraries = plan_libraries(json, &self.dirs, &ctx)?;
        fill_missing_sha1(&self.meta, &mut libraries.downloads).await?;
        let mut downloads = vec![plan_client(resolved, &self.dirs)?];
        downloads.extend(libraries.downloads);
        downloads.extend(plan_assets(&index, &self.dirs));
        downloads.extend(plan_logging(json, &self.dirs)?);
        self.downloader
            .fetch_all(
                downloads,
                &TaskId::new("install_minecraft").with("version", &resolved.vanilla_id),
                sink,
            )
            .await?;
        if let Some(install) = &resolved.forge {
            forge::install(self, resolved, install, &libraries.local, sink).await?;
        }

        let natives_dir = self.dirs.natives_dir(&json.id);
        let native_jars = libraries.natives;
        tokio::task::spawn_blocking(move || natives::extract_all(&native_jars, &natives_dir))
            .await
            .map_err(|e| {
                LauncherError::disk(
                    ErrorCode::NativesExtractFailed,
                    "Couldn't extract native libraries",
                )
                .with_detail(e)
            })??;
        let bytes = serde_json::to_vec_pretty(json).map_err(|e| {
            LauncherError::disk(ErrorCode::DiskWriteFailed, "Couldn't save the version file")
                .with_param("item", "version")
                .with_detail(e)
        })?;
        write_atomic(&self.dirs.version_json(&json.id), &bytes).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        meta::{merge::merge, mojang::AssetIndex},
        test_support::{fixture, version, WINDOWS_X64},
    };

    fn dirs() -> LauncherDirs {
        LauncherDirs::new("/pl")
    }

    #[test]
    fn modern_libraries_have_no_extraction() {
        let plan = plan_libraries(
            &version("1.20.1.json"),
            &dirs(),
            &RuleContext::new(WINDOWS_X64),
        )
        .unwrap();
        assert_eq!(plan.downloads.len(), 50);
        assert!(plan.natives.is_empty());
        let names: Vec<String> = plan
            .downloads
            .iter()
            .map(|d| d.dest.to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(names
            .iter()
            .any(|n| n.ends_with("org/lwjgl/lwjgl/3.3.1/lwjgl-3.3.1-natives-windows.jar")));
        assert!(!names.iter().any(|n| n.ends_with("natives-windows-x86.jar")));
    }

    #[test]
    fn legacy_natives_are_downloaded_and_extracted() {
        let plan = plan_libraries(
            &version("1.12.2.json"),
            &dirs(),
            &RuleContext::new(WINDOWS_X64),
        )
        .unwrap();
        assert_eq!(plan.downloads.len(), 36);
        assert_eq!(plan.natives.len(), 3);
        let jar = plan.natives[0].jar.to_string_lossy().replace('\\', "/");
        assert!(jar.contains("-natives-windows"), "{jar}");
    }

    /// Quilt v3 (and older Fabric) profiles list libraries as just `name` + `url`, with no hashes,
    /// which is why `fill_missing_sha1` looks up the Maven `.sha1` before downloading.
    #[test]
    fn quilt_style_library_has_no_published_hash() {
        let profile = r#"{"libraries":[{"name":"org.quiltmc:quilt-loader:0.29.1","url":"https://maven.quiltmc.org/repository/release/"}]}"#;
        let libs: serde_json::Value = serde_json::from_str(profile).unwrap();
        let lib: Library = serde_json::from_value(libs["libraries"][0].clone()).unwrap();
        let d = library_artifact(&lib, &dirs()).unwrap().unwrap();
        assert_eq!(
            d.urls[0],
            "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-loader/0.29.1/quilt-loader-0.29.1.jar"
        );
        assert!(d.hashes.is_empty());
        assert_eq!(d.size, None);
    }

    #[test]
    fn fabric_library_resolves_through_maven_url() {
        let lib = &version("fabric-1.20.1-0.16.14.json").libraries[0];
        let d = library_artifact(lib, &dirs()).unwrap().unwrap();
        assert_eq!(
            d.urls[0],
            "https://maven.fabricmc.net/org/ow2/asm/asm/9.8/asm-9.8.jar"
        );
        assert_eq!(
            d.hashes.sha1.as_deref(),
            Some("dc19ecb3f7889b7860697215cae99c0f9b6f6b4b")
        );
        assert_eq!(d.size, Some(126113));
        assert_eq!(
            d.dest,
            dirs().libraries().join("org/ow2/asm/asm/9.8/asm-9.8.jar")
        );
    }

    #[test]
    fn client_jar_uses_vanilla_id() {
        let resolved = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: merge(
                version("1.20.1.json"),
                version("fabric-1.20.1-0.16.14.json"),
            ),
            forge: None,
        };
        let client = plan_client(&resolved, &dirs()).unwrap();
        assert_eq!(client.dest, dirs().client_jar("1.20.1"));
        assert_eq!(
            client.hashes.sha1.as_deref(),
            Some("0c3ec587af28e5a785c0b4a7b8a30f9a8f78f838")
        );
        let cp = classpath(&resolved, &dirs(), &RuleContext::new(WINDOWS_X64)).unwrap();
        assert!(cp[0]
            .to_string_lossy()
            .replace('\\', "/")
            .ends_with("asm-9.8.jar"));
        assert_eq!(cp.last().unwrap(), &dirs().client_jar("1.20.1"));
        assert_eq!(cp.len(), 8 + 50 + 1);
    }

    #[test]
    fn plan_assets_dedupes_hashes() {
        let index: AssetIndex = serde_json::from_str(&fixture("asset-index-5.json")).unwrap();
        let plan = plan_assets(&index, &dirs());
        assert_eq!(plan.len(), 3575);
        let icon = plan
            .iter()
            .find(|d| d.hashes.sha1.as_deref() == Some("b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"))
            .unwrap();
        assert_eq!(
            icon.urls[0],
            "https://resources.download.minecraft.net/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
        assert_eq!(
            icon.dest,
            dirs()
                .assets()
                .join("objects/b6/b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356")
        );
    }

    #[test]
    fn logging_config_goes_to_log_configs() {
        let d = plan_logging(&version("1.20.1.json"), &dirs())
            .unwrap()
            .unwrap();
        assert_eq!(d.dest, dirs().log_config("client-1.12.xml"));
    }

    #[test]
    fn version_ids_must_be_a_single_plain_segment() {
        for ok in ["1.20.1", "26.4-snapshot-3", "fabric-loader-0.16.14-1.20.1"] {
            assert!(check_version_id(ok).is_ok(), "{ok}");
        }
        for bad in [
            "", ".", "..", "../x", "a/b", "a\\b", "/tmp/x", "C:\\x", "a:b", "a\0b",
        ] {
            let err = check_version_id(bad).unwrap_err();
            assert_eq!(err.category, crate::ErrorCategory::Integrity, "{bad:?}");
        }
    }

    #[test]
    fn traversing_library_path_is_an_integrity_error() {
        let mut v = version("1.20.1.json");
        let ctx = RuleContext::new(WINDOWS_X64);
        let lib = v
            .libraries
            .iter_mut()
            .find(|l| library_applies(l, &ctx))
            .unwrap();
        lib.downloads
            .as_mut()
            .unwrap()
            .artifact
            .as_mut()
            .unwrap()
            .path = Some("../../evil.jar".into());
        let err = plan_libraries(&v, &dirs(), &RuleContext::new(WINDOWS_X64)).unwrap_err();
        assert_eq!(err.category, crate::ErrorCategory::Integrity);
    }

    #[test]
    fn libraries_without_a_url_are_local_not_downloaded() {
        let json = merge(
            version("1.12.2.json"),
            version("forge-1.12.2-14.23.5.2860-version.json"),
        );
        let ctx = RuleContext::new(WINDOWS_X64);
        let plan = plan_libraries(&json, &dirs(), &ctx).unwrap();
        assert_eq!(plan.local.len(), 1);
        let local = plan.local[0].dest.to_string_lossy().replace('\\', "/");
        assert!(
            local.ends_with(
                "net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860.jar"
            ),
            "{local}"
        );
        assert!(plan
            .downloads
            .iter()
            .all(|d| d.urls.iter().all(|u| !u.is_empty())));
        // The jar the installer provides is still on the classpath.
        let resolved = ResolvedVersion {
            vanilla_id: "1.12.2".into(),
            json,
            forge: None,
        };
        let cp = classpath(&resolved, &dirs(), &ctx).unwrap();
        assert!(cp.contains(&plan.local[0].dest));
    }

    #[test]
    fn non_hex_asset_hashes_are_not_planned() {
        let evil = format!("{:.<40}", "../");
        assert_eq!(evil.len(), 40);
        let json = format!(
            r#"{{"objects":{{"a":{{"hash":"{evil}","size":1}},"b":{{"hash":"{}","size":1}},"c":{{"hash":"{}","size":1}}}}}}"#,
            "B62CA8EC10D07E6BF5AC8DAE0C8C1D2E6A1E3356", "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
        let index: AssetIndex = serde_json::from_str(&json).unwrap();
        let plan = plan_assets(&index, &dirs());
        assert_eq!(plan.len(), 1);
        assert_eq!(
            plan[0].hashes.sha1.as_deref(),
            Some("b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356")
        );
    }

    #[test]
    fn forge_classpath_names_the_game_jar_after_the_forge_version() {
        let resolved = crate::test_support::forge_resolved();
        let cp = classpath(&resolved, &dirs(), &RuleContext::new(WINDOWS_X64)).unwrap();
        assert_eq!(
            cp.last().unwrap(),
            &dirs().client_jar("1.20.1-forge-47.4.22")
        );
        assert_eq!(cp.len(), 29 + 50 + 1);
        assert_eq!(
            resolved.client_jar(&dirs()),
            dirs().client_jar("1.20.1-forge-47.4.22")
        );
        let vanilla = ResolvedVersion {
            vanilla_id: "1.20.1".into(),
            json: version("1.20.1.json"),
            forge: None,
        };
        assert_eq!(vanilla.client_jar(&dirs()), dirs().client_jar("1.20.1"));
    }
}
