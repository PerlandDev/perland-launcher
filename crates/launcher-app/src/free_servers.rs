//! A free server for friends ("Create a server for friends" spec §5 and §6.2): the player's own
//! free server on the session's Engine, as a card, and the commands that create, start, delete
//! and play it. Every call goes to the session's Engine with the session's token; a 401 forgets
//! the session. Nothing here logs a token or an invite.

use launcher_core::{
    instances::{ServerAddress, ServerStatus},
    invite::{non_empty, InviteLink, Url},
    perland::{CreateFreeServer, FreeServer, FreeServerSettings, Session},
    ErrorCode, LauncherError, Result,
};

use crate::{
    app::AppCore,
    dto::{FreeServerCard, FreeServerPage, PublicServerCard, ServerGame, ServerPlayers},
    events::Host,
    perland::{host_of, language_tag},
    public_servers::instance_of,
    runner::{BoxFuture, Runner},
};

/// The Engine's rule for a free server's name.
pub(crate) fn valid_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// The Engine's rule for a Modrinth version id.
pub(crate) fn valid_version_id(id: &str) -> bool {
    (1..=16).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric())
}

/// The Engine's compose-safe rule for a Minecraft version: 1 to 32 ASCII letters, digits, `.`,
/// `-` and `_` (`LATEST` passes). The Engine then looks it up in Mojang's list.
pub(crate) fn valid_minecraft_version(version: &str) -> bool {
    (1..=32).contains(&version.len())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// The game modes and difficulties a free server takes (`limits.settings` lists the same).
const GAME_MODES: [&str; 4] = ["survival", "creative", "adventure", "hardcore"];
const DIFFICULTIES: [&str; 4] = ["peaceful", "easy", "normal", "hard"];
/// The most bytes a seed or a server list message may have.
const SETTING_MAX_BYTES: usize = 64;

/// A refusal of one field, named as the Engine's `data.field` names it (`version`,
/// `settings.difficulty`, ...), so the UI can put the line under it.
fn invalid_field(field: &str, message: &str) -> LauncherError {
    LauncherError::network(ErrorCode::InvalidFreeServer, message).with_param("field", field)
}

/// The version to send: blank is none (the Engine's `LATEST`), else it must pass the Engine's
/// rule.
pub(crate) fn checked_version(version: Option<String>) -> Result<Option<String>> {
    match version.as_deref().map(str::trim) {
        None | Some("") => Ok(None),
        Some(v) if valid_minecraft_version(v) => Ok(Some(v.to_string())),
        Some(_) => Err(invalid_field(
            "version",
            "Free servers can't run that Minecraft version.",
        )),
    }
}

/// The settings to send, refused as the Engine would: a known game mode and difficulty (hard
/// with hardcore), at least one player, and a seed and message of at most 64 bytes with no
/// control characters (and no `§` in the message). Settings with nothing in them are none, so
/// an untouched form sends today's request.
pub(crate) fn checked_settings(
    settings: Option<FreeServerSettings>,
) -> Result<Option<FreeServerSettings>> {
    let Some(settings) = settings else {
        return Ok(None);
    };
    if settings == FreeServerSettings::default() {
        return Ok(None);
    }
    let bad =
        |name: &str| invalid_field(&format!("settings.{name}"), "Check the server's options.");
    if let Some(mode) = &settings.game_mode {
        if !GAME_MODES.contains(&mode.as_str()) {
            return Err(bad("gameMode"));
        }
    }
    if let Some(difficulty) = &settings.difficulty {
        let hardcore = settings.game_mode.as_deref() == Some("hardcore");
        if !DIFFICULTIES.contains(&difficulty.as_str()) || (hardcore && difficulty != "hard") {
            return Err(bad("difficulty"));
        }
    }
    if settings.max_players == Some(0) {
        return Err(bad("maxPlayers"));
    }
    let text_ok =
        |text: &str| text.len() <= SETTING_MAX_BYTES && !text.chars().any(char::is_control);
    if settings.seed.as_deref().is_some_and(|seed| !text_ok(seed)) {
        return Err(bad("seed"));
    }
    if settings
        .motd
        .as_deref()
        .is_some_and(|motd| !text_ok(motd) || motd.contains('§'))
    {
        return Err(bad("motd"));
    }
    Ok(Some(settings))
}

pub(crate) fn invalid() -> LauncherError {
    LauncherError::network(
        ErrorCode::InvalidFreeServer,
        "Check the server name: letters, numbers, - and _ only, up to 32.",
    )
}

fn signed_out() -> LauncherError {
    LauncherError::auth(
        ErrorCode::PerlandSignedOut,
        "You were signed out of Perland.",
    )
}

/// The statuses no game can join: Play is off on the card.
fn unplayable_status(status: &str) -> bool {
    matches!(status, "creating" | "deleting" | "stopped")
}

/// The mod loaders a free server's `game.type` can name.
fn modded_type(game_type: &str) -> bool {
    matches!(game_type, "fabric" | "forge" | "neoforge" | "quilt")
}

/// What a player types (`host`, `host:port`, `[v6]:port`) as `host:port`; `None` when it isn't
/// a valid address.
pub(crate) fn parse_address(raw: &str) -> Option<ServerAddress> {
    let raw = raw.trim();
    let (host, port) = if let Some(rest) = raw.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after {
            "" => Some(25565),
            _ => after.strip_prefix(':').and_then(|p| p.parse().ok()),
        };
        (host, port?)
    } else {
        match raw.split_once(':') {
            None => (raw, 25565),
            Some((host, port)) => (host, port.parse().ok()?),
        }
    };
    ServerAddress::new(host, port)
}

/// The free-server view as a card (spec §5 "Owner card"). `joined` is every server instance's
/// canonical invite and id, as `joined_invites` gives it.
pub(crate) fn free_card(server: &FreeServer, joined: &[(String, String)]) -> FreeServerCard {
    let link = server
        .invite_url
        .as_deref()
        .and_then(|url| InviteLink::parse(url).ok());
    let invite = link.as_ref().map(InviteLink::canonical);
    let status = match ServerStatus::parse(&server.status) {
        Some(status) => status,
        None if server.status == "stopped" => ServerStatus::Offline,
        None => ServerStatus::Sleeping,
    };
    let joinable = invite.is_some() && !unplayable_status(&server.status) && !server.disk.over;
    let instance = instance_of(joined, invite.as_deref());
    let owned = |s: &Option<String>| non_empty(s).map(str::to_string);
    // The Engine names a modpack server's loader in `game.type` only.
    let game_type = server.game.kind.trim().to_ascii_lowercase();
    let type_loader = (modded_type(&game_type) || (server.kind == "modrinth"))
        .then_some(game_type)
        .filter(|t| !t.is_empty());
    let loader = owned(&server.game.loader)
        .or(type_loader)
        .filter(|l| !l.eq_ignore_ascii_case("vanilla"));
    FreeServerCard {
        card: PublicServerCard {
            token: link
                .as_ref()
                .map(|l| l.token().to_string())
                .unwrap_or_default(),
            invite,
            joinable,
            address: server
                .address
                .as_deref()
                .and_then(parse_address)
                .map(|a| a.display()),
            name: server.name.trim().to_string(),
            display_name: server.name.trim().to_string(),
            icon_version: None,
            game: ServerGame {
                version: owned(&server.game.version),
                loader: loader.clone(),
                loader_version: owned(&server.game.loader_version),
            },
            modded: server.kind == "modrinth" || loader.is_some(),
            status,
            players: server.players.as_ref().map(ServerPlayers::from),
            whitelisted: false,
            pack: None,
            categories: vec![],
            language: String::new(),
            region: None,
            instance,
        },
        kind: server.kind.clone(),
        memory_mb: server.memory_mb,
        max_players: server.max_players,
        free_status: server.status.clone(),
        deletion: server.deletion.clone(),
        disk: server.disk.clone(),
        invite_url: server.invite_url.clone(),
        join_url: server.join_url.clone(),
        manage: None,
    }
}

/// Why Play can't join the free server: out of space, or nothing to join yet.
fn unplayable(server: &FreeServer, engine: &Url) -> Option<LauncherError> {
    if server.status == "stopped" || server.disk.over {
        return Some(
            LauncherError::network(
                ErrorCode::FreeServerOverDisk,
                "Your server is out of space, so it stays off.",
            )
            .with_param("host", host_of(engine))
            .with_param("usedBytes", server.disk.used_bytes.to_string())
            .with_param("limitBytes", server.disk.limit_bytes.to_string()),
        );
    }
    let has_invite = server
        .invite_url
        .as_deref()
        .is_some_and(|url| InviteLink::parse(url).is_ok());
    (unplayable_status(&server.status) || !has_invite).then(|| {
        let address = server
            .address
            .as_deref()
            .and_then(parse_address)
            .map(|a| a.display())
            .unwrap_or_else(|| host_of(engine).to_string());
        cannot_join(address)
    })
}

fn cannot_join(address: String) -> LauncherError {
    LauncherError::pack(
        ErrorCode::InviteUnsupported,
        format!(
            "This server can't be joined from the launcher yet. Join with the address {address}."
        ),
    )
    .with_param("address", address)
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// The session, or `perland_signed_out` without one.
    pub(crate) async fn free_session(&self) -> Result<Session> {
        self.perland_session().await.ok_or_else(signed_out)
    }

    /// `call` against the session's Engine; a 401 forgets the session.
    pub(crate) async fn free_call<T>(
        &self,
        session: &Session,
        call: impl FnOnce(Url, String) -> BoxFuture<Result<T>>,
    ) -> Result<T> {
        let answer = call(session.engine.clone(), session.token.clone()).await;
        if let Err(e) = &answer {
            if e.code == ErrorCode::PerlandSignedOut {
                self.drop_session(&session.token).await;
            }
        }
        answer
    }

    /// The free-server view as a card, with the instance that already joins it. The Engine
    /// already answered, so instances that can't be read leave the card without one rather
    /// than fail the call.
    pub(crate) async fn free_card_of(&self, server: &FreeServer) -> FreeServerCard {
        let joined = match self.joined_invites().await {
            Ok(joined) => joined,
            Err(e) => {
                tracing::warn!(code = ?e.code, "free server card without its instance");
                Vec::new()
            }
        };
        free_card(server, &joined)
    }

    /// The player's free server and what they may do, or `None` when signed out, when the
    /// Engine has free servers or accounts off, and after a 401 (which also signs them out).
    pub async fn free_server(&self) -> Result<Option<FreeServerPage>> {
        let Some(session) = self.perland_session().await else {
            return Ok(None);
        };
        let player = self.runner.player();
        let state = match self
            .free_call(&session, |e, t| player.free_server(e, t))
            .await
        {
            Ok(Some(state)) => state,
            Ok(None) => return Ok(None),
            Err(e)
                if matches!(
                    e.code,
                    ErrorCode::PerlandSignedOut | ErrorCode::PlayerAccountsClosed
                ) =>
            {
                return Ok(None)
            }
            Err(e) => return Err(e),
        };
        let server = match &state.server {
            Some(server) => {
                let (mut card, manage) =
                    tokio::join!(self.free_card_of(server), self.free_manage_ref(server));
                card.manage = manage;
                Some(card)
            }
            None => None,
        };
        Ok(Some(FreeServerPage {
            limits: state.limits,
            create: state.create,
            server,
        }))
    }

    /// Create the player's free server. Everything is checked against the Engine's rules
    /// before anything is sent: `kind` is `vanilla`, on `version` (blank or left out: the
    /// Engine's `LATEST`; no Modrinth version id is sent), or `modrinth` with the Modrinth
    /// version id of the pack (no version is sent: the pack decides it). `settings` left out
    /// keeps every default. `language` is the UI's.
    pub async fn free_server_create(
        &self,
        name: &str,
        kind: &str,
        version: Option<String>,
        modrinth_version_id: Option<String>,
        settings: Option<FreeServerSettings>,
        language: &str,
    ) -> Result<FreeServerCard> {
        let name = name.trim();
        if !valid_name(name) {
            return Err(invalid());
        }
        let (version, modrinth_version_id) = match kind {
            "vanilla" => (checked_version(version)?, None),
            "modrinth" => (
                None,
                Some(
                    modrinth_version_id
                        .filter(|id| valid_version_id(id))
                        .ok_or_else(invalid)?,
                ),
            ),
            _ => return Err(invalid()),
        };
        let request = CreateFreeServer {
            name: name.to_string(),
            kind: kind.to_string(),
            modrinth_version_id,
            version,
            settings: checked_settings(settings)?,
            language: language_tag(language).to_string(),
        };
        self.free_server_create_checked(request).await
    }

    /// Send a create already checked against the Engine's rules.
    pub(crate) async fn free_server_create_checked(
        &self,
        request: CreateFreeServer,
    ) -> Result<FreeServerCard> {
        let session = self.free_session().await?;
        let player = self.runner.player();
        let created = self
            .free_call(&session, |e, t| player.create_free_server(e, t, request))
            .await?;
        Ok(self.free_card_of(&created).await)
    }

    /// Start or wake the player's free server.
    pub async fn free_server_start(&self) -> Result<FreeServerCard> {
        let session = self.free_session().await?;
        let player = self.runner.player();
        let started = self
            .free_call(&session, |e, t| player.start_free_server(e, t))
            .await?;
        Ok(self.free_card_of(&started).await)
    }

    /// Delete the player's free server and its world. True when the Engine deleted one.
    pub async fn free_server_delete(&self) -> Result<bool> {
        let session = self.free_session().await?;
        let player = self.runner.player();
        self.free_call(&session, |e, t| player.delete_free_server(e, t))
            .await
    }

    /// Get the free server ready to play and answer the instance that joins it (spec §5 "Play
    /// on the owner's free server"): an `offline` server is started first, the invite is
    /// joined as any invite is, and after a start, or while the server starts, the instance's
    /// saved status is `starting`, so the run's wake waits until the server answers.
    pub async fn free_server_play(&self) -> Result<String> {
        let session = self.free_session().await?;
        let player = self.runner.player();
        let state = self
            .free_call(&session, |e, t| player.free_server(e, t))
            .await?;
        let mut server = state
            .and_then(|s| s.server)
            .ok_or_else(|| cannot_join(host_of(&session.engine).to_string()))?;
        if let Some(refusal) = unplayable(&server, &session.engine) {
            return Err(refusal);
        }
        let mut started = false;
        if server.status == "offline" {
            let fresh = self
                .free_call(&session, |e, t| player.start_free_server(e, t))
                .await?;
            server.invite_url = fresh.invite_url.or(server.invite_url);
            server.status = fresh.status;
            started = true;
        }
        let invite = server.invite_url.clone().unwrap_or_default();
        let instance = self.join_add(&invite).await?;
        if started || server.status == "starting" {
            self.update_server(&instance.id, false, |link| {
                if let Some(link) = link {
                    link.last_status = Some(ServerStatus::Starting);
                }
            })
            .await?;
        }
        Ok(instance.id)
    }
}

#[cfg(test)]
mod tests {
    use launcher_core::{
        accounts::BuildKind,
        instances::ServerStatus,
        invite::Url,
        perland::{FreeServerSettings, Profile, Session},
        ErrorCode, LauncherError,
    };

    use crate::{
        app::tests::{core, TestCore},
        servers::server_source,
        testing::{
            free_modrinth_view, free_state, free_view, invite, player_token, serve, server_view,
            FakeRunner,
        },
    };

    fn token(c: char) -> String {
        c.to_string().repeat(43)
    }

    fn sign_in(core: &TestCore, engine: &str) {
        core.perland_store
            .save(&Session {
                engine: Url::parse(engine).unwrap(),
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

    fn serve_free(runner: &FakeRunner, server: Option<launcher_core::perland::FreeServer>) {
        *runner.player.free.lock().unwrap() = Ok(Some(free_state(server)));
    }

    fn signed_out() -> LauncherError {
        LauncherError::auth(
            ErrorCode::PerlandSignedOut,
            "You were signed out of Perland.",
        )
    }

    #[tokio::test]
    async fn free_server_none_when_signed_out() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        assert_eq!(core.free_server().await.unwrap(), None);
        assert!(runner.player.calls.lock().unwrap().is_empty());
        for error in [
            core.free_server_start().await.unwrap_err(),
            core.free_server_delete().await.unwrap_err(),
            core.free_server_play().await.unwrap_err(),
            core.free_server_create("Survival", "vanilla", None, None, None, "en")
                .await
                .unwrap_err(),
        ] {
            assert_eq!(error.code, ErrorCode::PerlandSignedOut);
        }
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn free_server_none_when_off() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        assert_eq!(
            core.free_server().await.unwrap(),
            None,
            "the feature is off"
        );
        *runner.player.free.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::PlayerAccountsClosed,
            "Perland accounts aren't open yet.",
        ));
        assert_eq!(core.free_server().await.unwrap(), None, "accounts closed");
        assert!(core.perland_session().await.is_some());
        *runner.player.free.lock().unwrap() =
            Err(LauncherError::network(ErrorCode::Offline, "Couldn't reach"));
        assert_eq!(
            core.free_server().await.unwrap_err().code,
            ErrorCode::Offline
        );
    }

    #[tokio::test]
    async fn free_server_401_forgets() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        *runner.player.free.lock().unwrap() = Err(signed_out());
        assert_eq!(core.free_server().await.unwrap(), None);
        assert!(core.perland_session().await.is_none());
        assert!(core.perland_store.load().is_none());
    }

    #[tokio::test]
    async fn create_start_and_delete_after_401_forget() {
        for call in ["create", "start", "delete"] {
            let (core, runner, _host, _t) = core(BuildKind::Dev);
            sign_in(&core, "https://api.example.com");
            *runner.player.created.lock().unwrap() = Err(signed_out());
            *runner.player.started.lock().unwrap() = Err(signed_out());
            *runner.player.deleted.lock().unwrap() = Err(signed_out());
            let error = match call {
                "create" => core
                    .free_server_create("Survival", "vanilla", None, None, None, "en")
                    .await
                    .unwrap_err(),
                "start" => core.free_server_start().await.unwrap_err(),
                _ => core.free_server_delete().await.unwrap_err(),
            };
            assert_eq!(error.code, ErrorCode::PerlandSignedOut, "{call}");
            assert!(core.perland_session().await.is_none(), "{call}");
        }
    }

    #[tokio::test]
    async fn create_refuses_a_bad_name_without_a_call() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let long = "x".repeat(33);
        for (name, kind) in [
            ("", "vanilla"),
            ("my server", "vanilla"),
            (long.as_str(), "vanilla"),
            ("café", "vanilla"),
            ("a/b", "vanilla"),
            ("Survival", "paper"),
            ("Survival", ""),
        ] {
            let error = core
                .free_server_create(name, kind, None, None, None, "en")
                .await
                .unwrap_err();
            assert_eq!(
                error.code,
                ErrorCode::InvalidFreeServer,
                "{name:?} {kind:?}"
            );
        }
        assert!(runner.player.calls.lock().unwrap().is_empty());
        core.free_server_create(&"x".repeat(32), "vanilla", None, None, None, "en")
            .await
            .expect("32 characters is fine");
        core.free_server_create("my_world-2", "vanilla", None, None, None, "en")
            .await
            .unwrap();
        assert_eq!(runner.player.calls_to("create_free_server").len(), 2);
    }

    fn field(error: &LauncherError) -> Option<&str> {
        error.params.get("field").map(String::as_str)
    }

    #[tokio::test]
    async fn create_refuses_a_bad_version_without_a_call() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let long = "1".repeat(33);
        for version in ["1.21 1", long.as_str(), "1.21/1", "1.21\n2", "1.21.4é"] {
            let error = core
                .free_server_create(
                    "Survival",
                    "vanilla",
                    Some(version.into()),
                    None,
                    None,
                    "en",
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidFreeServer, "{version:?}");
            assert_eq!(field(&error), Some("version"), "{version:?}");
        }
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_sends_the_version() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let longest = "1".repeat(32);
        for version in ["1.21.4", "24w14a", "1.20.5-rc1", "LATEST", longest.as_str()] {
            core.free_server_create(
                "Survival",
                "vanilla",
                Some(version.into()),
                None,
                None,
                "en",
            )
            .await
            .unwrap();
        }
        // Blank is no version: the Engine's LATEST.
        core.free_server_create("Survival", "vanilla", Some("  ".into()), None, None, "en")
            .await
            .unwrap();
        // A Modrinth pack decides its own version.
        core.free_server_create(
            "Pack",
            "modrinth",
            Some("1.21.4".into()),
            Some("AbC123xy".into()),
            None,
            "en",
        )
        .await
        .unwrap();
        let sent: Vec<Option<String>> = runner
            .player
            .create_requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.version.clone())
            .collect();
        assert_eq!(
            sent,
            [
                Some("1.21.4".into()),
                Some("24w14a".into()),
                Some("1.20.5-rc1".into()),
                Some("LATEST".into()),
                Some(longest.clone()),
                None,
                None,
            ]
        );
    }

    #[tokio::test]
    async fn create_refuses_bad_settings_without_a_call() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let long = "s".repeat(65);
        let cases: Vec<(FreeServerSettings, &str)> = vec![
            (
                FreeServerSettings {
                    game_mode: Some("spectator".into()),
                    ..Default::default()
                },
                "settings.gameMode",
            ),
            (
                FreeServerSettings {
                    difficulty: Some("insane".into()),
                    ..Default::default()
                },
                "settings.difficulty",
            ),
            (
                FreeServerSettings {
                    game_mode: Some("hardcore".into()),
                    difficulty: Some("easy".into()),
                    ..Default::default()
                },
                "settings.difficulty",
            ),
            (
                FreeServerSettings {
                    max_players: Some(0),
                    ..Default::default()
                },
                "settings.maxPlayers",
            ),
            (
                FreeServerSettings {
                    seed: Some(long.clone()),
                    ..Default::default()
                },
                "settings.seed",
            ),
            (
                FreeServerSettings {
                    seed: Some("a\tb".into()),
                    ..Default::default()
                },
                "settings.seed",
            ),
            (
                FreeServerSettings {
                    motd: Some(long.clone()),
                    ..Default::default()
                },
                "settings.motd",
            ),
            (
                FreeServerSettings {
                    motd: Some("hi\nthere".into()),
                    ..Default::default()
                },
                "settings.motd",
            ),
            (
                FreeServerSettings {
                    motd: Some("§6gold".into()),
                    ..Default::default()
                },
                "settings.motd",
            ),
        ];
        for (settings, want) in cases {
            let error = core
                .free_server_create("Survival", "vanilla", None, None, Some(settings), "en")
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidFreeServer, "{want}");
            assert_eq!(field(&error), Some(want));
        }
        assert!(runner.player.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_sends_the_settings() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let settings = FreeServerSettings {
            game_mode: Some("hardcore".into()),
            difficulty: Some("hard".into()),
            max_players: Some(4),
            pvp: Some(false),
            whitelist: Some(true),
            seed: Some("s".repeat(64)),
            motd: Some("Hola amigos ñ".into()),
        };
        core.free_server_create(
            "Survival",
            "vanilla",
            Some("1.21.4".into()),
            None,
            Some(settings.clone()),
            "en",
        )
        .await
        .unwrap();
        // Settings with nothing in them are no settings: today's request.
        core.free_server_create(
            "Survival",
            "vanilla",
            None,
            None,
            Some(FreeServerSettings::default()),
            "en",
        )
        .await
        .unwrap();
        let sent = runner.player.create_requests.lock().unwrap().clone();
        assert_eq!(sent[0].settings, Some(settings));
        assert_eq!(sent[1].settings, None);
    }

    #[tokio::test]
    async fn create_modrinth_needs_a_version() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let long = "a".repeat(17);
        for version in [None, Some(""), Some("not-valid"), Some(long.as_str())] {
            let error = core
                .free_server_create(
                    "Pack",
                    "modrinth",
                    None,
                    version.map(Into::into),
                    None,
                    "en",
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidFreeServer, "{version:?}");
        }
        assert!(runner.player.calls.lock().unwrap().is_empty());
        core.free_server_create(
            "Pack",
            "modrinth",
            None,
            Some("AbC123xy".into()),
            None,
            "en",
        )
        .await
        .unwrap();
        let call = &runner.player.calls_to("create_free_server")[0];
        assert_eq!(
            call.args,
            [
                player_token('s'),
                "Pack".into(),
                "modrinth".into(),
                "AbC123xy".into(),
                "en".into()
            ]
        );
    }

    #[tokio::test]
    async fn create_sends_language() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let card = core
            .free_server_create(
                "Survival",
                "vanilla",
                None,
                Some("ignored1".into()),
                None,
                "es-MX",
            )
            .await
            .unwrap();
        core.free_server_create("Survival", "vanilla", None, None, None, "fr")
            .await
            .unwrap();
        let calls = runner.player.calls_to("create_free_server");
        assert_eq!(
            calls[0].args,
            [
                player_token('s'),
                "Survival".into(),
                "vanilla".into(),
                "".into(),
                "es".into()
            ],
            "a vanilla server sends no version id"
        );
        assert_eq!(calls[1].args[4], "en");
        assert_eq!(card.free_status, "starting");
        assert_eq!(card.card.status, ServerStatus::Starting);
        assert_eq!(card.card.invite.as_deref(), Some(invite('F').as_str()));
    }

    #[tokio::test]
    async fn card_built_from_view() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, Some(free_view("online")));
        let page = core.free_server().await.unwrap().expect("signed in and on");
        assert_eq!(page.limits.max_players, 10);
        assert_eq!(page.limits.memory_mb.modded, 6144);
        assert!(!page.create.allowed);
        assert_eq!(page.create.reason.as_deref(), Some("free_server_exists"));
        let server = page.server.expect("a server");
        let card = &server.card;
        assert_eq!(card.token, token('F'));
        assert_eq!(card.invite.as_deref(), Some(invite('F').as_str()));
        assert!(card.joinable);
        assert_eq!(card.status, ServerStatus::Online);
        assert_eq!(card.name, "Survival");
        assert_eq!(card.address.as_deref(), Some("play.example.com:25570"));
        assert_eq!(
            card.game.version.as_deref(),
            Some("LATEST"),
            "the UI says it in words"
        );
        assert_eq!(card.game.loader, None);
        assert!(!card.modded);
        assert_eq!(card.players.as_ref().unwrap().online, 1);
        assert_eq!(card.instance, None);
        assert_eq!(server.kind, "vanilla");
        assert_eq!(server.memory_mb, 2048);
        assert_eq!(server.max_players, 10);
        assert_eq!(server.free_status, "online");
        assert_eq!(server.invite_url.as_deref(), Some(invite('F').as_str()));
        assert!(server
            .join_url
            .as_deref()
            .unwrap()
            .starts_with("perland://join"));
        assert_eq!(server.disk.limit_bytes, 10_000);

        // Offline is kept, and an instance that already joins it is named.
        let mut offline = free_view("offline");
        offline.address = Some("play.example.com".into());
        serve_free(&runner, Some(offline));
        serve(&runner, &invite('F'), Ok(server_view("Survival")));
        let joined = core.join_add(&invite('F')).await.unwrap();
        let card = core
            .free_server()
            .await
            .unwrap()
            .unwrap()
            .server
            .unwrap()
            .card;
        assert_eq!(card.status, ServerStatus::Offline);
        assert!(card.joinable, "Play starts it");
        assert_eq!(card.instance.as_deref(), Some(joined.id.as_str()));
        assert_eq!(card.address.as_deref(), Some("play.example.com:25565"));
    }

    #[tokio::test]
    async fn free_server_card_gets_manage() {
        let (core1, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core1, "https://api.example.com");
        serve_free(&runner, Some(free_view("online")));
        let team = |id: &str, name: &str| launcher_core::perland::Membership {
            tenant_id: id.into(),
            tenant_name: name.into(),
            display_name: name.into(),
            role: "owner".into(),
            plan: None,
        };
        let item = |name: &str| launcher_core::perland::StackItem {
            stack_name: name.into(),
            status: "running".into(),
            ..Default::default()
        };
        // Another team's server of the same name comes first by team name; the free team wins.
        *runner.stack.memberships.lock().unwrap() =
            Ok(vec![team("t-a", "alpha"), team("t-f", "player-abc")]);
        runner.stack.serve_all("t-a", Ok(vec![item("Survival")]));
        runner.stack.serve_all("t-f", Ok(vec![item("survival")]));
        let card = core1.free_server().await.unwrap().unwrap().server.unwrap();
        assert_eq!(
            card.manage,
            Some(crate::dto::ManageRef {
                tenant_id: "t-f".into(),
                stack: "survival".into(),
            })
        );

        // Not in a free team: any team's server of that name.
        let (core2, runner2, _h, _t2) = core(BuildKind::Dev);
        sign_in(&core2, "https://api.example.com");
        serve_free(&runner2, Some(free_view("online")));
        *runner2.stack.memberships.lock().unwrap() = Ok(vec![team("t-a", "alpha")]);
        runner2.stack.serve_all("t-a", Ok(vec![item("Survival")]));
        let card = core2.free_server().await.unwrap().unwrap().server.unwrap();
        assert_eq!(card.manage.map(|m| m.tenant_id), Some("t-a".into()));

        // Managed servers that can't be read: the card stands without it.
        let (core3, runner3, _h3, _t3) = core(BuildKind::Dev);
        sign_in(&core3, "https://api.example.com");
        serve_free(&runner3, Some(free_view("online")));
        *runner3.stack.player_session.lock().unwrap() =
            Err(LauncherError::network(ErrorCode::HostTrouble, "trouble"));
        let card = core3.free_server().await.unwrap().unwrap().server.unwrap();
        assert_eq!(card.manage, None);
        assert_eq!(card.card.name, "Survival");
    }

    #[tokio::test]
    async fn a_modpack_card_names_its_loader() {
        // The Engine names a modpack server's loader in `game.type` only.
        let card = card_for(free_modrinth_view("sleeping")).await.card;
        assert_eq!(card.game.loader.as_deref(), Some("fabric"));
        assert_eq!(card.game.version.as_deref(), Some("1.20.1"));
        assert!(card.modded);
        // A loader in `type` names it on any kind; `vanilla` never does.
        let mut forge = free_view("online");
        forge.game.kind = "Forge".into();
        assert_eq!(
            card_for(forge).await.card.game.loader.as_deref(),
            Some("forge")
        );
        assert_eq!(card_for(free_view("online")).await.card.game.loader, None);
    }

    #[tokio::test]
    async fn no_server_is_a_page_without_a_card() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, None);
        let page = core.free_server().await.unwrap().unwrap();
        assert!(page.create.allowed);
        assert_eq!(page.server, None);
    }

    async fn card_for(server: launcher_core::perland::FreeServer) -> crate::dto::FreeServerCard {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, Some(server));
        core.free_server().await.unwrap().unwrap().server.unwrap()
    }

    #[tokio::test]
    async fn creating_is_not_joinable() {
        for status in ["creating", "deleting"] {
            let card = card_for(free_view(status)).await;
            assert!(!card.card.joinable, "{status}");
            assert_eq!(card.free_status, status);
        }
        let mut bare = free_view("online");
        bare.invite_url = None;
        bare.join_url = None;
        let card = card_for(bare).await;
        assert!(!card.card.joinable, "no invite");
        assert_eq!(card.card.token, "");
        assert_eq!(card.card.invite, None);
        assert_eq!(card.card.address.as_deref(), Some("play.example.com:25570"));
    }

    #[tokio::test]
    async fn stopped_is_not_joinable() {
        let card = card_for(free_view("stopped")).await;
        assert!(!card.card.joinable);
        assert_eq!(card.card.status, ServerStatus::Offline);
        assert_eq!(card.free_status, "stopped");
        let mut over = free_view("sleeping");
        over.disk.over = true;
        let card = card_for(over).await;
        assert!(!card.card.joinable, "out of space");
        assert_eq!(card.card.status, ServerStatus::Sleeping);
    }

    #[tokio::test]
    async fn start_and_delete_answer() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        let card = core.free_server_start().await.unwrap();
        assert_eq!(card.free_status, "starting");
        assert!(core.free_server_delete().await.unwrap());
        assert_eq!(runner.player.calls_to("start_free_server").len(), 1);
        assert_eq!(
            runner.player.calls_to("delete_free_server")[0].args,
            [player_token('s')]
        );
        *runner.player.started.lock().unwrap() = Err(LauncherError::network(
            ErrorCode::FreeServerOverDisk,
            "Your server is out of space, so it stays off.",
        ));
        assert_eq!(
            core.free_server_start().await.unwrap_err().code,
            ErrorCode::FreeServerOverDisk
        );
    }

    #[tokio::test]
    async fn create_and_start_answer_when_the_instances_cant_be_read() {
        let (core, _runner, _host, tmp) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        // The instances folder is a file: listing it fails.
        let dir = launcher_core::paths::LauncherDirs::new(tmp.path()).instances();
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        std::fs::write(&dir, b"not a folder").unwrap();
        assert!(core.joined_invites().await.is_err());
        let created = core
            .free_server_create("Survival", "vanilla", None, None, None, "en")
            .await
            .expect("the Engine created it");
        assert_eq!(created.card.instance, None);
        let started = core
            .free_server_start()
            .await
            .expect("the Engine started it");
        assert_eq!(started.card.instance, None);
    }

    /// The saved status of the instance `id`.
    async fn last_status(core: &TestCore, id: &str) -> Option<ServerStatus> {
        core.instances
            .load(id)
            .await
            .unwrap()
            .server
            .and_then(|s| s.last_status)
    }

    #[tokio::test]
    async fn play_offline_starts_and_primes_starting() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, Some(free_view("offline")));
        // The shared invite view lags behind the start.
        let mut lagging = server_view("Survival");
        lagging.view.status = "offline".into();
        serve(&runner, &invite('F'), Ok(lagging));
        let id = core.free_server_play().await.unwrap();
        assert_eq!(runner.player.calls_to("start_free_server").len(), 1);
        let instance = core.instances.load(&id).await.unwrap();
        let (link, _) = server_source(&instance).expect("a server instance");
        assert_eq!(link.canonical(), invite('F'));
        assert_eq!(last_status(&core, &id).await, Some(ServerStatus::Starting));
    }

    #[tokio::test]
    async fn play_starting_primes_starting_without_a_start() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, Some(free_view("starting")));
        serve(&runner, &invite('F'), Ok(server_view("Survival")));
        let id = core.free_server_play().await.unwrap();
        assert!(runner.player.calls_to("start_free_server").is_empty());
        assert_eq!(last_status(&core, &id).await, Some(ServerStatus::Starting));
        assert_eq!(
            core.free_server_play().await.unwrap(),
            id,
            "the instance that joins it already"
        );
    }

    #[tokio::test]
    async fn play_sleeping_does_not_start() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, Some(free_view("sleeping")));
        let mut sleeping = server_view("Survival");
        sleeping.view.status = "sleeping".into();
        serve(&runner, &invite('F'), Ok(sleeping));
        let id = core.free_server_play().await.unwrap();
        assert!(runner.player.calls_to("start_free_server").is_empty());
        assert_eq!(
            last_status(&core, &id).await,
            Some(ServerStatus::Sleeping),
            "the run's knock wakes it"
        );
    }

    #[tokio::test]
    async fn play_stopped_refuses() {
        let mut over = free_view("online");
        over.disk.over = true;
        let mut bare = free_view("online");
        bare.invite_url = None;
        for (server, code) in [
            (free_view("stopped"), ErrorCode::FreeServerOverDisk),
            (over, ErrorCode::FreeServerOverDisk),
            (free_view("creating"), ErrorCode::InviteUnsupported),
            (free_view("deleting"), ErrorCode::InviteUnsupported),
            (bare, ErrorCode::InviteUnsupported),
        ] {
            let (core, runner, _host, _t) = core(BuildKind::Dev);
            sign_in(&core, "https://api.example.com");
            let status = server.status.clone();
            serve_free(&runner, Some(server));
            let error = core.free_server_play().await.unwrap_err();
            assert_eq!(error.code, code, "{status}");
            assert!(runner.player.calls_to("start_free_server").is_empty());
            assert!(core.instances_list().await.unwrap().instances.is_empty());
        }
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        serve_free(&runner, None);
        assert_eq!(
            core.free_server_play().await.unwrap_err().code,
            ErrorCode::InviteUnsupported,
            "no server"
        );
    }

    #[tokio::test]
    async fn play_after_401_forgets() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://api.example.com");
        *runner.player.free.lock().unwrap() = Err(signed_out());
        assert_eq!(
            core.free_server_play().await.unwrap_err().code,
            ErrorCode::PerlandSignedOut
        );
        assert!(core.perland_session().await.is_none());
    }

    #[tokio::test]
    async fn calls_use_the_session_engine() {
        let (core, runner, _host, _t) = core(BuildKind::Dev);
        sign_in(&core, "https://engine-a.example.com");
        *runner.player.site.lock().unwrap() =
            Ok(Url::parse("https://engine-b.example.com").unwrap());
        serve_free(&runner, Some(free_view("offline")));
        serve(&runner, &invite('F'), Ok(server_view("Survival")));
        core.free_server().await.unwrap();
        core.free_server_create("Survival", "vanilla", None, None, None, "en")
            .await
            .unwrap();
        core.free_server_start().await.unwrap();
        core.free_server_play().await.unwrap();
        core.free_server_delete().await.unwrap();
        let a = Url::parse("https://engine-a.example.com").unwrap();
        let calls = runner.player.calls.lock().unwrap().clone();
        assert!(calls.len() >= 6);
        assert!(calls.iter().all(|c| c.engine == a));
        assert!(calls.iter().all(|c| c.args[0] == player_token('s')));
    }
}
