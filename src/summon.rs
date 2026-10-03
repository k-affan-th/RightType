//! Shift tapped twice (2.4 A1): the key that opens the list at the text
//! cursor. A tap is Shift pressed and let go on its own, quickly; two in a
//! row, close together, with nothing in between, open the list. Shift held
//! for a capital letter is never a tap (another key comes in between), nor
//! is Shift held long.
//!
//! Times in milliseconds from any clock; no OS calls.

/// Longest press that still counts as a tap.
pub const TAP_MS: u64 = 300;
/// Longest pause between the first tap's release and the second press.
pub const GAP_MS: u64 = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    ShiftDown,
    ShiftUp,
    /// Any other key, either way, or a mouse button.
    Other,
}

#[derive(Debug, Default, Clone)]
pub struct DoubleTap {
    down_at: Option<u64>,
    first_tap_up: Option<u64>,
}

impl DoubleTap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one event; true when it completes a double tap.
    pub fn input(&mut self, input: Input, now: u64) -> bool {
        match input {
            Input::Other => {
                *self = Self::default();
                false
            }
            Input::ShiftDown => {
                if self.down_at.is_none() {
                    // Too long since the first tap: this starts over.
                    if self
                        .first_tap_up
                        .is_some_and(|up| now.saturating_sub(up) > GAP_MS)
                    {
                        self.first_tap_up = None;
                    }
                    self.down_at = Some(now);
                }
                false
            }
            Input::ShiftUp => {
                let Some(down) = self.down_at.take() else {
                    return false;
                };
                if now.saturating_sub(down) > TAP_MS {
                    self.first_tap_up = None;
                    return false;
                }
                if self.first_tap_up.take().is_some() {
                    return true;
                }
                self.first_tap_up = Some(now);
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Input::*;

    fn run(events: &[(Input, u64)]) -> Vec<bool> {
        let mut d = DoubleTap::new();
        events.iter().map(|&(i, t)| d.input(i, t)).collect()
    }

    #[test]
    fn two_quick_taps_open() {
        let r = run(&[
            (ShiftDown, 0),
            (ShiftUp, 80),
            (ShiftDown, 200),
            (ShiftUp, 260),
        ]);
        assert_eq!(r, [false, false, false, true]);
    }

    #[test]
    fn capitals_never_open() {
        // Shift+A, then Shift+B: a key comes between press and release.
        let r = run(&[
            (ShiftDown, 0),
            (Other, 50),
            (ShiftUp, 90),
            (ShiftDown, 150),
            (Other, 180),
            (ShiftUp, 220),
        ]);
        assert!(!r.contains(&true));
        // A tap, then Shift+letter: still not.
        let r = run(&[
            (ShiftDown, 0),
            (ShiftUp, 60),
            (ShiftDown, 150),
            (Other, 170),
            (ShiftUp, 200),
        ]);
        assert!(!r.contains(&true));
    }

    #[test]
    fn slow_or_held_does_not_open() {
        // Held too long.
        let r = run(&[
            (ShiftDown, 0),
            (ShiftUp, 500),
            (ShiftDown, 600),
            (ShiftUp, 650),
        ]);
        assert!(!r.contains(&true));
        // Too far apart; the second tap then counts as a first one.
        let r = run(&[
            (ShiftDown, 0),
            (ShiftUp, 50),
            (ShiftDown, 900),
            (ShiftUp, 950),
            (ShiftDown, 1100),
            (ShiftUp, 1150),
        ]);
        assert_eq!(r, [false, false, false, false, false, true]);
    }

    #[test]
    fn three_taps_open_once() {
        let r = run(&[
            (ShiftDown, 0),
            (ShiftUp, 50),
            (ShiftDown, 150),
            (ShiftUp, 200),
            (ShiftDown, 300),
            (ShiftUp, 350),
        ]);
        assert_eq!(r.iter().filter(|x| **x).count(), 1);
    }
}
