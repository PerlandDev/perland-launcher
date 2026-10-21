//! "Create a server" on a team (free-server settings spec §6.3): the teams the player may create
//! on (owner or member, not the free team, whose server is the free server), how big a server
//! each can make now, the Engine's version lists for a server type, and the create itself, from
//! a type and version or from one of the player's instances. Every call carries a JWT scoped to
//! the team, minted as Manage mints it; nothing here logs one.
use launcher_core::{
    instances::SourceRef,
    meta::loaders::LoaderKind,
    perland::{
        CreateServer, CreatedServer, EngineVersion, FreeServerSettings, ServerCustomization,
        TeamUsage,
    },
    share::{self, PackMeta},
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{
        CreatableTeam, FriendsCheck, TeamCreateRequest, TeamInstanceRequest, TeamRoom,
        TeamServerCreated,
    },
    events::Host,
    free_servers::{checked_settings, valid_minecraft_version, valid_name, valid_version_id},
    manage::is_free,
    perland::host_of,
    runner::Runner,
};

/// The server types the form offers.
const TYPES: [&str; 6] = ["vanilla", "paper", "fabric", "forge", "neoforge", "quilt"];
/// The types that take a loader version.
const MODDED: [&str; 4] = ["fabric", "forge", "neoforge", "quilt"];
/// The memory picked first, and the cores a server gets (our call: not asked), unless the team
/// has less left.
const DEFAULT_GB: u32 = 4;
const DEFAULT_CPU: f64 = 2.0;
/// The Engine's smallest CPU limit.
const MIN_CPU: f64 = 0.1;
/// The most memory choices offered, so a large plan's select stays short.
const MAX_GB: u32 = 64;
/// The Engine's own cap on max players (a team has no plan cap).
const MAX_PLAYERS: u32 = 1000;
/// The largest pack the Engine takes (`packcheck.ModpackMaxBytes`).
const PACK_MAX_BYTES: u64 = 64 << 20;

/// A refusal of one field, named as the form names it (`name`, `version`, `ram`,
/// `settings.maxPlayers`, ...), so the UI puts the line under it.
fn invalid(field: &str, message: &str) -> LauncherError {
    LauncherError::network(ErrorCode::InvalidTeamServer, message).with_param("field", field)
}

fn bad_name() -> LauncherError {
    invalid("name", "Use letters, numbers, - and _ only, up to 32.")
}

/// A loader version the Engine could take: 1 to 64 ASCII letters, digits, `.`, `_`, `+`, `-`.
fn valid_loader_version(version: &str) -> bool {
    (1..=64).contains(&version.len())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

fn loader_type(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Fabric => "fabric",
        LoaderKind::Quilt => "quilt",
        LoaderKind::Forge => "forge",
        LoaderKind::NeoForge => "neoforge",
    }
}

/// How big a server the team can make: whole GB up to what it has available; with less than
/// 1 GB left (or nothing known), up to its plan's limit, so the server is made and waits,
/// stopped, for room (`started: false`). 4 GB first, or the most below it. 2 cores, or the
/// most the team has left below that.
pub(crate) fn room_of(usage: &TeamUsage) -> TeamRoom {
    let gb = |mb: Option<i64>| {
        mb.filter(|mb| *mb >= 1024)
            .map(|mb| u32::try_from(mb / 1024).unwrap_or(MAX_GB))
    };
    let max = gb(usage.available.memory_mb)
        .or_else(|| gb(usage.limits.max_memory_mb))
        .unwrap_or(DEFAULT_GB)
        .min(MAX_GB);
    let left = |cpu: Option<f64>| cpu.filter(|c| c.is_finite() && *c >= MIN_CPU);
    let cpu = left(usage.available.cpu)
        .or_else(|| left(usage.limits.max_cpu))
        .unwrap_or(DEFAULT_CPU)
        .min(DEFAULT_CPU);
    // Down to a tenth of a core, as the Engine takes it.
    let cpu = ((cpu * 10.0).floor() / 10.0).max(MIN_CPU);
    TeamRoom {
        memory_gb: (1..=max).collect(),
        default_gb: DEFAULT_GB.min(max),
        cpu,
        servers_left: usage
            .available
            .servers
            .map(|n| u32::try_from(n.max(0)).unwrap_or(u32::MAX)),
    }
}

/// The Options, refused as the Engine would, with a team's numbers (max players up to the
/// Engine's 1000, no plan cap).
fn checked_team_settings(
    settings: Option<FreeServerSettings>,
) -> Result<Option<ServerCustomization>> {
    let settings = checked_settings(settings).map_err(|e| {
        let field = e.params.get("field").cloned().unwrap_or_default();
        invalid(&field, "Check the server's options.")
    })?;
    let Some(settings) = settings else {
        return Ok(None);
    };
    if settings.max_players.is_some_and(|n| n > MAX_PLAYERS) {
        return Err(invalid(
            "settings.maxPlayers",
            "Check the server's options.",
        ));
    }
    Ok(ServerCustomization::from_settings(&settings))
}

fn created(tenant: &str, made: CreatedServer) -> TeamServerCreated {
    TeamServerCreated {
        tenant_id: tenant.to_string(),
        stack: made.server_name,
        started: made.started,
        not_started_reason: made.not_started_reason.filter(|r| !r.is_empty()),
        address: made.address.filter(|a| !a.trim().is_empty()),
    }
}

fn too_heavy(host: &str, required: u32, limit: u32) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ModpackTooHeavy,
        "This modpack needs more memory than you picked.",
    )
    .with_param("host", host)
    .with_param("requiredMemoryMb", required.to_string())
    .with_param("limitMemoryMb", limit.to_string())
}

/// As the Engine's /create-server/pack answers it: `modpack_unsupported`, reason `not_on_modrinth`.
fn not_on_modrinth(names: &[String]) -> LauncherError {
    LauncherError::pack(
        ErrorCode::ModpackUnsupported,
        "Servers only run mods from Modrinth.",
    )
    .with_param("reason", "not_on_modrinth")
    .with_detail(names.join(", "))
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The teams the player may create a server on: owner or member, never a viewer's, and not
    /// the free team. By name. Signed out, or on an Engine without player sessions: none.
    pub async fn creatable_teams(&self) -> Result<Vec<CreatableTeam>> {
        if self.perland_session().await.is_none() {
            return Ok(vec![]);
        }
        let teams = match self.memberships().await {
            Ok(teams) => teams,
            Err(e) if e.code == ErrorCode::PlayerAccountsClosed => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        let mut teams: Vec<CreatableTeam> = teams
            .into_iter()
            .filter(|t| matches!(t.role.as_str(), "owner" | "member") && !is_free(t))
            .map(|t| CreatableTeam {
                tenant_id: t.tenant_id,
                name: t.display_name,
            })
            .collect();
        teams.sort_by(|a, b| {
            (a.name.to_lowercase(), &a.tenant_id).cmp(&(b.name.to_lowercase(), &b.tenant_id))
        });
        Ok(teams)
    }

    /// How big a server `tenant` can make now.
    pub async fn team_room(&self, tenant: &str) -> Result<TeamRoom> {
        let api = self.runner.stack();
        let usage = self
            .with_jwt(tenant, |engine, jwt| api.usage(engine, jwt))
            .await?;
        Ok(room_of(&usage))
    }

    /// The Minecraft versions a server type runs on `tenant`, newest first; with `minecraft`,
    /// that version's loader versions (a modded type only).
    pub async fn team_versions(
        &self,
        tenant: &str,
        server_type: &str,
        minecraft: Option<String>,
    ) -> Result<Vec<EngineVersion>> {
        if !TYPES.contains(&server_type) {
            return Err(invalid("type", "Pick a server type."));
        }
        let minecraft = minecraft.filter(|m| !m.trim().is_empty());
        if let Some(minecraft) = &minecraft {
            if !MODDED.contains(&server_type) || !valid_minecraft_version(minecraft) {
                return Err(invalid("version", "Pick a Minecraft version."));
            }
        }
        let api = self.runner.stack();
        self.with_jwt(tenant, |engine, jwt| {
            api.versions(engine, jwt, server_type.to_string(), minecraft.clone())
        })
        .await
    }

    /// The memory `memory_gb` asks for, as the Engine's `ram`, and the cores, refused when the
    /// team's choices don't hold it.
    async fn team_size(&self, tenant: &str, memory_gb: u32) -> Result<(String, f64)> {
        let room = self.team_room(tenant).await?;
        if !room.memory_gb.contains(&memory_gb) {
            return Err(invalid("ram", "Pick how much memory the server gets."));
        }
        Ok((format!("{memory_gb}G"), room.cpu))
    }

    async fn send_create(
        &self,
        tenant: &str,
        server: CreateServer,
        pack: Option<Vec<u8>>,
    ) -> Result<TeamServerCreated> {
        let api = self.runner.stack();
        let made = match pack {
            None => {
                self.with_jwt(tenant, |engine, jwt| {
                    api.create_server(engine, jwt, server.clone())
                })
                .await?
            }
            Some(pack) => {
                self.with_jwt(tenant, |engine, jwt| {
                    api.create_server_from_pack(engine, jwt, server.clone(), pack.clone())
                })
                .await?
            }
        };
        Ok(created(tenant, made))
    }

    /// Create a server on a team from a type, a Minecraft version and, for a modded type, a
    /// loader version (left out: the newest). Everything is checked before anything is sent.
    pub async fn team_create(&self, request: TeamCreateRequest) -> Result<TeamServerCreated> {
        let name = request.name.trim();
        if !valid_name(name) {
            return Err(bad_name());
        }
        let server_type = request.r#type.as_str();
        if !TYPES.contains(&server_type) {
            return Err(invalid("type", "Pick a server type."));
        }
        let version = request.version.trim();
        if !valid_minecraft_version(version) {
            return Err(invalid("version", "Pick a Minecraft version."));
        }
        let loader_version = request
            .loader_version
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        if let Some(loader) = &loader_version {
            if !MODDED.contains(&server_type) || !valid_loader_version(loader) {
                return Err(invalid("loaderVersion", "Pick a loader version."));
            }
        }
        let customization = checked_team_settings(request.settings)?;
        let (ram, cpu) = self
            .team_size(&request.tenant_id, request.memory_gb)
            .await?;
        let server = CreateServer {
            server_name: name.to_string(),
            r#type: Some(server_type.to_string()),
            version: Some(version.to_string()),
            loader_version,
            ram,
            cpu,
            customization,
            modrinth_version_id: None,
        };
        self.send_create(&request.tenant_id, server, None).await
    }

    /// What a server made on `tenant` from instance `id`'s server pack would hold: as
    /// `friends_check`, against the most memory the team can give it.
    pub async fn team_check(&self, tenant: &str, id: &str) -> Result<FriendsCheck> {
        let instance = self.friends_instance(id).await.map_err(team_words)?;
        if instance.game.loader.is_none() {
            return Err(LauncherError::network(
                ErrorCode::InvalidTeamServer,
                "Only modded instances have mods to check.",
            ));
        }
        let room = self.team_room(tenant).await?;
        let plan = self.share_plan(&instance).await?;
        let check = share::server_check(&plan);
        {
            let mut plans = self.share_plans.lock().unwrap_or_else(|e| e.into_inner());
            plans.clear();
            plans.insert(instance.id.clone(), plan);
        }
        Ok(FriendsCheck {
            name: instance.name,
            mods: check.mods,
            server_mods: check.server_mods,
            required_mb: check.required_mb,
            limit_mb: room.memory_gb.last().copied().unwrap_or(0) * 1024,
            not_on_modrinth: check.not_on_modrinth,
            settings: check.settings,
        })
    }

    /// Create a server on a team from instance `id`. No loader: vanilla on the instance's
    /// version. Installed from a Modrinth modpack: that pack by its version id. A loader and no
    /// mods: that type, version and loader version. Any other modded instance: its server pack,
    /// checked against the memory picked before anything is sent, then uploaded.
    pub async fn team_create_from_instance(
        &self,
        request: TeamInstanceRequest,
    ) -> Result<TeamServerCreated> {
        let name = request.name.trim();
        if !valid_name(name) {
            return Err(bad_name());
        }
        let customization = checked_team_settings(request.settings)?;
        let instance = self
            .friends_instance(&request.id)
            .await
            .map_err(team_words)?;
        let tenant = request.tenant_id.as_str();
        let (ram, cpu) = self.team_size(tenant, request.memory_gb).await?;
        let base = CreateServer {
            server_name: name.to_string(),
            ram,
            cpu,
            customization,
            ..CreateServer::default()
        };
        let minecraft = instance.game.minecraft.clone();
        let Some(loader) = instance.game.loader.clone() else {
            if !valid_minecraft_version(&minecraft) {
                return Err(invalid(
                    "version",
                    "Servers can't run this instance's Minecraft version.",
                ));
            }
            let server = CreateServer {
                r#type: Some("vanilla".into()),
                version: Some(minecraft),
                ..base
            };
            return self.send_create(tenant, server, None).await;
        };
        if let SourceRef::Modrinth { version_id, .. } = &instance.source {
            if !valid_version_id(version_id) {
                return Err(invalid(
                    "pack",
                    "This modpack's Modrinth version can't be used. Reinstall the modpack from Browse.",
                ));
            }
            let server = CreateServer {
                modrinth_version_id: Some(version_id.clone()),
                ..base
            };
            return self.send_create(tenant, server, None).await;
        }
        let kept = self
            .share_plans
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&instance.id);
        let plan = match kept {
            Some(plan) => plan,
            None => self.share_plan(&instance).await?,
        };
        let check = share::server_check(&plan);
        if check.mods == 0 {
            let server = CreateServer {
                r#type: Some(loader_type(loader.kind).into()),
                version: Some(minecraft),
                loader_version: Some(loader.version.clone()).filter(|v| valid_loader_version(v)),
                ..base
            };
            return self.send_create(tenant, server, None).await;
        }
        if !check.not_on_modrinth.is_empty() {
            return Err(not_on_modrinth(&check.not_on_modrinth));
        }
        let limit_mb = request.memory_gb * 1024;
        if check.required_mb > limit_mb {
            let host = self
                .perland_session()
                .await
                .map(|s| host_of(&s.engine).to_string())
                .unwrap_or_default();
            return Err(too_heavy(&host, check.required_mb, limit_mb));
        }
        // Inside `.perland/`, which no scan reads, and removed once read.
        let out = self
            .instances
            .perland_dir(&instance.id)
            .join("server-pack.mrpack");
        let meta = PackMeta {
            name: instance.name.clone(),
            version_id: share::today_utc(),
            minecraft,
            loader: Some(loader),
        };
        share::write_server(&self.instances.game_dir(&instance.id), &plan, meta, &out).await?;
        let pack = tokio::fs::read(&out).await;
        let _ = tokio::fs::remove_file(&out).await;
        let pack = pack?;
        if pack.len() as u64 > PACK_MAX_BYTES {
            return Err(LauncherError::pack(
                ErrorCode::ShareTooLarge,
                "Your settings are too big for a server.",
            ));
        }
        self.send_create(tenant, base, Some(pack)).await
    }
}

/// The instance refusals `friends_instance` words for a free server, as a team's.
fn team_words(e: LauncherError) -> LauncherError {
    if e.code == ErrorCode::InvalidFreeServer {
        return LauncherError::network(ErrorCode::InvalidTeamServer, e.message);
    }
    e
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::LoaderSpec,
        invite::Url,
        perland::{Membership, Profile, Session, UsageAvailable, UsageLimits},
        share::Linked,
    };

    use super::*;
    use crate::{
        app::tests::{core, TestCore},
        dto::NewInstance,
        testing::{player_token, FakeRunner, FakeStack},
    };

    const CDN: &str = "https://cdn.modrinth.com/data/AAAA/versions/v1/known.jar";

    fn sign_in(core: &TestCore) {
        core.perland_store
            .save(&Session {
                engine: Url::parse("https://api.example.com/").unwrap(),
                token: player_token('s'),
                profile: Profile {
                    name: "Steve".into(),
                    email: None,
                    discord: true,
                    github: false,
                    minecraft: None,
                },
            })
            .unwrap();
    }

    fn team(id: &str, name: &str, display: &str, role: &str, plan: Option<&str>) -> Membership {
        Membership {
            tenant_id: id.into(),
            tenant_name: name.into(),
            display_name: display.into(),
            role: role.into(),
            plan: plan.map(str::to_string),
        }
    }

    fn usage(memory_mb: Option<i64>, cpu: Option<f64>) -> TeamUsage {
        TeamUsage {
            available: UsageAvailable {
                cpu,
                memory_mb,
                servers: Some(2),
            },
            limits: UsageLimits {
                max_cpu: Some(4.0),
                max_memory_mb: Some(8192),
                max_servers: Some(3),
            },
        }
    }

    fn stack(runner: &FakeRunner) -> std::sync::Arc<FakeStack> {
        runner.stack.clone()
    }

    fn roomy(runner: &FakeRunner) {
        *runner.stack.usage.lock().unwrap() = Ok(usage(Some(8192), Some(4.0)));
    }

    fn request(name: &str, server_type: &str, version: &str) -> TeamCreateRequest {
        TeamCreateRequest {
            tenant_id: "t-b".into(),
            name: name.into(),
            r#type: server_type.into(),
            version: version.into(),
            loader_version: None,
            memory_gb: 4,
            settings: None,
        }
    }

    #[test]
    fn room_offers_whole_gb_up_to_what_is_available() {
        let room = room_of(&usage(Some(6000), Some(3.0)));
        assert_eq!(room.memory_gb, [1, 2, 3, 4, 5]);
        assert_eq!(room.default_gb, 4);
        assert_eq!(room.cpu, 2.0);

        // Less than 4 GB left: the most there is; less than 2 cores: the most there is.
        let room = room_of(&usage(Some(3072), Some(1.25)));
        assert_eq!(room.memory_gb, [1, 2, 3]);
        assert_eq!(room.default_gb, 3);
        assert_eq!(room.cpu, 1.2);
    }

    #[test]
    fn room_keeps_how_many_more_servers_the_team_may_make() {
        assert_eq!(room_of(&usage(Some(6000), Some(3.0))).servers_left, Some(2));
        let mut full = usage(Some(6000), Some(3.0));
        full.available.servers = Some(0);
        assert_eq!(room_of(&full).servers_left, Some(0));
        full.available.servers = Some(-1);
        assert_eq!(room_of(&full).servers_left, Some(0));
        assert_eq!(room_of(&TeamUsage::default()).servers_left, None);
    }

    #[test]
    fn a_team_with_no_room_offers_its_plan() {
        // Nothing left to run it now: it is made stopped, so the plan's limit bounds it.
        let room = room_of(&usage(Some(512), Some(0.0)));
        assert_eq!(room.memory_gb, (1..=8).collect::<Vec<_>>());
        assert_eq!(room.default_gb, 4);
        assert_eq!(room.cpu, 2.0);
        // Docker unread: the same.
        let room = room_of(&usage(None, None));
        assert_eq!(room.memory_gb.len(), 8);
        // Nothing known at all.
        let room = room_of(&TeamUsage::default());
        assert_eq!(room.memory_gb, [1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn creatable_teams_are_owner_and_member_teams_but_the_free_one() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        *stack(&runner).memberships.lock().unwrap() = Ok(vec![
            team("t-b", "bravo", "Bravo", "member", Some("pro")),
            team("t-v", "viewer", "Viewer", "viewer", Some("pro")),
            team("t-a", "alpha", "alpha", "owner", None),
            team(
                "t-f",
                "player-abc",
                "Steve's server",
                "owner",
                Some("player-free"),
            ),
        ]);
        let teams = core.creatable_teams().await.unwrap();
        assert_eq!(
            teams,
            [
                CreatableTeam {
                    tenant_id: "t-a".into(),
                    name: "alpha".into()
                },
                CreatableTeam {
                    tenant_id: "t-b".into(),
                    name: "Bravo".into()
                },
            ]
        );
    }

    #[tokio::test]
    async fn creatable_teams_signed_out_are_none() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert!(core.creatable_teams().await.unwrap().is_empty());
        assert!(stack(&runner).calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn room_reads_the_teams_usage_with_its_jwt() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        *stack(&runner).usage.lock().unwrap() = Ok(usage(Some(2048), Some(1.0)));
        let room = core.team_room("t-b").await.unwrap();
        assert_eq!(
            (room.memory_gb, room.default_gb, room.cpu),
            (vec![1, 2], 2, 1.0)
        );
        assert_eq!(stack(&runner).calls_for_tenant("usage", "t-b").len(), 1);
    }

    #[tokio::test]
    async fn versions_ask_the_teams_engine() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        let fake = stack(&runner);
        fake.versions.lock().unwrap().insert(
            ("forge".into(), Some("1.20.1".into())),
            vec![EngineVersion {
                id: "47.3.0".into(),
                stable: true,
                recommended: true,
            }],
        );
        let list = core
            .team_versions("t-b", "forge", Some("1.20.1".into()))
            .await
            .unwrap();
        assert_eq!(list[0].id, "47.3.0");
        assert_eq!(
            fake.calls_for_tenant("versions", "t-b")[0].args,
            ["forge", "1.20.1"]
        );

        // A type the form doesn't offer, and a loader list for vanilla: refused unsent.
        let e = core.team_versions("t-b", "spigot", None).await.unwrap_err();
        assert_eq!(e.params.get("field").map(String::as_str), Some("type"));
        let e = core
            .team_versions("t-b", "vanilla", Some("1.20.1".into()))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidTeamServer);
        assert_eq!(fake.calls_to("versions").len(), 1);
    }

    #[tokio::test]
    async fn team_create_sends_the_form_sized_to_the_team() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        *stack(&runner).usage.lock().unwrap() = Ok(usage(Some(8192), Some(1.5)));
        let made = core
            .team_create(TeamCreateRequest {
                loader_version: Some("0.16.14".into()),
                memory_gb: 6,
                settings: Some(FreeServerSettings {
                    game_mode: Some("hardcore".into()),
                    max_players: Some(40),
                    ..FreeServerSettings::default()
                }),
                ..request(" Amigos ", "fabric", "1.20.1")
            })
            .await
            .unwrap();
        assert_eq!(
            made,
            TeamServerCreated {
                tenant_id: "t-b".into(),
                stack: "Amigos".into(),
                started: true,
                not_started_reason: None,
                address: None,
            }
        );
        let fake = stack(&runner);
        assert_eq!(
            *fake.created.lock().unwrap(),
            [CreateServer {
                server_name: "Amigos".into(),
                r#type: Some("fabric".into()),
                version: Some("1.20.1".into()),
                loader_version: Some("0.16.14".into()),
                ram: "6G".into(),
                cpu: 1.5,
                customization: Some(ServerCustomization {
                    mode: Some("survival".into()),
                    hardcore: Some(true),
                    difficulty: Some("hard".into()),
                    max_players: Some(40),
                    ..ServerCustomization::default()
                }),
                modrinth_version_id: None,
            }]
        );
        assert_eq!(fake.calls_for_tenant("create_server", "t-b").len(), 1);
    }

    #[tokio::test]
    async fn started_false_reaches_the_answer() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        *stack(&runner).create_server.lock().unwrap() = Ok(CreatedServer {
            server_name: "Amigos".into(),
            status: "stopped".into(),
            started: false,
            not_started_reason: Some("quota_exceeded".into()),
            address: Some("play.example.com:25570".into()),
        });
        let made = core
            .team_create(request("Amigos", "vanilla", "1.21.1"))
            .await
            .unwrap();
        assert!(!made.started);
        assert_eq!(made.not_started_reason.as_deref(), Some("quota_exceeded"));
        assert_eq!(made.address.as_deref(), Some("play.example.com:25570"));
    }

    #[tokio::test]
    async fn team_create_refuses_before_sending() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        let field = |e: LauncherError| {
            assert_eq!(e.code, ErrorCode::InvalidTeamServer);
            e.params.get("field").cloned().unwrap_or_default()
        };
        let cases = [
            (request("a b", "vanilla", "1.21.1"), "name"),
            (request("Amigos", "spigot", "1.21.1"), "type"),
            (request("Amigos", "vanilla", "1.21 1"), "version"),
            (request("Amigos", "vanilla", ""), "version"),
            (
                TeamCreateRequest {
                    loader_version: Some("0.16".into()),
                    ..request("Amigos", "paper", "1.21.1")
                },
                "loaderVersion",
            ),
            (
                TeamCreateRequest {
                    settings: Some(FreeServerSettings {
                        max_players: Some(1001),
                        ..FreeServerSettings::default()
                    }),
                    ..request("Amigos", "vanilla", "1.21.1")
                },
                "settings.maxPlayers",
            ),
            (
                TeamCreateRequest {
                    settings: Some(FreeServerSettings {
                        game_mode: Some("hardcore".into()),
                        difficulty: Some("easy".into()),
                        ..FreeServerSettings::default()
                    }),
                    ..request("Amigos", "vanilla", "1.21.1")
                },
                "settings.difficulty",
            ),
            (
                TeamCreateRequest {
                    memory_gb: 9,
                    ..request("Amigos", "vanilla", "1.21.1")
                },
                "ram",
            ),
        ];
        for (request, want) in cases {
            let e = core.team_create(request).await.unwrap_err();
            assert_eq!(field(e), want);
        }
        assert!(stack(&runner).created.lock().unwrap().is_empty());
    }

    async fn instance(core: &TestCore, loader: bool, files: &[(&str, &[u8])]) -> String {
        let instance = core
            .instance_create(NewInstance {
                name: "Sky Friends".into(),
                minecraft: "1.20.1".into(),
                loader: loader.then(|| LoaderSpec {
                    kind: LoaderKind::NeoForge,
                    version: "20.1.5".into(),
                }),
            })
            .await
            .unwrap();
        let game = core.instances.game_dir(&instance.id);
        for (rel, body) in files {
            let path = game.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        instance.id
    }

    fn from(id: &str, memory_gb: u32) -> TeamInstanceRequest {
        TeamInstanceRequest {
            tenant_id: "t-b".into(),
            id: id.into(),
            name: "Amigos".into(),
            memory_gb,
            settings: None,
        }
    }

    fn knows(runner: &FakeRunner, body: &[u8]) {
        let sha1 = launcher_core::download::hash::sha1_hex(body);
        runner.browser.share_linked.lock().unwrap().insert(
            sha1.clone(),
            Linked {
                sha1,
                sha512: launcher_core::download::hash::sha512_hex(body),
                size: body.len() as u64,
                url: CDN.into(),
                env: None,
            },
        );
    }

    #[tokio::test]
    async fn a_vanilla_instance_is_vanilla_on_its_version() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        let id = instance(&core, false, &[]).await;
        core.team_create_from_instance(from(&id, 2)).await.unwrap();
        let sent = stack(&runner).created.lock().unwrap().clone();
        assert_eq!(sent[0].r#type.as_deref(), Some("vanilla"));
        assert_eq!(sent[0].version.as_deref(), Some("1.20.1"));
        assert_eq!(sent[0].loader_version, None);
        assert_eq!(sent[0].ram, "2G");
    }

    #[tokio::test]
    async fn a_loader_instance_without_mods_is_that_type() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        let id = instance(&core, true, &[("config/a.toml", b"a = 1")]).await;
        core.team_create_from_instance(from(&id, 4)).await.unwrap();
        let fake = stack(&runner);
        let sent = fake.created.lock().unwrap().clone();
        assert_eq!(sent[0].r#type.as_deref(), Some("neoforge"));
        assert_eq!(sent[0].version.as_deref(), Some("1.20.1"));
        assert_eq!(sent[0].loader_version.as_deref(), Some("20.1.5"));
        assert!(fake.calls_to("create_server_from_pack").is_empty());
    }

    #[tokio::test]
    async fn a_modrinth_instance_is_its_version_id_and_no_type() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        let id = instance(&core, true, &[("mods/own.jar", b"own")]).await;
        let mut stored = core.instances.load(&id).await.unwrap();
        stored.source = SourceRef::Modrinth {
            project_id: "1KVo5zza".into(),
            version_id: "Yp8wLY1P".into(),
        };
        core.instances.update(&stored).await.unwrap();
        core.team_create_from_instance(from(&id, 4)).await.unwrap();
        let sent = stack(&runner).created.lock().unwrap().clone();
        assert_eq!(sent[0].modrinth_version_id.as_deref(), Some("Yp8wLY1P"));
        assert_eq!(
            (sent[0].r#type.clone(), sent[0].version.clone()),
            (None, None)
        );
        assert!(runner.browser.share_lookups.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_modded_instance_uploads_its_pack_checked_against_the_memory_picked() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        knows(&runner, b"known");
        let id = instance(&core, true, &[("mods/known.jar", b"known")]).await;
        let check = core.team_check("t-b", &id).await.unwrap();
        assert_eq!(
            (check.mods, check.required_mb, check.limit_mb),
            (1, 2560, 8192)
        );

        // 2560 MiB needed, 2 GB picked: refused, nothing sent.
        let e = core
            .team_create_from_instance(from(&id, 2))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::ModpackTooHeavy);
        assert_eq!(
            e.params.get("limitMemoryMb").map(String::as_str),
            Some("2048")
        );
        assert!(!e.message.to_lowercase().contains("free"), "{}", e.message);
        let fake = stack(&runner);
        assert!(fake.created.lock().unwrap().is_empty());

        core.team_create_from_instance(from(&id, 3)).await.unwrap();
        let sent = fake.created.lock().unwrap().clone();
        assert_eq!(
            fake.calls_for_tenant("create_server_from_pack", "t-b")
                .len(),
            1
        );
        assert_eq!(
            sent[0],
            CreateServer {
                server_name: "Amigos".into(),
                ram: "3G".into(),
                cpu: 2.0,
                ..CreateServer::default()
            }
        );
        let pack = fake.uploaded.lock().unwrap()[0].clone();
        let zip = zip::ZipArchive::new(std::io::Cursor::new(pack)).unwrap();
        let mut zip = zip;
        assert!(zip.by_name("modrinth.index.json").is_ok());
    }

    #[tokio::test]
    async fn mods_not_on_modrinth_are_refused_unsent() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        let id = instance(&core, true, &[("mods/own.jar", b"own")]).await;
        let e = core
            .team_create_from_instance(from(&id, 4))
            .await
            .unwrap_err();
        // As the Engine answers it on /create-server/pack.
        assert_eq!(e.code, ErrorCode::ModpackUnsupported);
        assert_eq!(
            e.params.get("reason").map(String::as_str),
            Some("not_on_modrinth")
        );
        assert!(!e.message.to_lowercase().contains("free"), "{}", e.message);
        assert!(stack(&runner).created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_linked_instance_is_refused_in_a_teams_words() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core);
        roomy(&runner);
        let id = instance(&core, false, &[]).await;
        let mut stored = core.instances.load(&id).await.unwrap();
        stored.source = SourceRef::Perland {
            invite: format!("https://play.example.com/{}", "A".repeat(43)),
            engine: "https://api.example.com".into(),
        };
        core.instances.update(&stored).await.unwrap();
        let e = core
            .team_create_from_instance(from(&id, 4))
            .await
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidTeamServer);
        assert!(stack(&runner).created.lock().unwrap().is_empty());
    }
}
