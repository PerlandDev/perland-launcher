//! Everything the desktop app does, as plain Rust: commands in, events out. No Tauri here,
//! so all of it is testable with the fakes in `testing`.
pub mod dto;
pub mod events;
pub mod logs;

pub use events::{AppEvent, ExitInfo, Host, RunStateKind};
