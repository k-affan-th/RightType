//! Fix a whole piece of text typed in the wrong layout — the core of the
//! **Fix text** window.
//!
//! Unlike the hook, this sees finished text rather than keystrokes, so each
//! word's layout is inferred from its characters: a word with Latin letters is
//! read as typed on the US layout, a word in Thai script as typed on the Thai
//! layout. Every word goes through the same [`policy::detect_token`] the hook
//! uses at a boundary — with the same guards against secrets and the same
//! dictionaries — so a word the hook would leave alone is left alone here too.
//! Whitespace is kept exactly.

use crate::dict::Dictionary;
use crate::policy::{self, InputLayout};

/// One word that was changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// Where the fixed word starts in [`Repaired::text`], in characters.
    pub start: usize,
    pub original: String,
    pub fixed: String,
}

/// The fixed text and what changed in it.
#[derive(Debug, Default)]
pub struct Repaired {
    pub text: String,
    pub changes: Vec<Change>,
}

/// The layout a finished word was most likely typed on.
fn layout_of(word: &str) -> Option<InputLayout> {
    if word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        Some(InputLayout::ThaiKedmanee)
    } else if word.chars().any(|c| c.is_ascii_alphabetic()) {
        Some(InputLayout::UsQwerty)
    } else {
        None
    }
}

/// Fix every wrong-layout word in `text`.
pub fn repair(text: &str, en: &Dictionary, th: &Dictionary) -> Repaired {
    let mut out = Repaired::default();
    let mut chars = 0usize;
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut Repaired, chars: &mut usize| {
        if word.is_empty() {
            return;
        }
        let fixed = layout_of(word)
            .and_then(|layout| policy::detect_token(word, layout, en, th))
            .map(|d| d.corrected)
            .filter(|fixed| fixed != word.as_str());
        match fixed {
            Some(fixed) => {
                out.changes.push(Change {
                    start: *chars,
                    original: std::mem::take(word),
                    fixed: fixed.clone(),
                });
                *chars += fixed.chars().count();
                out.text.push_str(&fixed);
            }
            None => {
                *chars += word.chars().count();
                out.text.push_str(word);
                word.clear();
            }
        }
    };
    for c in text.chars() {
        if c.is_whitespace() {
            flush(&mut word, &mut out, &mut chars);
            out.text.push(c);
            chars += 1;
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out, &mut chars);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    fn fix(text: &str) -> Repaired {
        repair(text, dict::english(), dict::thai())
    }

    #[test]
    fn fixes_thai_typed_on_the_english_layout() {
        let r = fix("l;ylfu 8iy[");
        assert_eq!(r.text, "สวัสดี ครับ");
        assert_eq!(r.changes.len(), 2);
        assert_eq!(r.changes[1].start, "สวัสดี ".chars().count());
    }

    #[test]
    fn fixes_english_typed_on_the_thai_layout() {
        let typed = crate::layout::en_to_th("correct");
        let r = fix(&format!("{typed} answer"));
        assert_eq!(r.text, "correct answer");
        assert_eq!(r.changes.len(), 1);
    }

    #[test]
    fn leaves_correct_text_and_whitespace_alone() {
        for text in [
            "the quick frontend codebase",
            "สวัสดีครับ ทุกคน",
            "PyThaiNLP กับ WangchanBERTa\r\n\tv2.0 12,480 64%",
            "",
            "   ",
        ] {
            let r = fix(text);
            assert_eq!(r.text, text);
            assert!(r.changes.is_empty(), "{text:?}");
        }
    }

    #[test]
    fn a_mixed_paragraph_keeps_its_right_words() {
        let r = fix("hello l;ylfu8iy[ middleware");
        assert_eq!(r.text, "hello สวัสดีครับ middleware");
    }

    #[test]
    fn secrets_are_never_rewritten() {
        let key = "5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ";
        assert_eq!(fix(key).text, key);
    }
}
