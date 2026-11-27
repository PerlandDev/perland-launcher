//! Keeps the game off a FreeType with the "minecraft2shell" hole (FreeType before 2.15.0: a font
//! in a resource pack can run code on the player's computer).
//!
//! Minecraft has drawn text with FreeType since 1.20.5 (24w03a), through `org.lwjgl:lwjgl-freetype`.
//! Mojang republished 26.1 and later with a patched library; on 2026-11-26, 1.20.5 to 1.21.11
//! still listed the old one. For any version that does, the FreeType native jars are swapped for
//! Mojang's patched build. The Java half of the library stays: it finds FreeType's functions by
//! name, and the patched build exports every one the old build did.

use crate::{
    download::Downloader,
    error::Result,
    install::plan_libraries,
    meta::{
        maven::MavenCoord,
        mojang::{Artifact, Library, LibraryDownloads, VersionJson},
        rules::RuleContext,
    },
    paths::LauncherDirs,
    platform::Platform,
};

const GROUP: &str = "org.lwjgl";
const ARTIFACT: &str = "lwjgl-freetype";
/// The newest plain LWJGL release whose FreeType has the hole. Later ones are left alone.
const LAST_VULNERABLE_LWJGL: [u32; 3] = [3, 4, 3];
/// The first FreeType without the hole.
const FIXED_FREETYPE: [u32; 3] = [2, 15, 0];

/// One patched native jar: its Maven classifier, and the checksum and size it must have.
#[derive(Debug, Clone, Copy)]
pub struct PatchedNative<'a> {
    pub classifier: &'a str,
    pub sha1: &'a str,
    pub size: u64,
}

/// A build of `lwjgl-freetype` whose native jars replace the vulnerable ones.
#[derive(Debug, Clone, Copy)]
pub struct PatchedNatives<'a> {
    pub version: &'a str,
    /// Maven repository base the jars are downloaded from.
    pub libraries_url: &'a str,
    pub natives: &'a [PatchedNative<'a>],
}

/// Mojang's FreeType 2.15.0 natives, as Minecraft 26.2's own version file lists them
/// (republished 2026-11-25). They cover every computer the vulnerable builds had a native for.
pub const MOJANG: PatchedNatives<'static> = PatchedNatives {
    version: "3.4.1-mojang-2.15.0.3",
    libraries_url: "https://libraries.minecraft.net",
    natives: &[
        PatchedNative {
            classifier: "natives-linux",
            sha1: "0a6df23d807a883883eda82941b7d0dc31813259",
            size: 1423205,
        },
        PatchedNative {
            classifier: "natives-macos",
            sha1: "5491169c04d6fd9088d7c0e1efa58820ad26d303",
            size: 1225021,
        },
        PatchedNative {
            classifier: "natives-macos-arm64",
            sha1: "8f1c52741cbb1ba5f5a9af7ec122dea7e3f68946",
            size: 1125234,
        },
        PatchedNative {
            classifier: "natives-windows",
            sha1: "c9d78fd1df6e7ff7eff4db0126786755a6a19cd9",
            size: 1190827,
        },
        PatchedNative {
            classifier: "natives-windows-arm64",
            sha1: "c97ff519f79011208e5fb8be9bed7743d3b6da51",
            size: 1043882,
        },
        PatchedNative {
            classifier: "natives-windows-x86",
            sha1: "cae34fec6b4622218af1e3b14af90fef58df3013",
            size: 1000778,
        },
    ],
};

/// Whether this `lwjgl-freetype` version carries a FreeType with the hole. Mojang names its
/// rebuilt ones `<lwjgl>-mojang-<freetype>`, which says it outright; a plain version is an
/// upstream LWJGL release, vulnerable up to 3.4.3. Anything unreadable is left alone.
pub fn is_vulnerable(version: &str) -> bool {
    let (lwjgl, build) = match version.split_once('-') {
        Some((lwjgl, build)) => (lwjgl, Some(build)),
        None => (version, None),
    };
    if let Some(freetype) = build.and_then(|b| b.strip_prefix("mojang-")) {
        return numbers(freetype).is_some_and(|v| v.as_slice() < FIXED_FREETYPE.as_slice());
    }
    numbers(lwjgl).is_some_and(|v| v.as_slice() <= LAST_VULNERABLE_LWJGL.as_slice())
}

/// `2.15` as `[2, 15, 0]`: at least three numbers, so a short version compares like a full one.
fn numbers(version: &str) -> Option<Vec<u32>> {
    let mut parts = version
        .split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<u32>>>()?;
    if parts.len() < 3 {
        parts.resize(3, 0);
    }
    Some(parts)
}

/// `json` with every vulnerable FreeType native replaced by its patched one from `set`.
/// `None` when there is nothing to replace.
pub fn patched(json: &VersionJson, set: &PatchedNatives<'_>) -> Option<VersionJson> {
    let mut replaced = false;
    let mut libraries: Vec<Library> = Vec::with_capacity(json.libraries.len());
    for lib in &json.libraries {
        match replacement(lib, set) {
            Some(new) => {
                replaced = true;
                if !libraries.iter().any(|l| l.name == new.name) {
                    libraries.push(new);
                }
            }
            None => libraries.push(lib.clone()),
        }
    }
    replaced.then(|| VersionJson {
        libraries,
        ..json.clone()
    })
}

/// The patched native that takes `lib`'s place, when `lib` is a vulnerable FreeType native that
/// `set` has a build for. It keeps `lib`'s rules, so it applies to the same computers.
fn replacement(lib: &Library, set: &PatchedNatives<'_>) -> Option<Library> {
    let coord = MavenCoord::parse(&lib.name)?;
    if coord.group != GROUP || coord.artifact != ARTIFACT || !is_vulnerable(&coord.version) {
        return None;
    }
    let classifier = coord.classifier?;
    // 1.20.5 to 1.21.11 name their Intel Mac native `natives-macos-patch`.
    let classifier = classifier.strip_suffix("-patch").unwrap_or(&classifier);
    let native = set.natives.iter().find(|n| n.classifier == classifier)?;
    let name = format!("{GROUP}:{ARTIFACT}:{}:{}", set.version, native.classifier);
    let path = MavenCoord::parse(&name)?.path();
    Some(Library {
        name,
        downloads: Some(LibraryDownloads {
            artifact: Some(Artifact {
                url: format!("{}/{path}", set.libraries_url.trim_end_matches('/')),
                path: Some(path),
                sha1: native.sha1.to_string(),
                size: native.size,
            }),
            classifiers: None,
        }),
        url: None,
        sha1: None,
        sha256: None,
        sha512: None,
        size: None,
        rules: lib.rules.clone(),
        natives: None,
        extract: None,
    })
}

/// `json`, with a vulnerable FreeType swapped for the patched one once this computer's patched
/// native is on disk with the right checksum. When it can't be had (offline, the host is down),
/// `json` comes back unchanged: the game starts as it did before, and the next launch tries again.
pub async fn secure(
    json: VersionJson,
    set: &PatchedNatives<'_>,
    downloader: &Downloader,
    dirs: &LauncherDirs,
    platform: Platform,
) -> VersionJson {
    let Some(patched) = patched(&json, set) else {
        return json;
    };
    match fetch_natives(&json, &patched, downloader, dirs, platform).await {
        Ok(()) => patched,
        Err(error) => {
            tracing::warn!(
                version = %json.id,
                error = %error.message,
                "the patched FreeType couldn't be downloaded; starting with the one this version lists"
            );
            json
        }
    }
}

/// Download the natives `patched` added on top of `original`, for this computer only.
async fn fetch_natives(
    original: &VersionJson,
    patched: &VersionJson,
    downloader: &Downloader,
    dirs: &LauncherDirs,
    platform: Platform,
) -> Result<()> {
    let added = VersionJson {
        libraries: patched
            .libraries
            .iter()
            .filter(|l| !original.libraries.contains(l))
            .cloned()
            .collect(),
        ..patched.clone()
    };
    // One try: a launch must not wait out retries on a network that isn't answering.
    let downloader = downloader.clone().with_retries(0);
    for request in plan_libraries(&added, dirs, &RuleContext::new(platform))?.downloads {
        downloader.fetch(&request).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        install::plan_libraries,
        meta::rules::RuleContext,
        paths::LauncherDirs,
        platform::{Arch, Os, Platform},
        test_support::version,
    };

    fn names(json: &VersionJson) -> Vec<&str> {
        json.libraries.iter().map(|l| l.name.as_str()).collect()
    }

    #[test]
    fn plain_lwjgl_up_to_3_4_3_ships_the_vulnerable_freetype() {
        for v in ["3.3.2", "3.3.3", "3.4.1", "3.4.3"] {
            assert!(is_vulnerable(v), "{v}");
        }
    }

    #[test]
    fn a_mojang_build_is_judged_by_its_freetype() {
        assert!(!is_vulnerable("3.4.3-mojang-2.15.0"));
        assert!(!is_vulnerable("3.4.1-mojang-2.15.0.3"));
        assert!(!is_vulnerable("3.3.3-mojang-2.15.1"));
        assert!(!is_vulnerable("3.3.3-mojang-2.16"));
        assert!(!is_vulnerable("3.3.3-mojang-2.15"));
        assert!(is_vulnerable("3.4.3-mojang-2.14.3"));
    }

    #[test]
    fn a_later_lwjgl_or_an_unreadable_version_is_left_alone() {
        for v in ["3.4.4", "3.5.0", "4.0.0", "", "latest", "3.x"] {
            assert!(!is_vulnerable(v), "{v}");
        }
    }

    #[test]
    fn a_vulnerable_version_gets_the_patched_natives() {
        let patched = patched(&version("freetype-1.21.11.json"), &MOJANG).unwrap();
        assert_eq!(
            names(&patched),
            vec![
                "org.lwjgl:lwjgl-freetype:3.3.3",
                "org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-linux",
                "org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-macos-arm64",
                "org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-macos",
                "org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-windows",
                "org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-windows-arm64",
                "org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-windows-x86",
            ]
        );
    }

    #[test]
    fn the_java_half_and_everything_else_stay_as_they_were() {
        let before = version("freetype-1.21.11.json");
        let patched = patched(&before, &MOJANG).unwrap();
        assert_eq!(patched.libraries[0], before.libraries[0]);
        assert_eq!(patched.id, before.id);
        assert_eq!(patched.main_class, before.main_class);
    }

    #[test]
    fn a_patched_native_is_pinned_to_mojangs_file_and_checksum() {
        let patched = patched(&version("freetype-1.21.11.json"), &MOJANG).unwrap();
        let windows = &patched.libraries[4];
        let art = windows
            .downloads
            .as_ref()
            .unwrap()
            .artifact
            .as_ref()
            .unwrap();
        assert_eq!(
            art.url,
            "https://libraries.minecraft.net/org/lwjgl/lwjgl-freetype/3.4.1-mojang-2.15.0.3/lwjgl-freetype-3.4.1-mojang-2.15.0.3-natives-windows.jar"
        );
        assert_eq!(
            art.path.as_deref(),
            Some("org/lwjgl/lwjgl-freetype/3.4.1-mojang-2.15.0.3/lwjgl-freetype-3.4.1-mojang-2.15.0.3-natives-windows.jar")
        );
        assert_eq!(art.sha1, "c9d78fd1df6e7ff7eff4db0126786755a6a19cd9");
        assert_eq!(art.size, 1190827);
    }

    #[test]
    fn a_patched_native_keeps_the_rules_of_the_one_it_replaces() {
        let before = version("freetype-1.21.11.json");
        let patched = patched(&before, &MOJANG).unwrap();
        for (old, new) in before.libraries.iter().zip(&patched.libraries) {
            assert_eq!(old.rules, new.rules, "{}", old.name);
        }
    }

    /// What ends up on the game's classpath, per computer: the patched native and never the old one.
    #[test]
    fn every_computer_gets_exactly_the_patched_native() {
        let patched = patched(&version("freetype-1.21.11.json"), &MOJANG).unwrap();
        let dirs = LauncherDirs::new("/pl");
        for (os, arch, jar) in [
            (Os::Windows, Arch::X64, "natives-windows.jar"),
            (Os::Windows, Arch::X86, "natives-windows-x86.jar"),
            (Os::Windows, Arch::Arm64, "natives-windows-arm64.jar"),
            (Os::Linux, Arch::X64, "natives-linux.jar"),
            (Os::MacOs, Arch::X64, "natives-macos.jar"),
            (Os::MacOs, Arch::Arm64, "natives-macos-arm64.jar"),
        ] {
            let ctx = RuleContext::new(Platform { os, arch });
            let natives: Vec<String> = plan_libraries(&patched, &dirs, &ctx)
                .unwrap()
                .downloads
                .iter()
                // Under the libraries folder, wherever that is on this computer (a drive on Windows).
                .map(|d| d.dest.strip_prefix(dirs.libraries()).unwrap().to_path_buf())
                .map(|d| d.to_string_lossy().replace('\\', "/"))
                .filter(|d| d.contains("natives"))
                .collect();
            assert_eq!(
                natives,
                vec![format!(
                    "org/lwjgl/lwjgl-freetype/3.4.1-mojang-2.15.0.3/lwjgl-freetype-3.4.1-mojang-2.15.0.3-{jar}"
                )],
                "{os:?} {arch:?}"
            );
        }
    }

    #[test]
    fn a_version_mojang_already_patched_is_left_alone() {
        assert_eq!(patched(&version("freetype-26.3.json"), &MOJANG), None);
    }

    #[test]
    fn a_version_from_before_freetype_is_left_alone() {
        assert_eq!(patched(&version("1.20.1.json"), &MOJANG), None);
        assert_eq!(patched(&version("1.12.2.json"), &MOJANG), None);
    }

    /// Two old natives for the same computer (`natives-macos` and `natives-macos-patch`) must
    /// not put the patched jar on the classpath twice.
    #[test]
    fn two_old_natives_for_one_computer_become_one() {
        let json: VersionJson = serde_json::from_str(
            r#"{"id":"x","mainClass":"M","libraries":[
                {"name":"org.lwjgl:lwjgl-freetype:3.3.2:natives-macos"},
                {"name":"org.lwjgl:lwjgl-freetype:3.3.2:natives-macos-patch"}]}"#,
        )
        .unwrap();
        assert_eq!(
            names(&patched(&json, &MOJANG).unwrap()),
            vec!["org.lwjgl:lwjgl-freetype:3.4.1-mojang-2.15.0.3:natives-macos"]
        );
    }

    /// Mojang has no patched build for some computers (Linux on ARM): that native stays.
    #[test]
    fn a_native_without_a_patched_build_stays() {
        let json: VersionJson = serde_json::from_str(
            r#"{"id":"x","mainClass":"M","libraries":[
                {"name":"org.lwjgl:lwjgl-freetype:3.3.3:natives-linux-arm64"}]}"#,
        )
        .unwrap();
        assert_eq!(patched(&json, &MOJANG), None);
    }

    #[test]
    fn other_lwjgl_libraries_are_not_touched() {
        let json: VersionJson = serde_json::from_str(
            r#"{"id":"x","mainClass":"M","libraries":[
                {"name":"org.lwjgl:lwjgl:3.3.3:natives-windows"},
                {"name":"org.lwjgl:lwjgl-stb:3.3.3:natives-windows"},
                {"name":"com.example:lwjgl-freetype:3.3.3:natives-windows"}]}"#,
        )
        .unwrap();
        assert_eq!(patched(&json, &MOJANG), None);
    }
}
