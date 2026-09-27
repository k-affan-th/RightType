//! Pure, testable policy for automatic boundary correction.
//!
//! The Windows hook supplies the active keyboard-layout identifier and a token
//! completed by whitespace. This module is the single production decision point
//! for whether that token is eligible for automatic correction.

use crate::detect::{self, Confidence, Detection, Evidence};
use crate::dict::Dictionary;
use crate::english;
use crate::layout::{en_to_th, th_to_en};
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
///
/// "English" here is wider than dictionary membership: compounds of frequent
/// words and user-learned words count too (see [`english`]), in both
/// directions. On the US layout that keeps `middleware` from being rewritten
/// as Thai; on the Thai layout it lets `workflow` typed by mistake come back.
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
    match layout {
        InputLayout::UsQwerty => {
            if !has_latin && !has_thai {
                return us_layout_letterless(token, th);
            }
            if !has_latin || has_thai || english::is_compound(token.trim()) {
                return None;
            }
            detect::detect(token, en, th).or_else(|| us_layout_thai_with_punctuation(token, en, th))
        }
        InputLayout::ThaiKedmanee => {
            if !has_thai || has_latin {
                return None;
            }
            detect::detect(token, en, th)
                .or_else(|| thai_layout_compound(token, th))
                .or_else(|| thai_layout_technical(token, en, th))
                .or_else(|| thai_layout_trailing_mark(token, th))
        }
    }
}

/// A Thai word typed on the English layout whose keys give no letters at all
/// — `57'` is ถึง, `]'` is ลง, `.0` is ใจ, `[688]` is บุคคล. The whole token
/// must read as one Thai dictionary word. Numbers stay numbers (`86` would
/// read คุ, `5,` in a list จม), with or without punctuation around them, and a
/// run of one repeated key (`''`, `--`) is left alone.
fn us_layout_letterless(token: &str, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    // Punctuation that goes with numbers (`5,` `3.` `50%` `(12)` `$5`); `'`
    // is not among them — on Kedmanee it is ง, and `57'` is ถึง.
    const NUMBER_MARKS: &[char] = &[',', '.', '%', '(', ')', '$', '#', '!', '?', ':', ';', '"'];
    let core = token.trim_matches(NUMBER_MARKS);
    if !core.is_empty() && core.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let mut chars = token.chars();
    let first = chars.next()?;
    if chars.all(|c| c == first) {
        return None;
    }
    let reading = crate::layout::en_to_th(token);
    if reading.chars().count() < 2 || !th.contains(&reading) {
        return None;
    }
    Some(Detection {
        corrected: reading,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
}

/// Punctuation the typist meant as ASCII even when it is typed next to a
/// wrong-layout Thai word. Each of these keys gives a character that almost
/// never ends (or starts) a Thai word — `:` gives ซ, `?` gives ฦ — unlike `,`
/// and `.`, which give the common ม and ใ and are left to the Thai reading.
const TRAILING_ASCII: &[char] = &[':', '?', '!', '"', ')'];
const LEADING_ASCII: &[char] = &['"', '('];

/// A Thai word typed on the English layout next to punctuation typed where
/// it belongs: `grnjvdkixit,;]z]4kKkwmp:` is `เพื่อการประมวลผลภาษาไทย:`. The
/// whole token does not read as Thai (`:` would be ซ), so the word is read
/// without its edges and the edges are kept as typed.
fn us_layout_thai_with_punctuation(
    token: &str,
    en: &Dictionary,
    th: &Dictionary,
) -> Option<Detection> {
    let token = token.trim();
    // An English word in quotes or brackets is just that.
    let ascii_core = token.trim_matches(|c: char| c.is_ascii_punctuation());
    if ascii_core.is_empty() || english::is_word(ascii_core, en) {
        return None;
    }
    // Punctuation typed with the Thai key for it (`W` is `"` on Kedmanee):
    // judge the Thai reading without its edge punctuation.
    let converted = en_to_th(token);
    let thai_core =
        converted.trim_matches(|c: char| c.is_ascii_punctuation() && c != '-' && c != '/');
    if thai_core.len() != converted.len()
        && thai_core.chars().count() >= 2
        && (th.contains(thai_core) || segment::is_fully_known(thai_core, th))
    {
        return Some(Detection {
            corrected: converted,
            confidence: Confidence::High,
            evidence: Evidence::FullSegmentation,
        });
    }
    let core = token.trim_start_matches(LEADING_ASCII);
    let lead = &token[..token.len() - core.len()];
    let inner = core.trim_end_matches(TRAILING_ASCII);
    let trail = &core[inner.len()..];
    if (lead.is_empty() && trail.is_empty()) || lead.len() > 2 || trail.len() > 2 {
        return None;
    }
    let d = detect::detect(inner, en, th)?;
    // Only a Thai reading: an English core would have been left alone anyway.
    if !d
        .corrected
        .chars()
        .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
    {
        return None;
    }
    Some(Detection {
        corrected: format!("{lead}{}{trail}", d.corrected),
        ..d
    })
}

/// Technical English typed on the Thai layout: numbers (`ๅ/มภคจ` is
/// `12,480`), acronyms, product names, derived and hyphenated terms — see
/// [`english::is_technical`] — optionally wrapped in quotes or brackets.
///
/// Punctuation typed right after a Thai word on the Thai layout, where its key
/// gives a Thai letter instead: `:` (Shift+`;`) is ซ and `?` (Shift+`/`) is ฦ,
/// so `คำสำคัญ:` arrives as `คำสำคัญซ`. Fixed only when the text before that
/// letter is complete, known Thai and the whole token is not — so a real word
/// ending in ซ (`ก๊าซ`) is left alone.
fn thai_layout_trailing_mark(token: &str, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    let last = token.chars().last()?;
    let mark = match last {
        'ซ' => ':',
        'ฦ' => '?',
        _ => return None,
    };
    let head = &token[..token.len() - last.len_utf8()];
    // Short heads are left alone: a two- or three-letter word plus ซ is as
    // likely a slip of the finger as a colon.
    // A long phrase needs at least three known words: a single misspelt
    // word followed by a stray ซ must not read as a phrase and a colon.
    let head_is_thai = th.contains(head)
        || (segment::is_fully_known(head, th) && segment::segment(head, th).len() >= 3);
    if head.chars().count() < 4
        || !head_is_thai
        || th.contains(token)
        || segment::is_fully_known(token, th)
    {
        return None;
    }
    Some(Detection {
        corrected: format!("{head}{mark}"),
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
}

/// The Thai text must not itself be Thai: not a dictionary word and not a
/// complete segmentation, so a real Thai word whose keys happen to spell such
/// a shape (`จุ` is `06`) is left alone.
fn thai_layout_technical(token: &str, en: &Dictionary, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    // Judge the Thai without punctuation around it: `จึง:` is a Thai word
    // and a colon, not the number `07`.
    // (`/` and `-` are number-row keys on Kedmanee, so `/จ` is `20`: the
    // length check is on the whole token.)
    let thai_core = token.trim_matches(|c: char| c.is_ascii_punctuation());
    if token.chars().count() < 2
        || th.contains(thai_core)
        || (thai_core.chars().count() >= 2 && segment::is_fully_known(thai_core, th))
    {
        return None;
    }
    let raw = th_to_en(token);
    // Edge punctuation only where its key is not also a Thai letter: `,` is
    // ม, `;` is ว and `'` is ง, so those stay part of the word.
    let core = raw.trim_start_matches(['"', '(']);
    let lead = raw.len() - core.len();
    let core = core.trim_end_matches(['"', ')', ':', '?', '!', '.']);
    let trail = raw.len() - lead - core.len();
    if lead > 2 || trail > 2 {
        return None;
    }
    // A trailing `.` can belong to a number (`3.`).
    let core_ok = english::is_technical(core, en)
        || (trail > 0 && english::is_technical(&raw[lead..lead + core.len() + 1], en));
    if !core_ok {
        return None;
    }
    // A lone acronym typed on the Thai layout never contains a Thai leading
    // vowel (เ แ โ ใ ไ): none of those is on a Shift+letter key except โ
    // (Shift+F), and a Thai syllable built on one (`โฮ๋` reads `FVJ`) is far
    // likelier than an acronym starting with F.
    if english::is_acronym(core)
        && thai_core
            .chars()
            .any(|c| matches!(c, 'เ' | 'แ' | 'โ' | 'ใ' | 'ไ'))
    {
        return None;
    }
    Some(Detection {
        corrected: raw,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
}

/// A compound English word typed on the Thai layout.
///
/// Deliberately *not* guarded by "the Thai segments cleanly": with 2-letter
/// Thai words in the dictionary almost anything segments (`middleware` typed
/// on the Thai layout reads as valid Thai), so that guard blocked the very
/// case this exists for — English typed right after a Thai word, while the
/// layout is still Thai. The compound itself is the strong signal: across
/// 200,000 synthetic 2–3-word Thai phrases none had keys spelling a compound.
fn thai_layout_compound(token: &str, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    if th.contains(token) {
        return None;
    }
    let converted = th_to_en(token);
    if !english::is_compound(&converted) {
        return None;
    }
    Some(Detection {
        corrected: converted,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
}

/// The boundary decision for a token RightType itself converted to Thai while
/// it was being typed (D-009).
///
/// Once a run is anchored the layout is switched and the rest of the word
/// arrives as native Thai, so the finished token on screen is entirely Thai.
/// The typist started it on the English layout, though, and only now is the
/// whole token visible. If its keystrokes spell English — a dictionary word, a
/// learned word or a compound — the early reading was wrong and the *whole*
/// token goes back, not just the part typed after the anchor.
pub fn revise_converted(token: &str, en: &Dictionary) -> Option<Detection> {
    let raw = th_to_en(token.trim());
    // Trailing sentence punctuation may follow a word (`middleware,`), but on
    // this layout most ASCII punctuation is a Thai letter (`[` is บ, `;` is น),
    // so only a short trailing run is set aside and the rest must be letters.
    let core = raw.trim_end_matches(|c: char| c.is_ascii_punctuation());
    if raw.len() - core.len() > 2
        || core.chars().count() < 3
        || !core.chars().all(|c| c.is_ascii_alphabetic())
        || !english::is_word(core, en)
    {
        return None;
    }
    Some(Detection {
        corrected: raw,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
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
    if english::has_continuation(token, en) || english::is_compound(token) {
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
    if english::is_word(run, en) {
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
    fn a_colon_or_question_mark_after_a_thai_word_comes_back() {
        let en = crate::dict::english();
        let th = crate::dict::thai();
        let fix = |t: &str| detect_token(t, InputLayout::ThaiKedmanee, en, th).map(|d| d.corrected);
        assert_eq!(
            fix("เพื่อการประมวลผลภาษาไทยซ").as_deref(),
            Some("เพื่อการประมวลผลภาษาไทย:")
        );
        assert_eq!(
            fix("เขาไปโรงเรียนหรือไม่ฦ").as_deref(),
            Some("เขาไปโรงเรียนหรือไม่?")
        );
        // Two known words before the ซ are not enough: a misspelt word often
        // splits that way too (the audit's unknown-Thai budget holds the line).
        assert_eq!(fix("คำสำคัญซ"), None);
        // Real words that end in ซ stay.
        assert_eq!(fix("ก๊าซ"), None);
        // Not a known word before the ซ: no guess.
        assert_eq!(fix("กขฃซ"), None);
    }

    #[test]
    fn thai_words_typed_without_letters_come_back_but_numbers_stay() {
        let en = crate::dict::english();
        let th = crate::dict::thai();
        let fix = |t: &str| detect_token(t, InputLayout::UsQwerty, en, th).map(|d| d.corrected);
        assert_eq!(fix("57'").as_deref(), Some("ถึง"));
        assert_eq!(fix("]'").as_deref(), Some("ลง"));
        assert_eq!(fix("0[").as_deref(), Some("จบ"));
        assert_eq!(fix("=,").as_deref(), Some("ชม"));
        assert_eq!(fix("[688]").as_deref(), Some("บุคคล"));
        for keep in [
            "86", "90", "469", "5,", "86,", "(97)", "''", "--", "3.14", "10:30", ":)",
        ] {
            assert_eq!(fix(keep), None, "{keep}");
        }
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

    fn on_thai(token: &str) -> Option<String> {
        detect_token(
            token,
            InputLayout::ThaiKedmanee,
            crate::dict::english(),
            crate::dict::thai(),
        )
        .map(|d| d.corrected)
    }

    fn on_us(token: &str) -> Option<String> {
        detect_token(
            token,
            InputLayout::UsQwerty,
            crate::dict::english(),
            crate::dict::thai(),
        )
        .map(|d| d.corrected)
    }

    #[test]
    fn technical_english_typed_on_the_thai_layout_comes_back() {
        for english in [
            "40",
            "12,480",
            "0.912",
            "64%",
            "2e-5",
            "GPU",
            "NVIDIA",
            "A100",
            "fine-tuning",
            "code-switching",
            "F1-score",
            "TF-IDF",
            "bag-of-words",
            "retrieval-augmented",
            "parameter-efficient",
            "tokenization",
            "(LLM)",
            "\"so-so\"",
        ] {
            let typed = crate::layout::en_to_th(english);
            assert_eq!(on_thai(&typed).as_deref(), Some(english), "{typed}");
        }
    }

    #[test]
    fn thai_that_only_looks_technical_stays_thai() {
        // Real words, a word with a colon after it, a Thai syllable whose
        // Shift keys read as capitals, and number-row Thai words.
        for thai in ["จุ", "ถึง:", "คำสำคัญ", "โฮ๋", "ภูมิ", "กรวว", "แบบ", "ก๊าซ"]
        {
            assert_eq!(on_thai(thai), None, "{thai}");
        }
    }

    #[test]
    fn english_with_punctuation_on_the_english_layout_stays_english() {
        for english in ["\"it\"", "adc:", "(me)", "hello?", "\"so\""] {
            assert_eq!(on_us(english), None, "{english}");
        }
    }

    #[test]
    fn thai_on_the_english_layout_keeps_punctuation_typed_in_english() {
        // `:` typed where it belongs after a Thai word typed on the wrong
        // layout, and Kedmanee's own `"` (the W key).
        let typed = format!("{}:", crate::layout::th_to_en("ภาษาไทย"));
        assert_eq!(on_us(&typed).as_deref(), Some("ภาษาไทย:"));
        let typed = crate::layout::th_to_en("ดีมาก\"");
        assert_eq!(on_us(&typed).as_deref(), Some("ดีมาก\""));
        // Thai typed on the English layout whose keys are punctuation.
        assert_eq!(on_us("c[[").as_deref(), Some("แบบ"));
    }
}
