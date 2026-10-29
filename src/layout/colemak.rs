//! Colemak, mapped onto the US physical keys.

use super::permuted::Permuted;
use super::{Layout, LayoutId};

/// Colemak (the original, not the DH variants).
pub struct ColemakEn;

const KEYS: Permuted = Permuted {
    qwerty: "ertyuiopsdfgjkl;n",
    other: "fpgjluy;rstdneiok",
};

impl Layout for ColemakEn {
    fn id(&self) -> LayoutId {
        LayoutId::ColemakEn
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
    fn colemak_known_keys() {
        let c = ColemakEn;
        assert_eq!(c.char_of('e'), Some('f'));
        assert_eq!(c.char_of('p'), Some(';'));
        assert_eq!(c.char_of(';'), Some('o'));
        assert_eq!(c.char_of('n'), Some('k'));
        assert_eq!(c.char_of('q'), Some('q'));
        assert_eq!(c.char_of('E'), Some('F'));
        assert_eq!(c.char_of(':'), Some('O'));
        assert_eq!(c.key_of('f'), Some('e'));
    }

    #[test]
    fn round_trips_every_printable_key() {
        let c = ColemakEn;
        let mut seen = std::collections::HashSet::new();
        for k in '!'..='~' {
            let out = c.char_of(k).unwrap();
            assert_eq!(c.key_of(out), Some(k), "{k}");
            seen.insert(out);
        }
        assert_eq!(seen.len(), 94);
    }
}
