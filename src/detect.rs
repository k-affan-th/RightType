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
use crate::secret::{self, SecretKind};
use crate::segment;

/// How sure we are that this is a real mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// As-typed is not a word, converted is — almost certainly a layout slip.
    High,
}

/// Evidence that made the converted candidate valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    ExactDictionary,
    FullSegmentation,
}

/// A proposed correction for a mistyped word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detection {
    pub corrected: String,
    pub confidence: Confidence,
    pub evidence: Evidence,
}

/// Shortest token length we bother analyzing (single keys are too ambiguous).
const MIN_LEN: usize = 2;

fn has_thai(word: &str) -> bool {
    word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

fn has_latin(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_alphabetic())
}

/// Is an English dictionary word wrapped only in ASCII punctuation?
///
/// The buffer keeps punctuation with the token because punctuation can be a Thai
/// character when the wrong layout is active.  Once Thai has been converted back
/// to English, however, a trailing comma or period should not make an otherwise
/// unambiguous word invisible to detection.
fn is_english_word_with_edge_punctuation(word: &str, en: &Dictionary) -> bool {
    if en.contains(word) {
        return true;
    }

    let chars: Vec<char> = word.chars().collect();
    let mut start = 0;
    let mut end = chars.len();
    while start < end && chars[start].is_ascii_punctuation() {
        start += 1;
    }
    while start < end && chars[end - 1].is_ascii_punctuation() {
        end -= 1;
    }
    start < end
        && (start != 0 || end != chars.len())
        && en.contains(&chars[start..end].iter().collect::<String>())
}

/// Inspect a completed `word`, returning a correction if it looks mistyped.
///
/// `en` / `th` are the reference dictionaries for each language.
pub fn detect(word: &str, en: &Dictionary, th: &Dictionary) -> Option<Detection> {
    let word = word.trim();
    if word.chars().count() < MIN_LEN {
        return None;
    }

    let thai = has_thai(word);
    let latin = has_latin(word);

    // Thai script that isn't a Thai word, but converts to a real English word.
    // Thai text is never ASCII, so the secret guard (which only fires on ASCII)
    // is fine to apply first — it will never block genuine Thai.
    if thai && !latin {
        if secret::is_secret_token(word) {
            return None;
        }
        if th.contains(word) {
            return None;
        }
        let converted = th_to_en(word);
        if is_english_word_with_edge_punctuation(&converted, en) {
            return Some(Detection {
                corrected: converted,
                confidence: Confidence::High,
                evidence: Evidence::ExactDictionary,
            });
        }
        return None;
    }

    // Latin script that isn't an English word, but converts to a real Thai word.
    //
    // The secret guard runs AFTER the conversion attempt here. Thai characters
    // map to QWERTY keys that include digits (0→ข, 5→ี, 8→า, 9→ถ) and punctuation
    // (;→น, '→ง, [→บ, ]→ล …). A Thai sentence typed on the wrong layout therefore
    // produces a mixed-class ASCII string that the entropy detector classifies as
    // a generated password. By trying the conversion first we avoid that false
    // positive: if the result is fully-known Thai, it is unambiguously a layout
    // slip, not a secret.
    if latin && !thai {
        if en.contains(word) {
            return None;
        }
        // Identifiable keys and addresses are hard-denied even when an
        // adversarial dictionary could make their layout conversion look valid.
        // HighEntropy/TooLong are ambiguous because real wrong-layout Thai often
        // contains digits and punctuation; those may proceed only to the strict
        // full-Thai validity check below.
        if matches!(
            secret::classify_token(word),
            Some(
                SecretKind::Hex
                    | SecretKind::Base58Wif
                    | SecretKind::Bech32
                    | SecretKind::ExtendedKey
            )
        ) {
            return None;
        }
        let converted = en_to_th(word);
        // Accept if the conversion is a single dictionary word OR a fully-segmented
        // Thai phrase (e.g. a long sentence typed without spaces on wrong layout).
        let evidence = if th.contains(&converted) {
            Some(Evidence::ExactDictionary)
        } else if segment::is_fully_known(&converted, th) {
            Some(Evidence::FullSegmentation)
        } else {
            None
        };
        if let Some(evidence) = evidence {
            return Some(Detection {
                corrected: converted,
                confidence: Confidence::High,
                evidence,
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

    #[test]
    fn english_word_with_trailing_punctuation_is_corrected() {
        let en = Dictionary::from_words(["hello"]);
        let th = Dictionary::from_words([] as [&str; 0]);
        let d = detect("้ำสสนม", &en, &th).unwrap();
        assert_eq!(d.corrected, "hello,");
    }

    #[test]
    fn identifiable_secrets_are_hard_denied_before_validity() {
        let raw = "bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq";
        let converted = en_to_th(raw);
        let en = Dictionary::from_words([] as [&str; 0]);
        let th = Dictionary::from_words([converted.as_str()]);
        assert!(detect(raw, &en, &th).is_none());
    }

    #[test]
    fn password_shaped_ascii_can_be_unambiguous_wrong_layout_thai() {
        let raw = "0ib'vp^jmuj;jklk,ki5cx]'d]y[wfh";
        assert_eq!(secret::classify_token(raw), Some(SecretKind::HighEntropy));
        let converted = en_to_th(raw);
        let en = Dictionary::from_words([] as [&str; 0]);
        let th = Dictionary::from_words([converted.as_str()]);
        assert_eq!(detect(raw, &en, &th).unwrap().corrected, converted);
    }
}
