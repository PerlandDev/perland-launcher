//! Where a file an update writes comes from, in words a player can weigh: published on Modrinth
//! or CurseForge, downloaded from another website, uploaded by the server's owner, or simply
//! inside the pack file (where the launcher can't tell which). Only what is known is said:
//! nothing here judges a file, it only names its source.
//!
//! A `.mrpack` carries files two ways. An index entry names a download host, which is plain.
//! An override entry is inside the pack file itself: a server's pack puts its owner's uploaded
//! jars there, and also the CurseForge files and web files the Engine fetched for it. The
//! Engine's mod list (`/invites/<token>/mods`) says which mods are uploaded, by file name, and
//! whether any came from the web; that is what tells them apart here.
use serde::Serialize;

use super::plan::{ChangedFile, FileFrom};
use crate::invite::{InviteMods, ModCounts};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileOrigin {
    Modrinth,
    CurseForge,
    /// Downloaded from another website.
    Web,
    /// Uploaded to the server's pack by its owner.
    Uploaded,
    /// Inside the pack file; which source put it there isn't known.
    Pack,
}

impl FileOrigin {
    /// Modrinth or CurseForge: a public catalogue the file was published to.
    pub fn is_catalogue(self) -> bool {
        matches!(self, FileOrigin::Modrinth | FileOrigin::CurseForge)
    }
}

/// What the launcher knows about a pack's files besides the files themselves.
#[derive(Debug, Clone, Default)]
pub struct OriginHints {
    /// The pack file is a Modrinth modpack version: what is inside it was published there.
    pub modrinth_pack: bool,
    /// The uploaded files' names, lowercase.
    uploaded: Vec<String>,
    /// The server's mod list counts, when it was read.
    counts: Option<ModCounts>,
}

impl OriginHints {
    /// A Modrinth modpack's version: its overrides are part of what Modrinth publishes.
    pub fn modrinth_pack() -> Self {
        Self {
            modrinth_pack: true,
            ..Self::default()
        }
    }

    /// A server's pack, with its mod list.
    pub fn server(mods: &InviteMods) -> Self {
        Self {
            modrinth_pack: false,
            uploaded: mods
                .uploaded_names()
                .iter()
                .map(|n| n.to_lowercase())
                .collect(),
            counts: Some(mods.counts()),
        }
    }
}

fn host_is(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Whether the file is code the game loads: a jar anywhere, or anything in `mods/`. Configs,
/// resource packs and shaders are not.
pub fn runs_code(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".jar") || lower.starts_with("mods/")
}

pub fn origin_of(file: &ChangedFile, hints: &OriginHints) -> FileOrigin {
    match &file.from {
        FileFrom::Host(host) if host_is(host, "modrinth.com") => FileOrigin::Modrinth,
        FileFrom::Host(host)
            if host_is(host, "forgecdn.net") || host_is(host, "curseforge.com") =>
        {
            FileOrigin::CurseForge
        }
        FileFrom::Host(_) => FileOrigin::Web,
        FileFrom::InPack if hints.modrinth_pack => FileOrigin::Modrinth,
        FileFrom::InPack => {
            let name = file_name(&file.path).to_lowercase();
            if hints.uploaded.contains(&name) {
                return FileOrigin::Uploaded;
            }
            // With no web or unknown sources on the list, a jar inside a server's pack can only
            // be a CurseForge file the Engine fetched (uploaded ones matched above).
            match hints.counts {
                Some(c)
                    if runs_code(&file.path) && c.curseforge > 0 && c.url == 0 && c.other == 0 =>
                {
                    FileOrigin::CurseForge
                }
                _ => FileOrigin::Pack,
            }
        }
    }
}

/// Whether an update writing `file` must be shown to the player before it is applied: code
/// that doesn't come from Modrinth or CurseForge.
pub fn needs_a_look(file: &ChangedFile, origin: FileOrigin) -> bool {
    runs_code(&file.path) && !origin.is_catalogue()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        invite::{InviteMod, ModSource},
        sync::plan::FileChange,
    };

    fn file(path: &str, from: FileFrom) -> ChangedFile {
        ChangedFile {
            name: file_name(path).into(),
            path: path.into(),
            change: FileChange::Added,
            from,
        }
    }

    fn host(h: &str) -> FileFrom {
        FileFrom::Host(h.into())
    }

    fn mods(list: &[(&str, ModSource)]) -> InviteMods {
        InviteMods {
            commit: "c".into(),
            mods: list
                .iter()
                .map(|(name, source)| InviteMod {
                    name: (*name).into(),
                    side: "both".into(),
                    folder: "mods".into(),
                    source: *source,
                })
                .collect(),
        }
    }

    #[test]
    fn hosts_name_their_catalogue() {
        let none = OriginHints::default();
        for (h, want) in [
            ("cdn.modrinth.com", FileOrigin::Modrinth),
            ("modrinth.com", FileOrigin::Modrinth),
            ("mediafilez.forgecdn.net", FileOrigin::CurseForge),
            ("edge.forgecdn.net", FileOrigin::CurseForge),
            ("github.com", FileOrigin::Web),
            ("evilmodrinth.com", FileOrigin::Web),
            ("modrinth.com.example", FileOrigin::Web),
            ("", FileOrigin::Web),
        ] {
            assert_eq!(origin_of(&file("mods/a.jar", host(h)), &none), want, "{h}");
        }
    }

    #[test]
    fn files_inside_a_server_pack_are_told_apart_by_the_mod_list() {
        let list = mods(&[
            ("Custom.jar", ModSource::Uploaded),
            ("JEI", ModSource::CurseForge),
        ]);
        let hints = OriginHints::server(&list);
        let inside = |p: &str| origin_of(&file(p, FileFrom::InPack), &hints);
        assert_eq!(inside("mods/custom.jar"), FileOrigin::Uploaded);
        assert_eq!(inside("mods/jei-1.2.jar"), FileOrigin::CurseForge);
        assert_eq!(inside("config/jei.toml"), FileOrigin::Pack);

        // A web source on the list: a jar inside the pack could be either.
        let mixed = OriginHints::server(&mods(&[
            ("JEI", ModSource::CurseForge),
            ("Tool", ModSource::Url),
        ]));
        assert_eq!(
            origin_of(&file("mods/jei.jar", FileFrom::InPack), &mixed),
            FileOrigin::Pack
        );
        // No list read: nothing is assumed.
        assert_eq!(
            origin_of(
                &file("mods/jei.jar", FileFrom::InPack),
                &OriginHints::default()
            ),
            FileOrigin::Pack
        );
        assert_eq!(
            origin_of(
                &file("mods/x.jar", FileFrom::InPack),
                &OriginHints::modrinth_pack()
            ),
            FileOrigin::Modrinth
        );
    }

    #[test]
    fn only_code_from_outside_the_catalogues_needs_a_look() {
        let f = |p: &str| file(p, FileFrom::InPack);
        assert!(needs_a_look(&f("mods/custom.jar"), FileOrigin::Uploaded));
        assert!(needs_a_look(&f("mods/x.jar"), FileOrigin::Pack));
        assert!(needs_a_look(&f("mods/x.jar"), FileOrigin::Web));
        assert!(needs_a_look(&f("libs/x.JAR"), FileOrigin::Pack));
        assert!(!needs_a_look(&f("mods/x.jar"), FileOrigin::Modrinth));
        assert!(!needs_a_look(&f("mods/x.jar"), FileOrigin::CurseForge));
        assert!(!needs_a_look(&f("config/x.toml"), FileOrigin::Uploaded));
        assert!(!needs_a_look(&f("resourcepacks/x.zip"), FileOrigin::Pack));
    }
}
