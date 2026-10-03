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
///
/// Each word is judged as the hook judges it at a boundary; then, as the
/// whole text is there to see, a short Thai word the hook leaves alone on
/// its own (รวม is `i;,`, short Thai words are too easily English typos) is
/// fixed too when a word next to it is Thai: `(i;, VAT` between ราคา…บาท
/// and แล้ว is (รวม VAT.
pub fn repair(text: &str, en: &Dictionary, th: &Dictionary) -> Repaired {
    // Words and the whitespace between them, in order.
    let mut parts: Vec<(String, bool)> = Vec::new();
    for c in text.chars() {
        let space = c.is_whitespace();
        match parts.last_mut() {
            Some((s, was_space)) if *was_space == space => s.push(c),
            _ => parts.push((c.to_string(), space)),
        }
    }
    let mut fixed: Vec<Option<String>> = parts
        .iter()
        .map(|(word, space)| {
            if *space {
                return None;
            }
            layout_of(word)
                .and_then(|layout| policy::detect_token(word, layout, en, th))
                .map(|d| d.corrected)
                .filter(|f| f != word)
        })
        .collect();
    let is_thai = |s: &str| s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    let words: Vec<usize> = (0..parts.len()).filter(|&i| !parts[i].1).collect();
    for (k, &i) in words.iter().enumerate() {
        if fixed[i].is_some() {
            continue;
        }
        let shown = |j: usize| fixed[j].clone().unwrap_or_else(|| parts[j].0.clone());
        let thai_beside = (k > 0 && is_thai(&shown(words[k - 1])))
            || (k + 1 < words.len() && is_thai(&shown(words[k + 1])));
        if thai_beside {
            fixed[i] = short_thai_word(&parts[i].0, en, th);
        }
    }
    let mut out = Repaired::default();
    let mut chars = 0usize;
    for ((word, _), fix) in parts.into_iter().zip(fixed) {
        match fix {
            Some(f) => {
                out.changes.push(Change {
                    start: chars,
                    original: word,
                    fixed: f.clone(),
                });
                chars += f.chars().count();
                out.text.push_str(&f);
            }
            None => {
                chars += word.chars().count();
                out.text.push_str(&word);
            }
        }
    }
    out
}

/// A Thai dictionary word typed on the English layout, with an opening
/// bracket or quote kept as typed: `(i;,` is (รวม. Not an English word.
fn short_thai_word(word: &str, en: &Dictionary, th: &Dictionary) -> Option<String> {
    if !word.chars().any(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let core = word.trim_start_matches(['(', '"']);
    let lead = &word[..word.len() - core.len()];
    // Letters only, it may be English (`ok`, `in`); with a Thai key's
    // punctuation among them (`i;,`) it is not.
    if core.is_empty()
        || core.chars().all(|c| c.is_ascii_alphabetic())
            && (crate::english::is_word(core, en) || en.contains(core))
    {
        return None;
    }
    let reading = crate::layout::en_to_th(core);
    (reading.chars().count() >= 2 && th.contains(&reading)).then(|| format!("{lead}{reading}"))
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
    fn a_short_thai_word_between_thai_ones() {
        // รวม alone is `i;,`, too short to fix on its own; between Thai
        // words it is Thai.
        let r = fix("ik8k 1,250 [km (i;, VAT c]h;)");
        assert_eq!(r.text, "ราคา 1,250 บาท (รวม VAT แล้ว)");
        // An English word next to Thai stays English.
        assert_eq!(fix("ok l;ylfu8iy[").text, "ok สวัสดีครับ");
        assert_eq!(fix("go to l;ylfu").text, "go to สวัสดี");
    }

    #[test]
    fn marks_after_a_thai_word_stay_as_typed() {
        assert_eq!(fix("-v[86I,kd8jt!").text, "ขอบคุณมากค่ะ!");
    }

    #[test]
    fn secrets_are_never_rewritten() {
        let key = "5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ";
        assert_eq!(fix(key).text, key);
    }
}
