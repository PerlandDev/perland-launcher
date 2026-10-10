//! Forge and NeoForge ship an installer jar instead of a launch profile. This module knows where
//! the installer lives and how to read it: `install_profile.json` (libraries, plus the processors
//! that build the patched game) and the version JSON that inherits from vanilla. It follows the
//! installer's published format; nothing here is ported from another launcher.
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Display,
    io::{Read, Seek},
    path::Path,
};

use serde::Deserialize;

use super::{
    loaders::{valid_component, LoaderKind},
    maven::MavenCoord,
    mojang::{Library, VersionJson},
};
use crate::error::{ErrorCode, LauncherError, Result};

pub const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";
pub const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

/// The maven repositories installers and version lists come from (tests point them elsewhere).
#[derive(Debug, Clone, Copy)]
pub struct MavenRepos<'a> {
    pub forge: &'a str,
    pub neoforge: &'a str,
}

pub const OFFICIAL_REPOS: MavenRepos<'static> = MavenRepos {
    forge: FORGE_MAVEN,
    neoforge: NEOFORGE_MAVEN,
};

/// JSON inside an installer is small; anything bigger isn't an installer.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// The installer's maven coordinate. `None` for Fabric/Quilt or version strings unsafe for a path.
pub fn installer_coord(kind: LoaderKind, mc: &str, loader: &str) -> Option<MavenCoord> {
    if !valid_component(mc) || !valid_component(loader) {
        return None;
    }
    let name = match kind {
        LoaderKind::Forge => format!("net.minecraftforge:forge:{mc}-{loader}:installer"),
        // NeoForge's only 1.20.1 line kept Forge's coordinates.
        LoaderKind::NeoForge if mc == "1.20.1" => {
            format!("net.neoforged:forge:{mc}-{loader}:installer")
        }
        LoaderKind::NeoForge => format!("net.neoforged:neoforge:{loader}:installer"),
        LoaderKind::Fabric | LoaderKind::Quilt => return None,
    };
    MavenCoord::parse(&name)
}

pub fn installer_url(kind: LoaderKind, mc: &str, loader: &str) -> Option<String> {
    installer_url_at(kind, mc, loader, OFFICIAL_REPOS)
}

pub fn installer_url_at(
    kind: LoaderKind,
    mc: &str,
    loader: &str,
    repos: MavenRepos,
) -> Option<String> {
    let base = match kind {
        LoaderKind::Forge => repos.forge,
        _ => repos.neoforge,
    };
    installer_coord(kind, mc, loader)
        .map(|c| format!("{}/{}", base.trim_end_matches('/'), c.path()))
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct InstallProfile {
    #[serde(default)]
    pub spec: u32,
    pub minecraft: String,
    /// Where the version JSON sits inside the installer, e.g. `/version.json`.
    pub json: String,
    /// A maven coordinate whose jar is bundled under the installer's `maven/` folder (1.12.2).
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub data: HashMap<String, SidedValue>,
    #[serde(default)]
    pub processors: Vec<Processor>,
    #[serde(default)]
    pub libraries: Vec<Library>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SidedValue {
    pub client: String,
    #[serde(default)]
    pub server: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Processor {
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// Output file → expected sha1, both written with `{TOKENS}`.
    #[serde(default)]
    pub outputs: BTreeMap<String, String>,
    /// `None` means both sides.
    #[serde(default)]
    pub sides: Option<Vec<String>>,
}

impl Processor {
    pub fn runs_on_client(&self) -> bool {
        self.sides
            .as_ref()
            .is_none_or(|s| s.iter().any(|x| x == "client"))
    }
}

#[derive(Debug, Clone)]
pub struct InstallerContents {
    pub profile: InstallProfile,
    pub version: VersionJson,
}

pub(crate) fn invalid(detail: impl Display) -> LauncherError {
    LauncherError::loader(
        ErrorCode::LoaderInstallerInvalid,
        "The mod loader's installer couldn't be read",
    )
    .with_fix("Try again. If it keeps happening, pick another loader version.")
    .with_detail(detail)
}

/// Read one zip entry, refusing entries larger than `limit` bytes.
pub(crate) fn read_entry<R: Read + Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    let entry = archive
        .by_name(name)
        .map_err(|e| invalid(format!("{name}: {e}")))?;
    let mut bytes = Vec::new();
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| invalid(format!("{name}: {e}")))?;
    if bytes.len() as u64 > limit {
        return Err(invalid(format!("{name} is too large")));
    }
    Ok(bytes)
}

/// Installers from before Forge 1.12.2-14.23.5.2851 use another format (`install` + `versionInfo`).
pub fn parse_profile(bytes: &[u8]) -> Result<InstallProfile> {
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("install_profile.json: {e}")))?;
    if value.get("versionInfo").is_some() {
        return Err(LauncherError::loader(
            ErrorCode::LoaderTooOld,
            "This loader version is too old for the launcher",
        )
        .with_fix("Pick a newer Minecraft version, or another loader")
        .with_detail("legacy installer format (versionInfo)"));
    }
    serde_json::from_value(value).map_err(|e| invalid(format!("install_profile.json: {e}")))
}

/// Read the install profile and the version JSON it names. Blocking: run it on `spawn_blocking`.
pub fn read_installer(jar: &Path) -> Result<InstallerContents> {
    let mut archive = zip::ZipArchive::new(std::fs::File::open(jar)?).map_err(invalid)?;
    let profile = parse_profile(&read_entry(
        &mut archive,
        "install_profile.json",
        MAX_JSON_BYTES,
    )?)?;
    let version_bytes = read_entry(
        &mut archive,
        profile.json.trim_start_matches('/'),
        MAX_JSON_BYTES,
    )?;
    let version: VersionJson = serde_json::from_slice(&version_bytes)
        .map_err(|e| invalid(format!("{}: {e}", profile.json)))?;
    Ok(InstallerContents { profile, version })
}

/// One install-profile data value, read the way the Forge installer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataValue {
    /// `[group:artifact:version]`: a file in the libraries folder.
    Artifact(MavenCoord),
    /// `'text'`: the text itself.
    Literal(String),
    /// Anything else (e.g. `/data/client.lzma`): a file inside the installer jar.
    InstallerFile(String),
}

pub fn parse_data_value(raw: &str) -> Result<DataValue> {
    if let Some(inner) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
        return MavenCoord::parse(inner)
            .map(DataValue::Artifact)
            .ok_or_else(|| unsafe_value(raw));
    }
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return Ok(DataValue::Literal(raw[1..raw.len() - 1].to_string()));
    }
    Ok(DataValue::InstallerFile(
        raw.trim_start_matches('/').to_string(),
    ))
}

pub(crate) fn unsafe_value(raw: &str) -> LauncherError {
    LauncherError::integrity(
        ErrorCode::UnsafeGameData,
        "The mod loader's installer contains an unsafe path",
    )
    .with_detail(raw)
}

/// Replace `{KEY}` with `data[KEY]` and unwrap `'quoted'` text, with `\` escaping the next
/// character: how the Forge installer reads processor arguments. An unknown key is an error.
pub fn replace_tokens(value: &str, data: &HashMap<String, String>) -> Result<String> {
    let dangling = || invalid(format!("dangling escape in {value}"));
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push(chars.next().ok_or_else(dangling)?),
            '{' | '\'' => {
                let close = if c == '{' { '}' } else { '\'' };
                let mut inner = String::new();
                loop {
                    match chars.next() {
                        None => return Err(invalid(format!("unclosed {c} in {value}"))),
                        Some('\\') => inner.push(chars.next().ok_or_else(dangling)?),
                        Some(d) if d == close => break,
                        Some(d) => inner.push(d),
                    }
                }
                if c == '{' {
                    let found = data
                        .get(&inner)
                        .ok_or_else(|| invalid(format!("unknown key {inner} in {value}")))?;
                    out.push_str(found);
                } else {
                    out.push_str(&inner);
                }
            }
            _ => out.push(c),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, write_zip};

    #[test]
    fn installer_urls() {
        assert_eq!(
            installer_url(LoaderKind::Forge, "1.20.1", "47.4.22").unwrap(),
            "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar"
        );
        assert_eq!(
            installer_url(LoaderKind::NeoForge, "1.21.1", "21.1.257").unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/21.1.257/neoforge-21.1.257-installer.jar"
        );
        // NeoForge's only 1.20.1 line kept Forge's coordinates.
        assert_eq!(
            installer_url(LoaderKind::NeoForge, "1.20.1", "47.1.106").unwrap(),
            "https://maven.neoforged.net/releases/net/neoforged/forge/1.20.1-47.1.106/forge-1.20.1-47.1.106-installer.jar"
        );
        assert!(installer_url(LoaderKind::Fabric, "1.20.1", "0.16.14").is_none());
        for (mc, loader) in [("1.20.1", "../x"), ("1.20.1/..", "1"), ("1.20.1", "")] {
            assert!(
                installer_url(LoaderKind::Forge, mc, loader).is_none(),
                "{mc} {loader}"
            );
        }
        let repos = MavenRepos {
            forge: "http://127.0.0.1:9/",
            neoforge: "http://127.0.0.1:8",
        };
        assert_eq!(
            installer_url_at(LoaderKind::Forge, "1.20.1", "47.4.22", repos).unwrap(),
            "http://127.0.0.1:9/net/minecraftforge/forge/1.20.1-47.4.22/forge-1.20.1-47.4.22-installer.jar"
        );
    }

    #[test]
    fn forge_1_20_1_profile_parses() {
        let p =
            parse_profile(fixture("forge-1.20.1-47.4.22-install_profile.json").as_bytes()).unwrap();
        assert_eq!(p.spec, 1);
        assert_eq!(p.minecraft, "1.20.1");
        assert_eq!(p.json, "/version.json");
        assert_eq!(p.libraries.len(), 36);
        assert_eq!(p.processors.len(), 10);
        assert_eq!(
            p.processors.iter().filter(|x| x.runs_on_client()).count(),
            6
        );
        assert_eq!(
            p.data["MC_SLIM_SHA"].client,
            "'de86b035d2da0f78940796bb95c39a932ed84834'"
        );
        let split = &p.processors[6];
        assert_eq!(split.jar, "net.minecraftforge:jarsplitter:1.1.4");
        assert_eq!(split.outputs["{MC_SLIM}"], "{MC_SLIM_SHA}");
    }

    #[test]
    fn neoforge_profile_has_fat_jars_and_no_outputs() {
        let p =
            parse_profile(fixture("neoforge-21.1.257-install_profile.json").as_bytes()).unwrap();
        assert_eq!(p.minecraft, "1.21.1");
        assert_eq!(p.libraries.len(), 70);
        let client: Vec<&Processor> = p.processors.iter().filter(|x| x.runs_on_client()).collect();
        assert_eq!(client.len(), 6);
        assert!(client.iter().all(|x| x.outputs.is_empty()));
        assert!(client.iter().any(|x| x.jar.ends_with(":fatjar")));
    }

    #[test]
    fn forge_1_12_2_profile_names_its_bundled_jar() {
        let p = parse_profile(fixture("forge-1.12.2-14.23.5.2860-install_profile.json").as_bytes())
            .unwrap();
        assert_eq!(
            p.path.as_deref(),
            Some("net.minecraftforge:forge:1.12.2-14.23.5.2860")
        );
        assert!(p.processors.is_empty());
    }

    #[test]
    fn legacy_installer_format_is_too_old() {
        let err =
            parse_profile(br#"{"install":{"target":"x"},"versionInfo":{"id":"x"}}"#).unwrap_err();
        assert_eq!(err.code, ErrorCode::LoaderTooOld);
        let garbage = parse_profile(b"<html>").unwrap_err();
        assert_eq!(garbage.code, ErrorCode::LoaderInstallerInvalid);
    }

    #[test]
    fn reads_profile_and_version_from_an_installer_jar() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("installer.jar");
        let profile = fixture("forge-1.20.1-47.4.22-install_profile.json");
        let version = fixture("forge-1.20.1-47.4.22-version.json");
        write_zip(
            &jar,
            &[
                ("install_profile.json", profile.as_bytes()),
                ("version.json", version.as_bytes()),
            ],
        );
        let contents = read_installer(&jar).unwrap();
        assert_eq!(contents.version.id, "1.20.1-forge-47.4.22");
        assert_eq!(contents.version.inherits_from.as_deref(), Some("1.20.1"));
        assert_eq!(contents.profile.processors.len(), 10);

        let broken = tmp.path().join("broken.jar");
        write_zip(&broken, &[("install_profile.json", profile.as_bytes())]);
        assert_eq!(
            read_installer(&broken).unwrap_err().code,
            ErrorCode::LoaderInstallerInvalid
        );
    }

    #[test]
    fn oversized_entries_are_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let jar = tmp.path().join("x.jar");
        write_zip(&jar, &[("a.json", b"0123456789")]);
        let mut archive = zip::ZipArchive::new(std::fs::File::open(&jar).unwrap()).unwrap();
        assert_eq!(
            read_entry(&mut archive, "a.json", 10).unwrap(),
            b"0123456789"
        );
        assert_eq!(
            read_entry(&mut archive, "a.json", 4).unwrap_err().code,
            ErrorCode::LoaderInstallerInvalid
        );
    }

    #[test]
    fn data_values_like_the_forge_installer() {
        assert_eq!(
            parse_data_value("[net.minecraft:client:1.20.1-20230612.114412:srg]").unwrap(),
            DataValue::Artifact(
                MavenCoord::parse("net.minecraft:client:1.20.1-20230612.114412:srg").unwrap()
            )
        );
        assert_eq!(
            parse_data_value("'de86b035'").unwrap(),
            DataValue::Literal("de86b035".into())
        );
        assert_eq!(
            parse_data_value("/data/client.lzma").unwrap(),
            DataValue::InstallerFile("data/client.lzma".into())
        );
        assert_eq!(
            parse_data_value("[../../evil]").unwrap_err().code,
            ErrorCode::UnsafeGameData
        );
    }

    #[test]
    fn tokens_like_the_forge_installer() {
        let data = HashMap::from([
            ("SIDE".to_string(), "client".to_string()),
            ("MC_SLIM".to_string(), "/l/slim.jar".to_string()),
        ]);
        assert_eq!(replace_tokens("{SIDE}", &data).unwrap(), "client");
        assert_eq!(
            replace_tokens("--x={MC_SLIM}", &data).unwrap(),
            "--x=/l/slim.jar"
        );
        assert_eq!(replace_tokens("'{SIDE}'", &data).unwrap(), "{SIDE}");
        assert_eq!(replace_tokens(r"a\{b", &data).unwrap(), "a{b");
        assert_eq!(replace_tokens("plain", &data).unwrap(), "plain");
        for bad in ["{NOPE}", "{SIDE", "'open", "x\\"] {
            assert_eq!(
                replace_tokens(bad, &data).unwrap_err().code,
                ErrorCode::LoaderInstallerInvalid,
                "{bad}"
            );
        }
    }
}
