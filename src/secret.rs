//! Secret-shaped **bail-out** — the core privacy defense.
//!
//! Before a token is corrected or learned, we classify its *shape* (never its
//! meaning). Identifiable keys and addresses are hard denies. High-entropy and
//! overlong tokens are ambiguous because wrong-layout Thai often has that shape;
//! detection may inspect their conversion but may accept only a complete known-Thai
//! segmentation, and learning must still reject them. A denied token is wiped and
//! left untouched.
//!
//! Heuristics are intentionally safety-first, but token-level BIP39 checks cannot
//! deny every individual dictionary word: ordinary English words can also belong
//! to BIP39. Phrase-level callers must track consecutive seed words.

use std::collections::HashSet;
use std::sync::OnceLock;

/// Why a token was judged secret-shaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    /// Long all-hex string containing digits — e.g. a raw private key.
    Hex,
    /// Base58 / WIF private key or address shape.
    Base58Wif,
    /// bech32 address (`bc1…` / `tb1…`).
    Bech32,
    /// Extended key (`xpub`/`xprv`/`ypub`/`zpub`/…).
    ExtendedKey,
    /// High-entropy or mixed-character token — e.g. a generated password.
    HighEntropy,
    /// Longer than any plausible real word.
    TooLong,
}

/// Longest plausible real Thai/English word; longer tokens are treated as secret-ish.
pub const MAX_WORD_LEN: usize = 24;
/// Consecutive BIP39 words that trigger a seed-phrase bail-out.
pub const SEED_MIN_RUN: usize = 4;

const HEX_MIN_LEN: usize = 12;
const BASE58_MIN_LEN: usize = 26;
const MIXED_MIN_LEN: usize = 8;
const ENTROPY_BITS_PER_CHAR: f64 = 3.5;

/// Base58 alphabet (Bitcoin): excludes `0`, `O`, `I`, `l`.
const BASE58_ALPHABET: &str = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";

const EXTKEY_PREFIXES: &[&str] = &[
    "xpub", "xprv", "ypub", "yprv", "zpub", "zprv", "tpub", "tprv", "vpub", "vprv",
];

fn bip39_set() -> &'static HashSet<&'static str> {
    static SET: OnceLock<HashSet<&'static str>> = OnceLock::new();
    SET.get_or_init(|| {
        include_str!("../assets/bip39.txt")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .collect()
    })
}

/// True if `word` is an exact BIP39 English wordlist entry (case-insensitive).
pub fn is_bip39_word(word: &str) -> bool {
    if !word.is_ascii() {
        return false;
    }
    bip39_set().contains(word.to_ascii_lowercase().as_str())
}

/// Classify a single token's shape. `None` means "looks like ordinary text."
pub fn classify_token(token: &str) -> Option<SecretKind> {
    let chars: Vec<char> = token.chars().collect();
    let len = chars.len();
    if len == 0 {
        return None;
    }

    // Non-ASCII (e.g. Thai) ordinary words are never secret-shaped by these rules.
    if !token.is_ascii() {
        return None;
    }

    let lower = token.to_ascii_lowercase();
    if lower.starts_with("bc1") || lower.starts_with("tb1") {
        return Some(SecretKind::Bech32);
    }
    if EXTKEY_PREFIXES.iter().any(|p| lower.starts_with(p)) {
        return Some(SecretKind::ExtendedKey);
    }

    let has_digit = chars.iter().any(char::is_ascii_digit);
    let all_hex = chars.iter().all(char::is_ascii_hexdigit);
    if all_hex && has_digit && len >= HEX_MIN_LEN {
        return Some(SecretKind::Hex);
    }

    let all_base58 = chars.iter().all(|c| BASE58_ALPHABET.contains(*c));
    // Explicit WIF: starts 5/K/L, 51–52 base58 chars.
    if all_base58 && (51..=52).contains(&len) && matches!(chars[0], '5' | 'K' | 'L') {
        return Some(SecretKind::Base58Wif);
    }
    // General long base58 token with class variety (address / key fragment).
    if all_base58 && len >= BASE58_MIN_LEN && class_count(&chars) >= 2 {
        return Some(SecretKind::Base58Wif);
    }

    // Generated-password shape: enough length + mixed character classes, or high entropy.
    if len >= MIXED_MIN_LEN
        && (class_count(&chars) >= 3 || shannon_bits_per_char(&chars) > ENTROPY_BITS_PER_CHAR)
    {
        return Some(SecretKind::HighEntropy);
    }

    // Last resort: longer than any plausible real word. Specific key/address
    // shapes above take precedence so their labels stay accurate.
    if len > MAX_WORD_LEN {
        return Some(SecretKind::TooLong);
    }

    None
}

/// Convenience: does this single token look secret-shaped?
pub fn is_secret_token(token: &str) -> bool {
    classify_token(token).is_some()
}

/// Tracks consecutive BIP39 words across a stream so a typed seed phrase can be
/// detected and avoided (≥ [`SEED_MIN_RUN`] in a row).
#[derive(Default, Debug)]
pub struct SeedTracker {
    run: usize,
}

impl SeedTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe the next completed word. Returns `true` once a seed-phrase run is
    /// detected, signalling the caller to bail and wipe recent context.
    pub fn observe(&mut self, word: &str) -> bool {
        self.observe_hit(is_bip39_word(word))
    }

    /// Observe a completed token by every reading it has. A token counts as a
    /// seed word when its wrong-layout candidate, its raw text, or — for text
    /// typed on the Thai layout — the English its *keys* spell is a BIP39 word.
    ///
    /// The key reading matters when detection declines: `cat` and `ski` typed
    /// on the Thai layout are the real Thai words `แฟะ` and `หาร`, so no
    /// candidate is offered, and without it the run used to reset in the
    /// middle of a phrase.
    pub fn observe_candidate(&mut self, raw: &str, candidate: Option<&str>) -> bool {
        let hit = candidate.is_some_and(is_bip39_word)
            || is_bip39_word(raw)
            || (!raw.is_ascii() && is_bip39_word(&crate::layout::th_to_en(raw)));
        self.observe_hit(hit)
    }

    fn observe_hit(&mut self, hit: bool) -> bool {
        self.run = if hit { self.run + 1 } else { 0 };
        self.run >= SEED_MIN_RUN
    }

    /// True while the next word could complete (or continue) a seed phrase:
    /// the run is one word short of the threshold or past it. Mid-word callers
    /// use this instead of [`observe`](Self::observe), which must see each
    /// completed token exactly once — a partial word is not a word, and
    /// observing prefixes would reset or inflate the run.
    pub fn guarding(&self) -> bool {
        self.run + 1 >= SEED_MIN_RUN
    }

    pub fn reset(&mut self) {
        self.run = 0;
    }
}

/// Number of distinct character classes present: lowercase, uppercase, digit, symbol.
fn class_count(chars: &[char]) -> usize {
    let mut lower = false;
    let mut upper = false;
    let mut digit = false;
    let mut symbol = false;
    for &c in chars {
        if c.is_ascii_lowercase() {
            lower = true;
        } else if c.is_ascii_uppercase() {
            upper = true;
        } else if c.is_ascii_digit() {
            digit = true;
        } else if c.is_ascii_graphic() {
            symbol = true;
        }
    }
    lower as usize + upper as usize + digit as usize + symbol as usize
}

/// Shannon entropy in bits per character of the token's character distribution.
fn shannon_bits_per_char(chars: &[char]) -> f64 {
    if chars.is_empty() {
        return 0.0;
    }
    let mut counts: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    for &c in chars {
        *counts.entry(c).or_insert(0) += 1;
    }
    let n = chars.len() as f64;
    -counts
        .values()
        .map(|&k| {
            let p = k as f64 / n;
            p * p.log2()
        })
        .sum::<f64>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_words_pass() {
        for w in [
            "hello", "correct", "twitter", "the", "Bitcoin", "decade", "deadbeef",
        ] {
            assert_eq!(classify_token(w), None, "{w} should be safe");
        }
        // Thai is never secret-shaped.
        assert_eq!(classify_token("สวัสดี"), None);
    }

    #[test]
    fn raw_hex_private_key() {
        let key = "e9873d79c6d87dc0fb6a5778633389f4453213303da61f20bd67fc233aa33262";
        assert_eq!(classify_token(key), Some(SecretKind::Hex));
    }

    #[test]
    fn wif_and_bech32_and_xpub() {
        assert_eq!(
            classify_token("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ"),
            Some(SecretKind::Base58Wif)
        );
        assert_eq!(
            classify_token("bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq"),
            Some(SecretKind::Bech32)
        );
        assert_eq!(
            classify_token("xpub661MyMwAqRbcFtXgS5sYJABqqG9YLmC4Q1Rdap9gSE8NqtwybGhePY2gZ29ESFjqJoCu1Rupje8YtGqsefD265TMg7usUDFdp6W1EGMcet"),
            Some(SecretKind::ExtendedKey)
        );
    }

    #[test]
    fn generated_password() {
        assert!(classify_token("Xq9$mZ2!").is_some());
        assert_eq!(classify_token("aaaaaaaa"), None); // low entropy, single class
    }

    #[test]
    fn overly_long_token() {
        let long = "a".repeat(MAX_WORD_LEN + 1);
        assert_eq!(classify_token(&long), Some(SecretKind::TooLong));
    }

    #[test]
    fn bip39_seed_run() {
        let mut t = SeedTracker::new();
        // A real BIP39 prefix run.
        let seed = ["abandon", "ability", "able", "about", "above"];
        let mut tripped = false;
        for w in seed {
            tripped |= t.observe(w);
        }
        assert!(tripped, "consecutive BIP39 words must trip the tracker");

        // Ordinary prose must not.
        let mut t2 = SeedTracker::new();
        let mut tripped2 = false;
        for w in ["the", "quick", "brown", "fox", "jumps"] {
            tripped2 |= t2.observe(w);
        }
        assert!(!tripped2);
    }

    #[test]
    fn single_bip39_word_is_fine_for_correction() {
        // One BIP39 word in isolation is just a normal English word; only a *run* bails.
        assert!(is_bip39_word("zoo"));
        assert_eq!(classify_token("zoo"), None);
    }

    #[test]
    fn wrong_layout_candidates_contribute_to_seed_run() {
        let mut tracker = SeedTracker::new();
        for (index, candidate) in ["abandon", "ability", "able", "about"].iter().enumerate() {
            let raw = crate::layout::en_to_th(candidate);
            assert_eq!(tracker.observe_candidate(&raw, Some(candidate)), index == 3);
        }
    }

    #[test]
    fn a_thai_layout_seed_word_without_a_candidate_still_counts() {
        // `cat` and `ski` typed on the Thai layout are real Thai words, so
        // detection offers no candidate; their keys still spell seed words.
        let mut tracker = SeedTracker::new();
        let phrase = ["abandon", "cat", "ski", "about"];
        for (index, word) in phrase.iter().enumerate() {
            let raw = crate::layout::en_to_th(word);
            let candidate = (*word != "cat" && *word != "ski").then_some(*word);
            assert_eq!(tracker.observe_candidate(&raw, candidate), index == 3);
        }
    }

    #[test]
    fn guarding_starts_one_word_before_the_threshold() {
        let mut tracker = SeedTracker::new();
        for word in ["abandon", "ability"] {
            tracker.observe(word);
            assert!(!tracker.guarding());
        }
        tracker.observe("able");
        assert!(tracker.guarding(), "a fourth seed word may be in progress");
        tracker.observe("keyboard");
        assert!(!tracker.guarding());
    }

    #[test]
    fn every_seed_word_counts_on_either_layout() {
        // Whatever detection decides, each BIP39 word must extend the run
        // whether it was typed on the US or the Thai layout.
        for word in bip39_set() {
            let mut us = SeedTracker::new();
            let mut th = SeedTracker::new();
            for _ in 0..SEED_MIN_RUN - 1 {
                us.observe("abandon");
                th.observe("abandon");
            }
            assert!(us.observe_candidate(word, None), "{word} on US");
            let raw = crate::layout::en_to_th(word);
            assert!(th.observe_candidate(&raw, None), "{word} on Thai");
        }
    }
}
