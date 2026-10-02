//! N-layout-generic keyboard-layout conversion engine.
//!
//! The model is deliberately simple and position-based so that adding a new
//! language later (Lao, Khmer, Russian, Thai Pattachote, …) is just a data
//! table — no engine changes.
//!
//! Every layout maps between a **canonical physical key** (identified by the US
//! QWERTY character it would produce) and the character it actually produces.
//! Conversion from layout *A* to layout *B* is therefore: for each input
//! character, find which physical key produced it in *A*, then ask *B* what that
//! same key produces. Characters with no mapping (spaces, already-correct
//! punctuation) pass through unchanged.

mod kedmanee;
mod manoonchai;
mod pattachote;
mod qwerty;

use std::sync::atomic::{AtomicU8, Ordering};

pub use kedmanee::Kedmanee;
pub use manoonchai::Manoonchai;
pub use pattachote::Pattachote;
pub use qwerty::{QwertyEn, QwertyUk};

/// Identifies a concrete keyboard layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LayoutId {
    /// US English QWERTY.
    QwertyEn,
    /// UK English QWERTY.
    QwertyUk,
    /// Thai Kedmanee (TIS-820.2538) mapped onto a US physical keyboard.
    Kedmanee,
    /// Thai Pattachote mapped onto a US physical keyboard.
    Pattachote,
    /// Thai Manoonchai mapped onto a US physical keyboard.
    Manoonchai,
}

/// Which Thai layout the typist uses (a setting: Windows does not say which
/// Thai layout a window has in a way the hook can read cheaply).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThaiVariant {
    Kedmanee,
    Pattachote,
    Manoonchai,
}

/// Which English layout is active (read from the window's keyboard layout).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EnglishVariant {
    Us,
    Uk,
}

static THAI: AtomicU8 = AtomicU8::new(0);
static ENGLISH: AtomicU8 = AtomicU8::new(0);

pub fn set_thai_variant(v: ThaiVariant) {
    THAI.store(v as u8, Ordering::Relaxed);
}

pub fn thai_variant() -> ThaiVariant {
    match THAI.load(Ordering::Relaxed) {
        v if v == ThaiVariant::Pattachote as u8 => ThaiVariant::Pattachote,
        v if v == ThaiVariant::Manoonchai as u8 => ThaiVariant::Manoonchai,
        _ => ThaiVariant::Kedmanee,
    }
}

pub fn set_english_variant(v: EnglishVariant) {
    ENGLISH.store(v as u8, Ordering::Relaxed);
}

pub fn english_variant() -> EnglishVariant {
    if ENGLISH.load(Ordering::Relaxed) == EnglishVariant::Uk as u8 {
        EnglishVariant::Uk
    } else {
        EnglishVariant::Us
    }
}

fn thai_layout() -> &'static dyn Layout {
    match thai_variant() {
        ThaiVariant::Kedmanee => &Kedmanee,
        ThaiVariant::Pattachote => &Pattachote,
        ThaiVariant::Manoonchai => &Manoonchai,
    }
}

fn english_layout() -> &'static dyn Layout {
    match english_variant() {
        EnglishVariant::Us => &QwertyEn,
        EnglishVariant::Uk => &QwertyUk,
    }
}

/// A keyboard layout: a bidirectional map between canonical physical keys
/// (US QWERTY characters) and the characters this layout produces.
pub trait Layout {
    /// Which layout this is.
    fn id(&self) -> LayoutId;

    /// The canonical physical key (US QWERTY char) that produces `ch` in this
    /// layout, or `None` if this layout does not produce `ch`.
    fn key_of(&self, ch: char) -> Option<char>;

    /// The character produced by canonical physical `key` in this layout, or
    /// `None` if that key produces nothing here.
    fn char_of(&self, key: char) -> Option<char>;
}

/// Convert `input` as if it had been typed with `to` selected instead of `from`.
///
/// Unmapped characters (spaces, punctuation a layout doesn't remap) are kept.
pub fn convert(input: &str, from: &dyn Layout, to: &dyn Layout) -> String {
    input
        .chars()
        .map(|c| from.key_of(c).and_then(|k| to.char_of(k)).unwrap_or(c))
        .collect()
}

/// Convenience: text typed on the English layout that was meant to be Thai
/// (with the English and Thai layouts currently selected; US and Kedmanee
/// unless set otherwise).
pub fn en_to_th(input: &str) -> String {
    convert(input, english_layout(), thai_layout())
}

/// Convenience: text typed on the Thai layout that was meant to be English.
pub fn th_to_en(input: &str) -> String {
    convert(input, thai_layout(), english_layout())
}

/// Convert to the *other* layout, choosing the direction from the script present.
///
/// Used by the manual hotkeys (fix-word / fix-selection): the user has explicitly
/// asked to convert this text, so we don't second-guess with the dictionary — we
/// just flip it. Any Thai character means it was typed on the Thai layout and is
/// meant to be English (`th_to_en`); otherwise it's ASCII meant to be Thai
/// (`en_to_th`). Characters the chosen direction doesn't remap pass through, so a
/// mixed selection only flips the part that belongs to that layout.
/// Thai digits to 0–9 when the text has any, otherwise 0–9 to Thai digits
/// (๐–๙): for documents that want one or the other.
pub fn swap_digits(input: &str) -> String {
    let thai = |c: char| ('\u{0E50}'..='\u{0E59}').contains(&c);
    let to_arabic = input.chars().any(thai);
    input
        .chars()
        .map(|c| {
            if to_arabic && thai(c) {
                char::from(b'0' + (c as u32 - 0x0E50) as u8)
            } else if !to_arabic && c.is_ascii_digit() {
                char::from_u32(0x0E50 + (c as u32 - '0' as u32)).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// UPPER CASE.
pub fn upper_case(input: &str) -> String {
    input.to_uppercase()
}

/// lower case.
pub fn lower_case(input: &str) -> String {
    input.to_lowercase()
}

/// Title Case: the first letter of each word up, the rest down.
pub fn title_case(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut start = true;
    for c in input.chars() {
        if c.is_alphabetic() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = c.is_whitespace() || c == '-' || c == '_';
        }
    }
    out
}

/// sWAP cASE: what CapsLock left on did, undone (`hELLO wORLD` → `Hello World`).
pub fn swap_case(input: &str) -> String {
    input
        .chars()
        .flat_map(|c| -> Vec<char> {
            if c.is_uppercase() {
                c.to_lowercase().collect()
            } else if c.is_lowercase() {
                c.to_uppercase().collect()
            } else {
                vec![c]
            }
        })
        .collect()
}

#[cfg(test)]
mod case_tests {
    #[test]
    fn cases() {
        assert_eq!(super::upper_case("let x = 1"), "LET X = 1");
        assert_eq!(super::lower_case("MAX_VALUE"), "max_value");
        assert_eq!(
            super::title_case("hELLO wORLD-wide สวัสดี"),
            "Hello World-Wide สวัสดี"
        );
        assert_eq!(super::swap_case("hELLO wORLD"), "Hello World");
    }
}

#[cfg(test)]
mod digit_tests {
    #[test]
    fn digits_swap_both_ways() {
        assert_eq!(super::swap_digits("ปี 2569 ข้อ 3"), "ปี ๒๕๖๙ ข้อ ๓");
        assert_eq!(super::swap_digits("ปี ๒๕๖๙ ข้อ 3"), "ปี 2569 ข้อ 3");
        assert_eq!(super::swap_digits("ไม่มีเลข"), "ไม่มีเลข");
    }
}

pub fn auto_convert(input: &str) -> String {
    let has_thai = input
        .chars()
        .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    if has_thai {
        th_to_en(input)
    } else {
        en_to_th(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The four canonical RightLang examples, verified against published sources.
    #[test]
    fn correct_roundtrip() {
        // "correct" typed while Thai is active -> "แนพพำแะ"
        assert_eq!(en_to_th("correct"), "แนพพำแะ");
        assert_eq!(th_to_en("แนพพำแะ"), "correct");
    }

    #[test]
    fn sawasdee_roundtrip() {
        // Thai "สวัสดี" typed while English is active -> "l;ylfu"
        assert_eq!(th_to_en("สวัสดี"), "l;ylfu");
        assert_eq!(en_to_th("l;ylfu"), "สวัสดี");
    }

    #[test]
    fn www_and_twitter() {
        assert_eq!(en_to_th("www"), "ไไไ");
        assert_eq!(en_to_th("twitter"), "ะไระะำพ");
        assert_eq!(en_to_th("what"), "ไ้ฟะ");
    }

    #[test]
    fn spaces_and_unmapped_pass_through() {
        assert_eq!(en_to_th("hi there"), {
            let mut s = en_to_th("hi");
            s.push(' ');
            s.push_str(&en_to_th("there"));
            s
        });
        // A bare space is preserved.
        assert_eq!(en_to_th(" "), " ");
    }

    #[test]
    fn shifted_consonants() {
        // Capital letters use the shifted Kedmanee row.
        assert_eq!(en_to_th("F"), "โ"); // shift+f -> โ (sara o)
        assert_eq!(th_to_en("โ"), "F");
    }

    #[test]
    fn auto_convert_picks_direction_by_script() {
        // ASCII gibberish was meant to be Thai.
        assert_eq!(auto_convert("l;ylfu"), "สวัสดี");
        // Thai gibberish was meant to be English.
        assert_eq!(auto_convert("แนพพำแะ"), "correct");
        // Works on a whole run with no internal spaces (the no-space-Thai case).
        assert_eq!(auto_convert("l;ylfud[yp"), en_to_th("l;ylfud[yp"));
    }
}
