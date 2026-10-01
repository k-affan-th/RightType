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
}
