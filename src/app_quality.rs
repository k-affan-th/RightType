//! How well corrections land, app by app (2.3 P4): per program, how many
//! words were fixed on their own, how many of those were read back as sent
//! or shown differently, and how many the typist undid. Program names and
//! counts only, held in memory for the session; no word is kept.

use std::collections::HashMap;

/// One app's counts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Quality {
    /// Words fixed on their own.
    pub fixed: u32,
    /// Fixes read back exactly as sent.
    pub shown_right: u32,
    /// Fixes the app showed differently (garbled, or slow to draw).
    pub shown_wrong: u32,
    /// Fixes the typist took back (Shift+Backspace).
    pub undone: u32,
}

/// What happened to one fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Fixed,
    ShownRight,
    ShownWrong,
    Undone,
}

/// How an app is doing, in a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Fixed, read back as sent, rarely undone.
    Good,
    /// Fixes undone often: RightType guesses wrong here; worth a look at
    /// the app's mode.
    OftenUndone,
    /// The app shows fixes differently: RightType already types slower
    /// there.
    ShownWrong,
    /// Too few fixes yet to tell, or the app does not share its text.
    Unknown,
}

/// More apps than this: the least used are dropped.
pub const MAX_APPS: usize = 40;

#[derive(Debug, Default, Clone)]
pub struct Book {
    apps: HashMap<String, Quality>,
}

impl Book {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, exe: &str, event: Event) {
        if exe.is_empty() {
            return;
        }
        let exe = exe.to_ascii_lowercase();
        if !self.apps.contains_key(&exe) && self.apps.len() >= MAX_APPS {
            if let Some(least) = self
                .apps
                .iter()
                .min_by_key(|(_, q)| q.fixed)
                .map(|(k, _)| k.clone())
            {
                self.apps.remove(&least);
            }
        }
        let q = self.apps.entry(exe).or_default();
        let n = match event {
            Event::Fixed => &mut q.fixed,
            Event::ShownRight => &mut q.shown_right,
            Event::ShownWrong => &mut q.shown_wrong,
            Event::Undone => &mut q.undone,
        };
        *n = n.saturating_add(1);
    }

    /// The apps, most fixes first.
    pub fn rows(&self) -> Vec<(String, Quality)> {
        let mut rows: Vec<(String, Quality)> =
            self.apps.iter().map(|(k, q)| (k.clone(), *q)).collect();
        rows.sort_by(|a, b| b.1.fixed.cmp(&a.1.fixed).then(a.0.cmp(&b.0)));
        rows
    }
}

/// The verdict for one app's counts.
pub fn verdict(q: Quality) -> Verdict {
    if q.shown_wrong > 0 && q.shown_wrong * 10 >= q.shown_right {
        return Verdict::ShownWrong;
    }
    if q.fixed < 5 {
        return Verdict::Unknown;
    }
    if q.undone * 5 >= q.fixed {
        return Verdict::OftenUndone;
    }
    Verdict::Good
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(fixed: u32, right: u32, wrong: u32, undone: u32) -> Quality {
        Quality {
            fixed,
            shown_right: right,
            shown_wrong: wrong,
            undone,
        }
    }

    #[test]
    fn counts_per_app_most_fixed_first() {
        let mut b = Book::new();
        for _ in 0..3 {
            b.record("notepad.exe", Event::Fixed);
        }
        b.record("Code.exe", Event::Fixed);
        b.record("CODE.EXE", Event::ShownRight);
        b.record("notepad.exe", Event::ShownRight);
        b.record("notepad.exe", Event::Undone);
        b.record("", Event::Fixed);
        let rows = b.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], ("notepad.exe".into(), q(3, 1, 0, 1)));
        assert_eq!(rows[1], ("code.exe".into(), q(1, 1, 0, 0)));
    }

    #[test]
    fn keeps_at_most_forty_apps() {
        let mut b = Book::new();
        for i in 0..60 {
            b.record(&format!("app{i}.exe"), Event::Fixed);
        }
        assert_eq!(b.rows().len(), MAX_APPS);
    }

    #[test]
    fn verdicts() {
        assert_eq!(verdict(q(2, 2, 0, 0)), Verdict::Unknown);
        assert_eq!(verdict(q(40, 30, 0, 1)), Verdict::Good);
        assert_eq!(verdict(q(20, 10, 0, 5)), Verdict::OftenUndone);
        assert_eq!(verdict(q(20, 10, 2, 0)), Verdict::ShownWrong);
        // A rare slow draw among many right ones is not a problem.
        assert_eq!(verdict(q(200, 150, 1, 0)), Verdict::Good);
    }
}
