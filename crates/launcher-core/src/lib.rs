//! Perland Launcher core: everything except the UI.
pub mod accounts;
pub mod download;
pub mod error;
pub mod events;
pub mod fsutil;
pub mod install;
pub mod instances;
pub mod java;
pub mod launch;
pub mod meta;
pub mod paths;
pub mod platform;
#[cfg(test)]
pub(crate) mod test_support;

pub use error::{ErrorCategory, LauncherError, Result};
