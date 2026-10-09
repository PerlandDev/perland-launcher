//! Perland Launcher core: everything except the UI.
pub mod error;
pub mod events;
pub mod paths;
pub mod platform;

pub use error::{ErrorCategory, LauncherError, Result};
