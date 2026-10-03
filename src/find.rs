//! One search for every list (2.4 F1–F2): the command palette, an app's
//! shortcut list and the list at the text cursor all rank the same way.
//!
//! A query is tried as typed and as typed on the other keyboard — on each
//! Thai keyboard RightType knows (Kedmanee, Pattachote, Manoonchai) — so
//! `ฟพนั` finds "alpha" and `lukl;` finds "ลูกศร" whichever keyboard was on.
//! Matches rank: the whole text, then its start, then the start of one of
//! its words, then anywhere in it, then a word one letter off. A match of
//! the query as typed outranks the same match of a conversion.
//!
//! No OS calls; nothing is kept.

use crate::layout::{self, ThaiVariant};

/// How a text matched (higher is better).
pub const EXACT: u32 = 1000;
pub const START: u32 = 900;
pub const WORD_START: u32 = 700;
pub const INSIDE: u32 = 500;
pub const ONE_OFF: u32 = 300;
/// Taken off a match of the query typed on the other keyboard.
const CONVERTED: u32 = 50;

/// A search, ready to score texts against.
#[derive(Debug, Clone)]
pub struct Query {
    /// As typed (lower case), then the other-keyboard readings.
    variants: Vec<String>,
}

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

impl Query {
    pub fn new(typed: &str) -> Query {
        let typed = typed.trim();
        let mut variants = vec![typed.to_lowercase()];
        if !typed.is_empty() {
            let thai = typed.chars().any(is_thai);
            for v in [
                ThaiVariant::Kedmanee,
                ThaiVariant::Pattachote,
                ThaiVariant::Manoonchai,
            ] {
                let table = layout::thai_table(v);
                // Converted from what was typed (Shift matters: `F` and `f`
                // are different Thai letters), then lower-cased.
                let other: String = typed
                    .chars()
                    .map(|c| {
                        if thai {
                            table.key_of(c).unwrap_or(c)
                        } else {
                            table.char_of(c).unwrap_or(c)
                        }
                    })
                    .collect::<String>()
                    .to_lowercase();
                if !variants.contains(&other) {
                    variants.push(other);
                }
            }
        }
        variants.retain(|v| !v.is_empty());
        Query { variants }
    }

    pub fn is_empty(&self) -> bool {
        self.variants.is_empty()
    }

    /// How well `text` matches, or `None`.
    pub fn score(&self, text: &str) -> Option<u32> {
        if self.variants.is_empty() {
            return None;
        }
        let hay = text.to_lowercase();
        self.variants
            .iter()
            .enumerate()
            .filter_map(|(i, q)| {
                let s = score_one(q, &hay)?;
                Some(if i == 0 {
                    s
                } else {
                    s.saturating_sub(CONVERTED)
                })
            })
            .max()
    }

    /// The best match over several texts of one item (a name and its
    /// keywords, say).
    pub fn score_any<'a>(&self, texts: impl IntoIterator<Item = &'a str>) -> Option<u32> {
        texts.into_iter().filter_map(|t| self.score(t)).max()
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || is_thai(c)
}

/// The words of `hay`, by where each starts.
fn words(hay: &str) -> impl Iterator<Item = &str> {
    hay.split(|c: char| !is_word_char(c))
        .filter(|w| !w.is_empty())
}

fn score_one(q: &str, hay: &str) -> Option<u32> {
    if hay == q {
        return Some(EXACT);
    }
    if hay.starts_with(q) {
        // A shorter text is the closer match.
        let extra = (hay.chars().count() - q.chars().count()).min(99) as u32;
        return Some(START - extra);
    }
    if words(hay).any(|w| w.starts_with(q)) {
        return Some(WORD_START);
    }
    if hay.contains(q) {
        return Some(INSIDE);
    }
    if q.chars().count() >= 4 && words(hay).any(|w| one_off(q, w)) {
        return Some(ONE_OFF);
    }
    None
}

/// `a` and `b` differ by one letter: one changed, added, dropped, or two
/// side by side swapped (`replcae`).
fn one_off(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (la, lb) = (a.len(), b.len());
    if la.abs_diff(lb) > 1 {
        return false;
    }
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    if head == la && head == lb {
        return false; // the same
    }
    let tail = |x: &[char], y: &[char]| {
        x.iter()
            .rev()
            .zip(y.iter().rev())
            .take_while(|(p, q)| p == q)
            .count()
    };
    if la == lb {
        let t = tail(&a[head..], &b[head..]);
        // One changed, or two swapped.
        head + t + 1 == la
            || (head + 2 + t == la && a[head] == b[head + 1] && a[head + 1] == b[head])
    } else {
        let (long, short) = if la > lb { (&a, &b) } else { (&b, &a) };
        head + tail(&long[head..], &short[head..]) + 1 >= long.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranks_whole_start_word_inside_one_off() {
        let q = Query::new("Replace");
        assert_eq!(q.score("replace"), Some(EXACT));
        assert!(q.score("Replace all").unwrap() >= START - 10);
        assert_eq!(q.score("Find and replace"), Some(WORD_START));
        assert_eq!(q.score("autoreplaced"), Some(INSIDE));
        assert_eq!(Query::new("replcae").score("Replace"), Some(ONE_OFF));
        assert_eq!(Query::new("replac").score("open file"), None);
        // A shorter text ranks first among starts.
        assert!(q.score("replace all").unwrap() > q.score("replace all of them").unwrap());
    }

    #[test]
    fn typed_on_the_wrong_keyboard_still_finds() {
        // "alpha" typed with the Kedmanee keyboard on.
        let q = Query::new(&layout::convert(
            "alpha",
            layout::us_table(),
            layout::thai_table(ThaiVariant::Kedmanee),
        ));
        assert_eq!(q.score("alpha").map(|s| s >= EXACT - CONVERTED), Some(true));
        // "ลูกศร" typed with the English keyboard on, for each Thai keyboard.
        for v in [
            ThaiVariant::Kedmanee,
            ThaiVariant::Pattachote,
            ThaiVariant::Manoonchai,
        ] {
            let keys = layout::convert("ลูกศร", layout::thai_table(v), layout::us_table());
            let q = Query::new(&keys);
            assert!(q.score("ลูกศรชี้ขวา").is_some(), "{v:?}: {keys}");
        }
    }

    #[test]
    fn as_typed_outranks_a_conversion() {
        // `l;ylfu` is not English, but as Thai it is สวัสดี.
        let q = Query::new("l;ylfu");
        assert_eq!(q.score("สวัสดี"), Some(EXACT - CONVERTED));
        assert_eq!(Query::new("สวัสดี").score("สวัสดี"), Some(EXACT));
    }

    #[test]
    fn one_letter_off() {
        assert!(one_off("arrow", "arow"));
        assert!(one_off("arrow", "arrows"));
        assert!(one_off("arrow", "arriw"));
        assert!(one_off("arrow", "arorw"));
        assert!(!one_off("arrow", "arrow"));
        assert!(!one_off("arrow", "aroww2"));
        assert!(!one_off("arrow", "error"));
    }

    #[test]
    fn empty_finds_nothing() {
        assert!(Query::new("  ").is_empty());
        assert_eq!(Query::new("").score("anything"), None);
    }
}
