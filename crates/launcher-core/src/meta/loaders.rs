use serde::{Deserialize, Serialize};

use super::{mojang::VersionJson, MetaClient};
use crate::error::{LauncherError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

/// A version string that is safe to place in a URL path segment: non-empty, only ASCII
/// alphanumerics or `.` `_` `+` `-`, and not a dot segment.
fn valid_component(s: &str) -> bool {
    !s.is_empty()
        && s != "."
        && s != ".."
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

/// Where to get a loader's launch profile (a version JSON that inherits from vanilla).
/// `None` if either version string is unsafe for a URL path, or the loader isn't supported yet.
pub fn profile_url(kind: LoaderKind, mc: &str, loader: &str) -> Option<String> {
    if !valid_component(mc) || !valid_component(loader) {
        return None;
    }
    match kind {
        LoaderKind::Fabric => Some(format!(
            "https://meta.fabricmc.net/v2/versions/loader/{mc}/{loader}/profile/json"
        )),
        LoaderKind::Quilt => Some(format!(
            "https://meta.quiltmc.org/v3/versions/loader/{mc}/{loader}/profile/json"
        )),
        LoaderKind::Forge | LoaderKind::NeoForge => None,
    }
}

pub async fn fetch_profile(
    meta: &MetaClient,
    kind: LoaderKind,
    mc: &str,
    loader: &str,
) -> Result<VersionJson> {
    for value in [mc, loader] {
        if !valid_component(value) {
            return Err(
                LauncherError::loader(format!("\"{value}\" isn't a valid version"))
                    .with_fix("Pick a version from the list instead of typing one in"),
            );
        }
    }
    let Some(url) = profile_url(kind, mc, loader) else {
        return Err(
            LauncherError::loader(format!("{kind:?} isn't supported yet"))
                .with_fix("Use Fabric or Quilt for now; Forge and NeoForge are coming"),
        );
    };
    meta.get_json(&url).await.map_err(|e| {
        LauncherError::loader(format!("Couldn't get {kind:?} {loader} for Minecraft {mc}"))
            .with_fix("Check that this loader version exists for this Minecraft version")
            .with_detail(e.message)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_urls() {
        assert_eq!(
            profile_url(LoaderKind::Fabric, "1.20.1", "0.16.14").unwrap(),
            "https://meta.fabricmc.net/v2/versions/loader/1.20.1/0.16.14/profile/json"
        );
        assert_eq!(
            profile_url(LoaderKind::Quilt, "1.20.1", "0.29.1").unwrap(),
            "https://meta.quiltmc.org/v3/versions/loader/1.20.1/0.29.1/profile/json"
        );
        assert!(profile_url(LoaderKind::Forge, "1.20.1", "47.4.22").is_none());
    }

    #[test]
    fn unsafe_version_strings_are_rejected() {
        for (mc, loader) in [
            ("1.20.1/../x", "0.16.14"),
            ("1.20.1", "a?b"),
            ("1.20.1", "a#b"),
            ("1.20.1", ""),
            ("1.20.1", ".."),
            ("", "0.16.14"),
            (".", "0.16.14"),
        ] {
            assert!(
                profile_url(LoaderKind::Fabric, mc, loader).is_none(),
                "{mc:?} / {loader:?}"
            );
            assert!(profile_url(LoaderKind::Quilt, mc, loader).is_none());
        }
    }

    #[test]
    fn real_world_version_strings_are_accepted() {
        assert!(profile_url(LoaderKind::Fabric, "1.21-pre1", "0.16.14+build.1").is_some());
        assert!(profile_url(LoaderKind::Quilt, "24w14a", "0.29.1-beta.4").is_some());
    }

    #[tokio::test]
    async fn fetch_profile_distinguishes_invalid_from_unsupported() {
        let dir = std::env::temp_dir().join("perland-loaders-test-unused");
        let meta = MetaClient::new(reqwest::Client::new(), dir);
        let invalid = fetch_profile(&meta, LoaderKind::Fabric, "1.20.1", "a?b")
            .await
            .unwrap_err();
        assert!(invalid.message.contains("\"a?b\" isn't a valid version"));
        let unsupported = fetch_profile(&meta, LoaderKind::Forge, "1.20.1", "47.4.22")
            .await
            .unwrap_err();
        assert!(unsupported.message.contains("isn't supported yet"));
    }

    #[test]
    fn kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&LoaderKind::NeoForge).unwrap(),
            "\"neoforge\""
        );
    }
}
