//! Word dictionaries used to decide whether a token is a real word.
//!
//! v1 uses simple hashed membership; the plan calls for swapping in an `fst`
//! set later to shrink resident memory. The public API stays the same either way.

use std::collections::HashSet;
use std::sync::{OnceLock, RwLock};

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
    /// Words the user taught at runtime (auto-learn). Kept apart from the
    /// bundled list so they can be forgotten wholesale and so the sorted
    /// prefix index never has to be rebuilt; the overlay stays small enough
    /// that scanning it for continuations is cheaper than any index.
    overlay: RwLock<HashSet<String>>,
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
            overlay: RwLock::new(HashSet::new()),
        }
    }

    /// Is `word` a known word (case-insensitive)? Learned words count.
    pub fn contains(&self, word: &str) -> bool {
        let key = normalize(word);
        self.words.contains(&key) || self.overlay_contains(&key)
    }

    /// Teach this dictionary a word at runtime. Returns `true` if it was new.
    pub fn learn(&self, word: &str) -> bool {
        let key = normalize(word);
        if key.is_empty() || self.words.contains(&key) {
            return false;
        }
        self.overlay
            .write()
            .map(|mut o| o.insert(key))
            .unwrap_or(false)
    }

    /// Forget every runtime-learned word; the bundled list is untouched.
    pub fn forget_learned(&self) {
        if let Ok(mut o) = self.overlay.write() {
            o.clear();
        }
    }

    /// Was this word learned at runtime (rather than bundled)?
    pub fn is_learned(&self, word: &str) -> bool {
        self.overlay_contains(&normalize(word))
    }

    fn overlay_contains(&self, key: &str) -> bool {
        self.overlay
            .read()
            .map(|o| o.contains(key))
            .unwrap_or(false)
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
            || self.overlay.read().is_ok_and(|o| {
                o.iter()
                    .any(|w| w.len() > prefix.len() && w.starts_with(prefix.as_str()))
            })
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
    fn learned_words_join_membership_and_continuations() {
        let d = Dictionary::from_words(["hello"]);
        assert!(!d.contains("kubernetes"));
        assert!(d.learn("Kubernetes"));
        assert!(!d.learn("kubernetes"), "learning twice is a no-op");
        assert!(!d.learn("hello"), "bundled words are never re-learned");
        assert!(d.contains("kubernetes"));
        assert!(d.is_learned("kubernetes"));
        assert!(d.has_extension("kuber"));
        d.forget_learned();
        assert!(!d.contains("kubernetes"));
        assert!(!d.has_extension("kuber"));
        assert!(d.contains("hello"));
    }

    #[test]
    fn bundled_lists_load_and_contain_basics() {
        assert!(english().contains("correct"));
        assert!(english().len() > 1000);
        assert!(thai().len() > 1000);
        assert!(thai().contains("สวัสดี"));
    }
}
