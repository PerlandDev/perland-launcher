//! Perland Launcher core: everything except the UI.
pub mod accounts;
pub mod download;
pub mod error;
pub mod events;
pub mod fsutil;
pub mod http;
pub mod install;
pub mod instances;
pub mod invite;
pub mod java;
pub mod launch;
pub mod locks;
pub mod meta;
pub mod mods;
pub mod paths;
pub mod platform;
pub mod play;
pub mod settings;
pub mod sources;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_support;

pub use error::{ErrorCategory, ErrorCode, LauncherError, Result};
