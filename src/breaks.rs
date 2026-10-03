//! Rest reminders: how long typing has gone on without a break. Only a
//! start time, the last key's time and a count are held, in memory; no
//! key is kept or written anywhere.

/// No key for this long is a break: the next key starts a new stretch.
pub const REST_MS: u64 = 5 * 60_000;
/// A stretch this long earns a reminder.
pub const WORK_MS: u64 = 50 * 60_000;
/// Still typing after a reminder: the next one this much later.
pub const AGAIN_MS: u64 = 20 * 60_000;

/// One stretch of typing.
#[derive(Debug, Default, Clone)]
pub struct Streak {
    active: bool,
    start: u64,
    last: u64,
    keys: u32,
    next: u64,
}

/// A reminder: minutes typed in a row, and the keys pressed in them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Due {
    pub minutes: u32,
    pub keys: u32,
}

impl Streak {
    pub fn new() -> Self {
        Self::default()
    }

    /// A key pressed at `now` (any clock in milliseconds).
    pub fn key(&mut self, now: u64) {
        if !self.active || now.saturating_sub(self.last) >= REST_MS {
            *self = Streak {
                active: true,
                start: now,
                last: now,
                keys: 0,
                next: now + WORK_MS,
            };
        }
        self.last = now;
        self.keys = self.keys.saturating_add(1);
    }

    /// Whether a reminder is due at `now` (once; the next comes
    /// [`AGAIN_MS`] later if typing goes on). None during a break.
    pub fn due(&mut self, now: u64) -> Option<Due> {
        if !self.active || now.saturating_sub(self.last) >= REST_MS || now < self.next {
            return None;
        }
        self.next = now + AGAIN_MS;
        Some(Due {
            minutes: (now.saturating_sub(self.start) / 60_000) as u32,
            keys: self.keys,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    /// A key every 2 seconds from `from` to `to` (minutes).
    fn type_for(s: &mut Streak, from: u64, to: u64) {
        let mut t = from * MIN;
        while t < to * MIN {
            s.key(t);
            t += 2_000;
        }
    }

    #[test]
    fn reminds_after_fifty_minutes_in_a_row_then_every_twenty() {
        let mut s = Streak::new();
        type_for(&mut s, 0, 49);
        assert_eq!(s.due(49 * MIN), None);
        type_for(&mut s, 49, 51);
        let due = s.due(51 * MIN).expect("due");
        assert_eq!(due.minutes, 51);
        assert_eq!(due.keys, 51 * 30);
        // Once only, until twenty more minutes.
        assert_eq!(s.due(52 * MIN), None);
        type_for(&mut s, 51, 72);
        assert!(s.due(72 * MIN).is_some());
    }

    #[test]
    fn a_break_starts_over_and_is_never_interrupted() {
        let mut s = Streak::new();
        type_for(&mut s, 0, 30);
        // Six minutes away, then twenty-five more: not fifty in a row.
        type_for(&mut s, 36, 61);
        assert_eq!(s.due(61 * MIN), None);
        // Away from the keyboard: no reminder however long.
        let mut s = Streak::new();
        type_for(&mut s, 0, 49);
        assert_eq!(s.due(60 * MIN), None);
    }
}
