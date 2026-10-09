//! Perland Launcher core: everything except the UI.
pub mod download;
pub mod error;
pub mod events;
pub mod fsutil;
pub mod meta;
pub mod paths;
pub mod platform;

pub use error::{ErrorCategory, LauncherError, Result};
