//! Word dictionaries used to decide whether a token is a real word.
//!
//! v1 uses simple hashed membership; the plan calls for swapping in an `fst`
//! set later to shrink resident memory. The public API stays the same either way.

use std::collections::HashSet;
use std::sync::OnceLock;

/// A set of known words for one language, normalized for case-insensitive lookup.
///
/// Alongside the membership set it keeps the same words sorted, so it can also
/// answer *"could this token still grow into a word?"* — the question the live
/// correction path has to ask before it may destroy an in-flight token.
pub struct Dictionary {
    words: HashSet<String>,
    /// Built on first prefix query only. Membership never needs it, and only
    /// the English list is ever asked for continuations, so the Thai list never
    /// pays for an index nobody reads — which matters against the resident-memory
    /// target in `docs/PLAN.md`.
    sorted: OnceLock<Vec<Box<str>>>,
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
            sorted: OnceLock::new(),
        }
    }

    /// Is `word` a known word (case-insensitive)?
    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(&normalize(word))
    }

    /// Is there a known word that **starts with** `prefix` but is longer than it?
    ///
    /// This is the "live continuation" test. A token that can still grow into a
    /// real word is not yet evidence of anything: `diffe` is not an English word,
    /// but it is on the way to `different`, so converting it is destructive
    /// rather than helpful. Answered by binary search over the sorted words, so
    /// it costs a handful of comparisons and no extra allocation per query.
    pub fn has_extension(&self, prefix: &str) -> bool {
        let prefix = normalize(prefix);
        if prefix.is_empty() {
            return false;
        }
        let sorted = self.sorted.get_or_init(|| {
            let mut v: Vec<Box<str>> = self.words.iter().map(|w| w.as_str().into()).collect();
            v.sort_unstable();
            v
        });
        let from = sorted.partition_point(|w| w.as_ref() < prefix.as_str());
        sorted[from..]
            .iter()
            .take_while(|w| w.starts_with(prefix.as_str()))
            .any(|w| w.as_ref() != prefix.as_str())
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
    fn has_extension_finds_live_continuations() {
        let d = Dictionary::from_words(["different", "differ", "hello", "ok"]);
        // On the way to a longer word.
        assert!(d.has_extension("diffe"));
        assert!(d.has_extension("differ")); // exact word that still extends
        assert!(d.has_extension("hell"));
        // Complete and terminal: nothing longer starts with these.
        assert!(!d.has_extension("different"));
        assert!(!d.has_extension("hello"));
        assert!(!d.has_extension("ok"));
        // Not on the way to anything.
        assert!(!d.has_extension("zzz"));
        assert!(!d.has_extension(""));
        // Case-insensitive, like membership.
        assert!(d.has_extension("DIFFE"));
    }

    #[test]
    fn has_extension_matches_the_bundled_english_list() {
        let en = english();
        assert!(en.has_extension("diffe"));
        assert!(en.has_extension("compu"));
        assert!(en.has_extension("wri"));
        assert!(!en.has_extension("qqqq"));
    }

    #[test]
    fn bundled_lists_load_and_contain_basics() {
        assert!(english().contains("correct"));
        assert!(english().len() > 1000);
        assert!(thai().len() > 1000);
        assert!(thai().contains("สวัสดี"));
    }
}
