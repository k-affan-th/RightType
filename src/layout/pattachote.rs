//! Thai Pattachote layout mapped onto a US physical keyboard.
//!
//! Generated from xkeyboard-config's `symbols/th`, variant `pat` (Pattachote
//! layout by Visanu Euarchukiati; MIT-style licence), key by key, so nothing
//! here is typed from memory. The same file's `basic` variant matches the
//! Kedmanee table exactly, which is how the extraction was checked.

use super::{Layout, LayoutId};
use std::collections::HashMap;
use std::sync::OnceLock;

/// `(canonical US-QWERTY key, Thai character produced by Pattachote)`.
#[rustfmt::skip]
const PAIRS: &[(char, char)] = &[
    // ── Number row, unshifted ──
    ('`','_'),('1','='),('2','๒'),('3','๓'),('4','๔'),('5','๕'),('6','ู'),
    ('7','๗'),('8','๘'),('9','๙'),('0','๐'),('-','๑'),('=','๖'),
    // ── Number row, shifted ──
    ('~','฿'),('!','+'),('@','"'),('#','/'),('$',','),('%','?'),('^','ุ'),
    ('&','_'),('*','.'),('(','('),(')',')'),('_','-'),('+','%'),
    // ── Top letter row, unshifted ──
    ('q','็'),('w','ต'),('e','ย'),('r','อ'),('t','ร'),('y','่'),('u','ด'),
    ('i','ม'),('o','ว'),('p','แ'),('[','ใ'),(']','ฌ'),('\\','ๅ'),
    // ── Top letter row, shifted ──
    ('Q','๊'),('W','ฤ'),('E','ๆ'),('R','ญ'),('T','ษ'),('Y','ึ'),('U','ฝ'),
    ('I','ซ'),('O','ถ'),('P','ฒ'),('{','ฯ'),('}','ฦ'),('|','ํ'),
    // ── Home row, unshifted ──
    ('a','้'),('s','ท'),('d','ง'),('f','ก'),('g','ั'),('h','ี'),('j','า'),
    ('k','น'),('l','เ'),(';','ไ'),('\'','ข'),
    // ── Home row, shifted ──
    ('A','๋'),('S','ธ'),('D','ำ'),('F','ณ'),('G','์'),('H','ื'),('J','ผ'),
    ('K','ช'),('L','โ'),(':','ฆ'),('"','ฑ'),
    // ── Bottom row, unshifted ──
    ('z','บ'),('x','ป'),('c','ล'),('v','ห'),('b','ิ'),('n','ค'),('m','ส'),
    (',','ะ'),('.','จ'),('/','พ'),
    // ── Bottom row, shifted ──
    ('Z','ฎ'),('X','ฏ'),('C','ฐ'),('V','ภ'),('B','ฺ'),('N','ศ'),('M','ฮ'),
    ('<','ฟ'),('>','ฉ'),('?','ฬ'),
];

struct Maps {
    /// canonical key -> Thai char
    en_to_th: HashMap<char, char>,
    /// Thai char -> canonical key
    th_to_en: HashMap<char, char>,
}

fn maps() -> &'static Maps {
    static MAPS: OnceLock<Maps> = OnceLock::new();
    MAPS.get_or_init(|| {
        let mut en_to_th = HashMap::with_capacity(PAIRS.len());
        let mut th_to_en = HashMap::with_capacity(PAIRS.len());
        for &(en, th) in PAIRS {
            en_to_th.entry(en).or_insert(th);
            // First writer wins so letter mappings (inserted in order) are stable
            // even where an ASCII-valued Thai key would otherwise collide.
            th_to_en.entry(th).or_insert(en);
        }
        Maps { en_to_th, th_to_en }
    })
}

/// Thai Pattachote layout.
pub struct Pattachote;

impl Pattachote {
    pub fn new() -> Self {
        Pattachote
    }
}

impl Default for Pattachote {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for Pattachote {
    fn id(&self) -> LayoutId {
        LayoutId::Pattachote
    }

    fn key_of(&self, ch: char) -> Option<char> {
        maps().th_to_en.get(&ch).copied()
    }

    fn char_of(&self, key: char) -> Option<char> {
        maps().en_to_th.get(&key).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_words_type_as_expected() {
        // สวัสดี on Pattachote: ส=m ว=o ั=g ส=m ด=u ี=h.
        let typed: String = "สวัสดี".chars().map(|c| maps().th_to_en[&c]).collect();
        assert_eq!(typed, "mogmuh");
        let back: String = "mogmuh".chars().map(|c| maps().en_to_th[&c]).collect();
        assert_eq!(back, "สวัสดี");
    }

    #[test]
    fn every_pair_round_trips_on_letters() {
        // For alphabetic canonical keys the mapping must be a clean bijection.
        for &(en, th) in PAIRS {
            if en.is_ascii_alphabetic() {
                assert_eq!(maps().en_to_th.get(&en), Some(&th), "en->th for {en}");
                assert_eq!(maps().th_to_en.get(&th), Some(&en), "th->en for {th}");
            }
        }
    }
}
