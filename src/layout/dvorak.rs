//! US Dvorak (Windows `KBDDV`), mapped onto the US physical keys.

use super::permuted::Permuted;
use super::{Layout, LayoutId};

/// US Dvorak.
pub struct DvorakEn;

const KEYS: Permuted = Permuted {
    qwerty: "-=qwertyuiop[]asdfghjkl;'zxcvbnm,./",
    other: "[]',.pyfgcrl/=aoeuidhtns-;qjkxbmwvz",
};

impl Layout for DvorakEn {
    fn id(&self) -> LayoutId {
        LayoutId::DvorakEn
    }

    fn key_of(&self, ch: char) -> Option<char> {
        KEYS.key_of(ch)
    }

    fn char_of(&self, key: char) -> Option<char> {
        KEYS.char_of(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dvorak_known_keys() {
        let d = DvorakEn;
        assert_eq!(d.char_of('s'), Some('o'));
        assert_eq!(d.char_of(';'), Some('s'));
        assert_eq!(d.char_of('/'), Some('z'));
        assert_eq!(d.char_of('q'), Some('\''));
        assert_eq!(d.char_of('Q'), Some('"'));
        assert_eq!(d.char_of('-'), Some('['));
        assert_eq!(d.char_of('_'), Some('{'));
        assert_eq!(d.char_of('1'), Some('1'));
        assert_eq!(d.key_of('o'), Some('s'));
    }

    #[test]
    fn round_trips_every_printable_key() {
        let d = DvorakEn;
        let mut seen = std::collections::HashSet::new();
        for c in '!'..='~' {
            let out = d.char_of(c).unwrap();
            assert_eq!(d.key_of(out), Some(c), "{c}");
            seen.insert(out);
        }
        assert_eq!(seen.len(), 94);
    }
}
