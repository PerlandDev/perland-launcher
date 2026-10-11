//! Browse (Plan 3c): Modrinth search, projects, and installing mods, resource packs and shaders
//! into an instance with their required dependencies. Modpacks go through `sources` instead.
pub mod api;
pub mod compat;
pub mod identify;
pub mod install;
pub mod plan;
pub mod project;
pub mod search;
