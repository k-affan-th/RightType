//! Pure, testable policy for automatic boundary correction.
//!
//! The Windows hook supplies the active keyboard-layout identifier and a token
//! completed by whitespace. This module is the single production decision point
//! for whether that token is eligible for automatic correction.

use crate::detect::{self, Detection};
use crate::dict::Dictionary;
use crate::layout::en_to_th;
use crate::secret::{self, SecretKind};
use crate::segment;

/// Shortest in-flight token the live path will consider at all. Two-character
/// candidates are excluded because valid short words (`สว`) are frequently true
/// prefixes of longer intended words (`สวัสดี`).
pub const MIN_LIVE_COMMIT_CHARS: usize = 3;

/// D-008: keystrokes a Thai reading must survive before the run is anchored —
/// the layout switched, the run released, and the reading no longer revisable.
///
/// Measured against the bundled dictionaries. At 4, every Thai sentence in the
/// corpus still arrives intact — including ones containing loanwords and names
/// that leave the dictionary — while mistyped English recovers 2.3x more often
/// than under a one-shot commit (2.36% of out-of-vocabulary typos mangled,
/// against 5.45%). At 5 and above the window is long enough that a Thai run
/// containing an unknown word is withdrawn wholesale instead of anchored, which
/// loses whole sentences; at 1 the behaviour degenerates back to D-007.
pub const COMMIT_HORIZON: usize = 4;

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

/// What the live (D-006) path may do with a token that is still being typed.
///
/// D-004 made destructive live-prefix conversion conditional on exactly this
/// three-state machine existing; before it the live path had only "convert" and
/// "do nothing", so a token that merely *looked* finished was converted as if
/// it were finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveDecision {
    /// No candidate — leave the token alone.
    None,
    /// A candidate exists, but the token can still grow into a different valid
    /// reading, so converting now would destroy text the typist is still in the
    /// middle of producing. Hold non-destructively and re-decide on the next key.
    Ambiguous,
    /// Evidence is decisive and no live alternative remains: safe to convert.
    Commit,
}

/// D-006: EN→TH commits without waiting for whitespace.
///
/// Thai prose has no inter-word spaces, so a whitespace trigger would never
/// fire during natural typing and would fabricate English-style gaps when it
/// did. A growing US-QWERTY token may therefore commit as soon as its complete
/// conversion is a fully-known High-confidence Thai candidate. TH→EN keeps the
/// whitespace contract — English really is space-delimited — and Suggest/
/// Manual paths are unchanged.
///
/// **The live path evaluates prefixes, so the completed-word guards in
/// [`detect`] do not protect it.** `detect` refuses to convert a token that is
/// already an English word, but `diffe` — on the way to `different` — is not a
/// word, so that guard is silent exactly where it is needed. The invariant
/// enforced here is therefore about the token's *future*, not its present:
/// never convert destructively while the token can still grow into a valid
/// reading in the language it is already written in.
pub fn live_decision(
    layout: Option<InputLayout>,
    token: &str,
    d: &Detection,
    en: &Dictionary,
) -> LiveDecision {
    if layout != Some(InputLayout::UsQwerty)
        || d.confidence != detect::Confidence::High
        || token.chars().count() < MIN_LIVE_COMMIT_CHARS
        || d.corrected.chars().any(|c| c.is_ascii_whitespace())
    {
        return LiveDecision::None;
    }
    // The token is still on its way to an English word, so the Thai reading is
    // one of at least two live readings. Hold: the next keystroke either kills
    // the English continuation (and this becomes a Commit) or completes an
    // English word (which `detect` then refuses outright). Either way the
    // typist's text survives, which a destructive commit here would not.
    if en.has_extension(token) {
        return LiveDecision::Ambiguous;
    }
    LiveDecision::Commit
}

/// How an in-flight run should currently read on screen (D-008).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reading {
    /// Leave the run exactly as the keystrokes produced it.
    AsTyped,
    /// Show this Thai text in place of the run.
    Thai(String),
}

/// The best reading of an un-anchored run typed on the US layout.
///
/// `holding_thai` is what RightType is currently showing for this run, and it
/// deliberately changes the question being asked:
///
/// * **Not holding** — the bar is a full [`LiveDecision::Commit`]: decisive
///   evidence and no live English continuation. Starting to rewrite text is a
///   visible act and must not be done on a maybe.
/// * **Holding** — the bar drops to "is the Thai reading still alive"
///   ([`segment::is_viable_prefix`]). A Thai run is invalid at almost every
///   intermediate keystroke, so demanding a complete parse here would make the
///   text flicker on every character. The reading is withdrawn only when Thai
///   genuinely dies, or when the raw keystrokes have become an English word.
///
/// The asymmetry is the point: entering the Thai reading is hard, staying in it
/// is easy, and leaving it is cheap and automatic. That is what lets a run be
/// re-decided instead of committed.
pub fn live_reading(run: &str, holding_thai: bool, en: &Dictionary, th: &Dictionary) -> Reading {
    if run.is_empty() {
        return Reading::AsTyped;
    }
    if !holding_thai {
        let Some(d) = detect_token(run, InputLayout::UsQwerty, en, th) else {
            return Reading::AsTyped;
        };
        return match live_decision(Some(InputLayout::UsQwerty), run, &d, en) {
            LiveDecision::Commit => Reading::Thai(d.corrected),
            LiveDecision::Ambiguous | LiveDecision::None => Reading::AsTyped,
        };
    }

    // Already showing Thai. Withdraw only on real evidence against it.
    if en.contains(run) {
        return Reading::AsTyped;
    }
    if matches!(
        secret::classify_token(run),
        Some(
            SecretKind::Hex | SecretKind::Base58Wif | SecretKind::Bech32 | SecretKind::ExtendedKey
        )
    ) {
        return Reading::AsTyped;
    }
    let converted = en_to_th(run);
    if segment::is_viable_prefix(&converted, th) {
        Reading::Thai(converted)
    } else {
        Reading::AsTyped
    }
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
        assert_eq!(
            live_decision(Some(InputLayout::UsQwerty), "l;ylfu", &d, &en),
            LiveDecision::Commit
        );
        assert_eq!(
            live_decision(Some(InputLayout::ThaiKedmanee), "l;ylfu", &d, &en),
            LiveDecision::None
        );
        assert_eq!(live_decision(None, "l;ylfu", &d, &en), LiveDecision::None);

        let to_en = detect_token("แนพพำแะ", InputLayout::ThaiKedmanee, &en, &th).unwrap();
        assert_eq!(
            live_decision(Some(InputLayout::ThaiKedmanee), "แนพพำแะ", &to_en, &en),
            LiveDecision::None
        );
    }

    #[test]
    fn a_fresh_run_needs_a_full_commit_to_start_reading_as_thai() {
        let (en, th) = dicts();
        assert_eq!(
            live_reading("l;ylfu", false, &en, &th),
            Reading::Thai("สวัสดี".to_string())
        );
        // `wri` is on its way to an English word: Ambiguous, so AsTyped.
        let en_full = crate::dict::english();
        let th_full = crate::dict::thai();
        assert_eq!(
            live_reading("wri", false, en_full, th_full),
            Reading::AsTyped
        );
    }

    #[test]
    fn a_held_thai_reading_survives_mid_word_keystrokes() {
        let en = crate::dict::english();
        let th = crate::dict::thai();
        // `l;ylfud` is `สวัสดีก` — not a phrase, but still going somewhere.
        assert!(matches!(
            live_reading("l;ylfud", true, en, th),
            Reading::Thai(_)
        ));
    }

    #[test]
    fn a_held_thai_reading_is_withdrawn_when_thai_dies() {
        let en = Dictionary::from_words(["adavnce"]);
        let th = Dictionary::from_words(["สวัสดี"]);
        // Nothing in this Thai dictionary can continue the run.
        assert_eq!(live_reading("zzqq", true, &en, &th), Reading::AsTyped);
    }

    #[test]
    fn live_commit_needs_min_length() {
        let (en, th) = dicts();
        let d = detect_token("l;ylfu", InputLayout::UsQwerty, &en, &th).unwrap();
        assert_eq!(
            live_decision(Some(InputLayout::UsQwerty), "l;", &d, &en),
            LiveDecision::None
        );
    }
}
