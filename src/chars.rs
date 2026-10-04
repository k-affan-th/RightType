//! Every character worth finding (2.4 B1–B3): Unicode's names, CLDR's
//! keywords in Thai and English, and the characters that look like a few
//! typed ASCII letters (`x` → ×, `-` → − –). Built by `tools/gen_chars.py`
//! from Unicode's own files (Unicode License V3, see THIRD_PARTY.md) and
//! decoded the first time a search needs them.
//!
//! No OS calls; nothing typed is kept.

use std::sync::OnceLock;

use crate::find::{Query, EXACT};

static NAMES: &[u8] = include_bytes!("../assets/chars/names.bin");
static CLDR: &str = include_str!("../assets/chars/cldr.txt");
static LOOK: &str = include_str!("../assets/chars/look.txt");

/// One character (or emoji sequence) and how to find it.
#[derive(Debug)]
pub struct Entry {
    /// What is typed when it is picked.
    pub text: Box<str>,
    /// Unicode's name, lower case (empty for a sequence Unicode does not
    /// name, such as a flag).
    pub name: Box<str>,
    /// CLDR keywords, `|`-separated, the first being its short name.
    pub th: &'static str,
    pub en: &'static str,
}

impl Entry {
    /// The Thai short name, else the English one, else Unicode's name.
    pub fn label(&self, thai: bool) -> &str {
        let first = |s: &'static str| s.split('|').next().filter(|f| !f.is_empty());
        let preferred = if thai { first(self.th) } else { first(self.en) };
        preferred.or_else(|| first(self.en)).unwrap_or(&self.name)
    }

    /// `U+2192`, or several for a sequence.
    pub fn code(&self) -> String {
        self.text
            .chars()
            .map(|c| format!("U+{:04X}", c as u32))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn decode_names() -> Vec<(char, String)> {
    let at = std::cell::Cell::new(0usize);
    let take = |n: usize| {
        let s = &NAMES[at.get()..at.get() + n];
        at.set(at.get() + n);
        s
    };
    let u16le = |b: &[u8]| u16::from_le_bytes([b[0], b[1]]) as usize;
    let count = u16le(take(2));
    let mut words = Vec::with_capacity(count);
    for _ in 0..count {
        let len = take(1)[0] as usize;
        words.push(std::str::from_utf8(take(len)).unwrap_or(""));
    }
    let mut out = Vec::new();
    while at.get() < NAMES.len() {
        let b = take(3);
        let cp = u32::from_le_bytes([b[0], b[1], b[2], 0]);
        let n = take(1)[0] as usize;
        let name = (0..n)
            .map(|_| words.get(u16le(take(2))).copied().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" ");
        if let Some(c) = char::from_u32(cp) {
            out.push((c, name));
        }
    }
    out
}

/// Every entry, in code point order (built once).
pub fn all() -> &'static [Entry] {
    static ALL: OnceLock<Vec<Entry>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut cldr: std::collections::HashMap<&'static str, (&'static str, &'static str)> = CLDR
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\t');
                Some((p.next()?, (p.next().unwrap_or(""), p.next().unwrap_or(""))))
            })
            .collect();
        let mut all: Vec<Entry> = decode_names()
            .into_iter()
            .map(|(c, name)| {
                let mut buf = [0u8; 4];
                let key: &str = c.encode_utf8(&mut buf);
                let (th, en) = cldr.remove(key).unwrap_or(("", ""));
                Entry {
                    text: key.into(),
                    name: name.into_boxed_str(),
                    th,
                    en,
                }
            })
            .collect();
        // Sequences (flags, families, skin tones) Unicode does not name.
        let mut rest: Vec<_> = cldr.into_iter().collect();
        rest.sort_unstable_by_key(|(k, _)| *k);
        all.extend(rest.into_iter().map(|(text, (th, en))| Entry {
            text: text.into(),
            name: "".into(),
            th,
            en,
        }));
        all
    })
}

/// The characters that look like `typed` (short ASCII: `x`, `-`, `<<`).
pub fn look_alikes(typed: &str) -> Vec<char> {
    LOOK.lines()
        .find_map(|l| {
            let (k, v) = l.split_once('\t')?;
            (k == typed).then(|| v.chars().collect())
        })
        .unwrap_or_default()
}

/// The best `limit` entries for `query`, best first (ties: lower code
/// point first).
pub fn search(query: &Query, typed: &str, limit: usize) -> Vec<(u32, &'static Entry)> {
    if query.is_empty() {
        return Vec::new();
    }
    let looks = look_alikes(typed.trim());
    let score = |e: &'static Entry, typos: bool| -> Option<u32> {
        let looks_like = (!looks.is_empty())
            .then(|| {
                let mut c = e.text.chars();
                let first = c.next()?;
                (c.next().is_none() && looks.contains(&first)).then_some(EXACT)
            })
            .flatten();
        let keywords = |s: &'static str| {
            s.split('|')
                .filter(|k| !k.is_empty())
                .filter_map(move |k| query.score_with(k, typos))
        };
        let mut best = query.score_with(&e.name, typos);
        if !e.th.is_empty() || !e.en.is_empty() {
            best = best.max(keywords(e.th).chain(keywords(e.en)).max());
        }
        best.max(looks_like)
    };
    let mut hits: Vec<(u32, &'static Entry)> = all()
        .iter()
        .filter_map(|e| Some((score(e, false)?, e)))
        .collect();
    // One letter off, only when the exact search found too little.
    if hits.len() < limit {
        hits = all()
            .iter()
            .filter_map(|e| Some((score(e, true)?, e)))
            .collect();
    }
    hits.sort_by_key(|a| std::cmp::Reverse(a.0));
    hits.truncate(limit);
    hits
}

#[cfg(test)]
mod tests {
    use super::*;

    fn top(typed: &str, n: usize) -> Vec<String> {
        search(&Query::new(typed), typed, n)
            .into_iter()
            .map(|(_, e)| e.text.to_string())
            .collect()
    }

    #[test]
    fn the_data_is_whole() {
        let all = all();
        assert!(all.len() > 28_000, "{}", all.len());
        let arrow = all.iter().find(|e| &*e.text == "→").expect("→");
        assert_eq!(&*arrow.name, "rightwards arrow");
        assert!(arrow.th.contains("ลูกศร"));
        assert_eq!(arrow.code(), "U+2192");
        assert_eq!(arrow.label(true), "ลูกศรชี้ขวา");
    }

    #[test]
    fn finds_by_english_name_thai_keyword_and_look() {
        assert!(top("rightwards arrow", 3).contains(&"→".to_string()));
        assert!(top("ยูโร", 5).contains(&"€".to_string()));
        assert!(top("ลูกศร", 40).contains(&"→".to_string()));
        assert!(top("x", 10).contains(&"×".to_string()));
        assert!(top("degree", 5).contains(&"°".to_string()));
        // ลูกศร typed with the English keyboard on (Kedmanee keys).
        let keys = crate::layout::convert(
            "ลูกศร",
            crate::layout::thai_table(crate::layout::ThaiVariant::Kedmanee),
            crate::layout::us_table(),
        );
        assert!(top(&keys, 40).contains(&"→".to_string()), "{keys}");
    }

    #[test]
    fn a_search_is_quick() {
        all();
        let t = std::time::Instant::now();
        let q = Query::new("arrow");
        let _ = search(&q, "arrow", 8);
        // Generous for a debug build on a slow machine; the release target
        // (5 ms) is checked by examples/startup_cost.
        assert!(t.elapsed().as_millis() < 500, "{:?}", t.elapsed());
    }
}
