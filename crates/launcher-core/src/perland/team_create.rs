//! A server made on one of the player's teams (free-server settings spec §6.3): the stack create
//! (`POST /api/v1/stacks/create-server`, and `/create-server/pack` with an uploaded `.mrpack`),
//! the team's room (`GET /api/v1/tenants/current/usage`) and the Engine's version lists
//! (`GET /api/v1/versions`). Every call carries the team's JWT; nothing here prints one.

use reqwest::{
    multipart::{Form, Part},
    Method, Url,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    api::{answer_lost, host_full, versions_unavailable, FreeServerSettings, CREATE_TIMEOUT},
    auth::SessionJwt,
    stacks::StackClient,
    wire::{failure, Credential, Reply},
};
use crate::{
    error::{ErrorCode, LauncherError, Result},
    invite::client::host_trouble,
};

/// The longest answer read, in bytes.
const BODY_CAP: usize = 512 << 10;
/// The pack upload route's name for its file, as the free create names it.
const PACK_FILE_NAME: &str = "pack.mrpack";

/// The create-server body. What is `None` is left out: a Modrinth create sends no type, version
/// or loader version (the pack decides them), and an uploaded pack's request neither.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateServer {
    pub server_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
    /// The memory limit, as `<N>G`.
    pub ram: String,
    /// Cores.
    pub cpu: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub customization: Option<ServerCustomization>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modrinth_version_id: Option<String>,
}

/// The stack's customization fields the create's Options set. Left out: itzg's default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerCustomization {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hardcore: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_players: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pvp: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_whitelist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enforce_whitelist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motd: Option<String>,
}

impl ServerCustomization {
    /// The Options as the stack's customization, written as the free create writes them:
    /// hardcore is survival with hardcore on and difficulty hard, and the whitelist turns enforce
    /// on too. `None` when the Options change nothing.
    pub fn from_settings(settings: &FreeServerSettings) -> Option<Self> {
        let mut c = Self {
            mode: settings.game_mode.clone(),
            difficulty: settings.difficulty.clone(),
            max_players: settings.max_players,
            pvp: settings.pvp,
            enable_whitelist: settings.whitelist,
            enforce_whitelist: settings.whitelist,
            seed: settings.seed.clone(),
            motd: settings.motd.clone(),
            ..Self::default()
        };
        if c.mode.as_deref() == Some("hardcore") {
            c.mode = Some("survival".into());
            c.hardcore = Some(true);
            c.difficulty = Some("hard".into());
        }
        (c != Self::default()).then_some(c)
    }
}

/// What the create answered. `started` is false when the team's running servers left no room:
/// the server exists, stopped, and starts once another one stops.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CreatedServer {
    #[serde(rename = "servername")]
    pub server_name: String,
    pub status: String,
    pub started: bool,
    /// `quota_exceeded` or `host_full` when not started.
    pub not_started_reason: Option<String>,
    pub address: Option<String>,
}

impl Default for CreatedServer {
    /// An Engine that doesn't say started it.
    fn default() -> Self {
        Self {
            server_name: String::new(),
            status: String::new(),
            started: true,
            not_started_reason: None,
            address: None,
        }
    }
}

/// What the team may still run (`available`) and its plan's limits. `available`'s memory and CPU
/// are absent when the Engine couldn't read what runs.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TeamUsage {
    pub available: UsageAvailable,
    pub limits: UsageLimits,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageAvailable {
    pub cpu: Option<f64>,
    /// MiB.
    pub memory_mb: Option<i64>,
    pub servers: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UsageLimits {
    pub max_cpu: Option<f64>,
    /// MiB.
    pub max_memory_mb: Option<i64>,
    pub max_servers: Option<i64>,
}

/// One Minecraft or loader version the Engine can run, newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EngineVersion {
    pub id: String,
    /// False for snapshots, pre-releases, betas and alphas.
    pub stable: bool,
    /// Forge's promoted build.
    pub recommended: bool,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Versions {
    versions: Vec<EngineVersion>,
}

impl StackClient {
    /// `POST /create-server`. The Engine builds the server while the request is open, so this
    /// waits as long as a free server's create.
    pub async fn create_server(
        &self,
        jwt: &SessionJwt,
        server: &CreateServer,
    ) -> Result<CreatedServer> {
        let body = serde_json::to_value(server).expect("a plain struct is JSON");
        let reply = self
            .wire
            .send(
                CREATE_TIMEOUT,
                Method::POST,
                "/api/v1/stacks/create-server",
                Credential::UserToken(jwt.as_str()),
                Some(&body),
                BODY_CAP,
            )
            .await?;
        self.created(&reply)
    }

    /// `POST /create-server/pack`: the server made from a `.mrpack` the player built, as
    /// multipart (`request`, `sha256`, and the pack as a file part). `server` leaves out type,
    /// version, loader version and Modrinth version id: the pack decides them.
    pub async fn create_server_from_pack(
        &self,
        jwt: &SessionJwt,
        server: &CreateServer,
        pack: Vec<u8>,
    ) -> Result<CreatedServer> {
        let request = serde_json::to_string(server).expect("a plain struct is JSON");
        let sha256 = crate::download::hash::sha256_hex(&pack);
        // A file part: the Engine reads a part without a file name as a plain field and refuses
        // it as too large.
        let part = Part::bytes(pack)
            .file_name(PACK_FILE_NAME)
            .mime_str("application/x-modrinth-modpack+zip")
            .expect("a valid mime type");
        let form = Form::new()
            .text("request", request)
            .text("sha256", sha256)
            .part("pack", part);
        let reply = self
            .wire
            .send_form(
                CREATE_TIMEOUT,
                "/api/v1/stacks/create-server/pack",
                Credential::UserToken(jwt.as_str()),
                form,
                BODY_CAP,
            )
            .await?;
        self.created(&reply)
    }

    /// `GET /tenants/current/usage`: the JWT's team.
    pub async fn usage(&self, jwt: &SessionJwt) -> Result<TeamUsage> {
        let reply = self
            .call(
                jwt,
                Method::GET,
                "/api/v1/tenants/current/usage".into(),
                None,
                self.timeout,
                BODY_CAP,
            )
            .await?;
        self.wire.data(&reply)
    }

    /// `GET /versions?type=`: the Minecraft versions a server type runs; with `minecraft`, that
    /// version's loader versions.
    pub async fn versions(
        &self,
        jwt: &SessionJwt,
        server_type: &str,
        minecraft: Option<&str>,
    ) -> Result<Vec<EngineVersion>> {
        let mut url = Url::parse("http://x/api/v1/versions").expect("a fixed URL");
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("type", server_type);
            if let Some(minecraft) = minecraft {
                query.append_pair("minecraft", minecraft);
            }
        }
        let path = format!("{}?{}", url.path(), url.query().unwrap_or_default());
        let reply = self
            .wire
            .send(
                self.timeout,
                Method::GET,
                &path,
                Credential::UserToken(jwt.as_str()),
                None,
                BODY_CAP,
            )
            .await?;
        if !(200..300).contains(&reply.status) {
            return Err(match failure(&reply).code.as_deref() {
                Some("versions_unavailable") => versions_unavailable(&self.host()),
                _ => self.refusal(&reply),
            });
        }
        Ok(self.wire.data::<Versions>(&reply)?.versions)
    }

    fn created(&self, reply: &Reply) -> Result<CreatedServer> {
        if (200..300).contains(&reply.status) {
            return self.wire.data(reply);
        }
        Err(self.create_refusal(reply))
    }

    /// A refused create, in a team's words (never a free server's): the create's own codes
    /// first, then what any server route means.
    fn create_refusal(&self, reply: &Reply) -> LauncherError {
        let host = self.host();
        let failure = failure(reply);
        let param = |name: &str| match failure.data.get(name) {
            Some(Value::Number(n)) => Some(n.to_string()),
            Some(Value::String(s)) => Some(s.clone()),
            _ => None,
        };
        let with = |e: LauncherError, name: &str, value: Option<String>| match value {
            Some(value) => e.with_param(name, value),
            None => e,
        };
        let net =
            |code, message: &str| LauncherError::network(code, message).with_param("host", &host);
        match failure.code.as_deref() {
            Some("quota_exceeded") => {
                return net(
                    ErrorCode::TeamServerTooBig,
                    "Your team's plan has no room for a server this size.",
                )
            }
            Some("name_taken") => {
                return net(
                    ErrorCode::ServerNameTaken,
                    "Your team already has a server with that name.",
                )
                .with_param("field", "name")
            }
            Some("modpack_unsupported") => {
                let e = LauncherError::pack(
                    ErrorCode::ModpackUnsupported,
                    "This modpack can't run as a server.",
                )
                .with_param("host", &host);
                return with(e, "reason", param("reason"));
            }
            Some("modpack_too_heavy") => {
                let e = LauncherError::pack(
                    ErrorCode::ModpackTooHeavy,
                    "This modpack needs more memory than you picked.",
                )
                .with_param("host", &host);
                let e = with(e, "requiredMemoryMb", param("requiredMemoryMb"));
                return with(e, "limitMemoryMb", param("limitMemoryMb"));
            }
            Some("modrinth_unavailable") => {
                return host_trouble(super::api::MODRINTH_HOST, reply.status)
            }
            Some("versions_unavailable") => return versions_unavailable(&host),
            Some("port_range_exhausted") => return host_full(&host),
            Some(_) => return self.refusal(reply),
            None => {}
        }
        // A reverse proxy that gave up on the long create answers its own 502 or 504: the
        // Engine may finish the create anyway.
        if matches!(reply.status, 502 | 504) {
            return answer_lost(&host, reply.status);
        }
        if reply.status == 400 {
            // The Engine names the field at fault (`data.field`) on a create it refuses
            // without a code; `serverName` is the form's name.
            let field = param("field").map(|f| if f == "serverName" { "name".into() } else { f });
            return with(
                net(
                    ErrorCode::InvalidTeamServer,
                    "The server couldn't be created with these choices.",
                ),
                "field",
                field,
            );
        }
        self.refusal(reply)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn untouched_options_are_no_customization() {
        assert_eq!(
            ServerCustomization::from_settings(&FreeServerSettings::default()),
            None
        );
    }

    #[test]
    fn hardcore_is_survival_with_hardcore_on_hard() {
        let c = ServerCustomization::from_settings(&FreeServerSettings {
            game_mode: Some("hardcore".into()),
            difficulty: Some("easy".into()),
            ..FreeServerSettings::default()
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(c).unwrap(),
            json!({ "mode": "survival", "hardcore": true, "difficulty": "hard" })
        );
    }

    #[test]
    fn the_whitelist_turns_enforce_on_too() {
        let c = ServerCustomization::from_settings(&FreeServerSettings {
            whitelist: Some(false),
            game_mode: Some("creative".into()),
            ..FreeServerSettings::default()
        })
        .unwrap();
        assert_eq!(
            serde_json::to_value(c).unwrap(),
            json!({ "mode": "creative", "enableWhitelist": false, "enforceWhitelist": false })
        );
    }

    #[test]
    fn a_create_body_leaves_out_what_is_none() {
        let body = serde_json::to_value(CreateServer {
            server_name: "a".into(),
            ram: "4G".into(),
            cpu: 2.0,
            ..CreateServer::default()
        })
        .unwrap();
        assert_eq!(body, json!({ "serverName": "a", "ram": "4G", "cpu": 2.0 }));
    }
}
