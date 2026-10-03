//! How long the keyboard hook takes per key, as counts in time buckets: no
//! keys, no order, only how many presses fell in each range.
//!
//! Windows lets a key through by itself when the hook takes longer than it
//! allows (`LowLevelHooksTimeout`), and removes a hook that does so often.
//! The problem report shows these numbers, and the time of each key spent
//! waiting for an app to answer (a text box, UI Automation) apart from the
//! rest, so a slow key can be traced to its cause.

use std::sync::atomic::{AtomicU64, Ordering};

/// Upper bounds of the buckets, in microseconds; the last one is open.
const BOUNDS_US: [u64; 9] = [
    500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000, 300_000,
];
const BUCKETS: usize = BOUNDS_US.len() + 1;

/// One measure: counts per bucket and the slowest seen.
pub struct Histogram {
    counts: [AtomicU64; BUCKETS],
    max_us: AtomicU64,
}

impl Histogram {
    pub const fn new() -> Self {
        Histogram {
            counts: [const { AtomicU64::new(0) }; BUCKETS],
            max_us: AtomicU64::new(0),
        }
    }

    pub fn record(&self, us: u64) {
        let i = BOUNDS_US
            .iter()
            .position(|&b| us < b)
            .unwrap_or(BUCKETS - 1);
        self.counts[i].fetch_add(1, Ordering::Relaxed);
        self.max_us.fetch_max(us, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for c in &self.counts {
            c.store(0, Ordering::Relaxed);
        }
        self.max_us.store(0, Ordering::Relaxed);
    }

    pub fn summary(&self) -> Summary {
        let counts: Vec<u64> = self
            .counts
            .iter()
            .map(|c| c.load(Ordering::Relaxed))
            .collect();
        let count: u64 = counts.iter().sum();
        // The bucket bound below which a share of the keys fell.
        let below = |share: f64| -> Option<u64> {
            if count == 0 {
                return None;
            }
            let want = (count as f64 * share).ceil() as u64;
            let mut seen = 0;
            for (i, c) in counts.iter().enumerate() {
                seen += c;
                if seen >= want {
                    return BOUNDS_US.get(i).copied();
                }
            }
            None
        };
        Summary {
            count,
            median_below_us: below(0.5),
            p99_below_us: below(0.99),
            max_us: self.max_us.load(Ordering::Relaxed),
        }
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub count: u64,
    /// Half the keys took less than this (`None`: no keys, or above every
    /// bound).
    pub median_below_us: Option<u64>,
    pub p99_below_us: Option<u64>,
    pub max_us: u64,
}

impl std::fmt::Display for Summary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ms = |us: u64| format!("{:.1} ms", us as f64 / 1000.0);
        let bound = |b: Option<u64>| b.map_or("more".to_string(), |us| format!("< {}", ms(us)));
        if self.count == 0 {
            return write!(f, "no keys");
        }
        write!(
            f,
            "{} keys, half {}, 99% {}, slowest {}",
            self.count,
            bound(self.median_below_us),
            bound(self.p99_below_us),
            ms(self.max_us)
        )
    }
}

/// Every key the hook handled, start to end.
pub static HOOK: Histogram = Histogram::new();
/// The part of each key spent waiting for an app to answer (keys that
/// waited at all).
pub static WAITING: Histogram = Histogram::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_and_summary() {
        let h = Histogram::new();
        assert_eq!(h.summary().to_string(), "no keys");
        for _ in 0..98 {
            h.record(100);
        }
        h.record(7_000);
        h.record(400_000);
        let s = h.summary();
        assert_eq!(s.count, 100);
        assert_eq!(s.median_below_us, Some(500));
        assert_eq!(s.p99_below_us, Some(10_000));
        assert_eq!(s.max_us, 400_000);
        assert_eq!(
            s.to_string(),
            "100 keys, half < 0.5 ms, 99% < 10.0 ms, slowest 400.0 ms"
        );
        h.clear();
        assert_eq!(h.summary().count, 0);
    }
}
