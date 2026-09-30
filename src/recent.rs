//! The last few completed words, for flipping back more than one word.
//!
//! `Shift`+`Backspace` right after a word boundary flips the word before the
//! caret to the other layout. Pressing it again flips the word before that one
//! too, and so on — up to [`Recent::CAP`] words — so a whole phrase typed in the
//! wrong layout comes back without selecting it.
//!
//! [`Recent`] mirrors the tail of the text on screen: each entry is a word as
//! it appears there, followed by the boundary that ended it. It only ever holds
//! words that sit next to each other, separated by single spaces; a word ended
//! by Enter or Tab starts a new run, because retyping across a line break is
//! not something every app does faithfully. The words are wiped from memory as
//! soon as they are dropped.
//!
//! The Windows hook owns one, clears it whenever the text before the caret may
//! no longer be what it recorded (Backspace, navigation, a command chord, a
//! focus or layout change, the seed-phrase guard), and applies each
//! [`FlipStep`] to the screen.

use std::collections::VecDeque;

use zeroize::Zeroize;

/// One completed word, as it is on screen.
struct Entry {
    word: String,
    /// The character that ended it: `' '`, `'\r'` or `'\t'`.
    boundary: char,
    /// RightType converted this word itself, so flipping it back teaches that
    /// the original was a real word.
    converted: bool,
    /// What the word was before the current run of flips changed it, to put
    /// back when a press finds nothing older to flip.
    before: Option<String>,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.word.zeroize();
        if let Some(before) = self.before.as_mut() {
            before.zeroize();
        }
    }
}

/// What one more `Shift`+`Backspace` changes on screen.
pub struct FlipStep {
    /// Characters to delete before the caret, including the last boundary.
    pub backspaces: usize,
    /// Text to type instead, **without** the last boundary (re-send that key).
    pub insert: String,
    /// The last word's boundary, to re-send after `insert`.
    pub boundary: char,
    /// What was on screen before this step (including the last boundary), to
    /// retype for Undo.
    pub restore: String,
    /// How many words are flipped in total after this step.
    pub words: usize,
    /// The word to learn, when the word flipped by this step was RightType's
    /// own conversion: the typist's flip-back says its original was real.
    pub learn: Option<String>,
    /// The newest word as it is on screen after this step, to switch the
    /// keyboard layout to.
    pub newest: String,
    /// Whether the word this step flipped was Thai before, and is now.
    pub was_thai: bool,
    pub now_thai: bool,
    /// This step puts back every word the run of flips changed (the press
    /// found nothing older to flip), rather than flipping one more.
    pub reverts: bool,
}

impl Drop for FlipStep {
    fn drop(&mut self) {
        self.insert.zeroize();
        self.restore.zeroize();
        self.newest.zeroize();
        if let Some(word) = self.learn.as_mut() {
            word.zeroize();
        }
    }
}

fn is_thai(word: &str) -> bool {
    word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// The recent words and how far the current run of flips has reached.
#[derive(Default)]
pub struct Recent {
    words: VecDeque<Entry>,
    /// Words flipped by the current run of `Shift`+`Backspace` presses.
    chain: usize,
}

impl Recent {
    /// At most this many words are kept (and can be flipped back in one run).
    pub const CAP: usize = 8;

    pub fn new() -> Self {
        Self::default()
    }

    /// A word was completed by `boundary` and is now on screen before the caret.
    pub fn push(&mut self, word: &str, boundary: char, converted: bool) {
        self.end_chain();
        // Only words separated by single spaces form one run.
        if self.words.back().is_some_and(|e| e.boundary != ' ') {
            self.words.clear();
        }
        if self.words.len() == Self::CAP {
            self.words.pop_front();
        }
        self.words.push_back(Entry {
            word: word.to_string(),
            boundary,
            converted,
            before: None,
        });
    }

    /// Forget every word (the text before the caret may have changed).
    pub fn clear(&mut self) {
        self.words.clear();
        self.chain = 0;
    }

    /// End the current run of flips; the next `Shift`+`Backspace` starts again
    /// from the newest word.
    pub fn end_chain(&mut self) {
        self.chain = 0;
        // What the words were before this run no longer matters: they are
        // what the typist kept.
        for entry in self.words.iter_mut() {
            if let Some(mut before) = entry.before.take() {
                before.zeroize();
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }

    /// Words currently held.
    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Flip one more word: the newest on the first call, then each older one.
    /// `convert` turns a word into the other layout. Returns `None` when there
    /// is no further word. The caller applies the step to the screen and then
    /// calls [`Recent::commit`] (or [`Recent::clear`] if that failed).
    pub fn next_step(&self, convert: impl Fn(&str) -> String) -> Option<FlipStep> {
        let k = self.chain + 1;
        if k > self.words.len() {
            return self.revert_step();
        }
        let first = self.words.len() - k;
        let span = self.words.range(first..);
        let mut restore = String::new();
        let mut insert = String::new();
        let mut backspaces = 0;
        let mut learn = None;
        let (mut was_thai, mut now_thai) = (false, false);
        let last = self.words.len() - 1;
        for (i, entry) in span.enumerate() {
            let index = first + i;
            restore.push_str(&entry.word);
            restore.push(entry.boundary);
            backspaces += entry.word.chars().count() + 1;
            if index == first {
                let mut flipped = convert(&entry.word);
                if entry.converted && flipped != entry.word {
                    learn = Some(flipped.clone());
                }
                was_thai = is_thai(&entry.word);
                now_thai = is_thai(&flipped);
                insert.push_str(&flipped);
                flipped.zeroize();
            } else {
                insert.push_str(&entry.word);
            }
            if index != last {
                insert.push(entry.boundary);
            }
        }
        let newest = if first == last {
            insert.clone()
        } else {
            self.words[last].word.clone()
        };
        Some(FlipStep {
            backspaces,
            insert,
            boundary: self.words[last].boundary,
            restore,
            words: k,
            learn,
            newest,
            was_thai,
            now_thai,
            reverts: false,
        })
    }

    /// Put back every word the current run of flips changed: the press after
    /// the oldest word was flipped. `None` when nothing was flipped.
    fn revert_step(&self) -> Option<FlipStep> {
        if self.chain == 0 {
            return None;
        }
        let first = self.words.len() - self.chain;
        let last = self.words.len() - 1;
        let mut restore = String::new();
        let mut insert = String::new();
        let mut backspaces = 0;
        for (i, entry) in self.words.range(first..).enumerate() {
            let original = entry.before.as_deref().unwrap_or(&entry.word);
            restore.push_str(&entry.word);
            restore.push(entry.boundary);
            backspaces += entry.word.chars().count() + 1;
            insert.push_str(original);
            if first + i != last {
                insert.push(entry.boundary);
            }
        }
        let oldest = &self.words[first];
        let newest = &self.words[last];
        Some(FlipStep {
            backspaces,
            insert,
            boundary: newest.boundary,
            restore,
            words: self.chain,
            learn: None,
            newest: newest.before.clone().unwrap_or_else(|| newest.word.clone()),
            was_thai: is_thai(&oldest.word),
            now_thai: is_thai(oldest.before.as_deref().unwrap_or(&oldest.word)),
            reverts: true,
        })
    }

    /// The step from [`Recent::next_step`] is on screen now: record it.
    pub fn commit(&mut self, convert: impl Fn(&str) -> String) {
        let k = self.chain + 1;
        let Some(index) = self.words.len().checked_sub(k) else {
            // The step put every flipped word back: the next press starts
            // again from the newest word.
            let first = self.words.len() - self.chain;
            for entry in self.words.range_mut(first..) {
                if let Some(before) = entry.before.take() {
                    entry.word.zeroize();
                    entry.word = before;
                }
            }
            self.chain = 0;
            return;
        };
        let entry = &mut self.words[index];
        let flipped = convert(&entry.word);
        let old = std::mem::replace(&mut entry.word, flipped);
        match entry.before {
            Some(_) => {
                let mut old = old;
                old.zeroize();
            }
            None => entry.before = Some(old),
        }
        // Flipped by the typist: it is theirs now.
        entry.converted = false;
        self.chain = k;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::auto_convert;

    fn apply(screen: &mut String, step: &FlipStep) {
        for _ in 0..step.backspaces {
            screen.pop();
        }
        screen.push_str(&step.insert);
        screen.push(step.boundary);
    }

    fn typed(words: &[&str]) -> (Recent, String) {
        let mut recent = Recent::new();
        let mut screen = String::new();
        for w in words {
            recent.push(w, ' ', false);
            screen.push_str(w);
            screen.push(' ');
        }
        (recent, screen)
    }

    #[test]
    fn flips_back_one_word_then_the_ones_before_it() {
        // "สวัสดีครับ ทุกคน" typed on the US layout.
        let (mut recent, mut screen) = typed(&["l;ylfu", "8ib'", "ljdkIT"]);

        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.words, 1);
        assert_eq!(step.restore, "ljdkIT ");
        apply(&mut screen, &step);
        assert_eq!(screen, format!("l;ylfu 8ib' {} ", auto_convert("ljdkIT")));
        recent.commit(auto_convert);

        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.words, 2);
        apply(&mut screen, &step);
        recent.commit(auto_convert);
        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.words, 3);
        apply(&mut screen, &step);
        recent.commit(auto_convert);
        let expected: Vec<String> = ["l;ylfu", "8ib'", "ljdkIT"]
            .iter()
            .map(|w| auto_convert(w))
            .collect();
        assert_eq!(screen, format!("{} ", expected.join(" ")));

        // Nothing older to flip: the next press puts all three back.
        let step = recent.next_step(auto_convert).unwrap();
        assert!(step.reverts);
        apply(&mut screen, &step);
        assert_eq!(screen, "l;ylfu 8ib' ljdkIT ");
    }

    #[test]
    fn pressing_again_with_nothing_older_puts_the_word_back() {
        // Found in real use: `reload`, Shift+Backspace (พำสนฟก), and a
        // second press said "nothing to flip" instead of bringing it back.
        let (mut recent, mut screen) = typed(&["reload"]);
        let step = recent.next_step(auto_convert).unwrap();
        apply(&mut screen, &step);
        recent.commit(auto_convert);
        assert_eq!(screen, format!("{} ", auto_convert("reload")));

        let step = recent.next_step(auto_convert).unwrap();
        assert!(step.reverts);
        assert_eq!(step.learn, None);
        apply(&mut screen, &step);
        recent.commit(auto_convert);
        assert_eq!(screen, "reload ");

        // And again flips it, like the first press.
        let step = recent.next_step(auto_convert).unwrap();
        assert!(!step.reverts);
        assert_eq!(step.words, 1);
    }

    #[test]
    fn restore_is_exactly_what_the_step_replaced() {
        let (mut recent, mut screen) = typed(&["hello", "l;ylfu"]);
        let step = recent.next_step(auto_convert).unwrap();
        apply(&mut screen, &step);
        recent.commit(auto_convert);
        let before = screen.clone();
        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.restore, before);
        assert_eq!(step.backspaces, before.chars().count());
    }

    #[test]
    fn a_line_break_starts_a_new_run() {
        let mut recent = Recent::new();
        recent.push("one", ' ', false);
        recent.push("two", '\r', false);
        recent.push("three", ' ', false);
        assert_eq!(recent.len(), 1);
        // The newest word may end with any boundary; it is re-sent as a key.
        recent.push("four", '\t', false);
        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.boundary, '\t');
        assert!(!step.insert.ends_with('\t'));
    }

    #[test]
    fn keeps_at_most_cap_words() {
        let mut recent = Recent::new();
        for _ in 0..Recent::CAP + 3 {
            recent.push("word", ' ', false);
        }
        assert_eq!(recent.len(), Recent::CAP);
    }

    #[test]
    fn flipping_back_our_own_conversion_learns_the_original() {
        let mut recent = Recent::new();
        // RightType turned "hello" typed on the Thai layout... into Thai; the
        // screen shows the conversion and the typist flips it back.
        let on_screen = auto_convert("hello");
        recent.push(&on_screen, ' ', true);
        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.learn.as_deref(), Some("hello"));
        recent.commit(auto_convert);
        // Flipping it again is not a lesson.
        recent.end_chain();
        let step = recent.next_step(auto_convert).unwrap();
        assert_eq!(step.learn, None);
    }

    #[test]
    fn another_key_ends_the_run_and_what_it_would_put_back() {
        let (mut recent, _) = typed(&["reload"]);
        let _ = recent.next_step(auto_convert).unwrap();
        recent.commit(auto_convert);
        recent.end_chain();
        // A new run: flips the word as it is now (Thai), and the press after
        // puts back that, not the `reload` of the run before.
        let step = recent.next_step(auto_convert).unwrap();
        assert!(!step.reverts);
        assert_eq!(step.insert, "reload");
        recent.commit(auto_convert);
        let step = recent.next_step(auto_convert).unwrap();
        assert!(step.reverts);
        assert_eq!(step.insert, auto_convert("reload"));
    }

    #[test]
    fn a_new_word_ends_the_chain() {
        let (mut recent, _) = typed(&["a", "b"]);
        let _ = recent.next_step(auto_convert).unwrap();
        recent.commit(auto_convert);
        recent.push("c", ' ', false);
        assert_eq!(recent.next_step(auto_convert).unwrap().words, 1);
    }
}
