//! An OS-free replay of the Windows keyboard hook in **Auto** mode.
//!
//! [`Engine`] makes exactly the decisions `hook.rs` makes for each keystroke,
//! in the same order, using the same core calls — `WordBuffer`,
//! [`policy::live_reading`], [`render::delta`], the [`policy::COMMIT_HORIZON`]
//! anchor, [`policy::revise_converted`] / [`policy::detect_token`] at the
//! boundary, the seed-phrase guard, and the layout switch RightType requests
//! after a correction — but applies them to a `String` standing in for the
//! focused text box instead of calling `SendInput`.
//!
//! It exists to measure the whole pipeline on long, realistic input (the
//! typing benchmark in `examples/typing_benchmark.rs`) and to regression-test
//! it. It does not model what only Windows can show: injection timing, apps
//! that drop or reorder injected input, focus changes, or learning.

use crate::buffer::{Key, WordBuffer};
use crate::dict::Dictionary;
use crate::layout::en_to_th;
use crate::policy::{self, InputLayout, Reading};
use crate::render;
use crate::secret::SeedTracker;

/// Whether a token is RightType's to reinterpret (`hook::TokenMark`, minus the
/// manual-decision state Auto-only replays never reach).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mark {
    Plain,
    /// Anchored mid-word (D-009): the boundary judges the whole token again.
    Converted,
}

/// A run RightType is currently rendering (`hook::OwnedRun`).
struct Owned {
    rendered: String,
    stable: usize,
}

/// What RightType did while typing, for reports.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counters {
    /// Words converted at a boundary (either direction).
    pub boundary_corrections: usize,
    /// Runs anchored mid-word into Thai.
    pub live_anchors: usize,
    /// Mid-word renderings later withdrawn (the keys spelled English after all).
    pub withdrawals: usize,
    /// Backspaces and characters RightType injected.
    pub injected_backspaces: usize,
    pub injected_chars: usize,
    /// Layout switches RightType requested.
    pub layout_switches: usize,
}

/// The hook's Auto-mode pipeline driving a simulated text box.
pub struct Engine<'d> {
    en: &'d Dictionary,
    th: &'d Dictionary,
    /// Whether RightType is running at all (off = the keys reach the app raw).
    pub enabled: bool,
    layout: InputLayout,
    buf: WordBuffer,
    owned: Option<Owned>,
    mark: Mark,
    seed: SeedTracker,
    /// The focused text box.
    pub screen: String,
    pub counters: Counters,
    /// CapsLock is on: letters on the English layout show in the other case.
    pub caps: bool,
}

impl<'d> Engine<'d> {
    pub fn new(en: &'d Dictionary, th: &'d Dictionary, layout: InputLayout) -> Self {
        Engine {
            en,
            th,
            enabled: true,
            layout,
            buf: WordBuffer::new(),
            owned: None,
            mark: Mark::Plain,
            seed: SeedTracker::new(),
            screen: String::new(),
            counters: Counters::default(),
            caps: false,
        }
    }

    /// The active keyboard layout.
    pub fn layout(&self) -> InputLayout {
        self.layout
    }

    /// The typist switches layout themselves (Alt+Shift / Win+Space).
    ///
    /// The hook sees the chord's modifier first: it withdraws a run it owns
    /// and drops the token, then Windows changes the layout.
    pub fn switch_layout(&mut self, to: InputLayout) {
        if self.enabled {
            self.withdraw_owned_run();
            self.buf.clear();
            self.mark = Mark::Plain;
        }
        self.layout = to;
    }

    /// One printable key, named by the character it types on US QWERTY
    /// (upper case and symbols mean Shift is held).
    pub fn key(&mut self, key: char) {
        let produced = match self.layout {
            // CapsLock swaps the case of letters on the English layout.
            InputLayout::UsQwerty if self.caps && key.is_ascii_alphabetic() => {
                if key.is_ascii_uppercase() {
                    key.to_ascii_lowercase()
                } else {
                    key.to_ascii_uppercase()
                }
            }
            InputLayout::UsQwerty => key,
            InputLayout::ThaiKedmanee => en_to_th(&key.to_string()).chars().next().unwrap_or(key),
        };
        if !self.enabled {
            self.screen.push(produced);
            return;
        }
        // The hook keeps the key as if CapsLock were off: what was meant.
        let meant = match self.layout {
            InputLayout::UsQwerty => key,
            InputLayout::ThaiKedmanee => produced,
        };
        self.buf.observe(Key::Char(meant));
        if self.mark == Mark::Plain && self.layout == InputLayout::UsQwerty && self.reconcile_run()
        {
            return;
        }
        if self.buf.is_poisoned() || self.buf.current().is_empty() {
            self.mark = Mark::Plain;
        }
        self.screen.push(produced);
    }

    /// CapsLock pressed on its own: the hook drops the word in progress (it
    /// keeps words as if CapsLock were off, so one typed across a toggle is
    /// left alone).
    pub fn toggle_caps(&mut self) {
        if self.enabled {
            self.buf.clear();
            self.owned = None;
            self.mark = Mark::Plain;
        }
        self.caps = !self.caps;
    }

    /// Backspace.
    pub fn backspace(&mut self) {
        if !self.enabled {
            self.screen.pop();
            return;
        }
        self.buf.observe(Key::Backspace);
        if self.owned.is_some()
            && self.mark == Mark::Plain
            && self.layout == InputLayout::UsQwerty
            && self.reconcile_run()
        {
            return;
        }
        if self.buf.is_poisoned() || self.buf.current().is_empty() {
            self.mark = Mark::Plain;
        }
        self.screen.pop();
    }

    /// Space, Enter or Tab (`boundary` is the character it types).
    pub fn boundary(&mut self, boundary: char) {
        if !self.enabled {
            self.screen.push(boundary);
            return;
        }
        let completed = self.buf.observe(Key::Boundary);
        let Some(word) = completed else {
            self.screen.push(boundary);
            return;
        };
        let mark = std::mem::replace(&mut self.mark, Mark::Plain);

        // A boundary ends a run we own: its reading is already on screen.
        if self.owned.is_some() {
            let rendered = self
                .owned
                .as_ref()
                .map(|o| o.rendered.clone())
                .unwrap_or_default();
            if !policy::run_goes_back(&word, &rendered, self.en, self.th) {
                self.owned = None;
                self.counters.live_anchors += 1;
                self.request_layout(InputLayout::ThaiKedmanee);
                self.screen.push(boundary);
                return;
            }
            // The word cannot end as Thai after all: put the keys back and
            // judge it like any other word.
            self.withdraw_owned_run_to(&word);
        }

        let converted = mark == Mark::Converted;
        let mut detection = if converted {
            policy::revise_converted(&word, self.en, self.th)
        } else {
            policy::detect_token(&word, self.layout, self.en, self.th)
        };
        let corrected = detection.as_ref().map(|d| d.corrected.as_str());
        if self.seed.observe_candidate(&word, corrected) {
            detection = None;
        }
        if let Some(d) = detection.as_mut() {
            d.corrected = policy::shown_with_caps(&d.corrected, self.caps);
        }
        match detection {
            Some(d) => {
                // Every character of the token reached the app; the boundary
                // was swallowed and is re-emitted after the correction.
                let n = word.chars().count();
                for _ in 0..n {
                    self.screen.pop();
                }
                self.screen.push_str(&d.corrected);
                self.screen.push(boundary);
                self.counters.boundary_corrections += 1;
                self.counters.injected_backspaces += n;
                self.counters.injected_chars += d.corrected.chars().count() + 1;
                let thai = d.corrected.chars().any(is_thai);
                self.request_layout(if thai {
                    InputLayout::ThaiKedmanee
                } else {
                    InputLayout::UsQwerty
                });
            }
            None => self.screen.push(boundary),
        }
    }

    /// `hook::reconcile_run`, including its idea of what the screen shows;
    /// the edit it computes is applied to the real screen. Returns whether the
    /// key was consumed (it then never reaches the app).
    fn reconcile_run(&mut self) -> bool {
        let run = self.buf.current().to_string();
        let holding = self.owned.is_some();
        if self.buf.is_poisoned() {
            if holding {
                self.owned = None;
            }
            return false;
        }
        if !holding && run.is_empty() {
            return false;
        }
        let mut reading = policy::live_reading(&run, holding, self.en, self.th);
        if matches!(reading, Reading::Thai(_)) && self.seed.guarding() {
            reading = Reading::AsTyped;
        }
        let target = match &reading {
            Reading::AsTyped => policy::shown_with_caps(&run, self.caps),
            Reading::Thai(thai) => thai.clone(),
        };
        // The hook's model of the screen: what it rendered, or — before it
        // owns the run — the run minus the key that has not reached the app.
        let assumed = match &self.owned {
            Some(o) => o.rendered.clone(),
            None => {
                if matches!(reading, Reading::AsTyped) {
                    return false;
                }
                run.chars()
                    .take(run.chars().count().saturating_sub(1))
                    .collect()
            }
        };
        let delta = render::delta(&assumed, &target);
        for _ in 0..delta.backspaces {
            self.screen.pop();
        }
        self.screen.push_str(&delta.insert);
        self.counters.injected_backspaces += delta.backspaces;
        self.counters.injected_chars += delta.insert.chars().count();

        match reading {
            Reading::AsTyped => {
                self.owned = None;
                self.counters.withdrawals += 1;
            }
            Reading::Thai(_) => {
                let stable = self.owned.as_ref().map_or(0, |o| o.stable) + 1;
                if stable >= policy::COMMIT_HORIZON {
                    // D-009: release the run, keep the token.
                    self.owned = None;
                    self.buf.replace(&target);
                    self.mark = Mark::Converted;
                    self.counters.live_anchors += 1;
                    self.request_layout(InputLayout::ThaiKedmanee);
                } else {
                    self.owned = Some(Owned {
                        rendered: target,
                        stable,
                    });
                }
            }
        }
        true
    }

    /// `hook::withdraw_owned_run`: put the raw keystrokes back.
    fn withdraw_owned_run(&mut self) {
        let run = self.buf.current().to_string();
        self.withdraw_owned_run_to(&run);
    }

    /// Put `run` (the keys as typed) back in place of the run we own.
    fn withdraw_owned_run_to(&mut self, run: &str) {
        let Some(owned) = self.owned.take() else {
            return;
        };
        let shown = policy::shown_with_caps(run, self.caps);
        let delta = render::delta(&owned.rendered, &shown);
        for _ in 0..delta.backspaces {
            self.screen.pop();
        }
        self.screen.push_str(&delta.insert);
        self.counters.withdrawals += 1;
    }

    fn request_layout(&mut self, to: InputLayout) {
        if self.layout != to {
            self.layout = to;
            self.counters.layout_switches += 1;
        }
    }
}

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

/// The US-QWERTY key that types `c`, and the layout it must be typed on.
/// `None` for a character neither layout has.
pub fn key_for(c: char) -> Option<(char, InputLayout)> {
    if is_thai(c) {
        let key = crate::layout::th_to_en(&c.to_string()).chars().next()?;
        (key != c).then_some((key, InputLayout::ThaiKedmanee))
    } else if c.is_ascii_graphic() {
        Some((c, InputLayout::UsQwerty))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dict;

    /// Type `text` without ever switching layout by hand, starting on `start`.
    fn type_blind(text: &str, start: InputLayout) -> String {
        let mut e = Engine::new(dict::english(), dict::thai(), start);
        for c in text.chars() {
            if c == ' ' || c == '\n' {
                e.boundary(c);
            } else {
                let (key, _) = key_for(c).expect("typeable");
                e.key(key);
            }
        }
        e.screen
    }

    #[test]
    fn a_thai_word_on_the_english_layout_arrives_as_thai() {
        assert_eq!(type_blind("สวัสดีครับ ", InputLayout::UsQwerty), "สวัสดีครับ ");
    }

    #[test]
    fn a_word_that_only_starts_like_thai_goes_back_at_space() {
        // `relogi` reads พำ + สน + เร, three known Thai words, so it turns
        // Thai mid-word; with the `n` the reading ends in a stray vowel, and
        // at the space the keys go back to what was typed.
        let en = crate::dict::Dictionary::from_words(["log"]);
        let th = crate::dict::Dictionary::from_words(["พำ", "สน", "เร", "เรือ"]);
        let mut e = Engine::new(&en, &th, InputLayout::UsQwerty);
        for c in "relogi".chars() {
            e.key(c);
        }
        assert_eq!(e.screen, "พำสนเร");
        // Still the start of a Thai word (เรือ), so it stays Thai while typed.
        e.key('n');
        assert_eq!(e.screen, "พำสนเรื");
        e.boundary(' ');
        assert_eq!(e.screen, "relogin ");
        assert_eq!(e.layout(), InputLayout::UsQwerty);
    }

    #[test]
    fn everyday_computer_words_stay_english() {
        assert_eq!(type_blind("relogin ", InputLayout::UsQwerty), "relogin ");
        assert_eq!(type_blind("logout ", InputLayout::UsQwerty), "logout ");
    }

    /// Keys typed with CapsLock on (the keys named as on US QWERTY; upper
    /// case still means Shift).
    fn type_with_caps(keys: &str) -> String {
        let mut e = Engine::new(dict::english(), dict::thai(), InputLayout::UsQwerty);
        e.caps = true;
        for c in keys.chars() {
            if c == ' ' {
                e.boundary(c);
            } else {
                e.key(c);
            }
        }
        e.screen
    }

    #[test]
    fn thai_typed_with_capslock_on_arrives_as_thai() {
        // CapsLock left on: the English layout shows `L;YLFU`, but the keys
        // are the ones for สวัสดี.
        assert_eq!(type_with_caps("l;ylfu "), "สวัสดี ");
        assert_eq!(type_with_caps("l;ylfu8iy[ "), "สวัสดีครับ ");
        assert_eq!(type_with_caps("giupo "), "เรียน ");
    }

    #[test]
    fn english_typed_with_capslock_on_stays_as_shown() {
        assert_eq!(type_with_caps("hello world "), "HELLO WORLD ");
        assert_eq!(type_with_caps("select from "), "SELECT FROM ");
        // Converted mid-word, then put back at the space: the keys come back
        // as they were shown.
        let en = crate::dict::Dictionary::from_words(["log"]);
        let th = crate::dict::Dictionary::from_words(["พำ", "สน", "เร", "เรือ"]);
        let mut e = Engine::new(&en, &th, InputLayout::UsQwerty);
        e.caps = true;
        for c in "relogin".chars() {
            e.key(c);
        }
        assert_eq!(e.screen, "พำสนเรื");
        e.boundary(' ');
        assert_eq!(e.screen, "RELOGIN ");
    }

    #[test]
    fn english_prefix_or_suffix_on_a_known_word_stays_english() {
        // Not in the dictionary, but a known word with a common prefix or
        // suffix: re + rise, multi + holes, re + sit.
        for word in ["rerise ", "multiholes ", "resit "] {
            assert_eq!(type_blind(word, InputLayout::UsQwerty), word);
        }
    }

    #[test]
    fn english_that_only_reads_as_short_thai_words_stays_english() {
        // Each of these reads, at some point mid-word, as nothing but known
        // Thai words of one or two letters (`reavi` = พำ + ฟ + อ + ร), and
        // used to be rewritten to Thai before the typist could see why.
        for word in ["reavik ", "demerest "] {
            assert_eq!(type_blind(word, InputLayout::UsQwerty), word);
        }
    }

    #[test]
    fn english_on_the_thai_layout_comes_back_at_space() {
        assert_eq!(
            type_blind("correct ", InputLayout::ThaiKedmanee),
            "correct "
        );
    }

    #[test]
    fn english_on_the_english_layout_is_untouched() {
        let text = "the quick frontend codebase middleware ";
        assert_eq!(type_blind(text, InputLayout::UsQwerty), text);
    }

    #[test]
    fn mixed_sentence_needs_no_manual_switch() {
        let text = "ระบบนี้ใช้ middleware สำหรับจัดการข้อมูล ";
        assert_eq!(type_blind(text, InputLayout::UsQwerty), text);
    }

    #[test]
    fn with_righttype_off_the_wrong_layout_stays_wrong() {
        let mut e = Engine::new(dict::english(), dict::thai(), InputLayout::UsQwerty);
        e.enabled = false;
        for c in "สวัสดี".chars() {
            e.key(key_for(c).unwrap().0);
        }
        assert_eq!(e.screen, "l;ylfu");
    }

    /// Type keys (US names), with `<` meaning Backspace.
    fn keys(seq: &str) -> String {
        let mut e = Engine::new(dict::english(), dict::thai(), InputLayout::UsQwerty);
        for c in seq.chars() {
            match c {
                '<' => e.backspace(),
                ' ' => e.boundary(' '),
                _ => e.key(c),
            }
        }
        e.screen
    }

    #[test]
    fn backspacing_back_into_a_thai_word_keeps_the_text_consistent() {
        // Whatever RightType decides, the screen after a Backspace must hold
        // either the raw keys or their Thai reading — never a mix of both.
        for (typed, raw, thai) in [
            ("l;ylfux<", "l;ylfu", "สวัสดี"),
            ("l;ylfu8iy[x<", "l;ylfu8iy[", "สวัสดีครับ"),
            ("mujouj1<", "mujouj", "ที่นี่"),
        ] {
            let got = keys(typed);
            assert!(got == raw || got == thai, "{typed:?} -> {got:?}");
        }
    }

    /// The physical keys that type `text` (Thai through Kedmanee, the rest as is).
    fn keys_of(text: &str) -> String {
        text.chars()
            .map(|c| key_for(c).map_or(c, |(k, _)| k))
            .collect()
    }

    #[test]
    fn random_typing_with_backspaces_never_mixes_keys_and_text() {
        // Keys that often spell Thai (home row, vowels, tone marks) mixed with
        // letters that pull toward English, plus Backspace (`<`).
        const ALPHABET: &[u8] = b"l;yfu8i[mjohkdrtgpbnvs<<";
        let mut state: u64 = 0x5eed_1234_abcd_ef01;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..3000 {
            let len = 3 + (next() % 12) as usize;
            let seq: String = (0..len)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize] as char)
                .collect();
            // The keys the typist meant, after their own Backspaces.
            let mut intended = String::new();
            for c in seq.chars() {
                if c == '<' {
                    intended.pop();
                } else {
                    intended.push(c);
                }
            }
            let screen = keys(&format!("{seq} "));
            let word = screen.trim_end_matches(' ');
            assert_eq!(
                keys_of(word),
                intended,
                "round {round}: {seq:?} left {word:?} on screen"
            );
        }
    }

    #[test]
    fn random_multi_word_streams_keep_every_key() {
        // Whole streams: spaces, corrections at the boundary and the layout
        // switches that follow them. The screen may show any mix of raw and
        // converted words, but always exactly the keys that were pressed.
        const ALPHABET: &[u8] = b"l;yfu8i[mjohkdrtgpbnvsaec   <";
        let mut state: u64 = 0x0dd_ba11_c0ff_ee00;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for round in 0..1500 {
            let len = 10 + (next() % 60) as usize;
            let seq: String = (0..len)
                .map(|_| ALPHABET[(next() % ALPHABET.len() as u64) as usize] as char)
                .collect();
            let mut intended = String::new();
            for c in seq.chars() {
                if c == '<' {
                    intended.pop();
                } else {
                    intended.push(c);
                }
            }
            let screen = keys(&format!("{seq} "));
            assert_eq!(
                keys_of(screen.trim_end_matches(' ')),
                intended.trim_end_matches(' '),
                "round {round}: {seq:?} left {screen:?}"
            );
        }
    }

    #[test]
    fn every_thai_character_used_in_prose_has_a_key() {
        for c in "กขฃคฅฆงจฉชซฌญฎฏฐฑฒณดตถทธนบปผฝพฟภมยรฤลฦวศษสหฬอฮะัาำิีึืุูเแโใไๅๆ็่้๊๋์".chars()
        {
            assert!(key_for(c).is_some(), "{c}");
        }
    }
}
