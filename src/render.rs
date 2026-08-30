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

#[cfg(test)]
mod tests {
    use super::*;

    fn d(rendered: &str, target: &str) -> (usize, String) {
        let delta = delta(rendered, target);
        (delta.backspaces, delta.insert.clone())
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
}
