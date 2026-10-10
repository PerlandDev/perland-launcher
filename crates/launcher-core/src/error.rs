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
    NameInvalid, MemoryOutOfRange,
    // java
    JavaUnavailable, JavaStartFailed, JavaPathMissing, JavaTestFailed,
    // launch
    NoGameArguments, GameFilesMissing,
    // loaders and metadata
    VersionInvalid, LoaderUnsupported, LoaderVersionNotFound, MetaCacheMissing, MetaNotFound,
    MetaChecksum, MetaUnexpected, LoaderInstallerInvalid, LoaderTooOld, LoaderProcessorFailed,
    LoaderOutputCorrupt,
    // settings and logs
    SettingsDamaged, LogWriteStopped,
    // mods
    ModNotJar, ModExists, ModNotFound, ModNameInvalid,
    // packs and sync
    PackInvalid, PackUnsupported, PackUnsafePath, PackNotFound, PackLinkInvalid, NoPack,
    NotEnoughSpace, InstanceLocked, ManualDownloadsNeeded, UpdateCheckFailed,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn pack_codes_exist() {
        for (code, wire) in [
            (ErrorCode::PackInvalid, "pack_invalid"),
            (ErrorCode::PackUnsupported, "pack_unsupported"),
            (ErrorCode::PackUnsafePath, "pack_unsafe_path"),
            (ErrorCode::PackNotFound, "pack_not_found"),
            (ErrorCode::PackLinkInvalid, "pack_link_invalid"),
            (ErrorCode::NoPack, "no_pack"),
            (ErrorCode::NotEnoughSpace, "not_enough_space"),
            (ErrorCode::InstanceLocked, "instance_locked"),
            (ErrorCode::ManualDownloadsNeeded, "manual_downloads_needed"),
            (ErrorCode::UpdateCheckFailed, "update_check_failed"),
        ] {
            assert_eq!(serde_json::to_value(code).unwrap(), wire);
            assert!(ErrorCode::ALL.contains(&code));
        }
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
