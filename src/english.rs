//! What counts as "English" beyond exact dictionary membership (OS-free).
//!
//! The bundled English list is conversational, so it misses the vocabulary
//! people actually type at work: `middleware`, `workflow`, `frontend`,
//! `codebase`. Those tokens are not words to [`Dictionary::contains`], and a
//! token that is not a word is exactly what the EN→TH path is allowed to
//! convert — so they were rewritten as Thai mid-word.
//!
//! Most of that vocabulary is a closed compound of ordinary words. This module
//! recognises such compounds, and the prefixes on their way to one, so the live
//! path can hold them the same way it holds `diffe` on its way to `different`.
//!
//! Compound parts are drawn only from the most frequent [`CORE_WORDS`] words of
//! the bundled list (plus anything the user has taught). The long tail of the
//! list is full of fragments (`dit`, `ouh`) that would let almost any Latin
//! string of wrong-layout Thai split into "English"; measured against every
//! pure-letter Thai dictionary word typed on the wrong layout, the full list
//! mistakes 28 of 6,574 for compounds while the top 30,000 mistakes 5, all rare
//! words that the Thai dictionary itself still claims first.

use std::sync::OnceLock;

use crate::dict::{self, Dictionary};

/// Frequency cut-off for compound parts. The bundled list is frequency-ordered.
pub const CORE_WORDS: usize = 30_000;
/// Shortest compound part. Two-letter parts make everything a compound.
const MIN_PART_CHARS: usize = 3;
/// Longest part considered, which also bounds the work per position.
const MAX_PART_CHARS: usize = 20;

/// The frequent head of the bundled English list, used for compound parts.
fn core() -> &'static Dictionary {
    static D: OnceLock<Dictionary> = OnceLock::new();
    D.get_or_init(|| {
        Dictionary::from_words(
            include_str!("../assets/en_words.txt")
                .lines()
                .take(CORE_WORDS),
        )
    })
}

/// Build the compound tables now rather than on the first keystroke that needs
/// them — that first build takes tens of milliseconds, which is too long to
/// spend inside a low-level keyboard hook.
pub fn warm() {
    let _ = core().has_extension("warm");
    let _ = dict::english().has_extension("warm");
    let _ = dict::thai().has_extension("warm");
}

fn is_part(s: &str) -> bool {
    core().contains(s) || dict::english().is_learned(s)
}

fn part_has_extension(s: &str) -> bool {
    core().has_extension(s)
}

/// Letters only, in one of the casings English words are actually typed in:
/// `word`, `Word`, `WORD`. Wrong-layout Thai produces arbitrary interior
/// capitals (`ditCud`) because Shift selects a different Thai letter.
fn compound_shape(token: &str) -> Option<String> {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let rest_lower = token.bytes().skip(1).all(|b| b.is_ascii_lowercase());
    let all_upper = token.bytes().all(|b| b.is_ascii_uppercase());
    (rest_lower || all_upper).then(|| token.to_ascii_lowercase())
}

/// Positions of `word` reachable by a whole number of compound parts.
fn reachable(word: &[u8]) -> Vec<bool> {
    let n = word.len();
    let mut reach = vec![false; n + 1];
    reach[0] = true;
    for start in 0..n {
        if !reach[start] {
            continue;
        }
        let end = (start + MAX_PART_CHARS).min(n);
        for next in (start + MIN_PART_CHARS)..=end {
            // ASCII-only by construction, so byte slicing is char slicing.
            if is_part(std::str::from_utf8(&word[start..next]).unwrap_or("")) {
                reach[next] = true;
            }
        }
    }
    reach
}

/// Is `token` two or more frequent English words run together
/// (`middleware`, `workflow`, `frontend`)?
pub fn is_compound(token: &str) -> bool {
    let Some(word) = compound_shape(token) else {
        return false;
    };
    let bytes = word.as_bytes();
    if bytes.len() < 2 * MIN_PART_CHARS {
        return false;
    }
    // A single part spanning the whole token is a plain word, not a compound;
    // require at least one interior split point that is itself reachable.
    let reach = reachable(bytes);
    reach[bytes.len()]
        && (MIN_PART_CHARS..=bytes.len() - MIN_PART_CHARS).any(|split| {
            reach[split] && reachable(&bytes[split..]).last().copied().unwrap_or(false)
        })
}

/// Is `token` English: a dictionary word (bundled or learned) or a compound?
pub fn is_word(token: &str, en: &Dictionary) -> bool {
    en.contains(token) || is_compound(token)
}

/// Can `token` still grow into English — a longer dictionary word, or a
/// compound whose last part is still being typed (`middlew` → `middleware`)?
pub fn has_continuation(token: &str, en: &Dictionary) -> bool {
    if en.has_extension(token) {
        return true;
    }
    let Some(word) = compound_shape(token) else {
        return false;
    };
    let bytes = word.as_bytes();
    let reach = reachable(bytes);
    // Some complete leading part(s), then a tail that begins a frequent word.
    (MIN_PART_CHARS..bytes.len()).any(|split| {
        reach[split] && {
            let tail = std::str::from_utf8(&bytes[split..]).unwrap_or("");
            part_has_extension(tail) || is_part(tail)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::th_to_en;

    #[test]
    fn everyday_technical_compounds_are_english() {
        for w in [
            "middleware",
            "workflow",
            "codebase",
            "frontend",
            "backend",
            "localhost",
            "hotkey",
            "Workflow",
            "FRONTEND",
        ] {
            assert!(is_compound(w), "{w} should be a compound");
        }
    }

    #[test]
    fn plain_words_and_non_words_are_not_compounds() {
        for w in [
            "hello", "the", "l;ylfu", "ditCud", "abc", "WorkFlow", "x1y2z3",
        ] {
            assert!(!is_compound(w), "{w} should not be a compound");
        }
    }

    #[test]
    fn a_compound_in_progress_is_a_live_continuation() {
        let en = dict::english();
        assert!(has_continuation("middlew", en));
        assert!(has_continuation("workfl", en));
        assert!(has_continuation("diffe", en));
        assert!(!has_continuation("l;yl", en));
    }

    #[test]
    fn wrong_layout_thai_sentences_are_not_english() {
        for phrase in [
            "สวัสดีครับ",
            "วันนี้วันจันทร์",
            "ผมชอบกินข้าวผัด",
            "ขอบคุณมากครับ",
            "เดี๋ยวโทรกลับนะ",
        ] {
            assert!(!is_word(&th_to_en(phrase), dict::english()), "{phrase}");
        }
    }
}
