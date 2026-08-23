//! Pure, testable policy for automatic boundary correction.
//!
//! The Windows hook supplies the active keyboard-layout identifier and a token
//! completed by whitespace. This module is the single production decision point
//! for whether that token is eligible for automatic correction.

use crate::detect::{self, Detection};
use crate::dict::Dictionary;

/// Exact keyboard layouts whose physical-key tables are bundled in v1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputLayout {
    UsQwerty,
    ThaiKedmanee,
}

pub const KLID_US_QWERTY: u32 = 0x0000_0409;
pub const KLID_THAI_KEDMANEE: u32 = 0x0000_041E;

/// Resolve a Windows `HKL` value only when its physical-key mapping is the
/// default layout for the language. Windows commonly returns default handles as
/// `0x04090409` / `0x041E041E` (device word equals LANGID), while canonical IDs
/// use a zero device word. Variant layouts use a different device word and are
/// rejected (for example Pattachote/Dvorak replacement handles).
pub fn supported_layout_id(hkl: u32) -> Option<InputLayout> {
    let language = hkl & 0xFFFF;
    let device = hkl >> 16;
    if device != 0 && device != language {
        return None;
    }
    match language {
        KLID_US_QWERTY => Some(InputLayout::UsQwerty),
        KLID_THAI_KEDMANEE => Some(InputLayout::ThaiKedmanee),
        _ => None,
    }
}

/// Evaluate one token completed by a whitespace boundary.
pub fn detect_token(
    token: &str,
    layout: InputLayout,
    en: &Dictionary,
    th: &Dictionary,
) -> Option<Detection> {
    let has_thai = token
        .chars()
        .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    let has_latin = token.chars().any(|c| c.is_ascii_alphabetic());
    let direction_matches = match layout {
        InputLayout::UsQwerty => has_latin && !has_thai,
        InputLayout::ThaiKedmanee => has_thai && !has_latin,
    };
    direction_matches
        .then(|| detect::detect(token, en, th))
        .flatten()
}

/// Learning is allowed only for ordinary text produced under exact US QWERTY
/// when the same production decision found no wrong-layout correction. Shape,
/// dictionary and repeat-count guards remain in the learning module itself.
pub fn allows_learning(layout: Option<InputLayout>, correction_proposed: bool) -> bool {
    layout == Some(InputLayout::UsQwerty) && !correction_proposed
}

/// D-006: EN→TH commits without waiting for whitespace.
///
/// Thai prose has no inter-word spaces, so a whitespace trigger would never
/// fire during natural typing and would fabricate English-style gaps when it
/// did. A growing US-QWERTY token may therefore commit as soon as its complete
/// conversion is a fully-known High-confidence Thai candidate. TH→EN keeps the
/// whitespace contract — English really is space-delimited — and Suggest/
/// Manual paths are unchanged.
pub fn allows_live_thai_commit(layout: Option<InputLayout>, d: &detect::Detection) -> bool {
    layout == Some(InputLayout::UsQwerty)
        && d.confidence == detect::Confidence::High
        && !d.corrected.chars().any(|c| c.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dicts() -> (Dictionary, Dictionary) {
        (
            Dictionary::from_words(["correct", "hello"]),
            Dictionary::from_words(["สวัสดี"]),
        )
    }

    #[test]
    fn exact_layout_gate_rejects_unsupported_variants() {
        assert_eq!(
            supported_layout_id(KLID_US_QWERTY),
            Some(InputLayout::UsQwerty)
        );
        assert_eq!(
            supported_layout_id(KLID_THAI_KEDMANEE),
            Some(InputLayout::ThaiKedmanee)
        );
        assert_eq!(
            supported_layout_id(0x0409_0409),
            Some(InputLayout::UsQwerty)
        );
        assert_eq!(
            supported_layout_id(0x041E_041E),
            Some(InputLayout::ThaiKedmanee)
        );
        assert_eq!(supported_layout_id(0x0000_0809), None); // UK English
        assert_eq!(supported_layout_id(0x0001_041E), None); // Thai Pattachote
        assert_eq!(supported_layout_id(0xF001_041E), None); // replacement variant handle
    }

    #[test]
    fn boundary_policy_handles_both_supported_directions() {
        let (en, th) = dicts();
        let to_thai = detect_token("l;ylfu", InputLayout::UsQwerty, &en, &th).unwrap();
        assert_eq!(to_thai.corrected, "สวัสดี");
        let to_english = detect_token("แนพพำแะ", InputLayout::ThaiKedmanee, &en, &th).unwrap();
        assert_eq!(to_english.corrected, "correct");
    }

    #[test]
    fn boundary_policy_rejects_wrong_layout_direction() {
        let (en, th) = dicts();
        assert!(detect_token("l;ylfu", InputLayout::ThaiKedmanee, &en, &th).is_none());
        assert!(detect_token("แนพพำแะ", InputLayout::UsQwerty, &en, &th).is_none());
    }

    #[test]
    fn learning_gate_rejects_candidates_and_non_us_layouts() {
        assert!(allows_learning(Some(InputLayout::UsQwerty), false));
        assert!(!allows_learning(Some(InputLayout::UsQwerty), true));
        assert!(!allows_learning(Some(InputLayout::ThaiKedmanee), false));
        assert!(!allows_learning(None, false));
    }

    #[test]
    fn live_thai_commit_only_on_us_layout_high_confidence() {
        let (en, th) = dicts();
        let d = detect_token("l;ylfu", InputLayout::UsQwerty, &en, &th).unwrap();
        assert!(allows_live_thai_commit(Some(InputLayout::UsQwerty), &d));
        assert!(!allows_live_thai_commit(
            Some(InputLayout::ThaiKedmanee),
            &d
        ));
        assert!(!allows_live_thai_commit(None, &d));

        let to_en = detect_token("แนพพำแะ", InputLayout::ThaiKedmanee, &en, &th).unwrap();
        assert!(!allows_live_thai_commit(
            Some(InputLayout::ThaiKedmanee),
            &to_en
        ));
    }
}
