use std::{collections::BTreeMap, fmt::Display};

use serde::Serialize;

/// What kind of thing went wrong. The UI picks icons and wording from this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCategory {
    Network,
    Auth,
    Integrity,
    Disk,
    Java,
    Loader,
    Pack,
    Game,
}

macro_rules! error_codes {
    ($($variant:ident),* $(,)?) => {
        /// A stable name for every player-facing failure. The UI translates by this code;
        /// `LauncherError::message` is only the English fallback.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
        #[serde(rename_all = "snake_case")]
        pub enum ErrorCode {
            $($variant),*
        }

        impl ErrorCode {
            /// Every code in declaration order. Built by the same macro as the enum, so it can't miss one.
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$variant),*];
        }
    };
}

error_codes! {
    // accounts
    OfflineNameInvalid, AccountsDamaged, OfflineLocked, MicrosoftUnavailable, NoAccount,
    // disk
    DiskIo, DiskWriteFailed,
    // network
    DownloadNoUrl, DownloadCorrupt, HostUnreachable, DownloadRestarted, HostTrouble,
    DownloadRejected, ConnectionDropped, HttpClientFailed, Offline,
    // game files
    UnsafeGameData, LibraryUnverifiable, NoClientDownload, MinecraftVersionNotFound,
    MojangMismatch, NoAssetIndex, NativesExtractFailed, VersionInUse, ArchiveInvalid,
    // instances
    InstanceNotFound, InstanceDamaged, InstanceIdInvalid, InstanceBusy, AlreadyRunning,
    NameInvalid, MemoryOutOfRange, ArtFileInvalid, BannerFocusInvalid,
    // java
    JavaUnavailable, JavaStartFailed, JavaPathMissing, JavaTestFailed, JvmArgsUnclosedQuote,
    JvmArgNotOption, JvmArgNotAllowed, JvmArgsMemory, JvmArgsTooLong,
    // launch
    NoGameArguments, GameFilesMissing,
    // loaders and metadata
    VersionInvalid, LoaderUnsupported, LoaderVersionNotFound, MetaCacheMissing, MetaNotFound,
    MetaChecksum, MetaUnexpected, LoaderInstallerInvalid, LoaderTooOld, LoaderProcessorFailed,
    LoaderOutputCorrupt,
    // settings and logs
    SettingsDamaged, LogWriteStopped, DiagnosticsPathInvalid,
    // mods
    ModNotJar, ModExists, ModNotFound, ModNameInvalid, ModManaged,
    // packs and sync
    PackInvalid, PackUnsupported, PackUnsafePath, PackNotFound, PackLinkInvalid, NoPack, MrpackCorrupt,
    NotEnoughSpace, InstanceLocked, InstanceFolderInUse, ManualDownloadsNeeded, UpdateCheckFailed, PackPathIsFolder, SyncCancelled, UpdateDownloadFailed,
    // browse
    BrowseRateLimited, ContentNotFound, ContentIncompatible, ContentDependencyMissing,
    ContentConflict, ContentNeedsLoader, ContentAlreadyInstalled, ContentBusy,
    ContentInstalling, ContentPlanExpired, ImageRefused, LinkRefused,
    // server invites (join by link)
    InviteLinkInvalid, InviteGone, InviteUnsupported, InviteSiteUnreachable, InviteRateLimited,
    LinkHasPack,
    ServerPackUnavailable, ServerPackPending, ServerAddressUnknown, ServerListFailed, ServerListClosed,
    ServerWakeFailed,
    // restore points
    RestorePointGone, RestoreFailed,
    // continue where you left off
    WorldContinueUnsupported,
    // launcher updates
    LauncherUpdateFailed, LauncherUpdateInvalid, LauncherUpdateBusy,
    // importing from another launcher
    ImportNotInstance, ImportInsideLauncher, ImportBusy, ImportCancelled,
    // Perland account
    PlayerAccountsClosed, SignInUnavailable, SignInFailed, SignInTimedOut, InvalidEmail,
    InvalidCode, TooManyCodes, EmailUnavailable, PerlandSignedOut, PlayerServersClosed,
    TooManyPlayerServers, MinecraftLinkClosed,
    // free servers
    EmailRequired, FreeServerExists, TooManyFreeServers, FreeServersFull, InvalidFreeServer,
    ModpackUnsupported, ModpackTooHeavy, FreeServerOverDisk, HostFull, FreeServersPaused,
    FreeServerBlocked,
    // sharing an instance
    SharePathInvalid, ShareTooLarge,
    // anything unexpected
    Internal,
}

/// A failure a player can read: a plain message, an optional fix, and technical detail behind a toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct LauncherError {
    pub category: ErrorCategory,
    pub code: ErrorCode,
    pub message: String,
    pub fix: Option<String>,
    pub detail: Option<String>,
    /// Values the translated message interpolates, e.g. `{"version": "1.20.1"}`.
    pub params: BTreeMap<String, String>,
}

pub type Result<T> = std::result::Result<T, LauncherError>;

impl LauncherError {
    pub fn new(category: ErrorCategory, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            category,
            code,
            message: message.into(),
            fix: None,
            detail: None,
            params: BTreeMap::new(),
        }
    }
    pub fn network(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Network, code, message)
    }
    pub fn auth(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Auth, code, message)
    }
    pub fn integrity(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Integrity, code, message)
    }
    pub fn disk(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Disk, code, message)
    }
    pub fn java(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Java, code, message)
    }
    pub fn loader(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Loader, code, message)
    }
    pub fn pack(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Pack, code, message)
    }
    pub fn game(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Game, code, message)
    }
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
    pub fn with_detail(mut self, detail: impl Display) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
    pub fn with_param(mut self, key: &str, value: impl Display) -> Self {
        self.params.insert(key.to_string(), value.to_string());
        self
    }
}

impl From<std::io::Error> for LauncherError {
    fn from(e: std::io::Error) -> Self {
        LauncherError::disk(ErrorCode::DiskIo, "Couldn't read or write a launcher file")
            .with_fix("Check free disk space and that the launcher folder isn't read-only")
            .with_detail(e)
    }
}

/// Going back to the instance's restore point.
pub mod restore {
    use super::{ErrorCode, LauncherError};

    /// No point to go back to: none was taken, it is too old, or it is damaged.
    pub fn point_gone() -> LauncherError {
        LauncherError::disk(
            ErrorCode::RestorePointGone,
            "There is nothing to go back to",
        )
    }

    /// A folder couldn't be swapped back (another program holds it).
    pub fn failed(detail: impl std::fmt::Display) -> LauncherError {
        LauncherError::disk(ErrorCode::RestoreFailed, "Couldn't go back to how it was")
            .with_fix("Close any window showing the instance's folder, then try again")
            .with_detail(detail)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn launcher_update_codes_exist() {
        for (code, wire) in [
            (ErrorCode::LauncherUpdateFailed, "launcher_update_failed"),
            (ErrorCode::LauncherUpdateInvalid, "launcher_update_invalid"),
            (ErrorCode::LauncherUpdateBusy, "launcher_update_busy"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn import_codes_exist() {
        for (code, wire) in [
            (ErrorCode::ImportNotInstance, "import_not_instance"),
            (ErrorCode::ImportInsideLauncher, "import_inside_launcher"),
            (ErrorCode::ImportBusy, "import_busy"),
            (ErrorCode::ImportCancelled, "import_cancelled"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn share_codes_exist() {
        for (code, wire) in [
            (ErrorCode::SharePathInvalid, "share_path_invalid"),
            (ErrorCode::ShareTooLarge, "share_too_large"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn diagnostics_code_exists() {
        let code = ErrorCode::DiagnosticsPathInvalid;
        assert_eq!(
            serde_json::to_value(code).unwrap(),
            "diagnostics_path_invalid"
        );
        assert!(ErrorCode::ALL.contains(&code));
    }

    #[test]
    fn pack_codes_exist() {
        for (code, wire) in [
            (ErrorCode::PackInvalid, "pack_invalid"),
            (ErrorCode::PackUnsupported, "pack_unsupported"),
            (ErrorCode::PackUnsafePath, "pack_unsafe_path"),
            (ErrorCode::PackNotFound, "pack_not_found"),
            (ErrorCode::PackLinkInvalid, "pack_link_invalid"),
            (ErrorCode::NoPack, "no_pack"),
            (ErrorCode::MrpackCorrupt, "mrpack_corrupt"),
            (ErrorCode::NotEnoughSpace, "not_enough_space"),
            (ErrorCode::InstanceLocked, "instance_locked"),
            (ErrorCode::InstanceFolderInUse, "instance_folder_in_use"),
            (ErrorCode::ManualDownloadsNeeded, "manual_downloads_needed"),
            (ErrorCode::UpdateCheckFailed, "update_check_failed"),
            (ErrorCode::PackPathIsFolder, "pack_path_is_folder"),
            (ErrorCode::SyncCancelled, "sync_cancelled"),
            (ErrorCode::UpdateDownloadFailed, "update_download_failed"),
            (ErrorCode::ModManaged, "mod_managed"),
            (ErrorCode::ArtFileInvalid, "art_file_invalid"),
            (ErrorCode::BannerFocusInvalid, "banner_focus_invalid"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn browse_codes_exist() {
        for (code, name) in [
            (ErrorCode::BrowseRateLimited, "browse_rate_limited"),
            (ErrorCode::ContentNotFound, "content_not_found"),
            (ErrorCode::ContentIncompatible, "content_incompatible"),
            (
                ErrorCode::ContentDependencyMissing,
                "content_dependency_missing",
            ),
            (ErrorCode::ContentConflict, "content_conflict"),
            (ErrorCode::ContentNeedsLoader, "content_needs_loader"),
            (
                ErrorCode::ContentAlreadyInstalled,
                "content_already_installed",
            ),
            (ErrorCode::ContentBusy, "content_busy"),
            (ErrorCode::ContentInstalling, "content_installing"),
            (ErrorCode::ContentPlanExpired, "content_plan_expired"),
            (ErrorCode::ImageRefused, "image_refused"),
            (ErrorCode::LinkRefused, "link_refused"),
        ] {
            assert!(ErrorCode::ALL.contains(&code));
            assert_eq!(serde_json::to_value(code).unwrap(), name);
        }
    }

    #[test]
    fn invite_codes_exist() {
        for (code, wire) in [
            (ErrorCode::InviteLinkInvalid, "invite_link_invalid"),
            (ErrorCode::InviteGone, "invite_gone"),
            (ErrorCode::InviteUnsupported, "invite_unsupported"),
            (ErrorCode::InviteSiteUnreachable, "invite_site_unreachable"),
            (ErrorCode::InviteRateLimited, "invite_rate_limited"),
            (ErrorCode::ServerPackUnavailable, "server_pack_unavailable"),
            (ErrorCode::ServerPackPending, "server_pack_pending"),
            (ErrorCode::ServerAddressUnknown, "server_address_unknown"),
            (ErrorCode::ServerListFailed, "server_list_failed"),
            (ErrorCode::ServerListClosed, "server_list_closed"),
            (ErrorCode::ServerWakeFailed, "server_wake_failed"),
            (ErrorCode::LinkHasPack, "link_has_pack"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn perland_account_codes_exist() {
        for (code, wire) in [
            (ErrorCode::PlayerAccountsClosed, "player_accounts_closed"),
            (ErrorCode::SignInUnavailable, "sign_in_unavailable"),
            (ErrorCode::SignInFailed, "sign_in_failed"),
            (ErrorCode::SignInTimedOut, "sign_in_timed_out"),
            (ErrorCode::InvalidEmail, "invalid_email"),
            (ErrorCode::InvalidCode, "invalid_code"),
            (ErrorCode::TooManyCodes, "too_many_codes"),
            (ErrorCode::EmailUnavailable, "email_unavailable"),
            (ErrorCode::PerlandSignedOut, "perland_signed_out"),
            (ErrorCode::PlayerServersClosed, "player_servers_closed"),
            (ErrorCode::TooManyPlayerServers, "too_many_player_servers"),
            (ErrorCode::MinecraftLinkClosed, "minecraft_link_closed"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn free_server_codes_exist() {
        for (code, wire) in [
            (ErrorCode::EmailRequired, "email_required"),
            (ErrorCode::FreeServerExists, "free_server_exists"),
            (ErrorCode::TooManyFreeServers, "too_many_free_servers"),
            (ErrorCode::FreeServersFull, "free_servers_full"),
            (ErrorCode::InvalidFreeServer, "invalid_free_server"),
            (ErrorCode::ModpackUnsupported, "modpack_unsupported"),
            (ErrorCode::ModpackTooHeavy, "modpack_too_heavy"),
            (ErrorCode::FreeServerOverDisk, "free_server_over_disk"),
            (ErrorCode::HostFull, "host_full"),
            (ErrorCode::FreeServersPaused, "free_servers_paused"),
            (ErrorCode::FreeServerBlocked, "free_server_blocked"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
    }

    #[test]
    fn restore_codes_exist() {
        for (code, wire) in [
            (ErrorCode::RestorePointGone, "restore_point_gone"),
            (ErrorCode::RestoreFailed, "restore_failed"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
        assert_eq!(restore::point_gone().code, ErrorCode::RestorePointGone);
        let failed = restore::failed("busy");
        assert_eq!(failed.category, ErrorCategory::Disk);
        assert!(failed.fix.is_some());
    }

    #[test]
    fn builders_fill_fix_detail_and_params() {
        let e = LauncherError::network(ErrorCode::HostUnreachable, "Couldn't reach Mojang")
            .with_fix("Check your connection")
            .with_detail("dns error")
            .with_param("host", "piston-meta.mojang.com");
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.code, ErrorCode::HostUnreachable);
        assert_eq!(e.to_string(), "Couldn't reach Mojang");
        assert_eq!(e.fix.as_deref(), Some("Check your connection"));
        assert_eq!(e.detail.as_deref(), Some("dns error"));
        assert_eq!(
            e.params.get("host").map(String::as_str),
            Some("piston-meta.mojang.com")
        );
    }

    #[test]
    fn io_errors_become_disk_errors() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e: LauncherError = io.into();
        assert_eq!(e.category, ErrorCategory::Disk);
        assert_eq!(e.code, ErrorCode::DiskIo);
        assert!(e.fix.is_some());
        assert!(e.detail.unwrap().contains("denied"));
    }

    #[test]
    fn serializes_code_and_params() {
        let json = serde_json::to_value(
            LauncherError::java(ErrorCode::JavaStartFailed, "Java couldn't be started")
                .with_param("version", "1.20.1"),
        )
        .unwrap();
        assert_eq!(json["category"], "java");
        assert_eq!(json["code"], "java_start_failed");
        assert_eq!(json["params"]["version"], "1.20.1");
        assert!(json["fix"].is_null());
        assert!(json["detail"].is_null());
    }

    #[test]
    fn every_code_is_unique_snake_case() {
        let mut seen = HashSet::new();
        for code in ErrorCode::ALL {
            let s = serde_json::to_value(code).unwrap();
            let s = s.as_str().unwrap().to_string();
            assert!(
                s.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{s}"
            );
            assert!(seen.insert(s.clone()), "duplicate {s}");
        }
        assert!(ErrorCode::ALL.contains(&ErrorCode::Internal));
        assert!(ErrorCode::ALL.len() >= 50);
    }
}
