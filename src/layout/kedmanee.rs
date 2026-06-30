//! Thai Kedmanee layout (TIS-820.2538) mapped onto a US physical keyboard.
//!
//! The table below pairs each canonical physical key (the US QWERTY character)
//! with the Thai character Kedmanee produces there, for both the unshifted and
//! shifted rows. The letter mappings are verified against the published
//! RightLang examples (e.g. `correct` → `แนพพำแะ`, `สวัสดี` → `l;ylfu`).

use super::{Layout, LayoutId};
use std::collections::HashMap;
use std::sync::OnceLock;

/// `(canonical US-QWERTY key, Thai character produced by Kedmanee)`.
#[rustfmt::skip]
const PAIRS: &[(char, char)] = &[
    // ── Number row, unshifted ──
    ('`','_'),('1','ๅ'),('2','/'),('3','-'),('4','ภ'),('5','ถ'),('6','ุ'),
    ('7','ึ'),('8','ค'),('9','ต'),('0','จ'),('-','ข'),('=','ช'),
    // ── Number row, shifted ──
    ('~','%'),('!','+'),('@','๑'),('#','๒'),('$','๓'),('%','๔'),('^','ู'),
    ('&','฿'),('*','๕'),('(','๖'),(')','๗'),('_','๘'),('+','๙'),
    // ── Top letter row, unshifted ──
    ('q','ๆ'),('w','ไ'),('e','ำ'),('r','พ'),('t','ะ'),('y','ั'),('u','ี'),
    ('i','ร'),('o','น'),('p','ย'),('[','บ'),(']','ล'),('\\','ฃ'),
    // ── Top letter row, shifted ──
    ('Q','๐'),('W','"'),('E','ฎ'),('R','ฑ'),('T','ธ'),('Y','ํ'),('U','๊'),
    ('I','ณ'),('O','ฯ'),('P','ญ'),('{','ฐ'),('}',','),('|','ฅ'),
    // ── Home row, unshifted ──
    ('a','ฟ'),('s','ห'),('d','ก'),('f','ด'),('g','เ'),('h','้'),('j','่'),
    ('k','า'),('l','ส'),(';','ว'),('\'','ง'),
    // ── Home row, shifted ──
    ('A','ฤ'),('S','ฆ'),('D','ฏ'),('F','โ'),('G','ฌ'),('H','็'),('J','๋'),
    ('K','ษ'),('L','ศ'),(':','ซ'),('"','.'),
    // ── Bottom row, unshifted ──
    ('z','ผ'),('x','ป'),('c','แ'),('v','อ'),('b','ิ'),('n','ื'),('m','ท'),
    (',','ม'),('.','ใ'),('/','ฝ'),
    // ── Bottom row, shifted ──
    ('Z','('),('X',')'),('C','ฉ'),('V','ฮ'),('B','ฺ'),('N','์'),('M','?'),
    ('<','ฒ'),('>','ฬ'),('?','ฦ'),
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

/// Thai Kedmanee layout.
pub struct Kedmanee;

impl Kedmanee {
    pub fn new() -> Self {
        Kedmanee
    }
}

impl Default for Kedmanee {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for Kedmanee {
    fn id(&self) -> LayoutId {
        LayoutId::Kedmanee
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
