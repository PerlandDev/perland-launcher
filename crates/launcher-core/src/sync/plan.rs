//! The decision table of core spec §3.2, as a pure function over what is on disk.
use std::collections::{BTreeMap, BTreeSet, HashSet};

use serde::Serialize;

use crate::{
    sources::{DesiredState, FileMode, PackFile},
    sync::lockfile::Managed,
};

/// What the survey found at a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    Missing,
    /// Present and matching the pack's hashes.
    Same {
        sha1: String,
        size: u64,
    },
    /// Present with other content. For paths the pack no longer lists: simply present.
    Different {
        sha1: String,
        size: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Planned {
    pub file: PackFile,
    /// A file is in the way; it goes to the trash before the new one moves in.
    pub replaces: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub file: PackFile,
    /// What the lockfile records for it (empty: the player's file).
    pub sha1: String,
    pub size: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub write: Vec<Planned>,
    pub keep: Vec<Kept>,
    /// Managed files the pack no longer wants that are still on disk.
    pub trash: Vec<String>,
    /// `.mrpack` override files the player changed, kept instead of updated.
    pub kept_modified: Vec<String>,
}

/// What an update does, in names a player recognizes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub from_version: Option<String>,
    pub to_version: String,
    pub added: Vec<String>,
    pub updated: Vec<String>,
    pub removed: Vec<String>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }
}

/// The choice for every optional client file: the player's, else the pack's default.
pub fn effective_choices(
    desired: &DesiredState,
    choices: &BTreeMap<String, bool>,
) -> BTreeMap<String, bool> {
    desired
        .files
        .iter()
        .filter(|f| f.side.on_client())
        .filter_map(|f| {
            let opt = f.optional.as_ref()?;
            Some((
                f.path.clone(),
                choices.get(&f.path).copied().unwrap_or(opt.default),
            ))
        })
        .collect()
}

/// The files this client should have: client and both-side ones, minus optional files turned off.
pub fn wanted<'a>(
    desired: &'a DesiredState,
    choices: &BTreeMap<String, bool>,
) -> Vec<&'a PackFile> {
    let effective = effective_choices(desired, choices);
    desired
        .files
        .iter()
        .filter(|f| f.side.on_client())
        .filter(|f| effective.get(&f.path).copied().unwrap_or(true))
        .collect()
}

/// Invariants: a disk key that is absent means `Missing`; only lockfile paths with a non-empty
/// sha1 (files sync wrote) are ever trashed; files that are neither wanted nor in the lockfile
/// are never planned.
pub fn plan(
    wanted: &[&PackFile],
    managed: Option<&Managed>,
    disk: &BTreeMap<String, OnDisk>,
) -> Plan {
    let mut plan = Plan::default();
    let lock = |path: &str| managed.and_then(|m| m.files.get(path));
    for &file in wanted {
        let keep = |sha1: &str, size: u64| Kept {
            file: file.clone(),
            sha1: sha1.to_string(),
            size,
        };
        match disk.get(&file.path).unwrap_or(&OnDisk::Missing) {
            OnDisk::Missing => plan.write.push(Planned {
                file: file.clone(),
                replaces: false,
            }),
            OnDisk::Same { sha1, size } => plan.keep.push(keep(sha1, *size)),
            OnDisk::Different { sha1, size } => match file.mode {
                FileMode::Replace => plan.write.push(Planned {
                    file: file.clone(),
                    replaces: true,
                }),
                FileMode::Preserve => match lock(&file.path) {
                    // Sync kept the player's file in place before: it is still theirs.
                    Some(entry) if entry.sha1.is_empty() => plan.keep.push(keep("", *size)),
                    // The launcher never wrote it: it is the player's.
                    None => plan.keep.push(keep("", *size)),
                    Some(_) => plan.keep.push(keep(sha1, *size)),
                },
                FileMode::ReplaceUnlessModified => match lock(&file.path) {
                    Some(entry) if !entry.sha1.is_empty() && entry.sha1 == *sha1 => {
                        plan.write.push(Planned {
                            file: file.clone(),
                            replaces: true,
                        })
                    }
                    Some(entry) => {
                        plan.keep.push(keep(&entry.sha1, entry.size));
                        if entry.declared != file.hashes {
                            plan.kept_modified.push(file.path.clone());
                        }
                    }
                    None => {
                        plan.keep.push(keep("", *size));
                        plan.kept_modified.push(file.path.clone());
                    }
                },
            },
        }
    }
    if let Some(m) = managed {
        let wanted_paths: HashSet<&str> = wanted.iter().map(|f| f.path.as_str()).collect();
        for (path, entry) in &m.files {
            // An empty sha1 marks the player's file that sync kept in place: never trashed.
            if entry.sha1.is_empty() {
                continue;
            }
            let present = !matches!(disk.get(path), None | Some(OnDisk::Missing));
            if present && !wanted_paths.contains(path.as_str()) {
                plan.trash.push(path.clone());
            }
        }
    }
    plan
}

/// The changelog for syncing to `wanted`. A mod whose jar was renamed (same name, new path)
/// counts as updated, not as removed plus added.
pub fn changes(wanted: &[&PackFile], managed: Option<&Managed>, to_version: &str) -> Changes {
    let mut added = BTreeSet::new();
    let mut updated = BTreeSet::new();
    let mut removed = BTreeSet::new();
    let wanted_paths: HashSet<&str> = wanted.iter().map(|f| f.path.as_str()).collect();
    for f in wanted {
        match managed.and_then(|m| m.files.get(&f.path)) {
            None => {
                added.insert(f.name.clone());
            }
            // A preserved file is only written when absent, so a new version never updates it.
            Some(entry) if entry.declared != f.hashes && f.mode != FileMode::Preserve => {
                updated.insert(f.name.clone());
            }
            Some(_) => {}
        }
    }
    if let Some(m) = managed {
        for (path, entry) in &m.files {
            // The player's files (empty sha1) leave the lockfile quietly; they are not removals.
            if !entry.sha1.is_empty() && !wanted_paths.contains(path.as_str()) {
                removed.insert(entry.name.clone());
            }
        }
    }
    let renamed: Vec<String> = added.intersection(&removed).cloned().collect();
    for name in renamed {
        added.remove(&name);
        removed.remove(&name);
        updated.insert(name);
    }
    Changes {
        from_version: managed.map(|m| m.pack_version.clone()),
        to_version: to_version.to_string(),
        added: added.into_iter().collect(),
        updated: updated.into_iter().collect(),
        removed: removed.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        download::Hashes,
        sources::{FileSource, OptionalFile, PackAbout, Side},
        sync::lockfile::ManagedFile,
    };

    fn pf(path: &str, mode: FileMode, hash: &str) -> PackFile {
        PackFile {
            path: path.into(),
            name: path.rsplit('/').next().unwrap().into(),
            source: FileSource::Remote { urls: vec![] },
            hashes: Hashes::sha1(hash),
            size: Some(1),
            side: Side::Both,
            optional: None,
            mode,
            manual: None,
        }
    }

    fn entry(name: &str, sha1: &str, declared: &str) -> ManagedFile {
        ManagedFile {
            name: name.into(),
            sha1: sha1.into(),
            size: 1,
            mode: FileMode::Replace,
            declared: Hashes::sha1(declared),
        }
    }

    fn managed(entries: &[(&str, ManagedFile)]) -> Managed {
        Managed {
            version: Managed::VERSION,
            stamp: "old".into(),
            pack_version: "1.0".into(),
            choices: BTreeMap::new(),
            files: entries
                .iter()
                .map(|(p, e)| (p.to_string(), e.clone()))
                .collect(),
        }
    }

    fn disk(entries: &[(&str, OnDisk)]) -> BTreeMap<String, OnDisk> {
        entries
            .iter()
            .map(|(p, d)| (p.to_string(), d.clone()))
            .collect()
    }

    fn diff(sha1: &str) -> OnDisk {
        OnDisk::Different {
            sha1: sha1.into(),
            size: 1,
        }
    }

    fn same(sha1: &str) -> OnDisk {
        OnDisk::Same {
            sha1: sha1.into(),
            size: 1,
        }
    }

    #[test]
    fn missing_same_and_different_files() {
        let a = pf("mods/a.jar", FileMode::Replace, "aa");
        let b = pf("mods/b.jar", FileMode::Replace, "bb");
        let c = pf("mods/c.jar", FileMode::Replace, "cc");
        let p = plan(
            &[&a, &b, &c],
            None,
            &disk(&[("mods/b.jar", same("bb")), ("mods/c.jar", diff("xx"))]),
        );
        assert_eq!(
            p.write,
            vec![
                Planned {
                    file: a.clone(),
                    replaces: false
                },
                Planned {
                    file: c.clone(),
                    replaces: true
                },
            ]
        );
        assert_eq!(
            p.keep,
            vec![Kept {
                file: b.clone(),
                sha1: "bb".into(),
                size: 1
            }]
        );
        assert!(p.trash.is_empty() && p.kept_modified.is_empty());
    }

    #[test]
    fn preserved_files_are_only_written_when_absent() {
        let cfg = pf("config/x.toml", FileMode::Preserve, "11");
        let p = plan(&[&cfg], None, &disk(&[("config/x.toml", diff("player"))]));
        assert!(p.write.is_empty());
        assert_eq!(
            p.keep,
            vec![Kept {
                file: cfg.clone(),
                sha1: String::new(),
                size: 1
            }]
        );
        assert!(p.kept_modified.is_empty(), "preserve is silent");
        let p = plan(&[&cfg], None, &disk(&[]));
        assert_eq!(
            p.write,
            vec![Planned {
                file: cfg,
                replaces: false
            }]
        );
    }

    #[test]
    fn dropped_managed_files_go_to_the_trash_and_player_files_stay() {
        let lock = managed(&[
            ("mods/gone.jar", entry("Gone", "g1", "g1")),
            ("mods/already-deleted.jar", entry("Deleted", "d1", "d1")),
        ]);
        let p = plan(
            &[],
            Some(&lock),
            &disk(&[
                ("mods/gone.jar", diff("g1")),
                ("mods/already-deleted.jar", OnDisk::Missing),
                ("mods/mine.jar", diff("m1")),
            ]),
        );
        assert_eq!(p.trash, vec!["mods/gone.jar".to_string()]);
        assert!(p.write.is_empty() && p.keep.is_empty());
    }

    #[test]
    fn mrpack_overrides_update_only_while_unmodified() {
        let mut cfg = pf("config/c.toml", FileMode::ReplaceUnlessModified, "new");
        // The player never touched it: the disk still holds what sync wrote.
        let lock = managed(&[("config/c.toml", entry("c.toml", "written", "old"))]);
        let p = plan(
            &[&cfg],
            Some(&lock),
            &disk(&[("config/c.toml", diff("written"))]),
        );
        assert_eq!(
            p.write,
            vec![Planned {
                file: cfg.clone(),
                replaces: true
            }]
        );

        // The player edited it: kept, reported, and the lockfile keeps the old sha1.
        let p = plan(
            &[&cfg],
            Some(&lock),
            &disk(&[("config/c.toml", diff("edited"))]),
        );
        assert!(p.write.is_empty());
        assert_eq!(
            p.keep,
            vec![Kept {
                file: cfg.clone(),
                sha1: "written".into(),
                size: 1
            }]
        );
        assert_eq!(p.kept_modified, vec!["config/c.toml".to_string()]);

        // A file the launcher never wrote is the player's.
        let p = plan(&[&cfg], None, &disk(&[("config/c.toml", diff("theirs"))]));
        assert_eq!(
            p.keep,
            vec![Kept {
                file: cfg.clone(),
                sha1: String::new(),
                size: 1
            }]
        );
        assert_eq!(p.kept_modified, vec!["config/c.toml".to_string()]);

        // Still the player's next time, and not reported again while the pack's copy is unchanged.
        cfg.hashes = Hashes::sha1("old");
        let theirs = managed(&[("config/c.toml", entry("c.toml", "", "old"))]);
        let p = plan(
            &[&cfg],
            Some(&theirs),
            &disk(&[("config/c.toml", diff("theirs"))]),
        );
        assert_eq!(p.keep[0].sha1, "");
        assert!(p.kept_modified.is_empty());
    }

    #[test]
    fn players_files_in_the_lockfile_are_never_trashed_or_reported_removed() {
        let lock = managed(&[("mods/mine.jar", entry("mine.jar", "", "m1"))]);
        let p = plan(&[], Some(&lock), &disk(&[("mods/mine.jar", diff("m1"))]));
        assert!(p.trash.is_empty());
        assert!(changes(&[], Some(&lock), "2.0").removed.is_empty());
    }

    #[test]
    fn preserve_without_a_lockfile_entry_is_the_players_file() {
        let cfg = pf("config/x.toml", FileMode::Preserve, "11");
        let other = managed(&[("mods/other.jar", entry("Other", "o1", "o1"))]);
        let p = plan(
            &[&cfg],
            Some(&other),
            &disk(&[("config/x.toml", diff("player"))]),
        );
        assert_eq!(
            p.keep,
            vec![Kept {
                file: cfg,
                sha1: String::new(),
                size: 1
            }]
        );
    }

    #[test]
    fn preserve_keeps_the_players_empty_sha1_marker() {
        let cfg = pf("config/x.toml", FileMode::Preserve, "11");
        let lock = managed(&[("config/x.toml", entry("x.toml", "", "11"))]);
        let p = plan(
            &[&cfg],
            Some(&lock),
            &disk(&[("config/x.toml", diff("player"))]),
        );
        assert_eq!(
            p.keep,
            vec![Kept {
                file: cfg.clone(),
                sha1: String::new(),
                size: 1
            }]
        );
        let lock = managed(&[("config/x.toml", entry("x.toml", "written", "11"))]);
        let p = plan(
            &[&cfg],
            Some(&lock),
            &disk(&[("config/x.toml", diff("edited"))]),
        );
        assert_eq!(
            p.keep,
            vec![Kept {
                file: cfg,
                sha1: "edited".into(),
                size: 1
            }]
        );
    }

    #[test]
    fn an_optional_file_turned_off_is_trashed() {
        let opt = PackFile {
            optional: Some(OptionalFile {
                default: true,
                description: None,
            }),
            ..pf("mods/opt.jar", FileMode::Replace, "o1")
        };
        let d = state(vec![opt]);
        let choices = BTreeMap::from([("mods/opt.jar".to_string(), false)]);
        let lock = managed(&[("mods/opt.jar", entry("Opt", "o1", "o1"))]);
        let p = plan(
            &wanted(&d, &choices),
            Some(&lock),
            &disk(&[("mods/opt.jar", diff("o1"))]),
        );
        assert_eq!(p.trash, vec!["mods/opt.jar".to_string()]);
    }

    fn state(files: Vec<PackFile>) -> DesiredState {
        DesiredState {
            name: "P".into(),
            pack_version: "2.0".into(),
            minecraft: "1.20.1".into(),
            loader: None,
            files,
            stamp: "new".into(),
            about: PackAbout::default(),
            archive: None,
        }
    }

    #[test]
    fn only_client_files_and_chosen_optionals_are_wanted() {
        let both = pf("mods/both.jar", FileMode::Replace, "1");
        let server = PackFile {
            side: Side::Server,
            ..pf("mods/server.jar", FileMode::Replace, "2")
        };
        let off = PackFile {
            optional: Some(OptionalFile {
                default: false,
                description: None,
            }),
            ..pf("mods/off.jar", FileMode::Replace, "3")
        };
        let on = PackFile {
            optional: Some(OptionalFile {
                default: true,
                description: None,
            }),
            ..pf("mods/on.jar", FileMode::Replace, "4")
        };
        let d = state(vec![both, server, off, on]);
        let paths = |choices: &BTreeMap<String, bool>| {
            wanted(&d, choices)
                .iter()
                .map(|f| f.path.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            paths(&BTreeMap::new()),
            vec!["mods/both.jar", "mods/on.jar"]
        );
        let flipped = BTreeMap::from([
            ("mods/off.jar".to_string(), true),
            ("mods/on.jar".to_string(), false),
        ]);
        assert_eq!(paths(&flipped), vec!["mods/both.jar", "mods/off.jar"]);
        assert_eq!(
            effective_choices(&d, &BTreeMap::new()),
            BTreeMap::from([
                ("mods/off.jar".to_string(), false),
                ("mods/on.jar".to_string(), true)
            ])
        );
    }

    #[test]
    fn changes_name_what_an_update_does() {
        let mut apple2 = pf("mods/apple-2.jar", FileMode::Replace, "a2");
        apple2.name = "AppleSkin".into();
        let mut jei = pf("mods/jei.jar", FileMode::Replace, "j2");
        jei.name = "JEI".into();
        let mut fresh = pf("mods/fresh.jar", FileMode::Replace, "f1");
        fresh.name = "Fresh".into();
        let keep = pf("mods/keep.jar", FileMode::Replace, "k1");
        let lock = managed(&[
            ("mods/apple-1.jar", entry("AppleSkin", "a1", "a1")),
            ("mods/jei.jar", entry("JEI", "j1", "j1")),
            ("mods/gone.jar", entry("Gone", "g1", "g1")),
            ("mods/keep.jar", entry("keep.jar", "k1", "k1")),
        ]);
        let c = changes(&[&apple2, &jei, &fresh, &keep], Some(&lock), "2.0");
        assert_eq!(c.from_version.as_deref(), Some("1.0"));
        assert_eq!(c.to_version, "2.0");
        assert_eq!(c.added, vec!["Fresh"]);
        assert_eq!(
            c.updated,
            vec!["AppleSkin", "JEI"],
            "a renamed jar of the same mod is an update"
        );
        assert_eq!(c.removed, vec!["Gone"]);
        let first = changes(&[&fresh], None, "2.0");
        assert_eq!(
            (first.from_version, first.added),
            (None, vec!["Fresh".to_string()])
        );
    }

    #[test]
    fn a_preserved_file_is_never_reported_as_updated() {
        // Preserve files are only written when absent, so a new pack version never changes them.
        let cfg = pf("config/a.toml", FileMode::Preserve, "c2");
        let lock = managed(&[("config/a.toml", entry("a.toml", "c1", "c1"))]);
        let c = changes(&[&cfg], Some(&lock), "2.0");
        assert!(c.is_empty(), "{c:?}");
    }
}
