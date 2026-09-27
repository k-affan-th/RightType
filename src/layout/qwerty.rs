//! US English QWERTY layout — the canonical reference layout.
//!
//! Because we identify physical keys *by* the US QWERTY character they produce,
//! this layout is the identity map for every printable ASCII character.

use super::{Layout, LayoutId};

/// US English QWERTY.
pub struct QwertyEn;

impl QwertyEn {
    pub fn new() -> Self {
        QwertyEn
    }
}

impl Default for QwertyEn {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for QwertyEn {
    fn id(&self) -> LayoutId {
        LayoutId::QwertyEn
    }

    fn key_of(&self, ch: char) -> Option<char> {
        // Any printable ASCII character *is* its own canonical key.
        if ch.is_ascii_graphic() {
            Some(ch)
        } else {
            None
        }
    }

    fn char_of(&self, key: char) -> Option<char> {
        if key.is_ascii_graphic() {
            Some(key)
        } else {
            None
        }
    }
}

/// UK English QWERTY: the US letters, with the punctuation keys where the UK
/// layout puts them (Shift+2 is `"`, Shift+' is `@`, Shift+3 is `£`, the key
/// by Enter is `#` / `~`, and Shift+` is `¬`). Keys are still identified by
/// the US character (see [`super`]); the UK's extra key by the left Shift has
/// no US counterpart and is not mapped.
pub struct QwertyUk;

/// `(US key, UK character)` wherever they differ.
const UK: &[(char, char)] = &[
    ('@', '"'),
    ('"', '@'),
    ('#', '£'),
    ('\\', '#'),
    ('|', '~'),
    ('~', '¬'),
];

impl Layout for QwertyUk {
    fn id(&self) -> LayoutId {
        LayoutId::QwertyUk
    }

    fn key_of(&self, ch: char) -> Option<char> {
        if let Some(&(key, _)) = UK.iter().find(|(_, c)| *c == ch) {
            return Some(key);
        }
        // Not moved: the US key of the same character, unless the UK layout
        // moved that key's character elsewhere.
        (ch.is_ascii_graphic() && !UK.iter().any(|(k, _)| *k == ch)).then_some(ch)
    }

    fn char_of(&self, key: char) -> Option<char> {
        if let Some(&(_, c)) = UK.iter().find(|(k, _)| *k == key) {
            return Some(c);
        }
        key.is_ascii_graphic().then_some(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uk_moves_only_the_punctuation() {
        let uk = QwertyUk;
        assert_eq!(uk.char_of('@'), Some('"'));
        assert_eq!(uk.key_of('"'), Some('@'));
        assert_eq!(uk.key_of('@'), Some('"'));
        assert_eq!(uk.key_of('£'), Some('#'));
        assert_eq!(uk.key_of('#'), Some('\\'));
        for c in "abcXYZ019;',./[]-=".chars() {
            assert_eq!(uk.key_of(c), Some(c), "{c}");
            assert_eq!(uk.char_of(c), Some(c), "{c}");
        }
    }
}
