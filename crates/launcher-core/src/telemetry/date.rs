//! Calendar days in UTC, with ISO weeks: all the launcher remembers about when it last pinged.
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The first week `installWeek` may name: the week this feature could first have run.
const EARLIEST_WEEK: (i32, u32) = (2026, 40);

/// One UTC calendar day. Ordered by date.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day {
    year: i32,
    month: u32,
    day: u32,
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if is_leap(year) => 29,
        _ => 28,
    }
}

/// 52 or 53: a year has 53 ISO weeks when it starts on a Thursday, or on a Wednesday in a
/// leap year.
fn iso_weeks_in(year: i32) -> u32 {
    let p = |y: i32| (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)).rem_euclid(7);
    if p(year) == 4 || p(year - 1) == 3 {
        53
    } else {
        52
    }
}

impl Day {
    /// `None` unless it is a real date between the years 1970 and 9999.
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        ((1970..=9999).contains(&year)
            && (1..=12).contains(&month)
            && (1..=days_in_month(year, month)).contains(&day))
        .then_some(Self { year, month, day })
    }

    /// The UTC day holding this many seconds since 1970. A time before 1970 is 1970-01-01.
    pub fn from_unix(secs: u64) -> Self {
        // Days to a civil date (Howard Hinnant's algorithm), for days since 1970-01-01.
        let z = (secs / 86_400).min(2_932_896) as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Self { year, month, day }
    }

    /// Today, by this computer's clock.
    pub fn today() -> Self {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Self::from_unix(secs)
    }

    pub fn year_month(self) -> (i32, u32) {
        (self.year, self.month)
    }

    /// The day of the year, from 1.
    fn ordinal(self) -> u32 {
        (1..self.month)
            .map(|m| days_in_month(self.year, m))
            .sum::<u32>()
            + self.day
    }

    /// Monday is 1, Sunday 7.
    fn weekday(self) -> u32 {
        // Days since 1970-01-01, a Thursday.
        let y = i64::from(self.year) - 1;
        let before = y * 365 + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400);
        let days = before + i64::from(self.ordinal()) - 719_163;
        ((days + 3).rem_euclid(7) + 1) as u32
    }

    /// The ISO year and week (weeks start on Monday; week 1 holds the year's first Thursday).
    pub fn iso_week(self) -> (i32, u32) {
        let week = (self.ordinal() + 10 - self.weekday()) / 7;
        if week < 1 {
            (self.year - 1, iso_weeks_in(self.year - 1))
        } else if week > iso_weeks_in(self.year) {
            (self.year + 1, 1)
        } else {
            (self.year, week)
        }
    }
}

/// `YYYY-Www` for an ISO year and week.
fn week_label((year, week): (i32, u32)) -> String {
    format!("{year:04}-W{week:02}")
}

/// The `installWeek` for an install first run on `day`: its ISO week, never before 2026-W40
/// (a clock set to the past doesn't invent an older install).
pub fn install_week(day: Day) -> String {
    week_label(day.iso_week().max(EARLIEST_WEEK))
}

/// The `installWeek` to send on `today`: the one remembered, but never a week after today's
/// (it was written while the clock was ahead, and the Engine refuses a week in the future).
pub fn install_week_to_send(remembered: &str, today: Day) -> String {
    // `YYYY-Www` sorts as text.
    let now = install_week(today);
    if remembered > now.as_str() {
        now
    } else {
        remembered.to_string()
    }
}

/// Is `label` an `installWeek` the launcher could have written?
pub fn is_install_week(label: &str) -> bool {
    let Some((year, week)) = label.split_once("-W") else {
        return false;
    };
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(year, 4) || !digits(week, 2) {
        return false;
    }
    match (year.parse::<i32>(), week.parse::<u32>()) {
        (Ok(year), Ok(week)) => {
            (year, week) >= EARLIEST_WEEK
                && year <= 9999
                && (1..=iso_weeks_in(year)).contains(&week)
        }
        _ => false,
    }
}

impl fmt::Display for Day {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl std::str::FromStr for Day {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        let mut parts = s.split('-');
        let (Some(y), Some(m), Some(d), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return Err(());
        };
        if y.len() != 4 || m.len() != 2 || d.len() != 2 {
            return Err(());
        }
        let num = |s: &str| s.parse::<u32>().map_err(|_| ());
        Day::new(num(y)? as i32, num(m)?, num(d)?).ok_or(())
    }
}

impl Serialize for Day {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Day {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(|()| serde::de::Error::custom("not a date"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> Day {
        s.parse().unwrap()
    }

    #[test]
    fn unix_seconds_become_utc_days() {
        assert_eq!(Day::from_unix(0).to_string(), "1970-01-01");
        assert_eq!(Day::from_unix(86_399).to_string(), "1970-01-01");
        assert_eq!(Day::from_unix(86_400).to_string(), "1970-01-02");
        // 2026-11-11T00:00:00Z and the second before it.
        assert_eq!(Day::from_unix(1_794_355_200).to_string(), "2026-11-11");
        assert_eq!(Day::from_unix(1_794_355_199).to_string(), "2026-11-10");
        // 2024-02-29T12:00:00Z.
        assert_eq!(Day::from_unix(1_709_208_000).to_string(), "2024-02-29");
        // Absurd clocks still give a date.
        assert_eq!(Day::from_unix(u64::MAX).year_month().0, 9999);
    }

    #[test]
    fn iso_weeks_at_the_edges_of_years() {
        for (date, week) in [
            ("2026-11-11", (2026, 46)),
            ("2026-11-09", (2026, 46)),
            ("2026-11-15", (2026, 46)),
            ("2026-11-16", (2026, 47)),
            ("2026-12-31", (2026, 53)),
            ("2027-01-03", (2026, 53)),
            ("2027-01-04", (2027, 1)),
            ("2024-12-30", (2025, 1)),
            ("2025-12-28", (2025, 52)),
            ("2025-12-29", (2026, 1)),
            ("2021-01-03", (2020, 53)),
            ("2023-01-01", (2022, 52)),
            ("1970-01-01", (1970, 1)),
        ] {
            assert_eq!(day(date).iso_week(), week, "{date}");
        }
    }

    #[test]
    fn days_order_by_date_and_round_trip_as_text() {
        assert!(day("2026-11-11") > day("2026-11-10"));
        assert!(day("2027-01-01") > day("2026-12-31"));
        assert_eq!(
            serde_json::to_string(&day("2026-01-05")).unwrap(),
            "\"2026-01-05\""
        );
        assert_eq!(
            serde_json::from_str::<Day>("\"2026-01-05\"").unwrap(),
            day("2026-01-05")
        );
        for bad in [
            "2026-02-30",
            "2026-13-01",
            "2026-1-5",
            "x",
            "2026-01-05T00",
            "1969-12-31",
            "",
        ] {
            assert!(bad.parse::<Day>().is_err(), "{bad}");
            assert!(serde_json::from_str::<Day>(&format!("\"{bad}\"")).is_err());
        }
    }

    #[test]
    fn the_install_week_is_never_before_the_feature() {
        assert_eq!(install_week(day("2026-11-11")), "2026-W46");
        assert_eq!(install_week(day("2027-01-03")), "2026-W53");
        assert_eq!(install_week(day("2026-09-28")), "2026-W40");
        // A clock in the past.
        assert_eq!(install_week(day("2026-09-27")), "2026-W40");
        assert_eq!(install_week(day("1970-01-01")), "2026-W40");
    }

    #[test]
    fn an_install_week_from_a_clock_that_ran_ahead_is_not_sent_as_the_future() {
        let today = day("2026-11-11");
        assert_eq!(install_week_to_send("2026-W46", today), "2026-W46");
        assert_eq!(install_week_to_send("2026-W41", today), "2026-W41");
        assert_eq!(install_week_to_send("2026-W47", today), "2026-W46");
        assert_eq!(install_week_to_send("2031-W02", today), "2026-W46");
        // Across a year's end, and never before the feature.
        assert_eq!(
            install_week_to_send("2026-W53", day("2027-01-05")),
            "2026-W53"
        );
        assert_eq!(
            install_week_to_send("2027-W02", day("2027-01-05")),
            "2027-W01"
        );
        assert_eq!(
            install_week_to_send("2026-W45", day("2020-01-01")),
            "2026-W40"
        );
    }

    #[test]
    fn only_real_install_weeks_are_accepted() {
        for good in ["2026-W40", "2026-W53", "2027-W01", "2030-W52"] {
            assert!(is_install_week(good), "{good}");
        }
        for bad in [
            "2026-W39",
            "2025-W50",
            "2027-W53",
            "2027-W00",
            "2027-W1",
            "2027-01",
            "",
            "abc",
            "2027-W01x",
        ] {
            assert!(!is_install_week(bad), "{bad}");
        }
    }
}
