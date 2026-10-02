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

use std::collections::HashSet;
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

/// One of the frequent English words (or a learned one) — lower case.
pub fn is_common(word: &str) -> bool {
    is_part(word)
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

/// A bundled technical term in exactly its usual casing (`PyThaiNLP`,
/// `RoBERTa`, `LoRA`) — see `assets/tech_terms.txt`. Exact casing is what
/// makes a mixed-case reading safe to trust: Thai typed on the Thai layout
/// produces arbitrary capitals, but not these particular ones.
pub fn is_tech_term(token: &str) -> bool {
    static TERMS: OnceLock<HashSet<&'static str>> = OnceLock::new();
    TERMS
        .get_or_init(|| dict::tech_terms().collect())
        .contains(token)
}

/// A number as written in running text: `40`, `12,480`, `0.912`, `64%`,
/// `2e-5`, `3.` (a numbered heading), `2567`.
pub fn is_number(token: &str) -> bool {
    let t = token.strip_suffix('%').unwrap_or(token);
    let t = t.strip_suffix('.').unwrap_or(t);
    if t.is_empty() || !t.starts_with(|c: char| c.is_ascii_digit()) {
        return false;
    }
    // Scientific notation: mantissa `e` optional sign, exponent.
    if let Some((mantissa, exponent)) = t.split_once(['e', 'E']) {
        let exponent = exponent.strip_prefix(['-', '+']).unwrap_or(exponent);
        return plain_number(mantissa)
            && !exponent.is_empty()
            && exponent.bytes().all(|b| b.is_ascii_digit());
    }
    plain_number(t)
}

/// Digits, with single `,` or `.` separators between digit groups.
fn plain_number(t: &str) -> bool {
    !t.is_empty()
        && t.split([',', '.'])
            .all(|group| !group.is_empty() && group.bytes().all(|b| b.is_ascii_digit()))
}

/// An acronym or model name: `GPU`, `NLP`, `TF`, `A100` — capitals and
/// digits, starting with a capital, at most 8 long, with two capitals or one
/// capital and at least three digits (`T09` is too easily a Thai slip).
pub fn is_acronym(token: &str) -> bool {
    let len = token.len();
    let upper = token.bytes().filter(u8::is_ascii_uppercase).count();
    let digits = token.bytes().filter(u8::is_ascii_digit).count();
    (2..=8).contains(&len)
        && token.starts_with(|c: char| c.is_ascii_uppercase())
        && upper + digits == len
        && (upper >= 2 || digits >= 3)
}

/// Two-letter words that may stand as a part of a hyphenated term
/// (`bag-of-words`, `up-to-date`, `so-so`).
const SHORT_PARTS: &[&str] = &[
    "of", "to", "in", "on", "by", "up", "so", "go", "no", "do", "at", "as", "is", "it", "be", "or",
    "an", "we", "me", "my", "re", "co", "ex",
];

/// Derivational endings, longest first.
const SUFFIXES: &[&str] = &[
    "izations", "ization", "isations", "isation", "ations", "ation", "izing", "ized", "izer",
    "ability", "ments", "ment", "ness", "able", "ings", "ing", "ers", "er", "ed", "ly",
];

/// A dictionary word plus a derivational ending: `tokenization`,
/// `embeddings`, `retrained`. Lower case (or capitalised), at least 7 letters,
/// stem of at least 4 letters that is itself English.
pub fn is_derived(token: &str, en: &Dictionary) -> bool {
    let Some(word) = compound_shape(token) else {
        return false;
    };
    if word.len() < 7 {
        return false;
    }
    SUFFIXES.iter().any(|suffix| {
        word.strip_suffix(suffix).is_some_and(|stem| {
            stem.len() >= 4
                && (en.contains(stem) || en.contains(&format!("{stem}e")) || is_compound(stem))
        })
    })
}

/// Prefixes that make new English words from old ones (`re` + `login`).
const PREFIXES: &[&str] = &[
    "re", "un", "pre", "de", "dis", "mis", "non", "sub", "over", "under", "out", "co", "in",
    "inter", "auto", "multi",
];

/// Prefixes that take a hyphen before a stem starting with the vowel they
/// end with (`re-enable`, `co-owner`, `anti-inflammatory`).
const VOWEL_PREFIXES: &[&str] = &["re", "pre", "de", "co", "anti", "semi", "multi"];

/// Closed compounds of a verb and a particle (`log in` → `login`): with a
/// prefix they are written with a hyphen (`re-login`, `pre-signup`).
const PARTICLE_STEMS: &[&str] = &["login", "logon", "signin", "signup"];

/// `word` as it is written with its prefix hyphenated, when style calls for
/// it: a prefix before the same vowel (`reenable` → `re-enable`), unless the
/// joined spelling is a dictionary word (`reenter`, `cooperate`), and `re` or
/// `pre` before a verb–particle compound (`relogin` → `re-login`). Lower
/// case words only; anything else is left alone.
pub fn hyphenated(word: &str, en: &Dictionary) -> Option<String> {
    if word.len() < 5 || !word.bytes().all(|b| b.is_ascii_lowercase()) {
        return None;
    }
    for prefix in ["re", "pre"] {
        if let Some(stem) = word.strip_prefix(prefix) {
            if PARTICLE_STEMS.contains(&stem) {
                return Some(format!("{prefix}-{stem}"));
            }
        }
    }
    if en.contains(word) {
        return None;
    }
    VOWEL_PREFIXES.iter().find_map(|prefix| {
        let stem = word.strip_prefix(prefix)?;
        let last = prefix.chars().last()?;
        let first = stem.chars().next()?;
        (first == last && "aeiou".contains(first) && stem.len() >= 3 && en.contains(stem))
            .then(|| format!("{prefix}-{stem}"))
    })
}

/// Endings for [`is_affixed`], longest first.
const AFFIX_SUFFIXES: &[&str] = &[
    "ization", "ations", "ation", "ments", "ment", "ness", "less", "able", "ings", "ing", "ized",
    "ize", "ful", "ers", "er", "es", "ed", "ly", "s",
];

/// A dictionary word (at least 3 letters) with a common prefix, suffix or
/// both: `rerise`, `multiholes`, `resit`, `unfollowers`. Such words are
/// missing from any word list but plainly English; typed on the English
/// layout they are not converted to Thai (see `policy::detect_token`).
/// Lower case or capitalised letters only.
pub fn is_affixed(token: &str, en: &Dictionary) -> bool {
    let Some(word) = compound_shape(token) else {
        return false;
    };
    let root = |r: &str| r.len() >= 3 && (en.contains(r) || en.contains(&format!("{r}e")));
    let unsuffixed = |w: &str| {
        AFFIX_SUFFIXES
            .iter()
            .any(|s| w.strip_suffix(s).is_some_and(root))
    };
    PREFIXES.iter().any(|p| {
        word.strip_prefix(p)
            .is_some_and(|rest| root(rest) || unsuffixed(rest))
    }) || unsuffixed(&word)
}

/// English as it appears in technical and academic writing, beyond single
/// dictionary words: numbers, acronyms, derived words and hyphenated terms
/// whose every part is one of those (`fine-tuning`, `F1-score`, `TF-IDF`,
/// `retrieval-augmented`).
///
/// Mixed-case product names (`PyThaiNLP`, `RoBERTa`) are deliberately not
/// recognised by shape: Thai typed on the Thai layout uses Shift for some
/// letters, so its key readings are full of interior capitals (`doyIRd`). They
/// become English the usual way — flip one back and it is learned.
///
/// Used only for text typed on the *Thai* layout, where a positive answer
/// means "these keys were meant as English" — see `policy::detect_token`.
pub fn is_technical(token: &str, en: &Dictionary) -> bool {
    // Parts of a hyphenated term must be solid on their own: a frequent
    // word (not merely something in the long dictionary tail, which holds
    // `d`, `pk` and `fd`), a short function word, an acronym such as `F1`, a
    // number, or a derived word.
    let part_ok = |p: &str| {
        if is_tech_term(p) {
            return true;
        }
        let lower = p.to_ascii_lowercase();
        (p.len() >= MIN_PART_CHARS
            && compound_shape(p).is_some()
            && (is_part(&lower) || is_compound(p)))
            || SHORT_PARTS.contains(&lower.as_str()) && compound_shape(p).is_some()
            || is_acronym(p)
            || (p.len() == 2
                && p.as_bytes()[0].is_ascii_uppercase()
                && p.as_bytes()[1].is_ascii_digit())
            || is_number(p)
            || is_derived(p, en)
    };
    if is_number(token) {
        return true;
    }
    if token.contains('-') && !token.starts_with('-') && !token.ends_with('-') {
        let parts: Vec<&str> = token.split('-').collect();
        return parts.len() >= 2
            && parts.iter().all(|p| !p.is_empty() && part_ok(p))
            // At least one part must carry letters: `3-4` is a range, and a
            // range is `is_number`'s business, not a hyphenated term.
            && parts.iter().any(|p| p.bytes().filter(u8::is_ascii_alphabetic).count() >= 2);
    }
    is_number(token) || is_acronym(token) || is_derived(token, en) || is_tech_term(token)
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
mod hyphen_tests {
    use super::*;
    use crate::dict;

    #[test]
    fn prefixes_take_their_hyphen_where_style_says() {
        let en = dict::english();
        assert_eq!(hyphenated("relogin", en).as_deref(), Some("re-login"));
        assert_eq!(hyphenated("presignup", en).as_deref(), Some("pre-signup"));
        assert_eq!(hyphenated("reenable", en).as_deref(), Some("re-enable"));
        // Written closed in the dictionary, or not a prefix case at all.
        for word in [
            "reenter",
            "cooperate",
            "reinstall",
            "reload",
            "react",
            "relaunch",
            "login",
            "Relogin",
            "re-login",
            "rest",
        ] {
            assert_eq!(hyphenated(word, en), None, "{word}");
        }
    }
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
    fn technical_english_is_recognised() {
        let en = dict::english();
        for w in [
            "40",
            "12,480",
            "0.912",
            "64%",
            "2e-5",
            "3.",
            "2567",
            "GPU",
            "NVIDIA",
            "A100",
            "tokenization",
            "fine-tuning",
            "code-switching",
            "F1-score",
            "TF-IDF",
            "bag-of-words",
            "retrieval-augmented",
            "parameter-efficient",
            "so-so",
        ] {
            assert!(is_technical(w, en), "{w} should be technical English");
        }
    }

    #[test]
    fn technical_shapes_reject_near_misses() {
        let en = dict::english();
        for w in [
            "",
            "-",
            "3-4",
            ",5",
            "1,,2",
            "e5",
            "Gp",
            "abcDef1",
            "ditCud",
            "doyIRd",
            "PyThaiNlp",
            "WORKFLOWING1",
            "x-",
            "-x",
            "zzzzqqqqing",
            "l;ylfu",
        ] {
            assert!(!is_technical(w, en), "{w} should not be technical English");
        }
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
