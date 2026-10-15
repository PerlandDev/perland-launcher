//! Everything the desktop app does, as plain Rust: commands in, events out. No Tauri here,
//! so all of it is testable with the fakes in `testing`.
pub mod app;
pub mod browse;
pub mod browser;
pub mod deep_link;
pub mod dto;
pub mod events;
#[cfg(test)]
mod live_tests;
pub mod logged;
pub mod logs;
mod packs;
pub mod runner;
mod runs;
mod servers;
#[cfg(test)]
pub(crate) mod testing;
pub mod updates;

pub use app::{AppConfig, AppCore};
pub use browser::{Browser, CoreBrowser};
pub use events::{AppEvent, ExitInfo, Host, RunStateKind};
pub use runner::{CoreRunner, Kill, Runner, RunningGame};
