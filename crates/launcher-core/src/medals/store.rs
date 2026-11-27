//! `<data root>/medals.json`: every medal tier earned on this computer with when it was first
//! reached, the counters no other file records, and the signed-in account's medals.
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::{error::Result, fsutil::write_atomic};

/// The file's name under the data root.
pub const MEDALS_FILE: &str = "medals.json";

/// The file's format.
const VERSION: u32 = 1;

/// Medal id, then tier (1 to 4), then the Unix second it was first reached.
pub type Earned = BTreeMap<String, BTreeMap<u8, u64>>;

/// What only this file records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Counters {
    /// Modpacks installed from Explore.
    pub modpacks: u64,
    /// Mods installed from Explore, dependencies included.
    pub mods: u64,
    /// Minecraft versions played.
    pub versions: BTreeSet<String>,
    /// UTC days with a run, as `YYYY-MM-DD`.
    pub days: BTreeSet<String>,
    pub longest_run_seconds: u64,
    pub first_game: bool,
    pub theme: bool,
    pub decorator: bool,
    pub tailor: bool,
    /// The instance whose crash the launcher diagnosed, until it runs 10 minutes.
    pub pending_doctor: Option<String>,
    pub doctor: bool,
}

/// The signed-in Perland account's medals as the Engine last answered them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AccountCache {
    pub earned: Earned,
}

/// The whole file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MedalsFile {
    pub version: u32,
    /// Earned on this computer, Perland medals included. Tiers are only ever added.
    pub earned: Earned,
    pub counters: Counters,
    /// None while signed out.
    pub account: Option<AccountCache>,
}

/// Add tiers 1..=`tier` of `id` dated `now`, keeping every date already held. True when a tier
/// was added.
pub fn record(earned: &mut Earned, id: &str, tier: u8, now: u64) -> bool {
    let tiers = earned.entry(id.to_string()).or_default();
    let mut added = false;
    for t in 1..=tier.min(4) {
        tiers.entry(t).or_insert_with(|| {
            added = true;
            now
        });
    }
    if tiers.is_empty() {
        earned.remove(id);
    }
    added
}

/// Every tier of either, the earliest date of each.
pub fn merge(a: &Earned, b: &Earned) -> Earned {
    let mut out = a.clone();
    for (id, tiers) in b {
        let into = out.entry(id.clone()).or_default();
        for (&t, &at) in tiers {
            into.entry(t)
                .and_modify(|old| *old = (*old).min(at))
                .or_insert(at);
        }
    }
    out
}

/// The highest tier of each medal.
pub fn highest(earned: &Earned) -> BTreeMap<String, u8> {
    earned
        .iter()
        .filter_map(|(id, tiers)| tiers.keys().max().map(|&t| (id.clone(), t)))
        .collect()
}

/// The file, read and written under one lock.
pub struct MedalsStore {
    path: PathBuf,
    lock: Mutex<()>,
}

impl MedalsStore {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    /// The file. Missing, damaged or from a newer format, it reads as empty: losing medals'
    /// dates is never worth stopping the launcher.
    pub async fn read(&self) -> MedalsFile {
        let _guard = self.lock.lock().await;
        self.load().await
    }

    async fn load(&self) -> MedalsFile {
        let Ok(bytes) = tokio::fs::read(&self.path).await else {
            return MedalsFile::default();
        };
        match serde_json::from_slice::<MedalsFile>(&bytes) {
            Ok(file) if file.version <= VERSION => file,
            _ => MedalsFile::default(),
        }
    }

    /// Read, change with `f` and write the file; answers the new file and what `f` returned.
    pub async fn update<T>(&self, f: impl FnOnce(&mut MedalsFile) -> T) -> Result<(MedalsFile, T)> {
        let _guard = self.lock.lock().await;
        let mut file = self.load().await;
        let out = f(&mut file);
        file.version = VERSION;
        let bytes = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
        write_atomic(&self.path, &bytes).await?;
        Ok((file, out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(dir: &tempfile::TempDir) -> MedalsStore {
        MedalsStore::new(dir.path().join(MEDALS_FILE))
    }

    #[tokio::test]
    async fn a_missing_damaged_or_newer_file_reads_empty_and_the_next_write_repairs_it() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        assert_eq!(s.read().await, MedalsFile::default());
        for raw in [
            "",
            "{garbage",
            r#"{"version":99,"earned":{"time":{"1":5}}}"#,
        ] {
            std::fs::write(dir.path().join(MEDALS_FILE), raw).unwrap();
            assert_eq!(s.read().await, MedalsFile::default(), "{raw}");
        }
        s.update(|f| record(&mut f.earned, "time", 1, 10))
            .await
            .unwrap();
        let file = s.read().await;
        assert_eq!(file.version, VERSION);
        assert_eq!(file.earned["time"][&1], 10);
    }

    #[test]
    fn record_adds_lower_tiers_and_never_moves_a_date() {
        let mut e = Earned::new();
        assert!(record(&mut e, "time", 3, 100));
        assert_eq!(e["time"], BTreeMap::from([(1, 100), (2, 100), (3, 100)]));
        assert!(!record(&mut e, "time", 2, 50));
        assert_eq!(e["time"][&2], 100);
        assert!(record(&mut e, "time", 4, 200));
        assert!(!record(&mut e, "theme", 0, 1));
        assert!(!e.contains_key("theme"));
    }

    #[test]
    fn merge_keeps_every_tier_and_the_earliest_date() {
        let a = Earned::from([("time".into(), BTreeMap::from([(1, 50), (2, 90)]))]);
        let b = Earned::from([
            ("time".into(), BTreeMap::from([(1, 40), (3, 95)])),
            ("tailor".into(), BTreeMap::from([(1, 7)])),
        ]);
        let m = merge(&a, &b);
        assert_eq!(m["time"], BTreeMap::from([(1, 40), (2, 90), (3, 95)]));
        assert_eq!(m["tailor"][&1], 7);
        assert_eq!(
            highest(&m),
            BTreeMap::from([("tailor".into(), 1), ("time".into(), 3)])
        );
    }

    #[tokio::test]
    async fn the_file_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let s = store(&dir);
        s.update(|f| {
            f.counters.versions.insert("1.21.4".into());
            f.counters.days.insert("2026-11-10".into());
            f.counters.pending_doctor = Some("a".into());
            f.account = Some(AccountCache {
                earned: Earned::from([("tailor".into(), BTreeMap::from([(1, 3)]))]),
            });
        })
        .await
        .unwrap();
        let file = s.read().await;
        assert!(file.counters.versions.contains("1.21.4"));
        assert_eq!(file.counters.pending_doctor.as_deref(), Some("a"));
        assert!(file.account.is_some());
    }
}
