use std::fmt::Display;

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

/// A failure a player can read: a plain message, an optional fix, and technical detail behind a toggle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{message}")]
pub struct LauncherError {
    pub category: ErrorCategory,
    pub message: String,
    pub fix: Option<String>,
    pub detail: Option<String>,
}

pub type Result<T> = std::result::Result<T, LauncherError>;

impl LauncherError {
    pub fn new(category: ErrorCategory, message: impl Into<String>) -> Self {
        Self {
            category,
            message: message.into(),
            fix: None,
            detail: None,
        }
    }
    pub fn network(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Network, message)
    }
    pub fn auth(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Auth, message)
    }
    pub fn integrity(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Integrity, message)
    }
    pub fn disk(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Disk, message)
    }
    pub fn java(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Java, message)
    }
    pub fn loader(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Loader, message)
    }
    pub fn pack(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Pack, message)
    }
    pub fn game(message: impl Into<String>) -> Self {
        Self::new(ErrorCategory::Game, message)
    }
    pub fn with_fix(mut self, fix: impl Into<String>) -> Self {
        self.fix = Some(fix.into());
        self
    }
    pub fn with_detail(mut self, detail: impl Display) -> Self {
        self.detail = Some(detail.to_string());
        self
    }
}

impl From<std::io::Error> for LauncherError {
    fn from(e: std::io::Error) -> Self {
        LauncherError::disk("Couldn't read or write a launcher file")
            .with_fix("Check free disk space and that the launcher folder isn't read-only")
            .with_detail(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_fill_fix_and_detail() {
        let e = LauncherError::network("Couldn't reach Mojang")
            .with_fix("Check your connection")
            .with_detail("dns error");
        assert_eq!(e.category, ErrorCategory::Network);
        assert_eq!(e.to_string(), "Couldn't reach Mojang");
        assert_eq!(e.fix.as_deref(), Some("Check your connection"));
        assert_eq!(e.detail.as_deref(), Some("dns error"));
    }

    #[test]
    fn io_errors_become_disk_errors() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let e: LauncherError = io.into();
        assert_eq!(e.category, ErrorCategory::Disk);
        assert!(e.fix.is_some());
        assert!(e.detail.unwrap().contains("denied"));
    }

    #[test]
    fn category_serializes_snake_case() {
        let json = serde_json::to_string(&LauncherError::integrity("bad")).unwrap();
        assert!(json.contains("\"category\":\"integrity\""));
    }
}
