//! Layout-mismatch detection.
//!
//! Given a freshly completed word, decide whether it was typed in the wrong
//! layout and, if so, what the corrected text is. The decision is intentionally
//! conservative: we only propose a correction when the word as typed is *not* a
//! real word but its converted form *is* — that is the unambiguous mistake case.
//!
//! Choosing whether to apply a [`Detection`] automatically or merely suggest it
//! is the caller's job (it depends on the user's selected mode), so detection
//! reports a [`Confidence`] rather than an action.

use crate::dict::Dictionary;
use crate::layout::{en_to_th, th_to_en};
use crate::secret;

/// How sure we are that this is a real mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// As-typed is not a word, converted is — almost certainly a layout slip.
    High,
}

/// A proposed correction for a mistyped word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub corrected: String,
    pub confidence: Confidence,
}

/// Shortest token length we bother analyzing (single keys are too ambiguous).
const MIN_LEN: usize = 2;

fn has_thai(word: &str) -> bool {
    word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

fn has_latin(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_alphabetic())
}

/// Inspect a completed `word`, returning a correction if it looks mistyped.
///
/// `en` / `th` are the reference dictionaries for each language.
pub fn detect(word: &str, en: &Dictionary, th: &Dictionary) -> Option<Detection> {
    let word = word.trim();
    if word.chars().count() < MIN_LEN {
        return None;
    }
    // Never analyze secret-shaped tokens (keys, passwords, …). Seed *phrases* are
    // handled at the stream level by `secret::SeedTracker` in the caller.
    if secret::is_secret_token(word) {
        return None;
    }

    let thai = has_thai(word);
    let latin = has_latin(word);

    // Thai script that isn't a Thai word, but converts to a real English word.
    if thai && !latin {
        if th.contains(word) {
            return None;
        }
        let converted = th_to_en(word);
        if en.contains(&converted) {
            return Some(Detection {
                corrected: converted,
                confidence: Confidence::High,
            });
        }
        return None;
    }

    // Latin script that isn't an English word, but converts to a real Thai word.
    if latin && !thai {
        if en.contains(word) {
            return None;
        }
        let converted = en_to_th(word);
        if th.contains(&converted) {
            return Some(Detection {
                corrected: converted,
                confidence: Confidence::High,
            });
        }
        return None;
    }

    // Mixed or non-letter tokens: leave alone.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dicts() -> (Dictionary, Dictionary) {
        let en = Dictionary::from_words(["correct", "hello", "what", "twitter", "the"]);
        let th = Dictionary::from_words(["สวัสดี", "ครับ"]);
        (en, th)
    }

    #[test]
    fn thai_gibberish_becomes_english() {
        let (en, th) = dicts();
        let d = detect("แนพพำแะ", &en, &th).unwrap();
        assert_eq!(d.corrected, "correct");
        assert_eq!(d.confidence, Confidence::High);
    }

    #[test]
    fn english_gibberish_becomes_thai() {
        let (en, th) = dicts();
        let d = detect("l;ylfu", &en, &th).unwrap();
        assert_eq!(d.corrected, "สวัสดี");
    }

    #[test]
    fn real_words_are_left_alone() {
        let (en, th) = dicts();
        assert!(detect("correct", &en, &th).is_none());
        assert!(detect("hello", &en, &th).is_none());
        assert!(detect("สวัสดี", &en, &th).is_none());
    }

    #[test]
    fn too_short_and_secrets_ignored() {
        let (en, th) = dicts();
        assert!(detect("a", &en, &th).is_none());
        let key = "e9873d79c6d87dc0fb6a5778633389f4453213303da61f20bd67fc233aa33262";
        assert!(detect(key, &en, &th).is_none());
    }

    #[test]
    fn unknown_gibberish_without_a_valid_conversion_is_left_alone() {
        let (en, th) = dicts();
        // Latin, not an English word, and its Thai form isn't a known Thai word.
        assert!(detect("zxqwy", &en, &th).is_none());
    }
}
