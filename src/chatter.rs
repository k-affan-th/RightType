//! Keys that type twice: a worn or dusty switch that bounces sends a second
//! press a few milliseconds after letting go (`hhello`). No finger does
//! that: a key typed twice on purpose (`ll` in `hello`) is pressed again
//! well after the release.
//!
//! [`Chatter`] sees each physical key event (scan code, down or up, the
//! time Windows stamped on it) and says whether a press is such a bounce.
//! It counts bounces per key — counts only, no order — and, for the keys the
//! typist chose to filter, says to drop the bounce and its release. Plain
//! state: no OS calls.

use std::collections::HashMap;

/// A press this soon after the same key's release is a bounce (ms).
pub const BOUNCE_MS: u32 = 30;
/// A key with this many bounces is worth pointing out.
pub const REPORT_AFTER: u32 = 3;

/// A key: scan code and Windows' extended flag.
pub type KeyId = (u16, bool);

#[derive(Debug, Default)]
pub struct Chatter {
    /// When each key was last released (Windows' millisecond tick).
    released: HashMap<KeyId, u32>,
    /// Bounces seen per key.
    bounces: HashMap<KeyId, u32>,
    /// Keys whose bounces are dropped.
    filtered: Vec<KeyId>,
    /// A dropped press whose release must be dropped too.
    dropping: Option<KeyId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// An ordinary key event.
    Pass,
    /// A bounce on a key not filtered: let through, but counted.
    Bounce,
    /// A bounce (or its release) on a filtered key: drop it.
    Drop,
}

impl Chatter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_filtered(&mut self, keys: Vec<KeyId>) {
        self.filtered = keys;
    }

    pub fn filtered(&self) -> &[KeyId] {
        &self.filtered
    }

    /// One physical key event (`down` or up) at Windows tick `time`.
    /// Auto-repeat (a down with no up between) is never a bounce.
    pub fn observe(&mut self, key: KeyId, down: bool, time: u32) -> Verdict {
        if !down {
            if self.dropping == Some(key) {
                self.dropping = None;
                return Verdict::Drop;
            }
            self.released.insert(key, time);
            return Verdict::Pass;
        }
        // A press: a bounce when the same key was released moments ago.
        let Some(up) = self.released.remove(&key) else {
            return Verdict::Pass;
        };
        if time.wrapping_sub(up) >= BOUNCE_MS {
            return Verdict::Pass;
        }
        *self.bounces.entry(key).or_insert(0) += 1;
        if self.filtered.contains(&key) {
            self.dropping = Some(key);
            Verdict::Drop
        } else {
            Verdict::Bounce
        }
    }

    /// Keys bounced at least [`REPORT_AFTER`] times, most first.
    pub fn suspects(&self) -> Vec<(KeyId, u32)> {
        let mut v: Vec<(KeyId, u32)> = self
            .bounces
            .iter()
            .filter(|(_, &n)| n >= REPORT_AFTER)
            .map(|(&k, &n)| (k, n))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    pub fn forget(&mut self) {
        self.bounces.clear();
        self.released.clear();
        self.dropping = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: KeyId = (0x23, false);
    const L: KeyId = (0x26, false);

    #[test]
    fn a_double_letter_typed_on_purpose_is_not_a_bounce() {
        let mut c = Chatter::new();
        // `ll`: released at 100, pressed again at 180.
        assert_eq!(c.observe(L, true, 0), Verdict::Pass);
        assert_eq!(c.observe(L, false, 100), Verdict::Pass);
        assert_eq!(c.observe(L, true, 180), Verdict::Pass);
        assert!(c.suspects().is_empty());
    }

    #[test]
    fn a_press_right_after_release_is_a_bounce() {
        let mut c = Chatter::new();
        for t in [0u32, 1000, 2000] {
            c.observe(H, true, t);
            c.observe(H, false, t + 60);
            assert_eq!(c.observe(H, true, t + 68), Verdict::Bounce);
            c.observe(H, false, t + 80);
        }
        assert_eq!(c.suspects(), vec![(H, 3)]);
    }

    #[test]
    fn a_filtered_key_loses_its_bounce_and_that_release() {
        let mut c = Chatter::new();
        c.set_filtered(vec![H]);
        assert_eq!(c.observe(H, true, 0), Verdict::Pass);
        assert_eq!(c.observe(H, false, 60), Verdict::Pass);
        assert_eq!(c.observe(H, true, 70), Verdict::Drop);
        assert_eq!(c.observe(H, false, 85), Verdict::Drop);
        // The next real press goes through.
        assert_eq!(c.observe(H, true, 400), Verdict::Pass);
    }

    #[test]
    fn auto_repeat_and_the_clock_wrapping_are_not_bounces() {
        let mut c = Chatter::new();
        assert_eq!(c.observe(L, true, 0), Verdict::Pass);
        assert_eq!(c.observe(L, true, 33), Verdict::Pass);
        assert_eq!(c.observe(L, true, 66), Verdict::Pass);
        c.observe(L, false, u32::MAX - 5);
        assert_eq!(c.observe(L, true, 10), Verdict::Bounce);
    }
}
