//! Daily correction counts for the weekly view (opt-in).
//!
//! Only two numbers per day — words fixed automatically and words fixed with
//! a hotkey — keyed by the local calendar day. Never a word, never a time of
//! day, never an app. Days older than [`KEEP_DAYS`] are dropped.

use std::collections::BTreeMap;

/// How many days of counts are kept.
pub const KEEP_DAYS: u32 = 56;

/// Days since 1970-01-01 for a calendar date (proleptic Gregorian).
pub fn day_number(year: i32, month: u32, day: u32) -> u32 {
    // Howard Hinnant's days_from_civil.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m = month as i32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) as u32 + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146_097 + doe as i32 - 719_468) as u32
}

/// `(month, day)` for a day number, for labels.
pub fn month_day(days: u32) -> (u32, u32) {
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = (z - era * 146_097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (m, d)
}

/// One day's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Day {
    pub auto: u64,
    pub manual: u64,
}

/// Counts per day.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Daily {
    days: BTreeMap<u32, Day>,
}

impl Daily {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_auto(&mut self, today: u32) {
        self.days.entry(today).or_default().auto += 1;
        self.prune(today);
    }

    pub fn record_manual(&mut self, today: u32, n: u64) {
        self.days.entry(today).or_default().manual += n;
        self.prune(today);
    }

    fn prune(&mut self, today: u32) {
        let oldest = today.saturating_sub(KEEP_DAYS - 1);
        self.days.retain(|&d, _| d >= oldest);
    }

    /// The 7 days ending with `today`, oldest first.
    pub fn last_week(&self, today: u32) -> [(u32, Day); 7] {
        std::array::from_fn(|i| {
            let d = today.saturating_sub(6 - i as u32);
            (d, self.days.get(&d).copied().unwrap_or_default())
        })
    }

    pub fn entries(&self) -> impl Iterator<Item = (u32, Day)> + '_ {
        self.days.iter().map(|(d, c)| (*d, *c))
    }

    pub fn from_entries(entries: impl IntoIterator<Item = (u32, Day)>, today: u32) -> Self {
        let mut daily = Self {
            days: entries.into_iter().collect(),
        };
        daily.prune(today);
        daily
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_numbers_round_trip() {
        assert_eq!(day_number(1970, 1, 1), 0);
        assert_eq!(day_number(2000, 3, 1), 11_017);
        let d = day_number(2026, 9, 27);
        assert_eq!(month_day(d), (9, 27));
        assert_eq!(month_day(day_number(2028, 2, 29)), (2, 29));
        assert_eq!(day_number(2027, 1, 1) - day_number(2026, 12, 31), 1);
    }

    #[test]
    fn the_week_view_has_seven_days_ending_today() {
        let today = day_number(2026, 9, 27);
        let mut daily = Daily::new();
        daily.record_auto(today);
        daily.record_auto(today);
        daily.record_manual(today - 3, 4);
        let week = daily.last_week(today);
        assert_eq!(week[6], (today, Day { auto: 2, manual: 0 }));
        assert_eq!(week[3].1, Day { auto: 0, manual: 4 });
        assert_eq!(week[0].0, today - 6);
    }

    #[test]
    fn old_days_are_dropped() {
        let today = day_number(2026, 9, 27);
        let daily = Daily::from_entries(
            [
                (today - 100, Day { auto: 5, manual: 0 }),
                (today, Day::default()),
            ],
            today,
        );
        assert_eq!(daily.entries().count(), 1);
    }
}
