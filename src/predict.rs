//! Predicting the language of a text field before you type in it.
//!
//! RightType can count, per *kind of field* (an app plus the kind of control
//! in it), how many finished words ended up Thai and how many English. Once a
//! field has shown a clear habit — at least [`MIN_WORDS`] words, at least
//! [`MIN_SHARE_PERCENT`] % in one language — focusing it switches the keyboard
//! to that language before the first key, so the first word is not typed in
//! the wrong one.
//!
//! Only counts are kept, keyed by the program's file name and the control's
//! class name: never a word, never a window title. The feature is off unless
//! the user turns it on, and the counts can be cleared at any time.

use std::collections::BTreeMap;

use crate::policy::InputLayout;

/// Words a field must have seen before RightType trusts its habit.
pub const MIN_WORDS: u32 = 20;
/// Share of those words that must be in one language.
pub const MIN_SHARE_PERCENT: u32 = 80;
/// Counts are halved when a field reaches this many words, so a habit that
/// changes is followed within a few hundred words.
const DECAY_AT: u32 = 400;
/// At most this many fields are remembered; the least used are dropped.
pub const MAX_FIELDS: usize = 500;

/// Finished words seen in one field.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub thai: u32,
    pub english: u32,
}

impl Counts {
    fn total(self) -> u32 {
        self.thai + self.english
    }

    /// The language this field clearly prefers, if it has shown one.
    pub fn preferred(self) -> Option<InputLayout> {
        let total = self.total();
        if total < MIN_WORDS {
            return None;
        }
        if self.thai * 100 >= total * MIN_SHARE_PERCENT {
            Some(InputLayout::ThaiKedmanee)
        } else if self.english * 100 >= total * MIN_SHARE_PERCENT {
            Some(InputLayout::UsQwerty)
        } else {
            None
        }
    }
}

/// The counts for every field seen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Habits {
    fields: BTreeMap<String, Counts>,
}

impl Habits {
    pub fn new() -> Self {
        Self::default()
    }

    /// A finished word in `layout` was typed in `field`.
    pub fn record(&mut self, field: &str, layout: InputLayout) {
        if !self.fields.contains_key(field) && self.fields.len() >= MAX_FIELDS {
            self.drop_least_used();
        }
        let counts = self.fields.entry(field.to_string()).or_default();
        match layout {
            InputLayout::ThaiKedmanee => counts.thai += 1,
            InputLayout::UsQwerty => counts.english += 1,
        }
        if counts.total() >= DECAY_AT {
            counts.thai /= 2;
            counts.english /= 2;
        }
    }

    /// A word already counted in `field` as `from` was changed to `to` (a
    /// flip, an accepted suggestion, an Undo): move it to the right count.
    pub fn correct(&mut self, field: &str, from: InputLayout, to: InputLayout) {
        if from == to {
            return;
        }
        let Some(counts) = self.fields.get_mut(field) else {
            return;
        };
        let (take, give) = match from {
            InputLayout::ThaiKedmanee => (&mut counts.thai, &mut counts.english),
            InputLayout::UsQwerty => (&mut counts.english, &mut counts.thai),
        };
        if *take > 0 {
            *take -= 1;
            *give += 1;
        }
    }

    /// The language to switch to when `field` gets focus, if any.
    pub fn preferred(&self, field: &str) -> Option<InputLayout> {
        self.fields.get(field).and_then(|c| c.preferred())
    }

    pub fn counts(&self, field: &str) -> Counts {
        self.fields.get(field).copied().unwrap_or_default()
    }

    pub fn len(&self) -> usize {
        self.fields.len()
    }

    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn clear(&mut self) {
        self.fields.clear();
    }

    fn drop_least_used(&mut self) {
        if let Some(key) = self
            .fields
            .iter()
            .min_by_key(|(_, c)| c.total())
            .map(|(k, _)| k.clone())
        {
            self.fields.remove(&key);
        }
    }

    /// Every field and its counts, for saving.
    pub fn entries(&self) -> impl Iterator<Item = (&str, Counts)> {
        self.fields.iter().map(|(k, c)| (k.as_str(), *c))
    }

    /// Rebuild from saved entries (unknown or oversized input is cut down to
    /// [`MAX_FIELDS`]).
    pub fn from_entries(entries: impl IntoIterator<Item = (String, Counts)>) -> Self {
        let mut habits = Self::new();
        for (field, counts) in entries.into_iter().take(MAX_FIELDS) {
            habits.fields.insert(field, counts);
        }
        habits
    }
}

/// The key for a field: the program's file name and the focused control's
/// class name, both lower case (`chrome.exe|chrome_renderwidgethosthwnd`).
pub fn field_key(exe: &str, class: &str) -> String {
    format!("{}|{}", exe.to_lowercase(), class.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: &str = "line.exe|edit";

    #[test]
    fn needs_enough_words_before_it_predicts() {
        let mut h = Habits::new();
        for _ in 0..MIN_WORDS - 1 {
            h.record(F, InputLayout::ThaiKedmanee);
        }
        assert_eq!(h.preferred(F), None);
        h.record(F, InputLayout::ThaiKedmanee);
        assert_eq!(h.preferred(F), Some(InputLayout::ThaiKedmanee));
    }

    #[test]
    fn a_mixed_field_predicts_nothing() {
        let mut h = Habits::new();
        for i in 0..40 {
            h.record(
                F,
                if i % 3 == 0 {
                    InputLayout::UsQwerty
                } else {
                    InputLayout::ThaiKedmanee
                },
            );
        }
        // 26 Thai of 40 = 65 %: below the bar.
        assert_eq!(h.preferred(F), None);
    }

    #[test]
    fn a_changed_habit_is_followed() {
        let mut h = Habits::new();
        for _ in 0..300 {
            h.record(F, InputLayout::ThaiKedmanee);
        }
        for _ in 0..600 {
            h.record(F, InputLayout::UsQwerty);
        }
        assert_eq!(h.preferred(F), Some(InputLayout::UsQwerty));
    }

    #[test]
    fn the_number_of_fields_is_bounded() {
        let mut h = Habits::new();
        for i in 0..MAX_FIELDS + 50 {
            h.record(&format!("app{i}.exe|edit"), InputLayout::UsQwerty);
        }
        assert_eq!(h.len(), MAX_FIELDS);
    }

    #[test]
    fn a_corrected_word_moves_to_the_language_kept() {
        let mut h = Habits::new();
        for _ in 0..MIN_WORDS {
            h.record(F, InputLayout::ThaiKedmanee);
        }
        for _ in 0..MIN_WORDS {
            h.correct(F, InputLayout::ThaiKedmanee, InputLayout::UsQwerty);
        }
        assert_eq!(
            h.counts(F),
            Counts {
                thai: 0,
                english: MIN_WORDS
            }
        );
        assert_eq!(h.preferred(F), Some(InputLayout::UsQwerty));
        // Nothing to take from: no change.
        h.correct(F, InputLayout::ThaiKedmanee, InputLayout::UsQwerty);
        assert_eq!(h.counts(F).english, MIN_WORDS);
    }

    #[test]
    fn keys_are_case_insensitive() {
        assert_eq!(field_key("LINE.EXE", "Edit"), "line.exe|edit");
    }
}
