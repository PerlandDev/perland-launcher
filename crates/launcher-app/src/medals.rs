//! Medals: counted where things happen, every tier dated in `<data root>/medals.json`, and
//! synced with the Perland account (docs/superpowers/specs/2026-11-11-medals-saved-and-synced-design.md).
use std::collections::BTreeMap;

use launcher_core::{
    medals::{
        rules::{local_tiers, local_value, perland_tiers, perland_value, tier_of},
        store::{highest, merge, record, AccountCache, Earned, MedalsFile},
        Group, Kind, Measures, Unit, CATALOGUE,
    },
    news_seen::rfc3339_millis,
    perland::{MedalEntry, PlayerStats},
    settings::Theme,
    telemetry::date::Day,
    Result,
};

use crate::{
    app::AppCore,
    dto::{MedalView, MedalsView},
    events::{AppEvent, Host},
    runner::Runner,
};

/// How long a run must last after a diagnosed crash to earn `doctor`.
const DOCTOR_SECONDS: u64 = 600;

/// Something that can move a medal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MedalEvent {
    /// A game run ended, however it ended.
    RunEnded {
        instance: String,
        version: String,
        seconds: u64,
        crashed: bool,
        /// The launcher explained the crash.
        diagnosed: bool,
    },
    ThemeSaved(Theme),
    OwnPicture,
    SkinSaved,
    PackInstalled,
    ModsInstalled(u64),
}

fn now() -> u64 {
    u64::try_from(launcher_core::instances::unix_now()).unwrap_or(0)
}

/// `secs` as RFC 3339 in UTC, to the second.
pub(crate) fn rfc3339(secs: u64) -> String {
    let day = Day::from_unix(secs);
    let s = secs % 86_400;
    format!("{day}T{:02}:{:02}:{:02}Z", s / 3600, s / 60 % 60, s % 60)
}

fn apply(file: &mut MedalsFile, event: &MedalEvent) {
    let c = &mut file.counters;
    match event {
        MedalEvent::RunEnded {
            instance,
            version,
            seconds,
            crashed,
            diagnosed,
        } => {
            c.first_game = true;
            c.days.insert(Day::from_unix(now()).to_string());
            if !version.is_empty() {
                c.versions.insert(version.clone());
            }
            c.longest_run_seconds = c.longest_run_seconds.max(*seconds);
            if *crashed {
                if *diagnosed {
                    c.pending_doctor = Some(instance.clone());
                }
            } else if c.pending_doctor.as_deref() == Some(instance) && *seconds >= DOCTOR_SECONDS {
                c.doctor = true;
                c.pending_doctor = None;
            }
        }
        MedalEvent::ThemeSaved(theme) => c.theme |= *theme != Theme::default(),
        MedalEvent::OwnPicture => c.decorator = true,
        MedalEvent::SkinSaved => c.tailor = true,
        MedalEvent::PackInstalled => c.modpacks = c.modpacks.saturating_add(1),
        MedalEvent::ModsInstalled(n) => c.mods = c.mods.saturating_add(*n),
    }
}

/// Every tier `tiers` reaches that `earned` lacks, dated `at`. True when one was added.
fn record_all(earned: &mut Earned, tiers: &BTreeMap<&'static str, u8>, at: u64) -> bool {
    let mut added = false;
    for (id, &tier) in tiers {
        added |= record(earned, id, tier, at);
    }
    added
}

fn to_entries(earned: &Earned) -> Vec<MedalEntry> {
    earned
        .iter()
        .flat_map(|(id, tiers)| {
            tiers.iter().map(move |(&tier, &at)| MedalEntry {
                id: id.clone(),
                tier,
                earned_at: rfc3339(at),
            })
        })
        .collect()
}

/// The entries the Engine answered, as `Earned`; ones the launcher can't read are dropped.
fn from_entries(entries: &[MedalEntry]) -> Earned {
    let mut out = Earned::new();
    for e in entries {
        let Some(ms) = rfc3339_millis(&e.earned_at) else {
            continue;
        };
        if !(1..=4).contains(&e.tier) || ms < 0 {
            continue;
        }
        let at = (ms / 1000) as u64;
        out.entry(e.id.clone())
            .or_default()
            .entry(e.tier)
            .and_modify(|old| *old = (*old).min(at))
            .or_insert(at);
    }
    out
}

impl<R: Runner, H: Host> AppCore<R, H> {
    /// What the launcher measured, from the counters and the instances on disk.
    async fn medal_measures(&self, file: &MedalsFile) -> Measures {
        let played = self.playtime_list().await.unwrap_or_default();
        let c = &file.counters;
        Measures {
            local_seconds: played.iter().map(|p| p.seconds).sum(),
            best_instance_seconds: played.iter().map(|p| p.seconds).max().unwrap_or(0),
            instances: played.len() as u64,
            modpacks: c.modpacks,
            mods: c.mods,
            versions: c.versions.len() as u64,
            days: c.days.len() as u64,
            longest_run_seconds: c.longest_run_seconds,
            first_game: c.first_game,
            theme: c.theme,
            decorator: c.decorator,
            tailor: c.tailor,
            doctor: c.doctor,
        }
    }

    /// Count `event`, then record any tier it reached. Never fails its caller: a medal is not
    /// worth an error, so a failure is logged.
    pub(crate) async fn medals_note(&self, event: MedalEvent) {
        if let Err(e) = self.medals.update(|f| apply(f, &event)).await {
            tracing::warn!(code = ?e.code, "couldn't count a medal");
            return;
        }
        self.medals_evaluate().await;
    }

    /// Record every tier the measures (and the Perland stats last read) reach now, and sync
    /// when one was added.
    pub(crate) async fn medals_evaluate(&self) {
        let file = self.medals.read().await;
        let measures = self.medal_measures(&file).await;
        let stats = self
            .medal_stats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let at = now();
        let result = self
            .medals
            .update(|f| {
                let held = highest(&f.earned);
                let mut added = record_all(&mut f.earned, &local_tiers(&measures, &held), at);
                if let Some(stats) = &stats {
                    added |= record_all(&mut f.earned, &perland_tiers(stats), at);
                }
                added
            })
            .await;
        match result {
            Ok((_, true)) => {
                self.host.emit(AppEvent::MedalsChanged);
                self.medals_sync().await;
            }
            Ok((_, false)) => {}
            Err(e) => tracing::warn!(code = ?e.code, "couldn't save a medal"),
        }
    }

    /// The Perland stats just read: remembered for the medal view and evaluated.
    pub(crate) async fn medals_stats(&self, stats: &PlayerStats) {
        *self.medal_stats.lock().unwrap_or_else(|e| e.into_inner()) = Some(stats.clone());
        // The Perland ladders' readings come from these stats, earned or not.
        self.host.emit(AppEvent::MedalsChanged);
        self.medals_evaluate().await;
        self.medals_sync().await;
    }

    /// At start: credit what is already on disk, then sync.
    pub async fn medals_start(&self) {
        self.medals_evaluate().await;
        self.medals_sync().await;
    }

    /// Send this computer's medals and the account's to the account, and keep its answer.
    /// Signed out, on an Engine without medals, or on any failure, nothing changes; the next
    /// trigger tries again. One sync at a time; a trigger during one is dropped.
    pub(crate) async fn medals_sync(&self) {
        let Ok(_one) = self.medals_syncing.try_lock() else {
            return;
        };
        let Some(session) = self.perland_session().await else {
            return;
        };
        let file = self.medals.read().await;
        let account = file.account.map(|a| a.earned).unwrap_or_default();
        let entries = to_entries(&merge(&file.earned, &account));
        let answer = self
            .runner
            .player()
            .save_medals(session.engine.clone(), session.token.clone(), entries)
            .await;
        let answered = match answer {
            Ok(Some(list)) => from_entries(&list),
            Ok(None) => return,
            Err(e) => {
                tracing::info!(code = ?e.code, "couldn't sync the medals");
                return;
            }
        };
        // The session may have changed while the request ran: keep only its own answer.
        let still = self.perland_session().await;
        if still.map(|s| s.token) != Some(session.token) {
            return;
        }
        let changed = self
            .medals
            .update(|f| {
                let changed = f.account.as_ref().map(|a| &a.earned) != Some(&answered);
                f.account = Some(AccountCache { earned: answered });
                changed
            })
            .await;
        if let Ok((_, true)) = changed {
            self.host.emit(AppEvent::MedalsChanged);
        }
    }

    /// Forget the account's medals (a sign-in or a sign-out): another account's never show.
    pub(crate) async fn medals_forget_account(&self) {
        *self.medal_stats.lock().unwrap_or_else(|e| e.into_inner()) = None;
        match self.medals.update(|f| f.account.take().is_some()).await {
            Ok((_, true)) => self.host.emit(AppEvent::MedalsChanged),
            Ok(_) => {}
            Err(e) => tracing::warn!(code = ?e.code, "couldn't forget the account's medals"),
        }
    }

    /// Every medal with its tier, progress and the date its tier was reached. Asks for a sync
    /// in the background, so the account's medals arrive soon after the screen opens.
    pub async fn medals_view(self: &std::sync::Arc<Self>) -> Result<MedalsView> {
        let file = self.medals.read().await;
        let measures = self.medal_measures(&file).await;
        let stats = self
            .medal_stats
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let saved_to_account = self.perland_session().await.is_some();
        let shown = match (&file.account, saved_to_account) {
            (Some(a), true) => merge(&file.earned, &a.earned),
            _ => file.earned.clone(),
        };
        let medals = CATALOGUE
            .iter()
            .map(|rule| {
                let value = match rule.group {
                    Group::Local => local_value(rule.id, &measures),
                    Group::Perland => stats
                        .as_ref()
                        .map(|s| perland_value(rule.id, s))
                        .unwrap_or(0),
                };
                let tiers = shown.get(rule.id);
                let tier = tiers
                    .and_then(|t| t.keys().max().copied())
                    .unwrap_or(0)
                    .max(if rule.id == "completionist" {
                        0
                    } else {
                        tier_of(rule, value)
                    });
                let earned_at = tiers.and_then(|t| t.get(&tier)).map(|&at| rfc3339(at));
                let ladder = rule.kind == Kind::Ladder;
                MedalView {
                    id: rule.id.to_string(),
                    group: match rule.group {
                        Group::Local => "local",
                        Group::Perland => "perland",
                    }
                    .to_string(),
                    kind: if ladder { "ladder" } else { "single" }.to_string(),
                    unit: match rule.unit {
                        Unit::Seconds => "time",
                        Unit::Count => "count",
                    }
                    .to_string(),
                    tier: if ladder {
                        tier
                    } else if tier > 0 {
                        rule.rim
                    } else {
                        0
                    },
                    rim: rule.rim,
                    value,
                    goal: if ladder {
                        rule.steps.get(tier as usize).copied()
                    } else {
                        None
                    },
                    steps: rule.steps.to_vec(),
                    earned_at,
                }
            })
            .collect();
        let me = self.clone();
        tokio::spawn(async move { me.medals_sync().await });
        Ok(MedalsView {
            medals,
            saved_to_account,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_round_trips_through_the_parser() {
        assert_eq!(rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339(1_762_862_400 + 3_723), "2025-11-11T13:02:03Z");
        let at = 1_790_000_000;
        assert_eq!(rfc3339_millis(&rfc3339(at)), Some(at as i64 * 1000));
    }

    #[test]
    fn entries_round_trip_and_keep_the_earliest_date() {
        let earned = Earned::from([(
            "time".to_string(),
            BTreeMap::from([(1u8, 1_790_000_000u64), (2, 1_790_000_100)]),
        )]);
        let mut entries = to_entries(&earned);
        entries.push(MedalEntry {
            id: "time".into(),
            tier: 1,
            earned_at: rfc3339(1_780_000_000),
        });
        entries.push(MedalEntry {
            id: "bad".into(),
            tier: 9,
            earned_at: rfc3339(1_780_000_000),
        });
        entries.push(MedalEntry {
            id: "bad".into(),
            tier: 1,
            earned_at: "yesterday".into(),
        });
        let back = from_entries(&entries);
        assert_eq!(back["time"][&1], 1_780_000_000);
        assert_eq!(back["time"][&2], 1_790_000_100);
        assert!(!back.contains_key("bad"));
    }

    fn run(instance: &str, seconds: u64, crashed: bool, diagnosed: bool) -> MedalEvent {
        MedalEvent::RunEnded {
            instance: instance.into(),
            version: "1.21.4".into(),
            seconds,
            crashed,
            diagnosed,
        }
    }

    #[test]
    fn doctor_needs_ten_minutes_after_a_diagnosed_crash() {
        let mut f = MedalsFile::default();
        apply(&mut f, &run("a", 30, true, false));
        apply(&mut f, &run("a", 700, false, false));
        assert!(!f.counters.doctor, "a crash the launcher did not explain");
        apply(&mut f, &run("a", 30, true, true));
        apply(&mut f, &run("b", 900, false, false));
        apply(&mut f, &run("a", 599, false, false));
        assert!(!f.counters.doctor);
        apply(&mut f, &run("a", 600, true, true));
        assert!(!f.counters.doctor, "a crashed run never completes it");
        apply(&mut f, &run("a", 600, false, false));
        assert!(f.counters.doctor);
        assert_eq!(f.counters.pending_doctor, None);
    }

    #[test]
    fn a_run_counts_however_it_ended() {
        let mut f = MedalsFile::default();
        apply(&mut f, &run("a", 9000, true, false));
        assert!(f.counters.first_game);
        assert_eq!(f.counters.longest_run_seconds, 9000);
        assert_eq!(f.counters.versions.len(), 1);
        assert_eq!(f.counters.days.len(), 1);
    }

    #[test]
    fn only_a_theme_other_than_the_default_counts() {
        let mut f = MedalsFile::default();
        apply(&mut f, &MedalEvent::ThemeSaved(Theme::default()));
        assert!(!f.counters.theme);
        apply(&mut f, &MedalEvent::ThemeSaved(Theme::Night));
        assert!(f.counters.theme);
        apply(&mut f, &MedalEvent::ThemeSaved(Theme::default()));
        assert!(f.counters.theme, "earned stays earned");
    }
}
