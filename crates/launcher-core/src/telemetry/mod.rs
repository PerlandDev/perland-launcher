//! Anonymous error reports and usage counts (design:
//! `docs/superpowers/specs/2026-11-11-anonymous-usage-and-errors-design.md`).
//!
//! Nothing here identifies a player or a computer: there is no install id, and the only state
//! kept is calendar dates, a handful of counters and the reports not sent yet. Every text that
//! leaves goes through [`Scrubber`] first.
pub mod date;
pub mod locale;
pub mod payload;
pub mod scrub;
pub mod send;
pub mod state;

pub use date::Day;
pub use locale::os_locale;
pub use payload::{report_for_error, Client, ErrorBatch, Ping, PingChannel, Report, ReportKind};
pub use scrub::Scrubber;
pub use send::{Outcome, Sender};
pub use state::{Counter, PingFlags, State, QUEUE_CAP, STATE_FILE};

/// What a player who never touched the switch shares. The one place to flip it to opt-in.
pub const SHARE_BY_DEFAULT: bool = true;
