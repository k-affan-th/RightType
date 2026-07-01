//! Current-word input buffer (OS-free).
//!
//! As the hook reports keystrokes, this buffer accumulates the **current word**
//! and emits it once a boundary (space/tab/enter) closes it. It holds nothing
//! more than the word in progress and **zeroizes** its storage on every boundary,
//! so secrets spend the minimum possible time in memory (see the privacy section
//! of `docs/PLAN.md`).
//!
//! It is deliberately OS-free: it consumes already-translated [`Key`] events, not
//! raw virtual-key codes, so the whole boundary/length policy is unit-testable
//! with no Win32. The Windows `hook` layer is responsible only for turning raw
//! events into [`Key`]s.
//!
//! ## What counts as a word boundary
//!
//! Only whitespace (space, tab, enter) and explicit control events end a word.
//! Punctuation does **not**, and this is load-bearing: in the *wrong* layout a
//! Thai letter surfaces as ASCII punctuation (e.g. `สวัสดี` typed on QWERTY is
//! `l;ylfu`, where `;` is the Thai `ว`). Treating `;`/`/`/`.` as boundaries would
//! shred exactly the gibberish we exist to repair, so every printable character
//! accumulates and only true separators split words.

use crate::secret::MAX_WORD_LEN;
use zeroize::Zeroize;

/// Widest a single UTF-8 character can be, in bytes.
const MAX_UTF8_BYTES: usize = 4;

/// A keystroke as seen by the buffer — already translated from a raw OS event to
/// a produced character or a control action by the hook layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// A printable character the keystroke produced in the *active* layout.
    Char(char),
    /// A word separator (space, tab, enter) — completes the current word.
    Boundary,
    /// Backspace — delete the last buffered character.
    Backspace,
    /// Focus change, navigation (arrows/home/end), or mouse click — discard the
    /// current word silently (the caret may have moved, so it's no longer "ours").
    Reset,
}

/// Accumulates the current word and emits it on a boundary.
///
/// Storage is zeroized whenever the word is cleared, and again on drop.
pub struct WordBuffer {
    buf: String,
    /// Maximum word length in characters; longer tokens are dropped unanalyzed.
    cap: usize,
    /// The current token already exceeded `cap` (or was otherwise poisoned) and
    /// must be discarded, not emitted, when its boundary arrives.
    poisoned: bool,
}

impl WordBuffer {
    /// A buffer capped at [`MAX_WORD_LEN`] — anything longer is treated as
    /// secret-ish/too-long and never emitted.
    pub fn new() -> Self {
        Self::with_cap(MAX_WORD_LEN)
    }

    /// A buffer with an explicit character cap (used by tests).
    ///
    /// Reserves capacity for `cap + 1` chars at the worst-case UTF-8 width up
    /// front (the `+1` covers the over-cap char that triggers poisoning), so
    /// this buffer's heap allocation **never reallocates** for its whole
    /// lifetime. That matters beyond performance: `Vec`/`String`'s `Zeroize`
    /// impl can only ever wipe the *current* allocation — its own docs note it
    /// "cannot ensure previous reallocations did not leave values on the heap".
    /// A stable allocation closes that gap, and lets the Windows layer
    /// `VirtualLock` this exact region once so it's never swapped to disk.
    pub fn with_cap(cap: usize) -> Self {
        Self {
            buf: String::with_capacity((cap + 1) * MAX_UTF8_BYTES),
            cap,
            poisoned: false,
        }
    }

    /// This buffer's stable backing allocation — `(pointer, byte capacity)` —
    /// for the OS layer to lock into RAM once (e.g. `VirtualLock` on Windows).
    /// Valid for the lifetime of this `WordBuffer` (capacity is fixed at
    /// construction and never grows).
    pub fn stable_region(&self) -> (*const u8, usize) {
        (self.buf.as_ptr(), self.buf.capacity())
    }

    /// Feed one translated key.
    ///
    /// Returns the completed word when a boundary closes a non-empty, non-poisoned
    /// token; otherwise `None`. The returned `String` is a *copy* of typed content
    /// — the caller owns it and is responsible for zeroizing it once analyzed.
    pub fn observe(&mut self, key: Key) -> Option<String> {
        match key {
            Key::Char(c) => {
                self.push(c);
                None
            }
            Key::Backspace => {
                self.backspace();
                None
            }
            Key::Reset => {
                self.clear();
                None
            }
            Key::Boundary => self.complete(),
        }
    }

    /// The word currently in progress (for manual "fix last word" / suggest mode).
    /// Empty while poisoned, since a poisoned token is not a real word.
    pub fn current(&self) -> &str {
        if self.poisoned {
            ""
        } else {
            &self.buf
        }
    }

    /// Discard the current word and wipe its storage. Idempotent.
    pub fn clear(&mut self) {
        self.buf.zeroize();
        self.buf.clear();
        self.poisoned = false;
    }

    fn push(&mut self, c: char) {
        if self.poisoned {
            return;
        }
        // Append, then check the cap. Counting chars (not bytes) keeps the limit
        // consistent across multibyte Thai and single-byte ASCII.
        self.buf.push(c);
        if self.buf.chars().count() > self.cap {
            // Too long to be a real word — poison and wipe now so an oversized
            // secret never lingers, but keep consuming until the boundary.
            self.buf.zeroize();
            self.buf.clear();
            self.poisoned = true;
        }
    }

    fn backspace(&mut self) {
        if self.poisoned {
            // We dropped the oversized token's contents; the safest reaction to
            // editing it is to keep ignoring it until the word ends.
            return;
        }
        self.buf.pop();
    }

    fn complete(&mut self) -> Option<String> {
        if self.poisoned || self.buf.is_empty() {
            self.clear();
            return None;
        }
        let word = self.buf.clone();
        self.clear();
        Some(word)
    }
}

impl Default for WordBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for WordBuffer {
    fn drop(&mut self) {
        self.buf.zeroize();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed a string as `Char` events; return whatever the final key emits.
    fn type_chars(b: &mut WordBuffer, s: &str) -> Option<String> {
        let mut last = None;
        for c in s.chars() {
            last = b.observe(Key::Char(c));
        }
        last
    }

    #[test]
    fn boundary_emits_the_completed_word() {
        let mut b = WordBuffer::new();
        assert_eq!(type_chars(&mut b, "correct"), None); // no emit mid-word
        assert_eq!(b.observe(Key::Boundary), Some("correct".to_string()));
        // Buffer is empty afterwards.
        assert_eq!(b.current(), "");
    }

    #[test]
    fn punctuation_stays_in_the_word() {
        // The wrong-layout case that must NOT be split: `สวัสดี` typed on QWERTY.
        let mut b = WordBuffer::new();
        type_chars(&mut b, "l;ylfu");
        assert_eq!(b.observe(Key::Boundary), Some("l;ylfu".to_string()));
    }

    #[test]
    fn thai_gibberish_round_trips_through_the_buffer() {
        let mut b = WordBuffer::new();
        type_chars(&mut b, "แนพพำแะ");
        assert_eq!(b.observe(Key::Boundary), Some("แนพพำแะ".to_string()));
    }

    #[test]
    fn empty_word_on_boundary_emits_nothing() {
        let mut b = WordBuffer::new();
        // Leading / repeated spaces produce no words.
        assert_eq!(b.observe(Key::Boundary), None);
        assert_eq!(b.observe(Key::Boundary), None);
    }

    #[test]
    fn backspace_edits_the_current_word() {
        let mut b = WordBuffer::new();
        type_chars(&mut b, "corrct");
        b.observe(Key::Backspace); // -> "corrc"
        b.observe(Key::Backspace); // -> "corr"
        type_chars(&mut b, "ect"); // -> "correct"
        assert_eq!(b.observe(Key::Boundary), Some("correct".to_string()));
    }

    #[test]
    fn backspace_on_empty_is_harmless() {
        let mut b = WordBuffer::new();
        b.observe(Key::Backspace);
        assert_eq!(b.current(), "");
        assert_eq!(b.observe(Key::Boundary), None);
    }

    #[test]
    fn reset_discards_without_emitting() {
        let mut b = WordBuffer::new();
        type_chars(&mut b, "hello");
        assert_eq!(b.observe(Key::Reset), None);
        // The discarded word is gone, not emitted on the next boundary.
        assert_eq!(b.observe(Key::Boundary), None);
        assert_eq!(b.current(), "");
    }

    #[test]
    fn over_cap_tokens_are_dropped_not_emitted() {
        let mut b = WordBuffer::with_cap(4);
        type_chars(&mut b, "abcd"); // exactly at cap
        assert_eq!(b.current(), "abcd");
        type_chars(&mut b, "e"); // exceeds cap -> poisoned, wiped
        assert_eq!(b.current(), "");
        // Further chars keep being ignored until the boundary, which emits nothing.
        type_chars(&mut b, "fghij");
        assert_eq!(b.observe(Key::Boundary), None);
        // And the buffer is clean and usable again afterwards.
        type_chars(&mut b, "ok");
        assert_eq!(b.observe(Key::Boundary), Some("ok".to_string()));
    }

    #[test]
    fn default_cap_is_max_word_len() {
        let mut b = WordBuffer::new();
        let exactly = "a".repeat(MAX_WORD_LEN);
        type_chars(&mut b, &exactly);
        assert_eq!(b.observe(Key::Boundary), Some(exactly));

        let mut b = WordBuffer::new();
        let too_long = "a".repeat(MAX_WORD_LEN + 1);
        type_chars(&mut b, &too_long);
        assert_eq!(b.observe(Key::Boundary), None);
    }

    #[test]
    fn capacity_never_changes_even_at_worst_case_utf8_width() {
        // Thai letters are 3 bytes each in UTF-8 — the worst case we actually
        // see. Filling the buffer to its cap, and one past it (poisoning),
        // must never trigger a reallocation: RAM hardening depends on this
        // buffer's pointer staying valid for its whole lifetime.
        let mut b = WordBuffer::new();
        let (_, cap0) = b.stable_region();
        for c in "ก".repeat(MAX_WORD_LEN + 1).chars() {
            b.observe(Key::Char(c));
            let (_, cap_now) = b.stable_region();
            assert_eq!(cap_now, cap0, "capacity must never grow");
        }
    }

    #[test]
    fn a_sentence_emits_one_word_per_boundary() {
        let mut b = WordBuffer::new();
        let mut words = Vec::new();
        for c in "the quick brown".chars() {
            if let Some(w) = b.observe(if c == ' ' { Key::Boundary } else { Key::Char(c) }) {
                words.push(w);
            }
        }
        // Trailing word needs a final boundary to flush.
        if let Some(w) = b.observe(Key::Boundary) {
            words.push(w);
        }
        assert_eq!(words, vec!["the", "quick", "brown"]);
    }
}
