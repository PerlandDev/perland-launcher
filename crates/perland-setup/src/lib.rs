//! Perland Launcher's one-click Windows setup (design: docs/superpowers/specs/2026-11-04-branded-setup-design.md).
//! Every decision lives in the platform-neutral modules; `win` is the Windows shell.

pub mod draw;
pub mod model;
pub mod payload;
pub mod text;
#[cfg(windows)]
pub mod win;

/// The launcher version this setup installs (the workspace version, which the release sets).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
