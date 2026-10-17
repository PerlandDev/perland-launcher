//! Perland Launcher core: everything except the UI.
pub mod accounts;
pub mod browse;
pub mod content;
pub mod diagnostics;
pub mod download;
pub mod error;
pub mod events;
pub mod fsutil;
pub mod http;
pub mod import;
pub mod install;
pub mod instance_order;
pub mod instances;
pub mod invite;
pub mod java;
pub mod launch;
pub mod locks;
pub mod logging;
pub mod meta;
pub mod mods;
pub mod news;
pub mod paths;
pub mod platform;
pub mod play;
pub mod playtime;
pub mod public_list;
pub mod server_list;
pub mod server_wake;
pub mod settings;
pub mod sources;
pub mod sync;
#[cfg(test)]
pub(crate) mod test_support;

pub use error::{ErrorCategory, ErrorCode, LauncherError, Result};
