//! Shift tapped twice (2.4 A1): the key that opens the list at the text
//! cursor. A tap is Shift pressed and let go on its own, quickly; two in a
//! row, close together, with nothing in between, open the list. Shift held
//! for a capital letter is never a tap (another key comes in between), nor
//! is Shift held long.
//!
//! Times in milliseconds from any clock; no OS calls.
//!
//! Also when a click puts away a pane that is open (see [`click_closes`]).

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

/// Where the top of a pane of height `h` goes: under the text cursor
/// (`caret` is its top and bottom, `gap` the space left), or above it when it
/// does not fit below, and always inside the work area `work` (top, bottom).
/// With no text cursor the pane goes at `fallback_top`, kept on the screen.
///
/// Asked again whenever the pane's height changes: a list that is placed for
/// one row and then grows with its rows (the list at the text cursor) ran
/// off the bottom of the screen when the cursor was low on it.
pub fn pane_top(
    caret: Option<(i32, i32)>,
    fallback_top: i32,
    h: i32,
    gap: i32,
    work: (i32, i32),
) -> i32 {
    let (top, bottom) = work;
    let fits = |y: i32| y >= top && y + h <= bottom;
    match caret {
        Some((caret_top, caret_bottom)) => {
            let below = caret_bottom + gap;
            let above = caret_top - gap - h;
            if fits(below) {
                below
            } else if fits(above) {
                above
            } else {
                (bottom - h).max(top)
            }
        }
        None => fallback_top.min(bottom - h).max(top),
    }
}

/// Whether a mouse click at (`x`, `y`) puts away a pane that is open: it does
/// when the click is outside the pane's rectangle `(left, top, right,
/// bottom)`, screen pixels, right and bottom edges not inside. A pane with no
/// rectangle (not open, or its window is gone) has nothing to put away, and
/// a click on the pane itself is the pane's own.
///
/// A pane that never takes the keyboard focus (the list at the text cursor,
/// the app's shortcut list) is not closed by the app losing focus, so a click
/// elsewhere in the same window would leave it open, taking keys: digits, the
/// arrows, Enter.
pub fn click_closes(pane: Option<(i32, i32, i32, i32)>, x: i32, y: i32) -> bool {
    match pane {
        Some((left, top, right, bottom)) => !(x >= left && x < right && y >= top && y < bottom),
        None => false,
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

    #[test]
    fn a_click_outside_a_pane_puts_it_away_and_one_on_it_does_not() {
        let pane = Some((100, 200, 400, 500));
        assert!(
            !click_closes(pane, 100, 200),
            "the top-left corner is on it"
        );
        assert!(!click_closes(pane, 250, 350), "the middle is on it");
        assert!(!click_closes(pane, 399, 499), "the last pixel is on it");
        assert!(click_closes(pane, 400, 350), "right edge is outside");
        assert!(click_closes(pane, 250, 500), "bottom edge is outside");
        assert!(click_closes(pane, 99, 350), "left of it");
        assert!(click_closes(pane, 250, 199), "above it");
        assert!(click_closes(pane, -50, -50), "another monitor");
        assert!(!click_closes(None, 250, 350), "no pane, nothing to close");
    }

    #[test]
    fn a_pane_goes_below_the_cursor_above_it_or_at_the_bottom_of_the_screen() {
        let work = (0, 1000);
        // Room below: under the cursor, with the gap.
        assert_eq!(pane_top(Some((100, 120)), 0, 300, 4, work), 124);
        // Not below, room above: above it.
        assert_eq!(pane_top(Some((800, 820)), 0, 300, 4, work), 496);
        // The cursor right at the bottom edge of the work area.
        assert_eq!(pane_top(Some((980, 1000)), 0, 300, 4, work), 676);
        // A tall pane with room neither side: kept on the screen.
        assert_eq!(pane_top(Some((400, 420)), 0, 700, 4, work), 300);
        // Taller than the screen: the top edge wins.
        assert_eq!(pane_top(Some((400, 420)), 0, 1200, 4, work), 0);
        // No cursor: where asked, but on the screen.
        assert_eq!(pane_top(None, 80, 300, 4, work), 80);
        assert_eq!(pane_top(None, 900, 300, 4, work), 700);
        assert_eq!(pane_top(None, -50, 300, 4, work), 0);
    }
}
