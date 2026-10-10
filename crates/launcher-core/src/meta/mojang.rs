use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";

/// The JVM arguments every pre-1.13 version implies: its JSON only has `minecraftArguments`.
pub const LEGACY_JVM_ARGS: [&str; 3] = [
    "-Djava.library.path=${natives_directory}",
    "-cp",
    "${classpath}",
];

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: Latest,
    pub versions: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Latest {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    pub sha1: String,
}

impl VersionManifest {
    /// Exact id lookup. Ids are opaque: "1.20.1", "26.3" and "26.4-snapshot-3" are all valid.
    pub fn find(&self, id: &str) -> Option<&ManifestEntry> {
        self.versions.iter().find(|v| v.id == id)
    }
}

/// A Mojang version JSON, or a loader profile that inherits from one (same shape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    pub main_class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<VersionDownloads>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersionReq>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<Logging>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<ArgValue>,
    #[serde(default)]
    pub jvm: Vec<ArgValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgValue {
    Plain(String),
    Conditional { rules: Vec<Rule>, value: OneOrMany },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s.clone()],
            OneOrMany::Many(v) => v.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Library {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base URL (Fabric/Quilt style libraries).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<Extract>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Extract {
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexRef {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionReq {
    pub component: String,
    pub major_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Logging {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingClient>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingClient {
    pub argument: String,
    pub file: LoggingFile,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{fixture, version};

    #[test]
    fn parses_modern_version() {
        let v = version("1.20.1.json");
        assert_eq!(v.id, "1.20.1");
        assert_eq!(v.main_class, "net.minecraft.client.main.Main");
        let java = v.java_version.as_ref().unwrap();
        assert_eq!(
            (java.component.as_str(), java.major_version),
            ("java-runtime-gamma", 17)
        );
        assert_eq!(v.asset_index.as_ref().unwrap().id, "5");
        assert_eq!(v.libraries.len(), 88);
        assert_eq!(
            v.downloads.as_ref().unwrap().client.sha1,
            "0c3ec587af28e5a785c0b4a7b8a30f9a8f78f838"
        );
        let args = v.arguments.as_ref().unwrap();
        assert_eq!(args.game[0], ArgValue::Plain("--username".into()));
        assert!(args
            .jvm
            .iter()
            .any(|a| matches!(a, ArgValue::Conditional { .. })));
        assert_eq!(
            v.logging.as_ref().unwrap().client.as_ref().unwrap().file.id,
            "client-1.12.xml"
        );
    }

    #[test]
    fn parses_legacy_version() {
        let v = version("1.12.2.json");
        assert!(v.arguments.is_none());
        assert!(v
            .minecraft_arguments
            .as_ref()
            .unwrap()
            .starts_with("--username ${auth_player_name}"));
        assert_eq!(v.java_version.as_ref().unwrap().component, "jre-legacy");
        let lwjgl = v
            .libraries
            .iter()
            .find(|l| l.name == "org.lwjgl.lwjgl:lwjgl-platform:2.9.4-nightly-20150209")
            .unwrap();
        assert_eq!(
            lwjgl.natives.as_ref().unwrap()["windows"],
            "natives-windows"
        );
        assert!(lwjgl
            .downloads
            .as_ref()
            .unwrap()
            .classifiers
            .as_ref()
            .unwrap()
            .contains_key("natives-windows"));
    }

    #[test]
    fn round_trips_through_serde() {
        let v = version("1.20.1.json");
        let again: VersionJson = serde_json::from_str(&serde_json::to_string(&v).unwrap()).unwrap();
        assert_eq!(again.libraries.len(), v.libraries.len());
        assert_eq!(again.arguments, v.arguments);
    }

    #[test]
    fn manifest_finds_year_style_ids() {
        let json = r#"{"latest":{"release":"26.3","snapshot":"26.4-snapshot-3"},"versions":[
            {"id":"26.4-snapshot-3","type":"snapshot","url":"https://x/a.json","sha1":"aa"},
            {"id":"26.3","type":"release","url":"https://x/b.json","sha1":"bb"},
            {"id":"1.20.1","type":"release","url":"https://x/c.json","sha1":"cc"}]}"#;
        let m: VersionManifest = serde_json::from_str(json).unwrap();
        assert_eq!(m.find("26.3").unwrap().url, "https://x/b.json");
        assert_eq!(m.find("26.4-snapshot-3").unwrap().kind, "snapshot");
        assert!(
            m.find("1.20").is_none(),
            "lookup is exact, never a prefix match"
        );
    }

    #[test]
    fn parses_asset_index() {
        let idx: AssetIndex = serde_json::from_str(&fixture("asset-index-5.json")).unwrap();
        assert_eq!(idx.objects.len(), 3598);
        assert_eq!(
            idx.objects["icons/icon_128x128.png"].hash,
            "b62ca8ec10d07e6bf5ac8dae0c8c1d2e6a1e3356"
        );
    }
}
