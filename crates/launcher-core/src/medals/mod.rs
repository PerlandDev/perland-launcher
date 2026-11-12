//! Medals: the rules of all 24 (docs/superpowers/specs/2026-11-11-medals-saved-and-synced-design.md
//! §5) and the file that keeps every tier earned with its date (§6).
pub mod rules;
pub mod store;

pub use rules::{Group, Kind, Measures, Rule, Unit, CATALOGUE};
pub use store::{Earned, MedalsFile, MedalsStore, MEDALS_FILE};
