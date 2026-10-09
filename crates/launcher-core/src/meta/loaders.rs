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

/// Where to get a loader's launch profile (a version JSON that inherits from vanilla).
pub fn profile_url(kind: LoaderKind, mc: &str, loader: &str) -> Option<String> {
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
    fn kind_serializes_lowercase() {
        assert_eq!(
            serde_json::to_string(&LoaderKind::NeoForge).unwrap(),
            "\"neoforge\""
        );
    }
}
