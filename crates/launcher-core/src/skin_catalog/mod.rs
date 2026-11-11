//! Perland's staff-curated skin catalog: the Engine's manifest and the content-addressed PNGs it
//! names, found through the Perland site's `config.json` like the public server list.

pub mod cache;
pub mod client;
pub mod manifest;

pub use cache::{CatalogCache, CatalogSource, CatalogStatus, SkinCatalog};
pub use client::{CatalogClient, Fetched};
pub use manifest::{
    CatalogArea, CatalogExpression, CatalogFiles, CatalogLook, CatalogPiece, CatalogSheet,
    CatalogSheets, CatalogTag, CatalogText, LookPiece, Manifest, FORMAT_VERSION, SHEET_COLUMNS,
};
