//! The characters each key can become (2.4): a short set per key — `2` is
//! ² ₂ ⅔ — that the typist can change, pinning more from the list at the
//! cursor. Tapping Right Alt after a character steps it through its set in
//! place; a held key (when that is on) offers the same set.
//!
//! Kept in `config.toml` only where the typist changed a key's set: the
//! characters chosen, never anything typed. No OS calls.

use std::collections::BTreeMap;

/// The sets RightType starts with: three or four each, the most used first.
const DEFAULTS: &[(char, &str)] = &[
    ('0', "⁰ ₀ °"),
    ('1', "¹ ₁ ½"),
    ('2', "² ₂ ⅔"),
    ('3', "³ ₃ ¾"),
    ('4', "⁴ ₄ ¼"),
    ('5', "⁵ ₅"),
    ('6', "⁶ ₆"),
    ('7', "⁷ ₇"),
    ('8', "⁸ ₈ ∞"),
    ('9', "⁹ ₉"),
    ('+', "± ⁺ ₊"),
    ('-', "– — − ⁻"),
    ('=', "≠ ≈ ≤ ≥"),
    ('<', "≤ ← «"),
    ('>', "≥ → »"),
    ('(', "⁽ ₍"),
    (')', "⁾ ₎"),
    ('*', "× • ★"),
    ('/', "÷ ⁄"),
    ('.', "… • °"),
    ('"', "“ ” « »"),
    ('\'', "‘ ’"),
    ('$', "฿ € £ ¥"),
    ('!', "¡"),
    ('?', "¿"),
    ('%', "‰"),
    ('~', "≈"),
    ('a', "á à â ä"),
    ('c', "ç ©"),
    ('e', "é è ê ë"),
    ('i', "í ì î ï"),
    ('m', "µ"),
    ('n', "ñ ⁿ"),
    ('o', "ó ò ô ö"),
    ('p', "π"),
    ('r', "®"),
    ('s', "ß §"),
    ('t', "™"),
    ('u', "ú ù û ü"),
    ('x', "× ˣ"),
    ('y', "ý"),
    ('ๆ', "ฯ ฯลฯ"),
];

/// Every key's set: the defaults, with the typist's own in their place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Sets {
    /// Keys the typist changed, with their whole set (empty: none).
    own: BTreeMap<char, Vec<String>>,
}

fn split(s: &str) -> Vec<String> {
    s.split_whitespace().map(str::to_string).collect()
}

impl Sets {
    /// From `config.toml`: key → its characters, space-separated.
    pub fn from_config(own: &BTreeMap<String, String>) -> Self {
        Self {
            own: own
                .iter()
                .filter_map(|(k, v)| {
                    let mut chars = k.chars();
                    match (chars.next(), chars.next()) {
                        (Some(c), None) => Some((c, split(v))),
                        _ => None,
                    }
                })
                .collect(),
        }
    }

    /// For `config.toml`: only the keys the typist changed.
    pub fn to_config(&self) -> BTreeMap<String, String> {
        self.own
            .iter()
            .map(|(k, v)| (k.to_string(), v.join(" ")))
            .collect()
    }

    fn default_of(key: char) -> Vec<String> {
        DEFAULTS
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, s)| split(s))
            .unwrap_or_default()
    }

    /// What `typed` can become, in order. A capital uses its letter's set
    /// in capitals (where a capital of the same length exists), unless the
    /// typist set the capital itself.
    pub fn of(&self, typed: char) -> Vec<String> {
        if let Some(own) = self.own.get(&typed) {
            return own.clone();
        }
        let lower = typed.to_lowercase().next().unwrap_or(typed);
        let base = self
            .own
            .get(&lower)
            .cloned()
            .unwrap_or_else(|| Self::default_of(lower));
        if lower == typed {
            return base;
        }
        base.into_iter()
            .filter_map(|s| {
                let up = s.to_uppercase();
                (up.chars().count() == s.chars().count()).then_some(up)
            })
            .collect()
    }

    /// Pin `ch` to `key` (at the end of its set), or take it off if it is
    /// there. True when it is in the set afterwards.
    pub fn toggle(&mut self, key: char, ch: &str) -> bool {
        let mut set = self.of(key);
        let on = match set.iter().position(|s| s == ch) {
            Some(i) => {
                set.remove(i);
                false
            }
            None => {
                set.push(ch.to_string());
                true
            }
        };
        if set == Self::default_of(key) {
            self.own.remove(&key);
        } else {
            self.own.insert(key, set);
        }
        on
    }

    /// Every character in any set, for the list at the cursor's first view.
    pub fn all_pinned(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let keys: Vec<char> = DEFAULTS
            .iter()
            .map(|(k, _)| *k)
            .chain(self.own.keys().copied())
            .collect();
        for k in keys {
            for c in self.of(k) {
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        out
    }
}

/// The strip shown while stepping through a set: every choice, the one
/// in place marked. `at` 0 is the character as typed.
pub fn strip(typed: &str, set: &[String], at: usize) -> String {
    std::iter::once(typed)
        .chain(set.iter().map(String::as_str))
        .enumerate()
        .map(|(i, c)| {
            if i == at {
                format!("[{c}]")
            } else {
                c.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_capitals() {
        let s = Sets::default();
        assert_eq!(s.of('2'), ["²", "₂", "⅔"]);
        assert_eq!(s.of('e')[0], "é");
        assert_eq!(s.of('E')[0], "É");
        assert!(s.of('S').iter().all(|c| c.chars().count() == 1), "no SS");
        assert!(s.of('k').is_empty());
        for (_, set) in DEFAULTS {
            assert!(split(set).len() <= 4, "{set}");
        }
    }

    #[test]
    fn pinning_and_the_config() {
        let mut s = Sets::default();
        assert!(s.toggle('2', "²⁺"));
        assert_eq!(s.of('2').last().unwrap(), "²⁺");
        assert!(s.toggle('k', "κ"));
        assert_eq!(s.of('k'), ["κ"]);
        let saved = s.to_config();
        assert_eq!(saved.get("k").unwrap(), "κ");
        assert_eq!(Sets::from_config(&saved), s);
        // Back to the defaults: nothing kept for that key.
        assert!(!s.toggle('2', "²⁺"));
        assert!(!s.to_config().contains_key("2"));
        assert!(s.all_pinned().contains(&"κ".to_string()));
    }

    #[test]
    fn the_strip_marks_the_one_in_place() {
        let set = Sets::default().of('2');
        assert_eq!(strip("2", &set, 0), "[2]  ²  ₂  ⅔");
        assert_eq!(strip("2", &set, 2), "2  ²  [₂]  ⅔");
    }
}
