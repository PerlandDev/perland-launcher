//! What the launcher keeps between starts, in `telemetry.json`: the week it was installed, the
//! day it last pinged, counters and the reports not sent yet. No identifier: two installs with
//! the same history have the same file.
use std::{collections::BTreeMap, path::Path};

use serde::{Deserialize, Serialize};

use super::{
    date::{install_week, is_install_week, Day},
    payload::Report,
};
use crate::fsutil::write_atomic_blocking;

pub const STATE_FILE: &str = "telemetry.json";
/// The most reports kept. Past it the oldest goes.
pub const QUEUE_CAP: usize = 50;
/// The most reports in one request.
pub const BATCH_REPORTS: usize = 20;
/// The most bytes of reports in one request, leaving room under the Engine's 64 KiB for the
/// body's other fields.
pub const BATCH_BYTES: usize = 56 << 10;
/// The most a counter holds.
pub const COUNTER_MAX: u32 = 100_000;

macro_rules! counters {
    ($($variant:ident => $name:literal: $doc:literal),* $(,)?) => {
        /// Things the launcher counts between pings. A counter says how often something
        /// happened, never which server, pack or account it happened with.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Counter {
            $(#[doc = $doc] $variant),*
        }

        impl Counter {
            pub const ALL: &'static [Counter] = &[$(Counter::$variant),*];

            /// The key in the ping's `counters`.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Counter::$variant => $name),*
                }
            }

            /// What the README says this counts.
            pub fn describes(self) -> &'static str {
                match self {
                    $(Counter::$variant => $doc),*
                }
            }
        }
    };
}

counters! {
    GameLaunches => "game_launches": "The game started.",
    InstancesCreated => "instances_created": "An instance was created by hand (New instance or the first run).",
    PacksInstalled => "packs_installed": "A modpack was added as a new instance (from Browse, a file or a link).",
    JoinsByLink => "joins_by_link": "A server was added from an invite link or the server list.",
    PerlandSignIns => "perland_sign_ins": "A sign-in to a Perland account finished.",
    MicrosoftSignIns => "microsoft_sign_ins": "A sign-in to a Microsoft account finished.",
    FreeServersCreated => "free_servers_created": "A free server was created.",
    SkinsChanged => "skins_changed": "A skin was put on an account.",
    CrashFixesApplied => "crash_fixes_applied": "A fix offered after a crash was applied.",
    UpdatesInstalled => "updates_installed": "A launcher update was installed.",
}

/// What a due ping says about the pings before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PingFlags {
    pub first_ever: bool,
    pub first_this_week: bool,
    pub first_this_month: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    version: u32,
    /// The ISO week of this install's first run with the feature, `YYYY-Www`.
    install_week: Option<String>,
    /// The UTC day of the last ping the Engine took.
    last_ping: Option<Day>,
    /// Sums since that ping.
    counters: BTreeMap<String, u32>,
    /// Reports not sent yet, oldest first.
    reports: Vec<Report>,
}

impl State {
    /// A missing, unreadable or damaged file is an empty state, and anything in it over a
    /// limit or unknown is cut or dropped: this file is never worth an error.
    pub fn load(path: &Path) -> State {
        let Ok(bytes) = std::fs::read(path) else {
            return State::default();
        };
        let Ok(read) = serde_json::from_slice::<State>(&bytes) else {
            return State::default();
        };
        let mut state = State {
            version: 1,
            install_week: read.install_week.filter(|w| is_install_week(w)),
            last_ping: read.last_ping,
            counters: BTreeMap::new(),
            reports: Vec::new(),
        };
        for counter in Counter::ALL {
            if let Some(n) = read.counters.get(counter.as_str()) {
                if *n > 0 {
                    state
                        .counters
                        .insert(counter.as_str().into(), (*n).min(COUNTER_MAX));
                }
            }
        }
        for report in read.reports.iter().rev().take(QUEUE_CAP).rev() {
            state.reports.push(report.rebuilt());
        }
        state
    }

    /// Blocking, and atomic: a reader never sees half a file.
    pub fn save(&self, path: &Path) -> crate::Result<()> {
        let mut out = self.clone();
        out.version = 1;
        let bytes = serde_json::to_vec_pretty(&out).map_err(std::io::Error::other)?;
        write_atomic_blocking(path, &bytes)
    }

    pub fn install_week(&self) -> Option<&str> {
        self.install_week.as_deref()
    }
    pub fn last_ping(&self) -> Option<Day> {
        self.last_ping
    }
    pub fn counters(&self) -> &BTreeMap<String, u32> {
        &self.counters
    }
    pub fn reports(&self) -> &[Report] {
        &self.reports
    }

    /// Remembers the install week the first time. `true` when it changed something.
    pub fn note_install(&mut self, today: Day) -> bool {
        if self.install_week.is_some() {
            return false;
        }
        self.install_week = Some(install_week(today));
        true
    }

    /// Queues a report. One identical to a queued one only raises that one's count.
    pub fn push(&mut self, report: Report) {
        if let Some(same) = self.reports.iter_mut().find(|r| r.same_as(&report)) {
            same.add_count(report.count());
            return;
        }
        self.reports.push(report);
        if self.reports.len() > QUEUE_CAP {
            let over = self.reports.len() - QUEUE_CAP;
            self.reports.drain(..over);
        }
    }

    pub fn count(&mut self, counter: Counter) {
        let n = self.counters.entry(counter.as_str().into()).or_insert(0);
        *n = n.saturating_add(1).min(COUNTER_MAX);
    }

    /// Whether a ping is due on `today`, and what it says. Never on a day at or before the
    /// last ping's: a clock set back sends nothing until it passes that day again.
    pub fn ping_due(&self, today: Day) -> Option<PingFlags> {
        match self.last_ping {
            None => Some(PingFlags {
                first_ever: true,
                first_this_week: true,
                first_this_month: true,
            }),
            Some(last) if today > last => Some(PingFlags {
                first_ever: false,
                first_this_week: today.iso_week() != last.iso_week(),
                first_this_month: today.year_month() != last.year_month(),
            }),
            Some(_) => None,
        }
    }

    /// The Engine took `today`'s ping, which carried `sent`: what was counted since stays.
    pub fn ping_delivered(&mut self, today: Day, sent: &BTreeMap<String, u32>) {
        self.last_ping = Some(self.last_ping.map_or(today, |last| last.max(today)));
        self.drop_counted(sent);
    }

    /// Takes `sent` off the counters without a ping having been taken (the Engine refused
    /// that body for good).
    pub fn drop_counted(&mut self, sent: &BTreeMap<String, u32>) {
        for (key, n) in sent {
            if let Some(have) = self.counters.get_mut(key) {
                *have = have.saturating_sub(*n);
            }
        }
        self.counters.retain(|_, n| *n > 0);
    }

    /// The queue cut into requests: at most [`BATCH_REPORTS`] reports and `max_bytes` of JSON
    /// each (one report over `max_bytes` goes alone).
    pub fn batches(&self, max_bytes: usize) -> Vec<Vec<Report>> {
        let mut batches: Vec<Vec<Report>> = Vec::new();
        let mut bytes = 0;
        for report in &self.reports {
            let size = serde_json::to_vec(report).map_or(0, |b| b.len() + 1);
            let fits = batches
                .last()
                .is_some_and(|b| b.len() < BATCH_REPORTS && bytes + size <= max_bytes);
            if fits {
                batches
                    .last_mut()
                    .expect("just checked")
                    .push(report.clone());
                bytes += size;
            } else {
                batches.push(vec![report.clone()]);
                bytes = size;
            }
        }
        batches
    }

    /// These reports are done with (taken or refused). One that happened again meanwhile
    /// keeps the count it gained.
    pub fn forget(&mut self, sent: &[Report]) {
        for done in sent {
            if let Some(at) = self.reports.iter().position(|r| r.same_as(done)) {
                if !self.reports[at].sub_count(done.count()) {
                    self.reports.remove(at);
                }
            }
        }
    }

    /// The player turned sharing off: reports and counters go. The two dates stay, so turning
    /// it back on doesn't count this install twice.
    pub fn clear_collected(&mut self) {
        self.reports.clear();
        self.counters.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.reports.is_empty() && self.counters.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::payload::ReportKind;

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    fn report(n: usize) -> Report {
        Report::new(ReportKind::AppError, &format!("error {n}")).code("disk_io")
    }

    #[test]
    fn counter_keys_fit_the_contract_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for counter in Counter::ALL {
            let key = counter.as_str();
            assert!(key.len() <= 40 && key.starts_with(|c: char| c.is_ascii_lowercase()));
            assert!(key
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'));
            assert!(seen.insert(key), "{key} twice");
            assert!(!counter.describes().is_empty());
        }
        assert!(Counter::ALL.len() <= 40);
    }

    #[test]
    fn the_first_ping_is_first_of_everything() {
        let flags = State::default().ping_due(day("2026-11-11")).unwrap();
        assert_eq!(
            flags,
            PingFlags {
                first_ever: true,
                first_this_week: true,
                first_this_month: true
            }
        );
    }

    #[test]
    fn later_pings_compare_with_the_last_one_taken() {
        let mut state = State::default();
        // A Wednesday.
        state.ping_delivered(day("2026-11-25"), &BTreeMap::new());
        assert_eq!(state.ping_due(day("2026-11-25")), None, "same day");
        let flags = |state: &State, date: &str| {
            let f = state.ping_due(day(date)).unwrap();
            assert!(!f.first_ever);
            (f.first_this_week, f.first_this_month)
        };
        assert_eq!(
            flags(&state, "2026-11-26"),
            (false, false),
            "next day, same week"
        );
        assert_eq!(
            flags(&state, "2026-11-29"),
            (false, false),
            "Sunday of that week"
        );
        assert_eq!(
            flags(&state, "2026-11-30"),
            (true, false),
            "Monday, same month"
        );
        assert_eq!(flags(&state, "2026-12-01"), (true, true), "next month");
        // The week of 2026-11-30 runs into December: same week, new month.
        state.ping_delivered(day("2026-11-30"), &BTreeMap::new());
        assert_eq!(flags(&state, "2026-12-02"), (false, true));
        // A year on, the same week number and month are still new.
        assert_eq!(flags(&state, "2027-11-30"), (true, true));
    }

    #[test]
    fn a_clock_set_back_never_pings_a_day_again() {
        let mut state = State::default();
        state.ping_delivered(day("2026-11-25"), &BTreeMap::new());
        assert_eq!(state.ping_due(day("2026-11-24")), None);
        assert_eq!(state.ping_due(day("2025-01-01")), None);
        assert_eq!(state.ping_due(day("1970-01-01")), None);
        // A ping taken on an earlier day (the clock went back between due and taken) doesn't
        // move the last day back.
        state.ping_delivered(day("2026-11-20"), &BTreeMap::new());
        assert_eq!(state.last_ping(), Some(day("2026-11-25")));
        assert!(state.ping_due(day("2026-11-26")).is_some());
    }

    #[test]
    fn counters_keep_what_was_counted_while_the_ping_travelled() {
        let mut state = State::default();
        for _ in 0..4 {
            state.count(Counter::GameLaunches);
        }
        state.count(Counter::SkinsChanged);
        let sent = state.counters().clone();
        state.count(Counter::GameLaunches);
        state.ping_delivered(day("2026-11-25"), &sent);
        assert_eq!(
            state.counters(),
            &BTreeMap::from([("game_launches".to_string(), 1)])
        );
        // More "sent" than counted can't go below zero.
        state.drop_counted(&BTreeMap::from([("game_launches".to_string(), 9)]));
        assert!(state.counters().is_empty());
    }

    #[test]
    fn a_counter_stops_at_its_limit() {
        let mut state = State::default();
        state
            .counters
            .insert("game_launches".into(), COUNTER_MAX - 1);
        state.count(Counter::GameLaunches);
        state.count(Counter::GameLaunches);
        assert_eq!(state.counters()["game_launches"], COUNTER_MAX);
    }

    #[test]
    fn the_queue_collapses_repeats_and_drops_the_oldest() {
        let mut state = State::default();
        state.push(report(0));
        state.push(report(0));
        state.push(report(0));
        assert_eq!(state.reports().len(), 1);
        assert_eq!(state.reports()[0].count(), 3);
        for n in 1..=QUEUE_CAP {
            state.push(report(n));
        }
        assert_eq!(state.reports().len(), QUEUE_CAP);
        assert_eq!(state.reports()[0].message(), "error 1");
        assert_eq!(state.reports()[QUEUE_CAP - 1].message(), "error 50");
    }

    #[test]
    fn a_repeat_count_stops_at_a_thousand() {
        let mut state = State::default();
        for _ in 0..1005 {
            state.push(report(0));
        }
        assert_eq!(state.reports()[0].count(), 1000);
    }

    #[test]
    fn batches_respect_both_limits() {
        let mut state = State::default();
        for n in 0..45 {
            state.push(report(n));
        }
        let sizes: Vec<usize> = state.batches(BATCH_BYTES).iter().map(Vec::len).collect();
        assert_eq!(sizes, [20, 20, 5]);

        let mut big = State::default();
        for n in 0..6 {
            big.push(report(n).stack(&"x".repeat(8000)));
        }
        let batches = big.batches(20_000);
        assert_eq!(batches.iter().map(Vec::len).collect::<Vec<_>>(), [2, 2, 2]);
        for batch in &batches {
            assert!(serde_json::to_vec(batch).unwrap().len() <= 20_000);
        }
        // One report over the limit still goes, alone.
        assert_eq!(big.batches(100).len(), 6);
        assert!(State::default().batches(BATCH_BYTES).is_empty());
    }

    #[test]
    fn the_largest_possible_batch_fits_the_engine_limit() {
        let mut state = State::default();
        for n in 0..QUEUE_CAP {
            let mut fat = Report::new(ReportKind::Panic, &format!("{n}{}", "\"".repeat(500)))
                .code(&"c".repeat(64))
                .stack(&"\\".repeat(8192));
            for k in 0..16 {
                fat = fat.context(&format!("k{k}{}", "k".repeat(28)), &"\"".repeat(200));
            }
            state.push(fat);
        }
        for batch in state.batches(BATCH_BYTES) {
            let body = serde_json::to_vec(&crate::telemetry::ErrorBatch::new(
                &crate::telemetry::Client::new(
                    &"9".repeat(32),
                    "windows",
                    &"a".repeat(16),
                    "es-MX-xx",
                ),
                batch,
            ))
            .unwrap();
            assert!(body.len() <= 64 << 10, "{}", body.len());
        }
    }

    #[test]
    fn forgetting_keeps_what_happened_again_meanwhile() {
        let mut state = State::default();
        state.push(report(0));
        state.push(report(1));
        let sent = state.batches(BATCH_BYTES).remove(0);
        state.push(report(0));
        state.push(report(0));
        state.forget(&sent);
        assert_eq!(state.reports().len(), 1);
        assert_eq!(state.reports()[0].message(), "error 0");
        assert_eq!(state.reports()[0].count(), 2);
    }

    #[test]
    fn turning_sharing_off_keeps_only_the_dates() {
        let mut state = State::default();
        state.note_install(day("2026-11-11"));
        state.ping_delivered(day("2026-11-11"), &BTreeMap::new());
        state.push(report(0));
        state.count(Counter::GameLaunches);
        assert!(!state.is_empty());
        state.clear_collected();
        assert!(state.is_empty());
        assert_eq!(state.install_week(), Some("2026-W46"));
        assert_eq!(state.last_ping(), Some(day("2026-11-11")));
    }

    #[test]
    fn the_install_week_is_noted_once() {
        let mut state = State::default();
        assert!(state.note_install(day("2026-11-11")));
        assert!(!state.note_install(day("2027-03-01")));
        assert_eq!(state.install_week(), Some("2026-W46"));
    }

    #[test]
    fn the_file_holds_dates_counters_and_reports_and_no_identifier() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(STATE_FILE);
        let mut state = State::default();
        state.note_install(day("2026-11-11"));
        state.ping_delivered(day("2026-11-12"), &BTreeMap::new());
        state.count(Counter::GameLaunches);
        state.push(report(0));
        state.save(&path).unwrap();

        let raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let keys: Vec<&str> = raw
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            ["counters", "installWeek", "lastPing", "reports", "version"]
        );
        assert_eq!(raw["installWeek"], "2026-W46");
        assert_eq!(raw["lastPing"], "2026-11-12");
        assert_eq!(raw["counters"], serde_json::json!({ "game_launches": 1 }));
        // Two installs with the same history write the same bytes.
        let other = tmp.path().join("other.json");
        State::load(&path).save(&other).unwrap();
        assert_eq!(
            std::fs::read(&path).unwrap(),
            std::fs::read(&other).unwrap()
        );
        assert_eq!(State::load(&path), {
            let mut expected = state.clone();
            expected.version = 1;
            expected
        });
    }

    #[test]
    fn a_damaged_or_edited_file_is_never_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join(STATE_FILE);
        assert_eq!(State::load(&path), State::default(), "missing");
        std::fs::write(&path, b"{not json").unwrap();
        assert_eq!(State::load(&path), State::default(), "damaged");
        std::fs::write(&path, br#"{"lastPing":"tomorrow"}"#).unwrap();
        assert_eq!(State::load(&path), State::default(), "a bad date");

        let reports: Vec<_> = (0..70)
            .map(|n| serde_json::json!({ "kind": "panic", "message": format!("m{n}") }))
            .collect();
        let edited = serde_json::json!({
            "installWeek": "1999-W01",
            "installId": "abc",
            "lastPing": "2026-11-12",
            "counters": { "game_launches": 4_000_000_000u32, "which_server": 3, "skins_changed": 0 },
            "reports": reports
        });
        std::fs::write(&path, serde_json::to_vec(&edited).unwrap()).unwrap();
        let state = State::load(&path);
        assert_eq!(state.install_week(), None);
        assert_eq!(state.last_ping(), Some(day("2026-11-12")));
        assert_eq!(
            state.counters(),
            &BTreeMap::from([("game_launches".to_string(), COUNTER_MAX)])
        );
        assert_eq!(state.reports().len(), QUEUE_CAP);
        assert_eq!(state.reports()[0].message(), "m20");
        // Saving it back drops what the launcher doesn't know.
        state.save(&path).unwrap();
        assert!(!std::fs::read_to_string(&path)
            .unwrap()
            .contains("installId"));
    }
}
