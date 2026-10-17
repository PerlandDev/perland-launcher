//! The newest news post a player has seen, per server, kept in `<data root>/news-seen.json`
//! so the UI can show a "new" dot.
use std::{collections::BTreeMap, path::PathBuf};

use tokio::sync::Mutex;

use crate::{error::Result, fsutil::write_atomic};

/// The file's name under the data root.
pub const NEWS_SEEN_FILE: &str = "news-seen.json";
/// Servers remembered at most; past it the entry with the oldest marker goes first.
pub const MAX_SEEN: usize = 500;

/// Per server (its canonical invite), the `createdAt` of the newest post the player has seen.
pub struct NewsSeen {
    path: PathBuf,
    cap: usize,
    lock: Mutex<()>,
}

impl NewsSeen {
    pub fn new(path: PathBuf) -> Self {
        Self::with_cap(path, MAX_SEEN)
    }

    /// Like [`NewsSeen::new`] with another cap on the servers remembered (for tests).
    pub fn with_cap(path: PathBuf, cap: usize) -> Self {
        Self {
            path,
            cap,
            lock: Mutex::new(()),
        }
    }

    /// The marker stored for `canonical`, as it was given. A missing or damaged file, or a
    /// stored value that is not a time, reads as none.
    pub async fn seen(&self, canonical: &str) -> Option<String> {
        let _guard = self.lock.lock().await;
        let map = self.read().await;
        map.get(canonical)
            .filter(|v| rfc3339_millis(v).is_some())
            .cloned()
    }

    /// Remember `newest_created_at` (the Engine's `createdAt` of the newest post seen) for
    /// `canonical`. The marker only moves forward: an older or equal time, or one that is not
    /// RFC3339, changes nothing and writes nothing.
    pub async fn mark_seen(&self, canonical: &str, newest_created_at: &str) -> Result<()> {
        let Some(new_ms) = rfc3339_millis(newest_created_at) else {
            return Ok(());
        };
        let _guard = self.lock.lock().await;
        let mut map = self.read().await;
        if let Some(old_ms) = map.get(canonical).and_then(|v| rfc3339_millis(v)) {
            if new_ms <= old_ms {
                return Ok(());
            }
        }
        map.insert(canonical.to_string(), newest_created_at.to_string());
        while map.len() > self.cap.max(1) {
            // The entry just written stays, so a mark is never lost the moment it is made.
            let oldest = map
                .iter()
                .filter(|(k, _)| k.as_str() != canonical)
                .min_by_key(|(k, v)| (rfc3339_millis(v).unwrap_or(i64::MIN), (*k).clone()))
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    map.remove(&k);
                }
                None => break,
            }
        }
        let bytes = serde_json::to_vec_pretty(&map).map_err(std::io::Error::other)?;
        write_atomic(&self.path, &bytes).await
    }

    async fn read(&self) -> BTreeMap<String, String> {
        match tokio::fs::read(&self.path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => BTreeMap::new(),
        }
    }
}

/// Days from 1970-01-01 to the civil date (proleptic Gregorian), by the days-from-civil
/// algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn digits(b: &[u8]) -> Option<i64> {
    if b.is_empty() || !b.iter().all(u8::is_ascii_digit) {
        return None;
    }
    Some(b.iter().fold(0i64, |n, d| n * 10 + i64::from(d - b'0')))
}

/// Unix milliseconds of an RFC3339 time as the Engine writes it:
/// `YYYY-MM-DDTHH:MM:SS[.fraction]` then `Z` or `±HH:MM` (converted to UTC). A fraction of 1 to
/// 9 digits is cut to milliseconds. Anything else is `None`.
pub fn rfc3339_millis(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year = digits(&b[0..4])?;
    let month = digits(&b[5..7])?;
    let day = digits(&b[8..10])?;
    let hour = digits(&b[11..13])?;
    let min = digits(&b[14..16])?;
    let sec = digits(&b[17..19])?;
    let dim = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => return None,
    };
    if !(1..=dim).contains(&day) || hour > 23 || min > 59 || sec > 59 {
        return None;
    }
    let mut i = 19;
    let mut millis = 0;
    if b[i] == b'.' {
        let start = i + 1;
        let mut end = start;
        while end < b.len() && b[end].is_ascii_digit() {
            end += 1;
        }
        let n = end - start;
        if !(1..=9).contains(&n) {
            return None;
        }
        let mut frac = b[start..end.min(start + 3)].to_vec();
        while frac.len() < 3 {
            frac.push(b'0');
        }
        millis = digits(&frac)?;
        i = end;
    }
    let offset_secs = match b.get(i..)? {
        b"Z" => 0,
        rest if rest.len() == 6 && (rest[0] == b'+' || rest[0] == b'-') && rest[3] == b':' => {
            let oh = digits(&rest[1..3])?;
            let om = digits(&rest[4..6])?;
            if oh > 23 || om > 59 {
                return None;
            }
            let secs = oh * 3600 + om * 60;
            if rest[0] == b'+' {
                secs
            } else {
                -secs
            }
        }
        _ => return None,
    };
    let local = days_from_civil(year, month, day) * 86_400 + hour * 3600 + min * 60 + sec;
    Some((local - offset_secs) * 1000 + millis)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(cap: usize) -> (tempfile::TempDir, NewsSeen) {
        let dir = tempfile::tempdir().unwrap();
        let s = NewsSeen::with_cap(dir.path().join(NEWS_SEEN_FILE), cap);
        (dir, s)
    }

    #[test]
    fn rfc3339_reads_utc() {
        assert_eq!(rfc3339_millis("1970-01-01T00:00:01Z"), Some(1000));
        assert_eq!(rfc3339_millis("1969-12-31T23:59:59Z"), Some(-1000));
        assert_eq!(
            rfc3339_millis("2026-10-16T18:00:00Z"),
            Some(1_792_173_600_000)
        );
    }

    #[test]
    fn rfc3339_reads_a_leap_day() {
        assert_eq!(
            rfc3339_millis("2024-02-29T00:00:00Z"),
            Some(1_709_164_800_000)
        );
        assert_eq!(rfc3339_millis("2025-02-29T00:00:00Z"), None);
    }

    #[test]
    fn rfc3339_reads_fractions_of_one_to_nine_digits() {
        let base = 1_792_173_600_000;
        assert_eq!(rfc3339_millis("2026-10-16T18:00:00.5Z"), Some(base + 500));
        assert_eq!(rfc3339_millis("2026-10-16T18:00:00.12Z"), Some(base + 120));
        assert_eq!(rfc3339_millis("2026-10-16T18:00:00.123Z"), Some(base + 123));
        assert_eq!(
            rfc3339_millis("2026-10-16T18:00:00.123456Z"),
            Some(base + 123)
        );
        assert_eq!(
            rfc3339_millis("2026-10-16T18:00:00.123999999Z"),
            Some(base + 123)
        );
        assert_eq!(rfc3339_millis("2026-10-16T18:00:00.1234567890Z"), None);
        assert_eq!(rfc3339_millis("2026-10-16T18:00:00.Z"), None);
    }

    #[test]
    fn rfc3339_converts_offsets_to_utc() {
        let utc = 1_792_173_600_000;
        assert_eq!(rfc3339_millis("2026-10-16T20:00:00+02:00"), Some(utc));
        assert_eq!(rfc3339_millis("2026-10-16T13:30:00-04:30"), Some(utc));
        assert_eq!(rfc3339_millis("2026-10-16T18:00:00+00:00"), Some(utc));
        assert_eq!(
            rfc3339_millis("2026-10-16T18:00:00.250-00:00"),
            Some(utc + 250)
        );
    }

    #[test]
    fn rfc3339_rejects_invalid_input() {
        for bad in [
            "",
            "soon",
            "2026-10-16",
            "2026-10-16T18:00:00",
            "2026-10-16 18:00:00Z",
            "2026-13-16T18:00:00Z",
            "2026-00-16T18:00:00Z",
            "2026-04-31T18:00:00Z",
            "2026-10-16T24:00:00Z",
            "2026-10-16T18:60:00Z",
            "2026-10-16T18:00:60Z",
            "2026-10-16T18:00:00+2:00",
            "2026-10-16T18:00:00+24:00",
            "2026-10-16T18:00:00+02:60",
            "2026-10-16T18:00:00+0200",
            "2026-10-16T18:00:00Zjunk",
            "26-10-16T18:00:00Z",
            "2026-10-16T18:00:0xZ",
        ] {
            assert_eq!(rfc3339_millis(bad), None, "{bad:?}");
        }
    }

    #[tokio::test]
    async fn a_marker_round_trips() {
        let (_d, s) = store(MAX_SEEN);
        assert_eq!(s.seen("a.example/x").await, None);
        s.mark_seen("a.example/x", "2026-10-16T18:00:00Z")
            .await
            .unwrap();
        assert_eq!(
            s.seen("a.example/x").await.as_deref(),
            Some("2026-10-16T18:00:00Z")
        );
    }

    #[tokio::test]
    async fn the_marker_survives_a_new_store_on_the_same_file() {
        let (d, s) = store(MAX_SEEN);
        s.mark_seen("k", "2026-10-16T18:00:00.5Z").await.unwrap();
        let again = NewsSeen::new(d.path().join(NEWS_SEEN_FILE));
        assert_eq!(
            again.seen("k").await.as_deref(),
            Some("2026-10-16T18:00:00.5Z")
        );
    }

    #[tokio::test]
    async fn the_marker_only_moves_forward() {
        let (d, s) = store(MAX_SEEN);
        s.mark_seen("k", "2026-10-16T18:00:00Z").await.unwrap();
        s.mark_seen("k", "2026-10-10T00:00:00Z").await.unwrap();
        assert_eq!(s.seen("k").await.as_deref(), Some("2026-10-16T18:00:00Z"));
        // The same instant in another form is not newer: nothing is rewritten.
        let before = std::fs::read(d.path().join(NEWS_SEEN_FILE)).unwrap();
        s.mark_seen("k", "2026-10-16T20:00:00+02:00").await.unwrap();
        assert_eq!(
            std::fs::read(d.path().join(NEWS_SEEN_FILE)).unwrap(),
            before
        );
        // Markers compare by time, not by text.
        s.mark_seen("k", "2026-10-16T19:00:00Z").await.unwrap();
        assert_eq!(s.seen("k").await.as_deref(), Some("2026-10-16T19:00:00Z"));
    }

    #[tokio::test]
    async fn an_invalid_time_is_ignored() {
        let (d, s) = store(MAX_SEEN);
        s.mark_seen("k", "soon").await.unwrap();
        assert_eq!(s.seen("k").await, None);
        assert!(!d.path().join(NEWS_SEEN_FILE).exists());
    }

    #[tokio::test]
    async fn the_oldest_markers_go_past_the_cap() {
        let (_d, s) = store(3);
        s.mark_seen("a", "2026-10-02T00:00:00Z").await.unwrap();
        s.mark_seen("b", "2026-10-01T00:00:00Z").await.unwrap();
        s.mark_seen("c", "2026-10-03T00:00:00Z").await.unwrap();
        s.mark_seen("d", "2026-10-04T00:00:00Z").await.unwrap();
        assert_eq!(s.seen("b").await, None);
        assert!(s.seen("a").await.is_some());
        assert!(s.seen("c").await.is_some());
        assert!(s.seen("d").await.is_some());
    }

    #[tokio::test]
    async fn a_new_marker_is_kept_even_when_it_is_the_oldest() {
        let (_d, s) = store(2);
        s.mark_seen("a", "2026-10-02T00:00:00Z").await.unwrap();
        s.mark_seen("b", "2026-10-03T00:00:00Z").await.unwrap();
        s.mark_seen("c", "2026-10-01T00:00:00Z").await.unwrap();
        assert!(s.seen("c").await.is_some());
        assert_eq!(s.seen("a").await, None);
        assert!(s.seen("b").await.is_some());
    }

    #[tokio::test]
    async fn a_corrupt_file_reads_as_empty() {
        let (d, s) = store(MAX_SEEN);
        let path = d.path().join(NEWS_SEEN_FILE);
        std::fs::write(&path, "{nope").unwrap();
        assert_eq!(s.seen("k").await, None);
        s.mark_seen("k", "2026-10-16T18:00:00Z").await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["k"], "2026-10-16T18:00:00Z");
    }

    #[tokio::test]
    async fn an_unparsable_stored_marker_is_replaced() {
        let (d, s) = store(MAX_SEEN);
        std::fs::write(d.path().join(NEWS_SEEN_FILE), r#"{"k":"garbage"}"#).unwrap();
        s.mark_seen("k", "2026-10-16T18:00:00Z").await.unwrap();
        assert_eq!(s.seen("k").await.as_deref(), Some("2026-10-16T18:00:00Z"));
    }
}
