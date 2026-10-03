//! Reconciling what is on screen with what the run should read as (OS-free).
//!
//! D-008 replaces "decide once, destructively" with "render, and keep the
//! rendering honest". While a run is un-anchored, RightType owns the characters
//! it has put on screen for that run; on every keystroke it recomputes the best
//! reading and moves the screen to it with the smallest possible edit.
//!
//! That is the whole reason the D-007 residual can be closed: a reading chosen
//! from four characters of evidence is not a verdict any more, because the fifth
//! character is allowed to overturn it. `adavnce` may render as Thai at `adav`
//! and be put back to `adavnce` one keystroke later, and the typist — who is not
//! looking at the screen — only ever sees the converged result.
//!
//! The edit is expressed as backspaces plus text so the Windows layer can send
//! it as one atomic `SendInput` batch, exactly like every other correction.

use zeroize::Zeroize;

/// The smallest edit that turns what is on screen into the new target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delta {
    /// Characters to delete from the end of the rendered text.
    pub backspaces: usize,
    /// Text to type after deleting.
    pub insert: String,
}

impl Delta {
    /// Nothing to do — screen already matches.
    pub fn is_empty(&self) -> bool {
        self.backspaces == 0 && self.insert.is_empty()
    }
}

impl Drop for Delta {
    fn drop(&mut self) {
        self.insert.zeroize();
    }
}

/// Compute the edit that turns `rendered` into `target`.
///
/// Only the differing suffix is touched: the shared prefix is left alone, which
/// keeps the common case (one more character appended to a Thai run) down to a
/// single injected character and no backspaces at all.
pub fn delta(rendered: &str, target: &str) -> Delta {
    let mut common = 0usize;
    let mut r = rendered.chars();
    let mut t = target.chars();
    loop {
        match (r.next(), t.next()) {
            (Some(a), Some(b)) if a == b => common += 1,
            _ => break,
        }
    }
    Delta {
        backspaces: rendered.chars().count() - common,
        insert: target.chars().skip(common).collect(),
    }
}

/// How many characters before the caret are the `expected` text, given what
/// a standard Windows text box actually holds there (`before_caret`).
///
/// Two things make them differ. A text box applies Thai input sequence
/// checking to typed keys and drops a vowel or tone mark that cannot follow
/// the character before it (English typed on the Thai layout: `there` gives
/// `ะ้ำพำ`, the box keeps `ะพำ`) — the marks that are missing are skipped, and
/// the answer is what is really there. And a slow app may not have handled
/// the latest keys yet when it is asked (they wait in its queue behind the
/// question) — then `None`: the text is not there yet, and a replacement
/// must go in as keys, which queue up behind the rest.
pub fn chars_on_screen(expected: &str, before_caret: &str) -> Option<usize> {
    let e: Vec<char> = expected.chars().collect();
    let b: Vec<char> = before_caret.chars().collect();
    // What sequence checking drops: marks above or below a letter, tone
    // marks, and ำ (CI: `ะ้ำพำ` kept as `ะพำ`). Never a letter or a vowel that
    // is written on the line (เ แ โ ใ ไ ะ า): those always land, so missing
    // ones mean the box has not caught up.
    let droppable = |c: char| {
        matches!(c, '\u{0E31}' | '\u{0E33}')
            || ('\u{0E34}'..='\u{0E3A}').contains(&c)
            || ('\u{0E47}'..='\u{0E4E}').contains(&c)
    };
    let (mut i, mut j) = (e.len(), b.len());
    while i > 0 {
        if j > 0 && e[i - 1] == b[j - 1] {
            i -= 1;
            j -= 1;
        } else if droppable(e[i - 1]) {
            // The first ones too: `Unicode` on the Thai layout is ๊ืรแนกำ,
            // and Word keeps รแนกำ (marks with no letter before them).
            i -= 1;
        } else {
            return None;
        }
    }
    // Something must have landed (of a word there is to find).
    (e.is_empty() || j < b.len()).then_some(b.len() - j)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(rendered: &str, target: &str) -> (usize, String) {
        let delta = delta(rendered, target);
        (delta.backspaces, delta.insert.clone())
    }

    #[test]
    fn marks_dropped_at_the_start_of_a_word() {
        // Word's sequence checking: `Unicode` on the Thai layout.
        assert_eq!(chars_on_screen("๊ืรแนกำ", "ใช้ รแนกำ"), Some(5));
        assert_eq!(chars_on_screen("ะ้ำพำ", "x ะพำ"), Some(3));
        assert_eq!(chars_on_screen("๊ื", "ใช้ "), None, "nothing landed");
        assert_eq!(chars_on_screen("สวัสดี", "สวัสดี"), Some(6));
    }

    #[test]
    fn appending_costs_one_character_and_no_backspaces() {
        assert_eq!(d("สวัสดี", "สวัสดีค"), (0, "ค".to_string()));
    }

    #[test]
    fn identical_text_is_a_no_op() {
        assert_eq!(d("สวัสดี", "สวัสดี"), (0, String::new()));
        assert!(delta("abc", "abc").is_empty());
    }

    #[test]
    fn reverting_a_wrong_reading_replaces_only_what_differs() {
        // The Thai reading is withdrawn and the raw keystrokes come back.
        assert_eq!(d("ฟกฟอ", "adavn"), (4, "adavn".to_string()));
    }

    #[test]
    fn a_shared_prefix_is_never_retyped() {
        assert_eq!(d("สวัสดีกก", "สวัสดีคน"), (2, "คน".to_string()));
    }

    #[test]
    fn deleting_back_to_a_prefix_inserts_nothing() {
        assert_eq!(d("abcdef", "abc"), (3, String::new()));
    }

    #[test]
    fn rendering_from_nothing_is_a_plain_insert() {
        assert_eq!(d("", "สวัสดี"), (0, "สวัสดี".to_string()));
    }

    #[test]
    fn char_counts_not_byte_counts_drive_the_edit() {
        // Thai characters are 3 bytes each; backspaces must count characters.
        let delta = delta("กขค", "ก");
        assert_eq!(delta.backspaces, 2);
        assert!(delta.insert.is_empty());
    }

    #[test]
    fn what_a_text_box_really_holds() {
        use super::chars_on_screen;
        // All there.
        assert_eq!(chars_on_screen("l;ylf", "hello l;ylf"), Some(5));
        assert_eq!(chars_on_screen("", "abc"), Some(0));
        // Thai input sequence checking dropped ้ and ำ from ะ้ำพำ.
        assert_eq!(chars_on_screen("ะ้ำพำ", "วันนี้ ะพำ"), Some(3));
        // The box has not handled the latest keys yet.
        assert_eq!(chars_on_screen("l;ylf", "relogin "), None);
        assert_eq!(chars_on_screen("l;ylf", "l;y"), None);
        // แ and ะ are never dropped: not there yet means not caught up (CI:
        // `correct` was put before them, leaving `correct แะ`).
        assert_eq!(chars_on_screen("แนพพำแะ", "แนพพำ"), None);
        // Only vowels and marks can be missing, never a letter.
        assert_eq!(chars_on_screen("ab", "b"), None);
        assert_eq!(chars_on_screen("ำ", ""), None);
    }
}
