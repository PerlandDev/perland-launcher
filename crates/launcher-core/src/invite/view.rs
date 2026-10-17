//! A serde mirror of the Engine's public invite view (`InviteViewDTO`). Unknown fields are
//! ignored, and every field the Engine may omit has a default, so an Engine that adds or drops
//! optional fields still parses.
use serde::{Deserialize, Deserializer};

use crate::{instances::ServerStatus, sources::valid_modrinth_id};

/// The Engine's pack states.
pub const PACK_READY: &str = "ready";
pub const PACK_PENDING: &str = "pending";
pub const PACK_UNAVAILABLE: &str = "unavailable";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    /// `stack` or `network`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub name: String,
    /// May hold `§` colour codes.
    #[serde(default)]
    pub motd: String,
    #[serde(default)]
    pub icon_version: Option<String>,
    #[serde(default)]
    pub address: InviteAddress,
    #[serde(default)]
    pub game: InviteGame,
    /// `online | starting | sleeping | offline`; see [`InviteView::status`].
    #[serde(default)]
    pub status: String,
    /// Present only while online.
    #[serde(default)]
    pub players: Option<InvitePlayers>,
    #[serde(default)]
    pub whitelisted: bool,
    #[serde(default)]
    pub note: Option<String>,
    /// `None` for networks and for stacks without a modpack.
    #[serde(default)]
    pub pack: Option<InvitePack>,
    /// The Modrinth modpack version a stack runs (a modpack free server, with `pack: null`).
    /// Absent otherwise; an id that isn't a valid Modrinth id reads as absent.
    #[serde(default, deserialize_with = "modrinth_ref")]
    pub modrinth: Option<InviteModrinth>,
    #[serde(default)]
    pub team: InviteTeam,
    /// Present when the Engine advertises the launcher (`LAUNCHER_DOWNLOAD_URL`).
    #[serde(default)]
    pub launcher: Option<InviteLauncher>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InviteAddress {
    /// Absent without the Engine's `PUBLIC_GAME_HOST`.
    #[serde(default)]
    pub host: Option<String>,
    /// May be 0. Kept wide so an out-of-range value still parses and is refused later.
    #[serde(default)]
    pub port: i64,
    #[serde(default)]
    pub display: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteGame {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub loader_version: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InvitePlayers {
    #[serde(default)]
    pub online: u32,
    /// Absent for networks.
    #[serde(default)]
    pub max: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitePack {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub minecraft: String,
    #[serde(default)]
    pub loader: Option<String>,
    #[serde(default)]
    pub loader_version: Option<String>,
    #[serde(default)]
    pub mod_count: u32,
    /// `ready | pending | unavailable`.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub zip_state: Option<String>,
    /// The pinned modpack commit, the `/pack` ETag without quotes. Absent on older Engines.
    #[serde(default)]
    pub commit: Option<String>,
}

/// A Modrinth modpack version: the project and the version a server installs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteModrinth {
    pub project_id: String,
    pub version_id: String,
}

impl InviteModrinth {
    /// Both ids go into Modrinth API paths, so only their documented characters pass.
    fn checked(self) -> Option<Self> {
        (valid_modrinth_id(&self.project_id) && valid_modrinth_id(&self.version_id)).then_some(self)
    }
}

/// `modrinth`, read leniently: a value of another shape, or an invalid id, is `None`, so an
/// odd field never fails the whole view.
pub(crate) fn modrinth_ref<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<InviteModrinth>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Lenient {
        Ref(InviteModrinth),
        Other(serde::de::IgnoredAny),
    }
    Ok(match Option::<Lenient>::deserialize(deserializer)? {
        Some(Lenient::Ref(r)) => r.checked(),
        Some(Lenient::Other(_)) | None => None,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteTeam {
    #[serde(default)]
    pub display_name: String,
    /// Empty, or `cyan | blue | violet | magenta`.
    #[serde(default)]
    pub accent: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteLauncher {
    #[serde(default)]
    pub download_url: Option<String>,
}

/// `Some` only for a non-blank string, trimmed.
pub fn non_empty(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

impl InviteView {
    pub fn status(&self) -> Option<ServerStatus> {
        ServerStatus::parse(&self.status)
    }

    pub fn is_network(&self) -> bool {
        self.kind == "network"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn view(modrinth: serde_json::Value) -> InviteView {
        serde_json::from_value(json!({
            "kind": "stack", "name": "pack",
            "game": { "type": "fabric", "version": "1.20.1", "loader": "fabric" },
            "pack": null, "modrinth": modrinth
        }))
        .unwrap()
    }

    #[test]
    fn view_reads_modrinth() {
        assert_eq!(
            view(json!({ "projectId": "1KVo5zza", "versionId": "Yp8wLY1P" })).modrinth,
            Some(InviteModrinth {
                project_id: "1KVo5zza".into(),
                version_id: "Yp8wLY1P".into(),
            })
        );
        assert_eq!(view(serde_json::Value::Null).modrinth, None);
    }

    #[test]
    fn view_drops_invalid_modrinth() {
        for bad in [
            json!({ "projectId": "../x", "versionId": "Yp8wLY1P" }),
            json!({ "projectId": "1KVo5zza", "versionId": "a/b" }),
            json!({ "projectId": "", "versionId": "Yp8wLY1P" }),
            json!({ "projectId": "1KVo5zza" }),
            json!("1KVo5zza"),
            json!({ "projectId": 7, "versionId": "Yp8wLY1P" }),
        ] {
            assert_eq!(view(bad.clone()).modrinth, None, "{bad}");
        }
    }
}
