//! Code mode: what to fix in a code editor.
//!
//! Code is written in one language at a time. Names, keywords and operators
//! are English; Thai turns up only in comments and string literals. So in a
//! code editor the two directions are judged differently:
//!
//! - **Typed on the Thai keyboard, meant as code** (`ฟหำแ` for `asdf`): code
//!   is English, so the keys are put back as English even when the result is
//!   not a dictionary word — a name rarely is — as long as it has the shape
//!   of a name and the Thai is not Thai words. Not inside a comment or a
//!   string, where Thai is at home. Where RightType cannot tell, it offers
//!   the English as a hint instead.
//! - **Typed on the English keyboard, meant as Thai**: only inside a comment
//!   or a string. In code itself the English stays, whatever it looks like.
//!   When RightType cannot read the line (many editors do not share their
//!   text), it offers the Thai as a hint instead of writing it.
//! - **Names are never touched**: `camelCase`, `snake_case`, `CONSTANT`, and
//!   anything with digits or `_ . :: ->` in it is code by its shape.
//! - **CapsLock is not an accident** here: `MAX_SIZE` is meant.
//!
//! Where the caret is ([`line_is_prose`]) is read from the text before it on
//! the same line: after `//`, `#`, `--`, inside `/* … */`, or inside quotes.

use crate::dict::Dictionary;
use crate::policy::InputLayout;

/// What Code mode does with a word another mode would fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Put it right, as Auto would.
    Fix(String),
    /// Offer it (Tab takes it), as Suggest would.
    Hint(String),
    /// Leave it as typed.
    Leave,
}

/// Decide for one completed word.
///
/// - `word`: the word as typed (Thai characters on the Thai keyboard).
/// - `detected`: what the ordinary detector would put instead, if anything.
/// - `prose`: whether the caret is in a comment or string (`None`: unknown).
pub fn verdict(
    word: &str,
    layout: InputLayout,
    detected: Option<&str>,
    prose: Option<bool>,
    th: &Dictionary,
) -> Verdict {
    match layout {
        InputLayout::ThaiKedmanee => {
            if prose == Some(true) {
                // Thai belongs in a comment: only what the detector is sure of.
                return detected.map_or(Verdict::Leave, |d| Verdict::Fix(d.to_string()));
            }
            if let Some(d) = detected {
                return Verdict::Fix(d.to_string());
            }
            let english = crate::layout::th_to_en(word);
            let thai_words = crate::segment::is_fully_known(word, th);
            if !thai_words && is_name(&english) && english.chars().count() >= 2 {
                // Not sure it is code (the editor does not share its text):
                // a Thai name in a string is possible, so offer it.
                if prose.is_none() {
                    Verdict::Hint(english)
                } else {
                    Verdict::Fix(english)
                }
            } else {
                Verdict::Leave
            }
        }
        InputLayout::UsQwerty => {
            let Some(d) = detected else {
                return Verdict::Leave;
            };
            if looks_like_identifier(word) {
                return Verdict::Leave;
            }
            match prose {
                Some(true) => Verdict::Fix(d.to_string()),
                Some(false) => Verdict::Leave,
                None => Verdict::Hint(d.to_string()),
            }
        }
    }
}

/// Has `s` the shape of a name in code: a letter or `_` first, then letters,
/// digits and `_` (a member access `a.b` counts).
pub fn is_name(s: &str) -> bool {
    let mut parts = s.split('.');
    parts.all(|p| {
        let mut chars = p.chars();
        chars
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

/// English that is code by its shape alone: `camelCase`, `PascalCase` with a
/// second capital, `snake_case`, `CONSTANT` (two or more capitals and no
/// lower case), or anything with a digit or `_ . :: ->` in it.
pub fn looks_like_identifier(word: &str) -> bool {
    if !word.is_ascii() || word.is_empty() {
        return false;
    }
    if word.contains(['_', '.', ':', '>', '(', ')', '[', ']', '{', '}', '=', '<'])
        || word.chars().any(|c| c.is_ascii_digit())
    {
        return true;
    }
    let letters: Vec<char> = word.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    let upper = letters.iter().filter(|c| c.is_ascii_uppercase()).count();
    let lower = letters.len() - upper;
    // CONSTANT
    if upper >= 2 && lower == 0 {
        return true;
    }
    // camelCase / PascalCase: a capital after a lower-case letter.
    letters
        .windows(2)
        .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
}

/// Is the caret in a comment or a string literal, judging by `before`, the
/// text before it (only its last line matters)?
pub fn line_is_prose(before: &str) -> bool {
    let line = before.rsplit(['\n', '\r']).next().unwrap_or("");
    let trimmed = line.trim_start();
    // Inside a block comment, continued: ` * text`.
    if trimmed.starts_with("* ") || trimmed == "*" {
        return true;
    }
    let chars: Vec<char> = line.chars().collect();
    let mut quote: Option<char> = None;
    let mut block = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if block {
            if c == '*' && next == Some('/') {
                block = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = quote {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match (c, next) {
            ('/', Some('/')) => return true,
            ('/', Some('*')) => {
                block = true;
                i += 2;
                continue;
            }
            ('-', Some('-')) => return true,
            ('<', Some('!')) if chars[i..].starts_with(&['<', '!', '-', '-']) => return true,
            ('#', _) => return true,
            ('"' | '`', _) => quote = Some(c),
            // An apostrophe is a quote only where a string could start:
            // after an operator or space, not inside a word (`don't`).
            ('\'', _) if i == 0 || !chars[i - 1].is_ascii_alphanumeric() => quote = Some(c),
            _ => {}
        }
        i += 1;
    }
    block || quote.is_some()
}

/// Programs that write code, where Code mode is the default (the typist can
/// choose another mode for any of them).
pub const CODE_EDITORS: &[&str] = &[
    "code.exe",
    "code - insiders.exe",
    "cursor.exe",
    "windsurf.exe",
    "zed.exe",
    "devenv.exe",
    "idea64.exe",
    "pycharm64.exe",
    "webstorm64.exe",
    "clion64.exe",
    "goland64.exe",
    "rider64.exe",
    "phpstorm64.exe",
    "rubymine64.exe",
    "datagrip64.exe",
    "rustrover64.exe",
    "studio64.exe",
    "sublime_text.exe",
    "notepad++.exe",
    "eclipse.exe",
    "gvim.exe",
    "nvim-qt.exe",
];

/// Is `exe` (lower case) a code editor?
pub fn is_code_editor(exe: &str) -> bool {
    CODE_EDITORS.contains(&exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    #[test]
    fn comments_and_strings_are_prose() {
        assert!(line_is_prose("    // อธิบาย "));
        assert!(line_is_prose("x = 1  # note "));
        assert!(line_is_prose("let s = \"hello "));
        assert!(line_is_prose("const s = `tem"));
        assert!(line_is_prose("/* start "));
        assert!(line_is_prose(" * continued "));
        assert!(line_is_prose("SELECT 1 -- why "));
        assert!(line_is_prose("<!-- note "));
        assert!(line_is_prose("print('hi "));
        assert!(line_is_prose("fn a() {}\n// next line "));
    }

    #[test]
    fn code_is_not_prose() {
        assert!(!line_is_prose("let x = "));
        assert!(!line_is_prose("let s = \"done\"; let t = "));
        assert!(!line_is_prose("/* done */ let y = "));
        assert!(!line_is_prose("let s = \"a \\\" b\"; "));
        assert!(!line_is_prose("// old line\nlet z = "));
        assert!(!line_is_prose("fn it(&self) -> "));
        // An apostrophe inside a word is not a quote.
        assert!(!line_is_prose("dont't = "));
    }

    #[test]
    fn names_have_a_shape() {
        for w in [
            "getUserName",
            "snake_case",
            "MAX_SIZE",
            "HTTP",
            "x1",
            "a.b",
            "std::io",
            "a->b",
        ] {
            assert!(looks_like_identifier(w), "{w}");
        }
        for w in ["hello", "Hello", "correct", "I", "", "สวัสดี"] {
            assert!(!looks_like_identifier(w), "{w}");
        }
        assert!(is_name("asdf"));
        assert!(is_name("self.value"));
        assert!(is_name("_x9"));
        assert!(!is_name("9x"));
        assert!(!is_name("a-b"));
    }

    #[test]
    fn thai_keys_in_code_become_the_name_typed() {
        let th = dict::thai();
        // `asdf` typed on the Thai keyboard: not a word, but a name.
        let typed = crate::layout::en_to_th("asdf");
        assert_eq!(
            verdict(&typed, InputLayout::ThaiKedmanee, None, Some(false), th),
            Verdict::Fix("asdf".into())
        );
        assert_eq!(
            verdict(&typed, InputLayout::ThaiKedmanee, None, None, th),
            Verdict::Hint("asdf".into())
        );
        // In a comment Thai is at home: only a sure fix.
        assert_eq!(
            verdict(&typed, InputLayout::ThaiKedmanee, None, Some(true), th),
            Verdict::Leave
        );
        // Real Thai words are left alone even in code.
        assert_eq!(
            verdict("สวัสดี", InputLayout::ThaiKedmanee, None, Some(false), th),
            Verdict::Leave
        );
    }

    #[test]
    fn english_keys_become_thai_only_in_prose() {
        let th = dict::thai();
        let d = Some("สวัสดี");
        assert_eq!(
            verdict("l;ylfu", InputLayout::UsQwerty, d, Some(true), th),
            Verdict::Fix("สวัสดี".into())
        );
        assert_eq!(
            verdict("l;ylfu", InputLayout::UsQwerty, d, Some(false), th),
            Verdict::Leave
        );
        assert_eq!(
            verdict("l;ylfu", InputLayout::UsQwerty, d, None, th),
            Verdict::Hint("สวัสดี".into())
        );
        // A name is never Thai, wherever it is.
        assert_eq!(
            verdict("getUser", InputLayout::UsQwerty, Some("เ"), Some(true), th),
            Verdict::Leave
        );
    }

    #[test]
    fn editors_are_known() {
        assert!(is_code_editor("code.exe"));
        assert!(!is_code_editor("notepad.exe"));
        assert!(CODE_EDITORS.iter().all(|e| *e == e.to_lowercase()));
    }
}
