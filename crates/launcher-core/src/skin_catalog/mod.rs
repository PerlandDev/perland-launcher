//! Perland's staff-curated skin catalog: the Engine's manifest and the content-addressed PNGs it
//! names, found through the Perland site's `config.json` like the public server list.

pub mod client;
pub mod manifest;

pub use client::{CatalogClient, Fetched};
pub use manifest::{
    CatalogArea, CatalogFiles, CatalogLook, CatalogPiece, CatalogTag, CatalogText, LookPiece,
    Manifest, FORMAT_VERSION,
};
