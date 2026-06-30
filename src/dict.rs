//! Word dictionaries used to decide whether a token is a real word.
//!
//! v1 uses simple hashed membership; the plan calls for swapping in an `fst`
//! set later to shrink resident memory. The public API stays the same either way.

use std::collections::HashSet;
use std::sync::OnceLock;

/// A set of known words for one language, normalized for case-insensitive lookup.
pub struct Dictionary {
    words: HashSet<String>,
}

impl Dictionary {
    /// Build a dictionary from any iterator of words (used by bundled lists and tests).
    pub fn from_words<I, S>(iter: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            words: iter
                .into_iter()
                .map(|s| normalize(s.as_ref()))
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    /// Is `word` a known word (case-insensitive)?
    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(&normalize(word))
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

/// Normalize a word for lookup: trim and lowercase. Lowercasing is a no-op for
/// Thai script, and folds English case so `Correct` matches `correct`.
fn normalize(s: &str) -> String {
    s.trim().to_lowercase()
}

/// Bundled English dictionary (common words).
pub fn english() -> &'static Dictionary {
    static D: OnceLock<Dictionary> = OnceLock::new();
    D.get_or_init(|| Dictionary::from_words(include_str!("../assets/en_words.txt").lines()))
}

/// Bundled Thai dictionary.
pub fn thai() -> &'static Dictionary {
    static D: OnceLock<Dictionary> = OnceLock::new();
    D.get_or_init(|| Dictionary::from_words(include_str!("../assets/th_words.txt").lines()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn membership_is_case_insensitive() {
        let d = Dictionary::from_words(["Correct", "hello"]);
        assert!(d.contains("correct"));
        assert!(d.contains("CORRECT"));
        assert!(!d.contains("nope"));
        assert_eq!(d.len(), 2);
    }

    #[test]
    fn bundled_lists_load_and_contain_basics() {
        assert!(english().contains("correct"));
        assert!(english().len() > 1000);
        assert!(thai().len() > 1000);
        assert!(thai().contains("สวัสดี"));
    }
}
