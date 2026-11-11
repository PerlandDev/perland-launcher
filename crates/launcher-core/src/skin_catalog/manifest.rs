//! A serde mirror of the Engine's skin catalog manifest (camelCase).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::skins_library::SkinModel;

/// The manifest format this launcher reads.
pub const FORMAT_VERSION: u32 = 1;

/// The widest a thumbnail sheet may be, in cells.
pub const SHEET_COLUMNS: u32 = 64;

/// A catalog file name: 64 lower-case hex digits.
pub(crate) fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// An id as the Engine writes them: 2 to 48 of `a-z`, `0-9` and `-`, not starting with `-`.
fn is_slug(s: &str) -> bool {
    (2..=48).contains(&s.len())
        && !s.starts_with('-')
        && s.bytes()
            .all(|b| matches!(b, b'a'..=b'z' | b'0'..=b'9' | b'-'))
}

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

/// A ready-made face: one piece id for each of the slots `eyes`, `brows` and `mouth`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogExpression {
    pub id: String,
    pub name: CatalogText,
    pub eyes: String,
    pub brows: String,
    pub mouth: String,
}

/// One picture holding the thumbnails of a whole list. Cell `i` (from 0, left to right, then
/// down, `columns` cells per row, each 48x96) is the thumbnail of the list's entry `i` in the
/// manifest's order. A list with no entries has no sheet: an empty `file`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSheet {
    /// The sheet's hash, or empty.
    pub file: String,
    pub columns: u32,
}

/// The thumbnail sheets of the manifest's pieces and looks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSheets {
    pub pieces: CatalogSheet,
    pub looks: CatalogSheet,
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
    /// Ready-made faces, in display order. None in a manifest written before they existed.
    #[serde(default)]
    pub expressions: Vec<CatalogExpression>,
    /// Absent in a manifest written before sheets existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheets: Option<CatalogSheets>,
}

impl Manifest {
    /// The sheets that exist: a list with no entries has none.
    pub(crate) fn named_sheets(&self) -> impl Iterator<Item = &CatalogSheet> {
        self.sheets
            .iter()
            .flat_map(|s| [&s.pieces, &s.looks])
            .filter(|s| !s.file.is_empty())
    }

    /// The columns of the thumbnail sheet named `sha256`, when the manifest names it as one.
    pub fn sheet_columns(&self, sha256: &str) -> Option<u32> {
        self.named_sheets()
            .find(|s| s.file == sha256)
            .map(|s| s.columns)
    }

    /// Whether what the manifest says holds: an expression's id and its three piece ids are
    /// slugs, and a sheet is a file hash with 1 to [`SHEET_COLUMNS`] columns. The error says
    /// what doesn't, without quoting the manifest.
    pub fn check(&self) -> std::result::Result<(), &'static str> {
        for e in &self.expressions {
            if ![&e.id, &e.eyes, &e.brows, &e.mouth]
                .into_iter()
                .all(|id| is_slug(id))
            {
                return Err("an expression's id isn't a slug");
            }
        }
        for sheet in self.named_sheets() {
            if !is_hash(&sheet.file) {
                return Err("a sheet isn't named by a hash");
            }
            if !(1..=SHEET_COLUMNS).contains(&sheet.columns) {
                return Err("a sheet's columns are out of range");
            }
        }
        Ok(())
    }

    /// Every file the manifest names: art, masks, thumbnails, paintings and thumbnail sheets.
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
        out.extend(self.named_sheets().map(|s| s.file.clone()));
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
            expressions: vec![],
            sheets: None,
        };
        let hashes = manifest.hashes();
        assert_eq!(hashes.len(), 6);
        assert!(hashes.contains(&h('5')) && hashes.contains(&h('3')));
    }

    fn bare() -> serde_json::Value {
        serde_json::json!({
            "formatVersion": 1, "version": "v", "license": "CC0-1.0",
            "slots": [], "tags": [], "pieces": [], "looks": []
        })
    }

    fn parse(value: serde_json::Value) -> Manifest {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_older_manifest_has_no_expressions_and_no_sheets() {
        let m = parse(bare());
        assert!(m.expressions.is_empty());
        assert_eq!(m.sheets, None);
        assert_eq!(m.check(), Ok(()));
        let out = serde_json::to_value(&m).unwrap();
        assert_eq!(out["expressions"], serde_json::json!([]));
        assert!(out.get("sheets").is_none());
    }

    #[test]
    fn expressions_and_sheets_are_read_in_order() {
        let mut v = bare();
        v["expressions"] = serde_json::json!([
            {"id": "expr-happy", "name": {"en": "Happy", "es": "Feliz"},
             "eyes": "eyes-round", "brows": "brows-straight", "mouth": "mouth-smile"},
            {"id": "expr-calm", "name": {"en": "Calm", "es": "Tranquila"},
             "eyes": "eyes-round", "brows": "brows-soft", "mouth": "mouth-flat"}
        ]);
        v["sheets"] = serde_json::json!({
            "pieces": {"file": h('a'), "columns": 24},
            "looks": {"file": "", "columns": 0}
        });
        let m = parse(v.clone());
        assert_eq!(m.check(), Ok(()));
        assert_eq!(m.expressions.len(), 2);
        assert_eq!(m.expressions[0].id, "expr-happy");
        assert_eq!(m.expressions[0].name.es, "Feliz");
        assert_eq!(m.expressions[0].eyes, "eyes-round");
        assert_eq!(m.expressions[1].brows, "brows-soft");
        assert_eq!(m.expressions[1].mouth, "mouth-flat");
        let sheets = m.sheets.clone().unwrap();
        assert_eq!(sheets.pieces.file, h('a'));
        assert_eq!(sheets.pieces.columns, 24);
        assert_eq!(sheets.looks.file, "");
        // What the launcher hands the UI is what it read.
        let out = serde_json::to_value(&m).unwrap();
        assert_eq!(out["expressions"], v["expressions"]);
        assert_eq!(out["sheets"], v["sheets"]);
    }

    #[test]
    fn hashes_and_sheet_columns_cover_the_sheets_that_exist() {
        let mut v = bare();
        v["sheets"] = serde_json::json!({
            "pieces": {"file": h('a'), "columns": 24},
            "looks": {"file": "", "columns": 0}
        });
        let m = parse(v.clone());
        assert_eq!(m.hashes(), BTreeSet::from([h('a')]));
        assert_eq!(m.sheet_columns(&h('a')), Some(24));
        assert_eq!(m.sheet_columns(""), None);
        assert_eq!(m.sheet_columns(&h('b')), None);
        v["sheets"]["looks"] = serde_json::json!({"file": h('b'), "columns": 8});
        let m = parse(v);
        assert_eq!(m.hashes(), BTreeSet::from([h('a'), h('b')]));
        assert_eq!(m.sheet_columns(&h('b')), Some(8));
        assert_eq!(parse(bare()).sheet_columns(&h('a')), None);
    }

    #[test]
    fn check_refuses_expression_ids_that_are_not_slugs() {
        for (field, bad) in [
            ("id", ""),
            ("id", "Expr-Happy"),
            ("eyes", "../eyes"),
            ("brows", "brows straight"),
            ("mouth", "-mouth"),
            ("mouth", "m"),
            ("eyes", &"e".repeat(49)),
        ] {
            let mut v = bare();
            v["expressions"] = serde_json::json!([
                {"id": "expr-happy", "name": {"en": "Happy", "es": "Feliz"},
                 "eyes": "eyes-round", "brows": "brows-straight", "mouth": "mouth-smile"}
            ]);
            v["expressions"][0][field] = serde_json::json!(bad);
            assert!(parse(v).check().is_err(), "{field} = {bad:?}");
        }
    }

    #[test]
    fn check_refuses_a_sheet_that_is_not_a_hash_with_1_to_64_columns() {
        let sheet = |file: &str, columns: u32| {
            let mut v = bare();
            v["sheets"] = serde_json::json!({
                "pieces": {"file": "", "columns": 0},
                "looks": {"file": file, "columns": columns}
            });
            parse(v).check()
        };
        assert_eq!(sheet(&h('a'), 1), Ok(()));
        assert_eq!(sheet(&h('a'), 64), Ok(()));
        // A list with no entries has no sheet, whatever its columns say.
        assert_eq!(sheet("", 0), Ok(()));
        assert_eq!(sheet("", 24), Ok(()));
        assert!(sheet(&h('a'), 0).is_err());
        assert!(sheet(&h('a'), 65).is_err());
        assert!(sheet(&h('A'), 24).is_err());
        assert!(sheet("../../x", 24).is_err());
        assert!(sheet(&h('a')[..63], 24).is_err());
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
