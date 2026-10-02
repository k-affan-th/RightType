//! Snippets: a short trigger typed, then a space, becomes a longer text.
//!
//! `;addr` + Space → the whole address. Each snippet says which keyboard it
//! works on:
//!
//! - **Thai** or **English**: only when that keyboard is on, matched by the
//!   letters typed.
//! - **Either**: matched by the *keys* pressed, so `;addr` works even when the
//!   Thai keyboard is on and the screen shows `;ฟกกพ` — the trigger is the
//!   same keys either way.
//!
//! A trigger is 2–32 characters without spaces; starting it with `;` keeps it
//! from ever being a word typed for itself. Snippets are written by the
//! typist in Settings and saved with the settings; nothing typed is saved.

use crate::layout::{en_to_th, th_to_en};
use crate::policy::InputLayout;

/// Which keyboard a snippet works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Thai,
    English,
    Either,
}

impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Scope::Thai => "thai",
            Scope::English => "english",
            Scope::Either => "either",
        }
    }

    pub fn parse(s: &str) -> Option<Scope> {
        match s.trim().to_ascii_lowercase().as_str() {
            "thai" => Some(Scope::Thai),
            "english" => Some(Scope::English),
            "either" | "both" => Some(Scope::Either),
            _ => None,
        }
    }
}

/// One snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub trigger: String,
    pub text: String,
    pub scope: Scope,
}

/// The most snippets kept, and the longest text.
pub const MAX_SNIPPETS: usize = 200;
pub const MAX_TEXT: usize = 1000;

/// Why a snippet cannot be saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    TriggerLength,
    TriggerSpace,
    TextEmpty,
    TextLong,
}

/// Check (and tidy) a snippet before it is saved: the trigger trimmed, line
/// breaks in the text kept as `\n`.
pub fn check(trigger: &str, text: &str, scope: Scope) -> Result<Snippet, Problem> {
    let trigger = trigger.trim();
    let n = trigger.chars().count();
    if !(2..=32).contains(&n) {
        return Err(Problem::TriggerLength);
    }
    if trigger.chars().any(char::is_whitespace) {
        return Err(Problem::TriggerSpace);
    }
    let text = text.replace("\r\n", "\n");
    if text.trim().is_empty() {
        return Err(Problem::TextEmpty);
    }
    if text.chars().count() > MAX_TEXT {
        return Err(Problem::TextLong);
    }
    Ok(Snippet {
        trigger: trigger.to_string(),
        text,
        scope,
    })
}

/// A string as the English keys that type it: Thai letters become the keys
/// they are on (`;ฟกกพ` → `;addr`), the rest stays.
fn as_keys(s: &str) -> String {
    if s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        th_to_en(s)
    } else {
        s.to_string()
    }
}

/// The snippet for `typed`, the word just finished with `layout` on (as the
/// keyboard put it on screen).
pub fn find<'a>(list: &'a [Snippet], typed: &str, layout: InputLayout) -> Option<&'a Snippet> {
    if typed.chars().count() < 2 {
        return None;
    }
    let keys = match layout {
        InputLayout::ThaiKedmanee => th_to_en(typed),
        InputLayout::UsQwerty => typed.to_string(),
    };
    list.iter().find(|s| match s.scope {
        Scope::Thai => {
            layout == InputLayout::ThaiKedmanee
                && (s.trigger == typed || en_to_th(&s.trigger) == typed)
        }
        Scope::English => layout == InputLayout::UsQwerty && as_keys(&s.trigger) == typed,
        Scope::Either => as_keys(&s.trigger) == keys,
    })
}

/// The moment a snippet is typed, for its date and time fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Now {
    /// Common Era.
    pub year: u32,
    /// 1–12.
    pub month: u32,
    pub day: u32,
    /// 0 = Sunday … 6 = Saturday.
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

const MONTHS: [&str; 12] = [
    "มกราคม",
    "กุมภาพันธ์",
    "มีนาคม",
    "เมษายน",
    "พฤษภาคม",
    "มิถุนายน",
    "กรกฎาคม",
    "สิงหาคม",
    "กันยายน",
    "ตุลาคม",
    "พฤศจิกายน",
    "ธันวาคม",
];
const MONTHS_SHORT: [&str; 12] = [
    "ม.ค.",
    "ก.พ.",
    "มี.ค.",
    "เม.ย.",
    "พ.ค.",
    "มิ.ย.",
    "ก.ค.",
    "ส.ค.",
    "ก.ย.",
    "ต.ค.",
    "พ.ย.",
    "ธ.ค.",
];
const MONTHS_EN: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const WEEKDAYS: [&str; 7] = ["อาทิตย์", "จันทร์", "อังคาร", "พุธ", "พฤหัสบดี", "ศุกร์", "เสาร์"];

/// The date and time fields a snippet's text can hold, each with its Thai
/// and English name and what it gives (2 October 2026, 14:30, as an
/// example). Settings lists them to insert.
pub const FIELDS: &[(&str, &str, &str)] = &[
    ("{วันที่}", "{date-th}", "2 ตุลาคม 2569"),
    ("{วันที่เต็ม}", "{date-th-long}", "วันพฤหัสบดีที่ 2 ตุลาคม พ.ศ. 2569"),
    ("{วันที่ย่อ}", "{date-th-short}", "2 ต.ค. 69"),
    ("{วันที่เลขไทย}", "{date-th-digits}", "๒ ตุลาคม ๒๕๖๙"),
    ("{วันที่ตัวเลข}", "{date-th-num}", "02/10/2569"),
    ("{เวลา}", "{time-th}", "14.30 น."),
    ("{date}", "{date-en}", "2 October 2026"),
    ("{date-us}", "{date-us}", "October 2, 2026"),
    ("{iso}", "{date-iso}", "2026-10-02"),
    ("{time}", "{time-24}", "14:30"),
];

/// What one field gives at `now`, or `None` for a name that is not a field.
fn field(name: &str, now: &Now) -> Option<String> {
    let be = now.year + 543;
    let m = (now.month.clamp(1, 12) - 1) as usize;
    let thai_digits = |s: String| crate::layout::swap_digits(&s);
    Some(match name {
        "{วันที่}" | "{date-th}" => format!("{} {} {be}", now.day, MONTHS[m]),
        "{วันที่เต็ม}" | "{date-th-long}" => format!(
            "วัน{}ที่ {} {} พ.ศ. {be}",
            WEEKDAYS[(now.weekday % 7) as usize],
            now.day,
            MONTHS[m]
        ),
        "{วันที่ย่อ}" | "{date-th-short}" => {
            format!("{} {} {:02}", now.day, MONTHS_SHORT[m], be % 100)
        }
        "{วันที่เลขไทย}" | "{date-th-digits}" => {
            thai_digits(format!("{} {} {be}", now.day, MONTHS[m]))
        }
        "{วันที่ตัวเลข}" | "{date-th-num}" => {
            format!("{:02}/{:02}/{be}", now.day, now.month)
        }
        "{เวลา}" | "{time-th}" => format!("{:02}.{:02} น.", now.hour, now.minute),
        "{date}" | "{date-en}" => format!("{} {} {}", now.day, MONTHS_EN[m], now.year),
        "{date-us}" => format!("{} {}, {}", MONTHS_EN[m], now.day, now.year),
        "{iso}" | "{date-iso}" => format!("{}-{:02}-{:02}", now.year, now.month, now.day),
        "{time}" | "{time-24}" => format!("{:02}:{:02}", now.hour, now.minute),
        _ => return None,
    })
}

/// A snippet's text with its date and time fields filled in for `now`.
/// Braces that do not name a field are kept as written.
pub fn fill(text: &str, now: &Now) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open..];
        match after.find('}').and_then(|close| {
            let name = &after[..=close];
            field(name, now).map(|v| (v, close))
        }) {
            Some((value, close)) => {
                out.push_str(&value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Vec<Snippet> {
        vec![
            check(";addr", "99 ถนนสุขุมวิท กรุงเทพฯ", Scope::Either).unwrap(),
            check("ขอบคุณ", "ขอบคุณมากครับ", Scope::Thai).unwrap(),
            check(";sig", "Best regards,\nSomchai", Scope::English).unwrap(),
        ]
    }

    #[test]
    fn either_matches_the_keys_on_both_keyboards() {
        let l = list();
        let on_thai = en_to_th(";addr");
        assert_eq!(
            find(&l, ";addr", InputLayout::UsQwerty).map(|s| s.text.as_str()),
            Some("99 ถนนสุขุมวิท กรุงเทพฯ")
        );
        assert_eq!(
            find(&l, &on_thai, InputLayout::ThaiKedmanee).map(|s| s.trigger.as_str()),
            Some(";addr")
        );
    }

    #[test]
    fn a_keyboard_scope_is_kept() {
        let l = list();
        assert!(find(&l, "ขอบคุณ", InputLayout::ThaiKedmanee).is_some());
        // The same keys on the English keyboard are not it.
        assert!(find(&l, &th_to_en("ขอบคุณ"), InputLayout::UsQwerty).is_none());
        assert!(find(&l, ";sig", InputLayout::UsQwerty).is_some());
        assert!(find(&l, &en_to_th(";sig"), InputLayout::ThaiKedmanee).is_none());
        assert!(find(&l, "hello", InputLayout::UsQwerty).is_none());
    }

    #[test]
    fn snippets_are_checked() {
        assert_eq!(check("a", "x", Scope::Either), Err(Problem::TriggerLength));
        assert_eq!(check("a b", "x", Scope::Either), Err(Problem::TriggerSpace));
        assert_eq!(check(";a", "  ", Scope::Either), Err(Problem::TextEmpty));
        let long = "x".repeat(MAX_TEXT + 1);
        assert_eq!(check(";a", &long, Scope::Either), Err(Problem::TextLong));
        let s = check(" ;a ", "one\r\ntwo", Scope::Thai).unwrap();
        assert_eq!((s.trigger.as_str(), s.text.as_str()), (";a", "one\ntwo"));
        assert_eq!(Scope::parse("Both"), Some(Scope::Either));
        assert_eq!(Scope::parse(Scope::Thai.name()), Some(Scope::Thai));
    }

    #[test]
    fn date_and_time_fields_are_filled() {
        let now = Now {
            year: 2026,
            month: 10,
            day: 2,
            weekday: 4,
            hour: 14,
            minute: 5,
        };
        assert_eq!(fill("{วันที่}", &now), "2 ตุลาคม 2569");
        assert_eq!(fill("{วันที่เต็ม}", &now), "วันพฤหัสบดีที่ 2 ตุลาคม พ.ศ. 2569");
        assert_eq!(fill("{วันที่ย่อ}", &now), "2 ต.ค. 69");
        assert_eq!(fill("{วันที่เลขไทย}", &now), "๒ ตุลาคม ๒๕๖๙");
        assert_eq!(fill("{วันที่ตัวเลข}", &now), "02/10/2569");
        assert_eq!(fill("เวลา {เวลา}", &now), "เวลา 14.05 น.");
        assert_eq!(fill("{date}", &now), "2 October 2026");
        assert_eq!(fill("{date-us}", &now), "October 2, 2026");
        assert_eq!(fill("{iso}", &now), "2026-10-02");
        assert_eq!(fill("{time}", &now), "14:05");
        // English names work too; other braces stay as written.
        assert_eq!(fill("{date-th}", &now), "2 ตุลาคม 2569");
        assert_eq!(fill("{x} {} {วันที่", &now), "{x} {} {วันที่");
        assert_eq!(fill("no fields", &now), "no fields");
        // Every listed field gives its example at 2 October 2026, 14:30.
        let ex = Now { minute: 30, ..now };
        for (th, en, example) in FIELDS {
            assert_eq!(fill(th, &ex), *example, "{th}");
            assert_eq!(fill(en, &ex), *example, "{en}");
        }
    }
}
