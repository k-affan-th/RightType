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
mod qwerty;

pub use kedmanee::Kedmanee;
pub use qwerty::QwertyEn;

/// Identifies a concrete keyboard layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum LayoutId {
    /// US English QWERTY.
    QwertyEn,
    /// Thai Kedmanee (TIS-820.2538) mapped onto a US physical keyboard.
    Kedmanee,
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

/// Convenience: text typed on an EN layout that was meant to be Thai.
pub fn en_to_th(input: &str) -> String {
    convert(input, &QwertyEn::new(), &Kedmanee::new())
}

/// Convenience: text typed on a Thai layout that was meant to be English.
pub fn th_to_en(input: &str) -> String {
    convert(input, &Kedmanee::new(), &QwertyEn::new())
}

/// Convert to the *other* layout, choosing the direction from the script present.
///
/// Used by the manual hotkeys (fix-word / fix-selection): the user has explicitly
/// asked to convert this text, so we don't second-guess with the dictionary — we
/// just flip it. Any Thai character means it was typed on the Thai layout and is
/// meant to be English (`th_to_en`); otherwise it's ASCII meant to be Thai
/// (`en_to_th`). Characters the chosen direction doesn't remap pass through, so a
/// mixed selection only flips the part that belongs to that layout.
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
