//! Version lists for the "New instance" pickers.
use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use super::{
    loaders::{valid_component, LoaderKind},
    mojang::VERSION_MANIFEST_URL,
    MetaClient,
};
use crate::error::{ErrorCode, LauncherError, Result};

pub const FABRIC_META: &str = "https://meta.fabricmc.net/v2";
pub const QUILT_META: &str = "https://meta.quiltmc.org/v3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McKind {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    /// Any type Mojang adds later; skipped instead of breaking the list.
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McVersion {
    pub id: String,
    pub kind: McKind,
    pub release_time: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    pub stable: bool,
}

#[derive(Deserialize)]
struct Manifest {
    versions: Vec<ManifestEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManifestEntry {
    id: String,
    #[serde(rename = "type")]
    kind: McKind,
    #[serde(default)]
    release_time: String,
}

#[derive(Deserialize)]
struct LoaderEntry {
    loader: LoaderInfo,
}

#[derive(Deserialize)]
struct LoaderInfo {
    version: String,
    #[serde(default)]
    stable: Option<bool>,
}

/// Every Minecraft version, newest first (Mojang's order).
pub async fn minecraft_versions(meta: &MetaClient) -> Result<Vec<McVersion>> {
    minecraft_versions_at(meta, VERSION_MANIFEST_URL).await
}

pub async fn minecraft_versions_at(meta: &MetaClient, url: &str) -> Result<Vec<McVersion>> {
    let manifest: Manifest = meta.get_json(url).await?;
    Ok(manifest
        .versions
        .into_iter()
        .filter(|v| v.kind != McKind::Other)
        .map(|v| McVersion {
            id: v.id,
            kind: v.kind,
            release_time: v.release_time,
        })
        .collect())
}

/// Loader versions for one Minecraft version, newest first. Empty when the loader has none for it.
pub async fn loader_versions(
    meta: &MetaClient,
    kind: LoaderKind,
    mc: &str,
) -> Result<Vec<LoaderVersion>> {
    loader_versions_at(meta, kind, mc, FABRIC_META, QUILT_META).await
}

pub async fn loader_versions_at(
    meta: &MetaClient,
    kind: LoaderKind,
    mc: &str,
    fabric_base: &str,
    quilt_base: &str,
) -> Result<Vec<LoaderVersion>> {
    let url = match kind {
        LoaderKind::Fabric => format!("{fabric_base}/versions/loader/{mc}"),
        LoaderKind::Quilt => format!("{quilt_base}/versions/loader/{mc}"),
        LoaderKind::Forge | LoaderKind::NeoForge => {
            return Err(LauncherError::loader(
                ErrorCode::LoaderUnsupported,
                format!("{kind:?} isn't supported yet"),
            )
            .with_param("loader", format!("{kind:?}"))
            .with_fix("Use Fabric or Quilt for now; Forge and NeoForge are coming"))
        }
    };
    if !valid_component(mc) {
        return Err(LauncherError::loader(
            ErrorCode::VersionInvalid,
            format!("\"{mc}\" isn't a valid version"),
        )
        .with_param("value", mc));
    }
    let entries: Vec<LoaderEntry> = match meta.get_json(&url).await {
        Ok(entries) => entries,
        // The loader answered "no such game version": that's an empty list, not a failure.
        Err(e) if is_unknown_game_version(&e) => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut out: Vec<LoaderVersion> = entries
        .into_iter()
        .map(|e| LoaderVersion {
            stable: e.loader.stable.unwrap_or(!e.loader.version.contains('-')),
            version: e.loader.version,
        })
        .collect();
    out.sort_by(|a, b| compare_versions(&b.version, &a.version));
    out.dedup_by(|a, b| a.version == b.version);
    Ok(out)
}

/// Fabric answers 400 and Quilt answers 404 for a game version they don't know. Any other
/// failure (rate limit, forbidden, a malformed body, offline) is a real error, not an empty list.
fn is_unknown_game_version(e: &LauncherError) -> bool {
    e.code == ErrorCode::MetaNotFound
        && matches!(
            e.params.get("status").map(String::as_str),
            Some("400" | "404")
        )
}

/// Compare dotted versions numerically; a release sorts above its own pre-releases
/// (`0.24.0` > `0.24.0-beta.2` > `0.24.0-beta.1`). Non-numeric parts compare as text.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    fn split(v: &str) -> (Vec<&str>, Option<Vec<&str>>) {
        match v.split_once('-') {
            Some((base, pre)) => (base.split('.').collect(), Some(pre.split('.').collect())),
            None => (v.split('.').collect(), None),
        }
    }
    fn parts(a: &[&str], b: &[&str]) -> Ordering {
        for i in 0..a.len().max(b.len()) {
            let (x, y) = (
                a.get(i).copied().unwrap_or("0"),
                b.get(i).copied().unwrap_or("0"),
            );
            let ord = match (x.parse::<u64>(), y.parse::<u64>()) {
                (Ok(x), Ok(y)) => x.cmp(&y),
                _ => x.cmp(y),
            };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    }
    let (base_a, pre_a) = split(a);
    let (base_b, pre_b) = split(b);
    parts(&base_a, &base_b).then_with(|| match (pre_a, pre_b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => parts(&x, &y),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_versions_orders_releases_and_prereleases() {
        assert_eq!(compare_versions("0.24.0", "0.20.2"), Ordering::Greater);
        assert_eq!(compare_versions("0.10.0", "0.9.9"), Ordering::Greater);
        assert_eq!(
            compare_versions("0.24.0", "0.24.0-beta.2"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("0.24.0-beta.10", "0.24.0-beta.9"),
            Ordering::Greater
        );
        assert_eq!(compare_versions("1.0", "1.0.0"), Ordering::Equal);
    }
}
