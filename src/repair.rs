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

/// Does a message of these `words` look typed on the wrong keyboard? At
/// least two of its words, and at least half of them, would be fixed —
/// one stray word is not enough to hold a message back.
pub fn looks_mistyped(words: &[&str], en: &Dictionary, th: &Dictionary) -> bool {
    let wrong = words
        .iter()
        .filter(|w| {
            layout_of(w)
                .and_then(|layout| policy::detect_token(w, layout, en, th))
                .is_some_and(|d| d.corrected != **w)
        })
        .count();
    wrong >= 2 && wrong * 2 >= words.len()
}

impl Repaired {
    /// The text with only the changes `keep` says yes to (by their place in
    /// [`Repaired::changes`]); the others go back to what was typed.
    pub fn with_only(&self, keep: &[bool]) -> String {
        let mut chars: Vec<char> = self.text.chars().collect();
        // From the end, so earlier starts stay where they are.
        for (i, change) in self.changes.iter().enumerate().rev() {
            if keep.get(i).copied().unwrap_or(false) {
                continue;
            }
            let len = change.fixed.chars().count();
            let end = (change.start + len).min(chars.len());
            chars.splice(change.start..end, change.original.chars());
        }
        let out = chars.iter().collect();
        chars.iter_mut().for_each(|c| *c = '\0');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    #[test]
    fn a_message_on_the_wrong_keyboard_is_noticed() {
        let (en, th) = (dict::english(), dict::thai());
        assert!(looks_mistyped(&["l;ylfu", "8iy["], en, th));
        assert!(looks_mistyped(&["l;ylfu", "8iy[", "ok"], en, th));
        // One stray word, or right as typed: sent.
        assert!(!looks_mistyped(&["l;ylfu"], en, th));
        assert!(!looks_mistyped(
            &["see", "you", "l;ylfu", "8iy[", "at", "the", "office"],
            en,
            th
        ));
        assert!(!looks_mistyped(&["สวัสดี", "ครับ"], en, th));
        assert!(!looks_mistyped(&["hello", "world"], en, th));
        assert!(!looks_mistyped(&[], en, th));
    }

    #[test]
    fn only_the_kept_changes_are_made() {
        let r = fix("l;ylfu 8iy[ hello l;ylfu");
        assert_eq!(r.changes.len(), 3);
        assert_eq!(r.with_only(&[true, true, true]), r.text);
        assert_eq!(
            r.with_only(&[false, true, false]),
            "l;ylfu ครับ hello l;ylfu"
        );
        assert_eq!(r.with_only(&[true, false, true]), "สวัสดี 8iy[ hello สวัสดี");
        assert_eq!(r.with_only(&[]), "l;ylfu 8iy[ hello l;ylfu");
    }

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
