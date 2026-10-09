//! Perland Launcher core: everything except the UI.
pub mod download;
pub mod error;
pub mod events;
pub mod fsutil;
pub mod meta;
pub mod paths;
pub mod platform;
#[cfg(test)]
pub(crate) mod test_support;

pub use error::{ErrorCategory, LauncherError, Result};
