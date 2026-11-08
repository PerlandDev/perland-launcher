//! A serde mirror of the Engine's skin catalog manifest (camelCase).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::skins_library::SkinModel;

/// The manifest format this launcher reads.
pub const FORMAT_VERSION: u32 = 1;

/// A name in both languages.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogText {
    pub en: String,
    pub es: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogArea {
    pub id: String,
    pub name: CatalogText,
    /// The colour the piece is drawn in until the player picks one, `#rrggbb`.
    pub default: String,
}

/// The hashes of a piece's art and colour mask for one arm model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogFiles {
    pub art: String,
    pub mask: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogPiece {
    pub id: String,
    pub slot: String,
    pub name: CatalogText,
    pub tags: Vec<String>,
    pub areas: Vec<CatalogArea>,
    /// By model: `classic`, `slim`.
    pub files: BTreeMap<String, CatalogFiles>,
    pub thumb: String,
    pub retired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookPiece {
    pub id: String,
    /// Area id → `#rrggbb`.
    pub colours: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogLook {
    pub id: String,
    pub name: CatalogText,
    pub tags: Vec<String>,
    pub model: SkinModel,
    pub pieces: Vec<LookPiece>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub painting: Option<String>,
    pub thumb: String,
    pub retired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogTag {
    pub id: String,
    pub name: CatalogText,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    /// The ETag: a hash of the whole catalog.
    pub version: String,
    pub license: String,
    pub slots: Vec<String>,
    pub tags: Vec<CatalogTag>,
    pub pieces: Vec<CatalogPiece>,
    pub looks: Vec<CatalogLook>,
}

impl Manifest {
    /// Every file the manifest names: art, masks, thumbnails and paintings.
    pub fn hashes(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for piece in &self.pieces {
            out.insert(piece.thumb.clone());
            for files in piece.files.values() {
                out.insert(files.art.clone());
                out.insert(files.mask.clone());
            }
        }
        for look in &self.looks {
            out.insert(look.thumb.clone());
            if let Some(painting) = &look.painting {
                out.insert(painting.clone());
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(c: char) -> String {
        c.to_string().repeat(64)
    }

    #[test]
    fn hashes_cover_every_file() {
        let manifest = Manifest {
            format_version: FORMAT_VERSION,
            version: h('0'),
            license: "CC0-1.0".into(),
            slots: vec![],
            tags: vec![],
            pieces: vec![CatalogPiece {
                id: "a".into(),
                slot: "hat".into(),
                name: CatalogText {
                    en: "A".into(),
                    es: "A".into(),
                },
                tags: vec![],
                areas: vec![],
                files: BTreeMap::from([
                    (
                        "classic".into(),
                        CatalogFiles {
                            art: h('1'),
                            mask: h('2'),
                        },
                    ),
                    (
                        "slim".into(),
                        CatalogFiles {
                            art: h('1'),
                            mask: h('3'),
                        },
                    ),
                ]),
                thumb: h('4'),
                retired: false,
            }],
            looks: vec![CatalogLook {
                id: "l".into(),
                name: CatalogText {
                    en: "L".into(),
                    es: "L".into(),
                },
                tags: vec![],
                model: SkinModel::Slim,
                pieces: vec![],
                painting: Some(h('5')),
                thumb: h('6'),
                retired: false,
            }],
        };
        let hashes = manifest.hashes();
        assert_eq!(hashes.len(), 6);
        assert!(hashes.contains(&h('5')) && hashes.contains(&h('3')));
    }

    #[test]
    fn a_look_without_a_painting_omits_it() {
        let look: CatalogLook = serde_json::from_value(serde_json::json!({
            "id": "l", "name": {"en": "L", "es": "L"}, "tags": [], "model": "classic",
            "pieces": [], "thumb": "t", "retired": false
        }))
        .unwrap();
        assert_eq!(look.painting, None);
        assert!(serde_json::to_value(&look)
            .unwrap()
            .get("painting")
            .is_none());
    }
}
