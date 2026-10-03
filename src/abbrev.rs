//! Thai abbreviations (อักษรย่อ): ก.ค., ส.ส., ดร., กทม. — a few Thai
//! letters, each group ending with a period. The dictionary has words, not
//! these, so a word typed on the English layout as `d"8"` (Kedmanee's period
//! is the `"` key) was left as typed, quote marks and all.
//!
//! [`reads_as_abbreviation`] says whether a Thai reading is made of known
//! abbreviations, optionally followed by a word (or, after a title or a
//! place word, a name: ดร.สมชาย, ถ.สุขุมวิท). No OS calls.

use std::sync::OnceLock;

use crate::dict::Dictionary;

const LIST: &str = include_str!("../assets/th_abbrev.txt");

/// An abbreviation, and whether a name may follow it.
struct Abbrev {
    text: &'static str,
    name_follows: bool,
}

fn all() -> &'static [Abbrev] {
    static ALL: OnceLock<Vec<Abbrev>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut v: Vec<Abbrev> = LIST
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| match l.strip_suffix('!') {
                Some(text) => Abbrev {
                    text,
                    name_follows: true,
                },
                None => Abbrev {
                    text: l,
                    name_follows: false,
                },
            })
            .collect();
        // Longest first: พ.ต.อ. before พ.ต.
        v.sort_by_key(|a| std::cmp::Reverse(a.text.len()));
        v
    })
}

/// Is `text` one known abbreviation (`ก.ค.`)?
pub fn is_abbreviation(text: &str) -> bool {
    all().iter().any(|a| a.text == text)
}

fn is_thai_letter(c: char) -> bool {
    ('\u{0E01}'..='\u{0E4E}').contains(&c) && c != '\u{0E3F}'
}

/// Could `c` start a Thai word: a consonant, or a vowel written before one.
fn starts_word(c: char) -> bool {
    ('\u{0E01}'..='\u{0E2E}').contains(&c) || ('\u{0E40}'..='\u{0E44}').contains(&c)
}

/// Is `reading` one or more known abbreviations, then nothing, a Thai word,
/// or — after a title or a place word — a name? `ก.ค.`, `ศ.ดร.`,
/// `ป.ป.ช.`, `ดร.สมชาย`; not `ระ.` (`it"`), not `ก.ค` (no last period).
pub fn reads_as_abbreviation(reading: &str, th: &Dictionary) -> bool {
    let mut rest = reading;
    let mut last: Option<&Abbrev> = None;
    while let Some(a) = all().iter().find(|a| rest.starts_with(a.text)) {
        rest = &rest[a.text.len()..];
        last = Some(a);
    }
    let Some(last) = last else {
        return false;
    };
    if rest.is_empty() {
        // One short abbreviation alone (นพ., ดร., ป.) is an English word
        // before a closing quote as often (`or"`, `fi"`, `x"`): it needs
        // two periods (ก.ค.), three letters (กทม.), or a name after it.
        let periods = reading.matches('.').count();
        return periods >= 2 || reading.chars().count() >= 4;
    }
    if !rest.chars().all(is_thai_letter) || !rest.chars().next().is_some_and(starts_word) {
        return false;
    }
    th.contains(rest)
        || crate::segment::is_fully_known(rest, th)
        || (last.name_follows && rest.chars().count() >= 2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    #[test]
    fn abbreviations_and_what_follows_them() {
        let th = dict::thai();
        for ok in [
            "ก.ค.",
            "ส.ส.",
            "พ.ศ.",
            "ศ.ดร.",
            "ป.ป.ช.",
            "พ.ต.อ.",
            "ดร.สมชาย",
            "ถ.สุขุมวิท",
            "กทม.",
        ] {
            assert!(reads_as_abbreviation(ok, th), "{ok}");
        }
        for no in [
            "ระ.",
            "ก.ค",
            ".ก.ค.",
            "ก.ค.x",
            "ส.ส.ุ",
            "",
            "กข.",
            "นพ.",
            "ดร.",
            "ป.",
        ] {
            assert!(!reads_as_abbreviation(no, th), "{no}");
        }
        assert!(is_abbreviation("ม.ค."));
        // Typed on the English layout, live and in Fix text alike.
        let en = dict::english();
        for (typed, thai) in [
            ("d\"8\"", "ก.ค."),
            ("l\"l\"", "ส.ส."),
            ("fi\"l,=kp", "ดร.สมชาย"),
            ("dm,\"", "กทม."),
        ] {
            let d =
                crate::policy::detect_token(typed, crate::policy::InputLayout::UsQwerty, en, th);
            assert_eq!(d.map(|d| d.corrected).as_deref(), Some(thai), "{typed}");
        }
        assert!(
            crate::policy::detect_token("it\"", crate::policy::InputLayout::UsQwerty, en, th)
                .is_none()
        );
        assert!(!is_abbreviation("ม.ค"));
    }
}
