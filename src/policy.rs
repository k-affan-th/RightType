//! Pure, testable policy for automatic boundary correction.
//!
//! The Windows hook supplies the active keyboard-layout identifier and a token
//! completed by whitespace. This module is the single production decision point
//! for whether that token is eligible for automatic correction.

use crate::detect::{self, Confidence, Detection, Evidence};
use crate::dict::Dictionary;
use crate::english;
use crate::layout::{en_to_th, th_to_en, ThaiVariant};
use crate::secret::{self, SecretKind};
use crate::segment;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

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

/// Resolve a Windows `HKL` (or KLID) to a layout RightType has a table for.
///
/// The low word is the language, the high word the keyboard (zero, or equal
/// to the language, for the language's default keyboard).
///
/// * **English**, any country, typed on the US keyboard (device `0x0409`, or
///   the default keyboard of US / Australia / New Zealand English) or on the
///   UK keyboard (device `0x0809`, or UK English's default). Which one is
///   reported by [`english_variant_of`]. Canada is covered when its keyboard
///   is set to US; its default handle can mean the Canadian French layout.
/// * **Thai**: the default keyboard (Kedmanee). Other Thai keyboards are
///   accepted only when the user has chosen Pattachote in Settings (their
///   handles vary, so the choice is the user's, not guessed), and then the
///   default one is not: keys are only ever read through the matching table.
///
/// Anything else (Dvorak, other languages) is `None`: RightType stays out.
pub fn supported_layout_id(hkl: u32) -> Option<InputLayout> {
    let language = hkl & 0xFFFF;
    if language & 0x3FF == 0x09 {
        return english_variant_of(hkl).map(|_| InputLayout::UsQwerty);
    }
    if language == KLID_THAI_KEDMANEE {
        return thai_keyboard_matches(hkl, crate::layout::thai_variant())
            .then_some(InputLayout::ThaiKedmanee);
    }
    None
}

/// Web addresses, email addresses and numbers typed with the Thai keyboard
/// on are put back (on by default; the typist can turn it off).
static FIXES_ADDRESSES: AtomicBool = AtomicBool::new(true);

pub fn fixes_addresses() -> bool {
    FIXES_ADDRESSES.load(Ordering::Relaxed)
}

pub fn set_fixes_addresses(on: bool) {
    FIXES_ADDRESSES.store(on, Ordering::Relaxed);
}

/// The Thai keyboards other than the default one, by the high word of their
/// handle, with the table each one follows. Filled by the Windows layer from
/// the keyboard's layout file (Windows also has a Kedmanee without ShiftLock,
/// and a Pattachote without it). Empty until then.
static THAI_KEYBOARDS: RwLock<Vec<(u16, ThaiVariant)>> = RwLock::new(Vec::new());

/// Tell the policy which table each installed non-default Thai keyboard uses.
pub fn set_thai_keyboards(keyboards: Vec<(u16, ThaiVariant)>) {
    if let Ok(mut k) = THAI_KEYBOARDS.write() {
        *k = keyboards;
    }
}

/// Which table a Thai `HKL` follows: Kedmanee for the default keyboard, what
/// `known` says for the others. A keyboard not in `known` (its layout file
/// could not be read) is `None`: RightType stays off rather than guess.
fn thai_table_of(hkl: u32, known: &[(u16, ThaiVariant)]) -> Option<ThaiVariant> {
    let language = hkl & 0xFFFF;
    let device = hkl >> 16;
    if device == 0 || device == language {
        return Some(ThaiVariant::Kedmanee);
    }
    known
        .iter()
        .find(|(d, _)| u32::from(*d) == device)
        .map(|(_, v)| *v)
}

/// Whether a Thai `HKL` is a keyboard of the table in use. Only those are
/// accepted, so keys are never read through the other table when both kinds
/// of keyboard are installed.
fn thai_keyboard_matches(hkl: u32, variant: ThaiVariant) -> bool {
    THAI_KEYBOARDS
        .read()
        .map(|known| thai_table_of(hkl, &known) == Some(variant))
        .unwrap_or(false)
}

/// Whether `hkl` is the keyboard whose table is in use: the chosen Thai
/// keyboard (the default one for Kedmanee, another one for Pattachote), or
/// the English keyboard last seen.
pub fn is_preferred_layout(hkl: u32) -> bool {
    use crate::layout::{english_variant, thai_variant};
    if hkl & 0xFFFF == KLID_THAI_KEDMANEE {
        return thai_keyboard_matches(hkl, thai_variant());
    }
    english_variant_of(hkl) == Some(english_variant())
}

/// Which English keyboard an English `HKL` uses, if RightType knows it.
pub fn english_variant_of(hkl: u32) -> Option<crate::layout::EnglishVariant> {
    use crate::layout::EnglishVariant::{Uk, Us};
    let language = hkl & 0xFFFF;
    let device = hkl >> 16;
    if language & 0x3FF != 0x09 {
        return None;
    }
    let keyboard = if device == 0 { language } else { device };
    match keyboard {
        0x0409 | 0x0C09 | 0x1409 => Some(Us),
        0x0809 => Some(Uk),
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
            if let Some(d) = us_layout_abbreviation(token, th) {
                return Some(d);
            }
            detect::detect(token, en, th)
                .or_else(|| us_layout_thai_with_punctuation(token, en, th))
                .filter(|d| !only_short_thai_words(&d.corrected, th))
                // A known English word with a prefix or suffix stays English,
                // unless its Thai reading is itself a Thai word (กำหนด is
                // `desof`, de + sof).
                .filter(|d| !english::is_affixed(token, en) || th.contains(&d.corrected))
        }
        InputLayout::ThaiKedmanee => {
            if !has_thai || has_latin {
                return None;
            }
            // Thai text on screen is Thai, whatever its keys spell.
            let is_thai = th.contains(token)
                || segment::is_fully_known(token, th)
                || crate::abbrev::reads_as_abbreviation(token, th);
            (!is_thai && fixes_addresses())
                .then(|| thai_layout_address(token).or_else(|| thai_layout_number(token, th)))
                .flatten()
                .or_else(|| detect::detect(token, en, th))
                .or_else(|| thai_layout_compound(token, th))
                .or_else(|| thai_layout_technical(token, en, th))
                .or_else(|| thai_layout_trailing_mark(token, th))
        }
    }
}

/// A Thai abbreviation typed on the English layout: Kedmanee's period is the
/// `"` key, so ก.ค. comes in as `d"8"` and ดร.สมชาย as `fi"l,=kp`. Only a
/// reading made of known abbreviations ([`crate::abbrev`]) counts, so an
/// English word before a closing quote (`it"`, ระ.) stays as typed.
fn us_layout_abbreviation(token: &str, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    if !token.contains('"') || token.starts_with('"') {
        return None;
    }
    let reading = crate::layout::en_to_th(token);
    crate::abbrev::reads_as_abbreviation(&reading, th).then_some(Detection {
        corrected: reading,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
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
/// An email or web address typed with the Thai keyboard on: `@` is `๑` and
/// `.` is `ใ` there, so `name@gmail.com` shows as `ืฟทำ๑เทฟรสใแนท`. Judged by
/// the keys pressed, not the dictionary: the whole token, read as English
/// keys, is an address.
fn thai_layout_address(token: &str) -> Option<Detection> {
    let keys = crate::layout::th_to_en(token.trim());
    let address = english::is_email(&keys) || english::is_web_address(&keys);
    address.then_some(Detection {
        corrected: keys,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
}

/// A number typed with the Thai keyboard on: the number row gives
/// `ๅ / - ภ ถ ุ ึ ค ต จ`, so `100` shows as `ๅจจ` and `10:30` as `ๅจซ-จ`.
/// The keys must make a number — digits (at least two), with `, . : / -`
/// between groups, and an optional `%`, `$` or `฿` — and the screen must
/// not be a Thai word.
fn thai_layout_number(token: &str, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    let keys = crate::layout::th_to_en(token);
    let core = keys
        .strip_prefix(['$', '฿'])
        .unwrap_or(&keys)
        .strip_suffix('%')
        .unwrap_or_else(|| keys.strip_prefix(['$', '฿']).unwrap_or(&keys));
    let digits = core.chars().filter(char::is_ascii_digit).count();
    let shaped = !core.is_empty()
        && core.starts_with(|c: char| c.is_ascii_digit())
        && core.ends_with(|c: char| c.is_ascii_digit())
        && core
            .chars()
            .all(|c| c.is_ascii_digit() || ",.:/-".contains(c))
        && !core.contains(",,")
        && !core.contains("..");
    (digits >= 2 && shaped && !th.contains(token)).then_some(Detection {
        corrected: keys,
        confidence: Confidence::High,
        evidence: Evidence::ExactDictionary,
    })
}

fn thai_layout_trailing_mark(token: &str, th: &Dictionary) -> Option<Detection> {
    let token = token.trim();
    let last = token.chars().last()?;
    // Whatever the Thai layout: the character on the key that gives `:` or
    // `?` in English (ซ and ฦ on Kedmanee).
    if !('\u{0E00}'..='\u{0E7F}').contains(&last) {
        return None;
    }
    let mark = match th_to_en(&last.to_string()).as_str() {
        ":" => ':',
        "?" => '?',
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

/// A Thai word typed in an order that looks right but is not: two เ for แ,
/// nikhahit + า for ำ, a tone mark before the vowel above or below it, the
/// same mark twice. Search and word breaking miss such words. `Some` with the
/// word put right only when that is a Thai dictionary word (one word, not a
/// split of several: random keys can split into short words).
pub fn thai_spelling(word: &str, th: &Dictionary) -> Option<String> {
    const TONES: &[char] = &['\u{0E48}', '\u{0E49}', '\u{0E4A}', '\u{0E4B}', '\u{0E4C}'];
    const ABOVE_BELOW: &[char] = &[
        '\u{0E31}', '\u{0E34}', '\u{0E35}', '\u{0E36}', '\u{0E37}', '\u{0E38}', '\u{0E39}',
    ];
    if word.is_empty() || !word.chars().all(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        return None;
    }
    let mut c: Vec<char> = word.replace("เเ", "แ").chars().collect();
    let mut i = 0;
    while i + 1 < c.len() {
        if c[i] == '\u{0E4D}' && c[i + 1] == '\u{0E32}' {
            // ํ + า is ำ (a tone mark may sit between: นํ้า).
            c[i] = '\u{0E33}';
            c.remove(i + 1);
        } else if c[i] == '\u{0E4D}'
            && i + 2 < c.len()
            && TONES.contains(&c[i + 1])
            && c[i + 2] == '\u{0E32}'
        {
            let tone = c[i + 1];
            c[i] = tone;
            c[i + 1] = '\u{0E33}';
            c.remove(i + 2);
        } else if TONES.contains(&c[i]) && ABOVE_BELOW.contains(&c[i + 1]) {
            c.swap(i, i + 1);
        } else if c[i] == c[i + 1] && (TONES.contains(&c[i]) || ABOVE_BELOW.contains(&c[i])) {
            c.remove(i + 1);
            continue;
        }
        i += 1;
    }
    let fixed: String = c.into_iter().collect();
    (fixed != word && th.contains(&fixed)).then_some(fixed)
}

/// A word typed with CapsLock left on by accident, as the typist meant it
/// (`intended` is the word as if CapsLock were off), or `None`.
///
/// - English: Shift on the first letter and lower case after it (`Hello`,
///   shown as `hELLO`) — nobody means that; `HELLO` in capitals may be meant
///   and is left alone.
/// - Thai has no capitals: on the Thai layout CapsLock shifts every key
///   (`สวัสดี` comes out `ศซํศโ๊`), so a word whose keys spell known Thai is
///   put right.
pub fn caps_accident(intended: &str, layout: InputLayout, th: &Dictionary) -> Option<String> {
    let mut chars = intended.chars();
    let ok = match layout {
        InputLayout::UsQwerty => {
            chars.next().is_some_and(|c| c.is_ascii_uppercase())
                && intended.chars().count() >= 2
                && chars.all(|c| c.is_ascii_lowercase())
        }
        InputLayout::ThaiKedmanee => {
            !intended.is_empty()
                && intended
                    .chars()
                    .all(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
                && crate::segment::segment(intended, th)
                    .iter()
                    .all(|s| s.known)
        }
    };
    ok.then(|| intended.to_string())
}

/// `keys` as the English layout shows them with CapsLock on: letters in the
/// other case. RightType keeps the keys as if CapsLock were off, so a Thai
/// word typed with CapsLock left on still reads as Thai; text it puts back
/// "as typed" is shown this way.
pub fn shown_with_caps(keys: &str, caps: bool) -> String {
    if !caps {
        return keys.to_string();
    }
    keys.chars()
        .map(|c| {
            if c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c.to_ascii_uppercase()
            }
        })
        .collect()
}

/// The boundary decision for a token RightType itself converted to Thai while
/// it was being typed (D-009).
///
/// Once a run is anchored the layout is switched and the rest of the word
/// arrives as native Thai, so the finished token on screen is entirely Thai.
/// The typist started it on the English layout, though, and only now is the
/// whole token visible. If its keystrokes spell English — a dictionary word, a
/// learned word or a compound — the early reading was wrong and the *whole*
/// token goes back, not just the part typed after the anchor. So does a
/// token that cannot end as Thai ([`is_unfinished_thai`]), and one whose keys
/// are a known English word with a prefix or suffix ([`english::is_affixed`])
/// unless the Thai itself is a dictionary word.
pub fn revise_converted(token: &str, en: &Dictionary, th: &Dictionary) -> Option<Detection> {
    let raw = th_to_en(token.trim());
    // Trailing sentence punctuation may follow a word (`middleware,`), but on
    // this layout most ASCII punctuation is a Thai letter (`[` is บ, `;` is น),
    // so only a short trailing run is set aside and the rest must be letters.
    let core = raw.trim_end_matches(|c: char| c.is_ascii_punctuation());
    if raw.len() - core.len() > 2
        || core.chars().count() < 3
        || !core.chars().all(|c| c.is_ascii_alphabetic())
        || !(english::is_word(core, en)
            || is_unfinished_thai(token.trim())
            || (english::is_affixed(core, en) && !th.contains(token.trim())))
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

/// Is `thai` nothing but three or more known words of one or two letters?
///
/// Thai has many such words (พำ, สน, ฟ, อ, ร, …), so almost any English
/// letters read as a chain of them for a while: `reavi` is พำ + ฟ + อ + ร.
/// That alone is too little evidence to rewrite a word, mid-way or at its
/// boundary. Measured on 20,000 unknown
/// English and 20,000 unknown Thai words (`examples/live_thai_study.rs`):
/// English wrongly turned Thai drops from 246 to 170, Thai kept from 5,502
/// to 5,501.
pub fn only_short_thai_words(thai: &str, th: &Dictionary) -> bool {
    let segs = segment::segment(thai, th);
    segs.len() >= 3 && segs.iter().all(|s| s.known && s.text.chars().count() <= 2)
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
/// Thai that cannot be the end of a word: it ends in a leading vowel
/// (เ แ โ ใ ไ) or in a vowel that must be followed by a consonant (ั ึ ื),
/// tone marks aside.
///
/// While a word is typed its Thai reading only has to be a viable start of
/// Thai, so an English word RightType does not know can pass for Thai until
/// its last keys: `relogi` reads as three known Thai words (พำ + สน + เร),
/// and `relogin` then ends in a bare ื (พำสนเรื). At the boundary such a
/// reading goes back to the keys as typed. A Thai word that merely is not in
/// the dictionary (a name, a typo) is still well formed and stays Thai.
pub fn is_unfinished_thai(text: &str) -> bool {
    let core = text.trim_end_matches(|c| ('\u{0E48}'..='\u{0E4B}').contains(&c));
    matches!(
        core.chars().last(),
        Some('เ' | 'แ' | 'โ' | 'ใ' | 'ไ' | '\u{0E31}' | '\u{0E36}' | '\u{0E37}')
    )
}

/// At a boundary: whether a run shown as Thai while it was typed goes back to
/// the keys. Only when its reading cannot end as Thai
/// ([`is_unfinished_thai`]) and every key was a letter, as in an English word
/// (Thai typed on the English layout nearly always uses punctuation keys too,
/// which carry ง ว น บ ล ใ ม ฝ ฃ).
pub fn goes_back_to_keys(keys: &str, reading: &str) -> bool {
    is_unfinished_thai(reading) && keys.chars().all(|c| c.is_ascii_alphabetic())
}

/// At a boundary, with the dictionaries: [`goes_back_to_keys`], or the keys
/// make a known English word with a prefix or suffix ([`english::is_affixed`]:
/// `rerise`, shown as Thai since `reris`) and the reading is not itself a Thai
/// word.
pub fn run_goes_back(keys: &str, reading: &str, en: &Dictionary, th: &Dictionary) -> bool {
    goes_back_to_keys(keys, reading) || (english::is_affixed(keys, en) && !th.contains(reading))
}

/// Auto, mid-word, before anything is rewritten: the Thai the keys typed so
/// far are heading for, to show next to the cursor (`l;ylf` → `สวัสด`), or
/// `None`.
///
/// Auto rewrites a word only once it is sure (see [`live_reading`]), which
/// takes a few keys; until then the typist sees English and cannot tell
/// whether a fix is coming. The preview says so without touching the text.
/// It needs three keys or more that all turn into Thai and start real Thai
/// words, and is not shown for keys that are (or can still become) an
/// English word, a secret's shape, or once Auto is already rewriting.
pub fn preview(run: &str, en: &Dictionary, th: &Dictionary) -> Option<String> {
    if run.chars().count() < 3
        || english::is_word(run, en)
        || en.has_extension(run)
        || secret::classify_token(run).is_some()
        || matches!(live_reading(run, false, en, th), Reading::Thai(_))
    {
        return None;
    }
    let thai = en_to_th(run);
    let all_thai = thai.chars().all(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    (all_thai && segment::is_viable_prefix(&thai, th)).then_some(thai)
}

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

    #[test]
    fn a_preview_shows_where_the_keys_are_heading() {
        let (en, th) = (crate::dict::english(), crate::dict::thai());
        // On the way to สวัสดี, before Auto is sure.
        assert_eq!(preview("l;yl", en, th).as_deref(), Some("สวัส"));
        // Too short to say; English (or its beginning); Auto already on it.
        assert_eq!(preview("l;", en, th), None);
        assert_eq!(preview("hell", en, th), None);
        assert_eq!(preview("hello", en, th), None);
        assert_eq!(preview("l;ylfu", en, th), None);
    }

    fn dicts() -> (Dictionary, Dictionary) {
        (
            Dictionary::from_words(["correct", "hello"]),
            Dictionary::from_words(["สวัสดี"]),
        )
    }

    #[test]
    fn thai_that_cannot_end_a_word() {
        // A bare ื, ึ or ั, or a leading vowel, cannot close a word.
        assert!(is_unfinished_thai("พำสนเรื"));
        assert!(is_unfinished_thai("กั"));
        assert!(is_unfinished_thai("ขึ้"));
        assert!(is_unfinished_thai("สวัสดีเ"));
        // Well-formed words, known or not, can.
        for word in ["สวัสดี", "เรือ", "ก็", "จันทร์", "ทำ", "ศุภวิชญ์"]
        {
            assert!(!is_unfinished_thai(word), "{word}");
        }
        // Only when every key was a letter: Thai typed on the English layout
        // usually needs punctuation keys too.
        assert!(goes_back_to_keys("relogin", "พำสนเรื"));
        assert!(!goes_back_to_keys("l;ylf", "สวัสด"));
        assert!(!goes_back_to_keys("relogin", "พำสนเรือ"));
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
        use crate::layout::EnglishVariant::{Uk, Us};
        // UK English, and English of other countries on the US keyboard.
        assert_eq!(
            supported_layout_id(0x0000_0809),
            Some(InputLayout::UsQwerty)
        );
        assert_eq!(english_variant_of(0x0809_0809), Some(Uk));
        // Australia (on the US keyboard, and its own default).
        assert_eq!(english_variant_of(0x0409_0C09), Some(Us));
        assert_eq!(english_variant_of(0x0C09_0C09), Some(Us));
        // Canada set to the US keyboard; its default may be Canadian French.
        assert_eq!(english_variant_of(0x0409_1009), Some(Us));
        assert_eq!(english_variant_of(0x1009_1009), None);
        // US Dvorak, and Japanese.
        assert_eq!(supported_layout_id(0xF002_0409), None);
        assert_eq!(supported_layout_id(0x0000_0411), None);
        // Thai variants only once the user has chosen Pattachote.
        assert_eq!(supported_layout_id(0x0001_041E), None);
        assert_eq!(supported_layout_id(0xF001_041E), None);
        // And then only they: Kedmanee is never read through Pattachote,
        // including Kedmanee without ShiftLock (also not the default).
        use crate::layout::ThaiVariant::{Kedmanee, Pattachote};
        let known = [
            (0xF001, Pattachote),
            (0xF002, Kedmanee),
            (0xF003, Pattachote),
        ];
        assert_eq!(thai_table_of(0x041E_041E, &known), Some(Kedmanee));
        assert_eq!(thai_table_of(0x0000_041E, &known), Some(Kedmanee));
        assert_eq!(thai_table_of(0xF001_041E, &known), Some(Pattachote));
        assert_eq!(thai_table_of(0xF002_041E, &known), Some(Kedmanee));
        assert_eq!(thai_table_of(0xF003_041E, &known), Some(Pattachote));
        assert_eq!(thai_table_of(0xF00F_041E, &known), None);
        // Layout files unreadable: no guessing, RightType stays off.
        assert_eq!(thai_table_of(0xF001_041E, &[]), None);
        assert_eq!(thai_table_of(0x0000_041E, &[]), Some(Kedmanee));
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

    #[test]
    fn addresses_typed_on_the_thai_keyboard_come_back() {
        let (en, th) = (crate::dict::english(), crate::dict::thai());
        for address in [
            "name@gmail.com",
            "www.google.co.th",
            "https://chula.ac.th",
            "example.com",
        ] {
            let typed = crate::layout::en_to_th(address);
            let d = detect_token(&typed, InputLayout::ThaiKedmanee, en, th);
            assert_eq!(d.map(|d| d.corrected).as_deref(), Some(address), "{typed}");
        }
        // Numbers come back as typed on the number row.
        for number in [
            "100",
            "1,250",
            "3.50",
            "100%",
            "10:30",
            "2/10",
            "081-234-5678",
            "$5.99",
        ] {
            let typed = crate::layout::en_to_th(number);
            let d = detect_token(&typed, InputLayout::ThaiKedmanee, en, th);
            assert_eq!(d.map(|d| d.corrected).as_deref(), Some(number), "{typed}");
        }
        // One digit, or a Thai word made of number-row letters, stays.
        for word in [
            crate::layout::en_to_th("1"),
            "ภาค".to_string(),
            "จุด".to_string(),
        ] {
            assert!(thai_layout_number(&word, th).is_none(), "{word}");
        }
        // No Thai dictionary word reads as an address or a number (the
        // other Thai keyboards: examples/false_positive_audit.rs).
        for word in include_str!("../assets/th_words.txt").lines() {
            let d = detect_token(word, InputLayout::ThaiKedmanee, en, th);
            let keys = d.map(|d| d.corrected).unwrap_or_default();
            assert!(
                !crate::english::is_email(&keys) && !crate::english::is_web_address(&keys),
                "{word} -> {keys}"
            );
        }
        for word in include_str!("../assets/th_words.txt").lines() {
            assert!(thai_layout_address(word).is_none(), "{word}");
        }
        // Thai words stay Thai.
        for word in ["สวัสดี", "ใจ", "เกม", "ใน"] {
            let d = detect_token(word, InputLayout::ThaiKedmanee, en, th);
            assert!(d.is_none(), "{word}");
        }
    }
}
