//! A serde mirror of the Engine's public mod list (`InviteModsDTO`, `GET /invites/<token>/mods`):
//! the mods, resource packs and shaders of the pack commit a server runs, each with where it
//! comes from. Unknown fields are ignored and an unknown source reads as [`ModSource::Other`],
//! so a newer Engine still parses.
//!
//! Seam: the Engine's mod-safety scan will add a per-entry verdict (and a pack summary on the
//! view). It is not live, so nothing here reads one; it belongs on [`InviteMod`] beside
//! `source`, read as leniently.
use serde::{Deserialize, Deserializer};

/// Where one mod of the pack comes from, as the Engine says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ModSource {
    Modrinth,
    CurseForge,
    /// Downloaded from a web address the server's team chose.
    Url,
    /// A file the team committed to the pack as it is.
    Uploaded,
    /// A source this launcher doesn't know yet: counted apart, never as Modrinth or CurseForge.
    #[default]
    Other,
}

impl ModSource {
    pub fn parse(s: &str) -> ModSource {
        match s {
            "modrinth" => ModSource::Modrinth,
            "curseforge" => ModSource::CurseForge,
            "url" => ModSource::Url,
            "uploaded" => ModSource::Uploaded,
            _ => ModSource::Other,
        }
    }

    /// Modrinth or CurseForge: a public catalogue the file was published to.
    pub fn is_catalogue(self) -> bool {
        matches!(self, ModSource::Modrinth | ModSource::CurseForge)
    }
}

fn source<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<ModSource, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => ModSource::parse(&s),
        _ => ModSource::Other,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct InviteMod {
    pub name: String,
    /// `client`, `server` or `both`.
    pub side: String,
    /// The pack folder it goes in: `mods`, `resourcepacks`, `shaderpacks`, ...
    pub folder: String,
    #[serde(deserialize_with = "source")]
    pub source: ModSource,
}

impl InviteMod {
    /// Whether it reaches the player's computer: everything but server-only files.
    pub fn installs_here(&self) -> bool {
        !self.side.eq_ignore_ascii_case("server")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct InviteMods {
    pub commit: String,
    pub mods: Vec<InviteMod>,
}

/// How many of the files that install on the player's computer come from each source.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModCounts {
    pub total: u32,
    pub modrinth: u32,
    pub curseforge: u32,
    pub url: u32,
    pub uploaded: u32,
    pub other: u32,
}

impl InviteMods {
    pub fn counts(&self) -> ModCounts {
        let mut c = ModCounts::default();
        for m in self.mods.iter().filter(|m| m.installs_here()) {
            c.total += 1;
            match m.source {
                ModSource::Modrinth => c.modrinth += 1,
                ModSource::CurseForge => c.curseforge += 1,
                ModSource::Url => c.url += 1,
                ModSource::Uploaded => c.uploaded += 1,
                ModSource::Other => c.other += 1,
            }
        }
        c
    }

    /// The uploaded files' names. The Engine lists an uploaded jar by its file name, which is
    /// how the update summary tells it apart from the pack's other files.
    pub fn uploaded_names(&self) -> Vec<String> {
        self.mods
            .iter()
            .filter(|m| m.source == ModSource::Uploaded)
            .map(|m| m.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn odd_entries_still_parse() {
        let mods: InviteMods = serde_json::from_value(json!({
            "commit": "c",
            "mods": [{ "name": "a", "source": 7 }, { "name": "b" }, { "name": "c", "source": "uploaded", "side": "SERVER" }]
        }))
        .unwrap();
        assert_eq!(mods.mods[0].source, ModSource::Other);
        assert_eq!(mods.mods[1].source, ModSource::Other);
        assert!(mods.mods[1].installs_here());
        assert!(!mods.mods[2].installs_here());
        assert_eq!(mods.counts().total, 2);
    }
}
