//! Thai Manoonchai layout (v1.0) mapped onto a US physical keyboard.
//!
//! A modern Thai layout by Manassarn Manoonchai, MIT licence
//! (github.com/Manoonchai/Manoonchai). Its base layer is the v0.3 table of
//! the layout's README, which v1.0 keeps (v1.0 adds only an AltGr layer,
//! not read here). The number row and its shifted symbols are as on the US
//! keyboard.

use super::{Layout, LayoutId};
use std::collections::HashMap;
use std::sync::OnceLock;

/// `(canonical US-QWERTY key, character produced by Manoonchai)`.
#[rustfmt::skip]
const PAIRS: &[(char, char)] = &[
    // ── Number row, unshifted ──
    ('1','1'),('2','2'),('3','3'),('4','4'),('5','5'),('6','6'),('7','7'),('8','8'),
    ('9','9'),('0','0'),('-','-'),('=','='),
    // ── Top letter row, unshifted ──
    ('q','ใ'),('w','ต'),('e','ห'),('r','ล'),('t','ส'),('y','ป'),('u','ั'),('i','ก'),
    ('o','ิ'),('p','บ'),('[','็'),(']','ฬ'),('\\','ฯ'),
    // ── Home row, unshifted ──
    ('a','ง'),('s','เ'),('d','ร'),('f','น'),('g','ม'),('h','อ'),('j','า'),('k','่'),
    ('l','้'),(';','ว'),('\'','ื'),
    // ── Bottom row, unshifted ──
    ('z','ุ'),('x','ไ'),('c','ท'),('v','ย'),('b','จ'),('n','ค'),('m','ี'),(',','ด'),
    ('.','ะ'),('/','ู'),
    // ── Number row, shifted ──
    ('!','!'),('@','@'),('#','#'),('$','$'),('%','%'),('^','^'),('&','&'),('*','*'),
    ('(','('),(')',')'),('_','_'),('+','+'),
    // ── Top letter row, shifted ──
    ('Q','ฒ'),('W','ฏ'),('E','ซ'),('R','ญ'),('T','ฟ'),('Y','ฉ'),('U','ึ'),('I','ธ'),
    ('O','ฐ'),('P','ฎ'),('{','ฆ'),('}','ฑ'),('|','ฌ'),
    // ── Home row, shifted ──
    ('A','ษ'),('S','ถ'),('D','แ'),('F','ช'),('G','พ'),('H','ผ'),('J','ำ'),('K','ข'),
    ('L','โ'),(':','ภ'),('"','"'),
    // ── Bottom row, shifted ──
    ('Z','ฤ'),('X','ฝ'),('C','ๆ'),('V','ณ'),('B','๊'),('N','๋'),('M','์'),('<','ศ'),
    ('>','ฮ'),('?','?'),
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

/// Thai Manoonchai layout.
pub struct Manoonchai;

impl Manoonchai {
    pub fn new() -> Self {
        Manoonchai
    }
}

impl Default for Manoonchai {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for Manoonchai {
    fn id(&self) -> LayoutId {
        LayoutId::Manoonchai
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
        // สวัสดี on Manoonchai: ส=t ว=; ั=u ส=t ด=, ี=m.
        let typed: String = "สวัสดี".chars().map(|c| maps().th_to_en[&c]).collect();
        assert_eq!(typed, "t;ut,m");
        let back: String = "t;ut,m".chars().map(|c| maps().en_to_th[&c]).collect();
        assert_eq!(back, "สวัสดี");
    }

    #[test]
    fn every_letter_key_has_one_thai_character() {
        for &(en, th) in PAIRS {
            if en.is_ascii_alphabetic() {
                assert_eq!(maps().en_to_th.get(&en), Some(&th), "en->th for {en}");
                assert_eq!(maps().th_to_en.get(&th), Some(&en), "th->en for {th}");
            }
        }
        // Every Thai consonant is somewhere on it.
        for c in '\u{0E01}'..='\u{0E2E}' {
            if c == 'ฃ' || c == 'ฅ' || c == 'ฦ' {
                continue; // on the AltGr layer only
            }
            assert!(maps().th_to_en.contains_key(&c), "{c}");
        }
    }
}
