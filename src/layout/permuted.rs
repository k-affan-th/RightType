//! Shared engine for English layouts that only rearrange the US keys
//! (Dvorak, Colemak): an unshifted key sequence and where each key's
//! character went. Shifted characters follow their unshifted ones.

/// US shift pairs `(unshifted, shifted)` for the punctuation keys that move
/// with the letters; letters shift to uppercase.
const SHIFT: &[(char, char)] = &[
    ('-', '_'),
    ('=', '+'),
    ('[', '{'),
    (']', '}'),
    (';', ':'),
    ('\'', '"'),
    (',', '<'),
    ('.', '>'),
    ('/', '?'),
];

fn unshift(c: char) -> (char, bool) {
    if c.is_ascii_uppercase() {
        return (c.to_ascii_lowercase(), true);
    }
    match SHIFT.iter().find(|(_, s)| *s == c) {
        Some(&(u, _)) => (u, true),
        None => (c, false),
    }
}

fn shift(c: char) -> char {
    if c.is_ascii_lowercase() {
        return c.to_ascii_uppercase();
    }
    SHIFT.iter().find(|(u, _)| *u == c).map_or(c, |&(_, s)| s)
}

/// `qwerty[i]` is the US key whose character this layout moved to `other[i]`.
pub(super) struct Permuted {
    pub qwerty: &'static str,
    pub other: &'static str,
}

impl Permuted {
    pub(super) fn char_of(&self, key: char) -> Option<char> {
        if !key.is_ascii_graphic() {
            return None;
        }
        let (base, shifted) = unshift(key);
        let out = match self.qwerty.chars().position(|c| c == base) {
            Some(i) => self.other.chars().nth(i)?,
            None => base,
        };
        Some(if shifted { shift(out) } else { out })
    }

    pub(super) fn key_of(&self, ch: char) -> Option<char> {
        if !ch.is_ascii_graphic() {
            return None;
        }
        let (base, shifted) = unshift(ch);
        let key = match self.other.chars().position(|c| c == base) {
            Some(i) => self.qwerty.chars().nth(i)?,
            None => base,
        };
        Some(if shifted { shift(key) } else { key })
    }
}
