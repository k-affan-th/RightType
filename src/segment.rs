//! Thai word segmentation by greedy longest-match ("maximal matching").
//!
//! Thai writes words with no spaces between them, so to auto-correct wrong-layout
//! Thai we have to find word boundaries ourselves. At each position we greedily
//! take the longest dictionary word; runs that match nothing become a single
//! "unknown" segment. This is the classic, cheap approach — not perfect on
//! ambiguous text, but enough to recognise that a converted run *is* real Thai and
//! where its words end, which is what Auto mode needs to decide when to convert.

use crate::dict::Dictionary;

/// Longest dictionary word we attempt to match — bounds the lookahead per position.
const MAX_WORD_CHARS: usize = 20;
/// Shortest run we treat as a real word; single characters are too ambiguous to be
/// a confident boundary, so they fall through to "unknown".
const MIN_WORD_CHARS: usize = 2;

/// One piece of a segmentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    /// Did this run match a dictionary word? (`false` = leftover/unknown text.)
    pub known: bool,
}

/// Split `text` into segments by greedy longest dictionary match. Consecutive
/// unmatched characters are merged into one `known: false` segment.
pub fn segment(text: &str, dict: &Dictionary) -> Vec<Segment> {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<Segment> = Vec::new();
    let mut i = 0;

    while i < chars.len() {
        let max_j = (i + MAX_WORD_CHARS).min(chars.len());
        // Longest match first, down to MIN_WORD_CHARS.
        let mut hit = None;
        let mut j = max_j;
        while j >= i + MIN_WORD_CHARS {
            let candidate: String = chars[i..j].iter().collect();
            if dict.contains(&candidate) {
                hit = Some((candidate, j));
                break;
            }
            j -= 1;
        }

        match hit {
            Some((word, end)) => {
                out.push(Segment {
                    text: word,
                    known: true,
                });
                i = end;
            }
            None => {
                // Unknown character: extend the previous unknown run, or start one.
                match out.last_mut() {
                    Some(seg) if !seg.known => seg.text.push(chars[i]),
                    _ => out.push(Segment {
                        text: chars[i].to_string(),
                        known: false,
                    }),
                }
                i += 1;
            }
        }
    }

    out
}

/// Does `text` partition entirely into known dictionary words? A strong signal the
/// run is genuine Thai (and thus that a wrong-layout conversion was correct).
///
/// This deliberately uses dynamic programming rather than [`segment`]'s greedy
/// display-oriented split.  Greedy longest-match can take a valid longer prefix
/// that leaves an unknown suffix even though a shorter prefix would partition the
/// complete text.
pub fn is_fully_known(text: &str, dict: &Dictionary) -> bool {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return false;
    }

    let mut reachable = vec![false; chars.len() + 1];
    reachable[0] = true;
    for start in 0..chars.len() {
        if !reachable[start] {
            continue;
        }
        let end = (start + MAX_WORD_CHARS).min(chars.len());
        for next in (start + MIN_WORD_CHARS)..=end {
            let candidate: String = chars[start..next].iter().collect();
            if dict.contains(&candidate) {
                reachable[next] = true;
            }
        }
    }
    reachable[chars.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dict() -> Dictionary {
        Dictionary::from_words(["สวัสดี", "ครับ", "วัน", "นี้", "จันทร์"])
    }

    fn known(s: &str) -> Segment {
        Segment {
            text: s.to_string(),
            known: true,
        }
    }

    #[test]
    fn splits_space_less_thai_into_words() {
        let segs = segment("สวัสดีครับ", &dict());
        assert_eq!(segs, vec![known("สวัสดี"), known("ครับ")]);
    }

    #[test]
    fn greedy_prefers_the_longest_word() {
        // "วันนี้" should split วัน + นี้, not stop short.
        let segs = segment("วันนี้", &dict());
        assert_eq!(segs, vec![known("วัน"), known("นี้")]);
    }

    #[test]
    fn unknown_runs_are_grouped() {
        let segs = segment("วันxyz", &dict());
        assert_eq!(
            segs,
            vec![
                known("วัน"),
                Segment {
                    text: "xyz".into(),
                    known: false
                }
            ]
        );
    }

    #[test]
    fn fully_known_detects_real_thai() {
        let d = dict();
        assert!(is_fully_known("สวัสดีครับ", &d));
        assert!(is_fully_known("วันนี้วันจันทร์", &d));
        assert!(!is_fully_known("สวัสดีqq", &d));
        assert!(!is_fully_known("", &d));
    }

    #[test]
    fn fully_known_backtracks_when_greedy_split_dead_ends() {
        let d = Dictionary::from_words(["มี", "มีเทน", "เทนนิส"]);
        // `segment` greedily takes "มีเทน" and leaves "นิส" unknown, but
        // "มี" + "เทนนิส" is a complete valid split.
        assert!(!segment("มีเทนนิส", &d).iter().all(|s| s.known));
        assert!(is_fully_known("มีเทนนิส", &d));
    }

    #[test]
    fn works_against_the_bundled_dictionary() {
        let d = crate::dict::thai();
        assert!(is_fully_known("สวัสดีครับ", d));
        // "วันนี้วันจันทร์" — the user's real test phrase — is all real words.
        assert!(is_fully_known("วันนี้วันจันทร์", d));
    }
}
