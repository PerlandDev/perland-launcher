//! Everything the desktop app does, as plain Rust: commands in, events out. No Tauri here,
//! so all of it is testable with the fakes in `testing`.
pub mod app;
pub mod deep_link;
pub mod dto;
pub mod events;
pub mod logs;
mod packs;
pub mod runner;
mod runs;
#[cfg(test)]
pub(crate) mod testing;

pub use app::{AppConfig, AppCore};
pub use events::{AppEvent, ExitInfo, Host, RunStateKind};
pub use runner::{CoreRunner, Kill, Runner, RunningGame};
