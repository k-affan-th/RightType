//! An outcome ledger (accuracy plan P0): for every rewrite RightType makes, how
//! it came about and what became of it, so that "wrong a lot of the time" can
//! be told apart by cause instead of guessed at.
//!
//! No text is kept, and none can be: a cell is named by a handful of enums (the
//! path the rewrite took, its direction, how the word ended, the kind of field,
//! a length bucket) and holds counts. There is no string field to put a word
//! in, and [`Ledger::lines`] prints only those names and numbers. Program names
//! stay in [`crate::app_quality`], which already holds them.
//!
//! This module is OS-free. The simulator fills it in (see
//! `examples/survey_*.rs`); wiring it into the keyboard hook is the next step.

use std::collections::BTreeMap;

/// How the rewrite came about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Path {
    /// Shown mid-word and kept at the word's end.
    LiveKept,
    /// Shown mid-word, then put back at the word's end.
    LiveWithdrawn,
    /// Judged at the word's end (`detect_token`).
    Boundary,
    /// A word the live path had finished, judged again (`revise_converted`).
    Revised,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Direction {
    EnglishToThai,
    ThaiToEnglish,
}

/// What ended the word.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ending {
    Space,
    Enter,
    Tab,
    /// No end at all: the caret moved away from the word.
    None,
}

/// What kind of field the text went into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Field {
    /// An ordinary text field, or one not told apart yet.
    Edit,
    Search,
    Combo,
    Address,
}

/// The word's length in characters, in buckets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Len {
    One,
    Two,
    ThreeToFour,
    FiveToEight,
    NineOrMore,
}

impl Len {
    pub fn of(chars: usize) -> Len {
        match chars {
            0 | 1 => Len::One,
            2 => Len::Two,
            3..=4 => Len::ThreeToFour,
            5..=8 => Len::FiveToEight,
            _ => Len::NineOrMore,
        }
    }
}

/// What became of a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing was done about it within the watch window.
    Kept,
    /// The typist undid it (Shift+Backspace or the app's Undo).
    Undone,
    /// The typist backspaced over it.
    BackspacedOver,
    /// The caret moved while a live reading was still on screen, so the word
    /// was never judged at an end.
    CaretMovedWhileLive,
}

/// The cause side of a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cause {
    pub path: Path,
    pub direction: Direction,
    pub ending: Ending,
    pub field: Field,
    pub len: Len,
}

/// The outcome counts for one cause.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub kept: u32,
    pub undone: u32,
    pub backspaced_over: u32,
    pub caret_moved_while_live: u32,
}

impl Counts {
    fn slot(&mut self, outcome: Outcome) -> &mut u32 {
        match outcome {
            Outcome::Kept => &mut self.kept,
            Outcome::Undone => &mut self.undone,
            Outcome::BackspacedOver => &mut self.backspaced_over,
            Outcome::CaretMovedWhileLive => &mut self.caret_moved_while_live,
        }
    }

    pub fn total(&self) -> u32 {
        self.kept + self.undone + self.backspaced_over + self.caret_moved_while_live
    }

    /// Rewrites the typist took back or never saw judged.
    pub fn doubtful(&self) -> u32 {
        self.undone + self.backspaced_over + self.caret_moved_while_live
    }
}

/// At most this many causes: the enums allow 4 x 2 x 4 x 4 x 5 = 640, so the
/// map cannot outgrow it; the cap is a second belt.
const MAX_CELLS: usize = 640;

#[derive(Debug, Default, Clone)]
pub struct Ledger {
    cells: BTreeMap<Cause, Counts>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, cause: Cause, outcome: Outcome) {
        if !self.cells.contains_key(&cause) && self.cells.len() >= MAX_CELLS {
            return;
        }
        let n = self.cells.entry(cause).or_default().slot(outcome);
        *n = n.saturating_add(1);
    }

    /// A rewrite counted as `from` turned out to be `to` (a Kept one the typist
    /// then undid, say). Does nothing if no such rewrite is counted.
    pub fn change(&mut self, cause: Cause, from: Outcome, to: Outcome) {
        let Some(counts) = self.cells.get_mut(&cause) else {
            return;
        };
        if *counts.slot(from) == 0 {
            return;
        }
        *counts.slot(from) -= 1;
        let n = counts.slot(to);
        *n = n.saturating_add(1);
    }

    /// Every cause with counts, in a fixed order.
    pub fn cells(&self) -> impl Iterator<Item = (&Cause, &Counts)> {
        self.cells.iter()
    }

    /// Counts over every cause that `keep` accepts.
    pub fn sum(&self, keep: impl Fn(&Cause) -> bool) -> Counts {
        let mut all = Counts::default();
        for (cause, c) in &self.cells {
            if keep(cause) {
                all.kept += c.kept;
                all.undone += c.undone;
                all.backspaced_over += c.backspaced_over;
                all.caret_moved_while_live += c.caret_moved_while_live;
            }
        }
        all
    }

    /// One line per cause, for a problem report: names and numbers only.
    pub fn lines(&self) -> Vec<String> {
        self.cells
            .iter()
            .map(|(c, n)| {
                format!(
                    "path={:?} dir={:?} end={:?} field={:?} len={:?} kept={} undone={} backspaced={} caret_moved={}",
                    c.path,
                    c.direction,
                    c.ending,
                    c.field,
                    c.len,
                    n.kept,
                    n.undone,
                    n.backspaced_over,
                    n.caret_moved_while_live
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cause(path: Path, len: Len) -> Cause {
        Cause {
            path,
            direction: Direction::EnglishToThai,
            ending: Ending::Space,
            field: Field::Edit,
            len,
        }
    }

    #[test]
    fn counts_are_kept_per_cause_and_summed() {
        let mut l = Ledger::new();
        let boundary = cause(Path::Boundary, Len::FiveToEight);
        l.record(boundary, Outcome::Kept);
        l.record(boundary, Outcome::Kept);
        l.record(boundary, Outcome::Undone);
        l.record(
            cause(Path::LiveKept, Len::Two),
            Outcome::CaretMovedWhileLive,
        );
        let b = l.sum(|c| c.path == Path::Boundary);
        assert_eq!((b.kept, b.undone, b.total(), b.doubtful()), (2, 1, 3, 1));
        assert_eq!(l.sum(|_| true).total(), 4);
    }

    #[test]
    fn a_kept_rewrite_can_turn_out_undone() {
        let mut l = Ledger::new();
        let c = cause(Path::Boundary, Len::Two);
        l.record(c, Outcome::Kept);
        l.record(c, Outcome::Kept);
        l.change(c, Outcome::Kept, Outcome::Undone);
        let n = l.sum(|_| true);
        assert_eq!((n.kept, n.undone, n.total()), (1, 1, 2));
        // Nothing to move: no change, no new cell.
        l.change(c, Outcome::BackspacedOver, Outcome::Undone);
        l.change(
            cause(Path::Revised, Len::One),
            Outcome::Kept,
            Outcome::Undone,
        );
        assert_eq!(l.sum(|_| true).total(), 2);
        assert_eq!(l.cells().count(), 1);
    }

    #[test]
    fn a_kept_rewrite_can_be_backspaced_over_once() {
        let mut l = Ledger::new();
        let c = cause(Path::LiveKept, Len::ThreeToFour);
        l.record(c, Outcome::Kept);
        l.change(c, Outcome::Kept, Outcome::BackspacedOver);
        l.change(c, Outcome::Kept, Outcome::BackspacedOver);
        let n = l.sum(|_| true);
        assert_eq!((n.kept, n.backspaced_over, n.total()), (0, 1, 1));
        assert_eq!(n.doubtful(), 1);
    }

    #[test]
    fn the_field_kind_names_a_cell_and_stays_text_free() {
        let mut l = Ledger::new();
        for field in [Field::Edit, Field::Search, Field::Combo, Field::Address] {
            l.record(
                Cause {
                    field,
                    ..cause(Path::Boundary, Len::Two)
                },
                Outcome::Kept,
            );
        }
        assert_eq!(l.cells().count(), 4);
        assert_eq!(l.sum(|c| c.field == Field::Search).total(), 1);
        let lines = l.lines();
        for name in ["Edit", "Search", "Combo", "Address"] {
            assert!(lines.iter().any(|x| x.contains(&format!("field={name} "))));
        }
        assert!(lines.iter().all(|x| x
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " =_".contains(c))));
    }

    #[test]
    fn length_buckets() {
        let buckets: Vec<Len> = [1, 2, 3, 4, 5, 8, 9, 40].map(Len::of).to_vec();
        assert_eq!(
            buckets,
            [
                Len::One,
                Len::Two,
                Len::ThreeToFour,
                Len::ThreeToFour,
                Len::FiveToEight,
                Len::FiveToEight,
                Len::NineOrMore,
                Len::NineOrMore
            ]
        );
    }

    #[test]
    fn a_report_has_names_and_numbers_only() {
        // The ledger takes no string, so no word can reach a report; this
        // pins the shape of what a line may contain.
        let mut l = Ledger::new();
        l.record(cause(Path::Revised, Len::Two), Outcome::Undone);
        let lines = l.lines();
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("path=Revised dir=EnglishToThai end=Space field=Edit len=Two ")
        );
        assert!(lines[0]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || " =_".contains(c)));
    }

    #[test]
    fn it_cannot_outgrow_its_cap() {
        // Every combination of the enums fits under the cap.
        let paths = [
            Path::LiveKept,
            Path::LiveWithdrawn,
            Path::Boundary,
            Path::Revised,
        ];
        let dirs = [Direction::EnglishToThai, Direction::ThaiToEnglish];
        let ends = [Ending::Space, Ending::Enter, Ending::Tab, Ending::None];
        let fields = [Field::Edit, Field::Search, Field::Combo, Field::Address];
        let lens = [
            Len::One,
            Len::Two,
            Len::ThreeToFour,
            Len::FiveToEight,
            Len::NineOrMore,
        ];
        let mut l = Ledger::new();
        for &path in &paths {
            for &direction in &dirs {
                for &ending in &ends {
                    for &field in &fields {
                        for &len in &lens {
                            l.record(
                                Cause {
                                    path,
                                    direction,
                                    ending,
                                    field,
                                    len,
                                },
                                Outcome::Kept,
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(l.cells().count(), MAX_CELLS);
    }
}
