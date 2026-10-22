//! The Thai text tools and detectors added in 2.5 never panic and keep their
//! simple promises on text typed with the wrong layout, tone marks out of
//! place, mixed Thai/English, Thai digits and stray symbols.

use righttype::{
    charsets, detect, dict, english, layout, repair, secret, segment, spelling, thai_text,
};

/// The same strings on every run.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

const PIECES: &[&str] = &[
    "ก",
    "ข",
    "ส",
    "ว",
    "ด",
    "ี",
    "่",
    "้",
    "์",
    "ั",
    "ำ",
    "ๆ",
    "ๅ",
    "เ",
    "แ",
    "ใ",
    "ไ",
    "สวัสดี",
    "เหตุการณ์",
    "เหตุการณ",
    "ครับ",
    "พ.ศ.",
    "พ.ศ. 2569",
    "ค.ศ. 2026",
    "๑",
    "๒๕๖๙",
    "123",
    "1,250.75",
    "0.5",
    "บาท",
    "สตางค์",
    "hello",
    "Hello",
    "GitHub",
    "kubernetes",
    "ok",
    "a",
    "I",
    "l;ylfu",
    "hk",
    "abandon",
    "abandon abandon",
    " ",
    " ",
    "  ",
    "\t",
    "\n",
    ".",
    ",",
    "-",
    "/",
    "@",
    "_",
    "(",
    ")",
    "é",
    "😀",
    "\u{200b}",
    "\u{0e3a}",
    "\u{0}",
];

fn random(rng: &mut Lcg) -> String {
    let n = rng.below(10);
    (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect()
}

#[test]
fn thai_text_tools_never_panic() {
    let mut rng = Lcg(20_261_021);
    for _ in 0..40_000 {
        let s = random(&mut rng);
        let _ = thai_text::normalize(&s);
        let _ = thai_text::swap_era(&s);
        let _ = thai_text::number_words(&s);
        let _ = thai_text::baht_words(&s);
        let _ = thai_text::tidy_spacing(&s);
    }
}

#[test]
fn detectors_never_panic() {
    let en = dict::english();
    let th = dict::thai();
    let mut rng = Lcg(20_261_022);
    for _ in 0..30_000 {
        let s = random(&mut rng);
        let _ = detect::detect(&s, en, th);
        let _ = spelling::fix(&s, th);
        let _ = segment::segment(&s, th);
        let _ = segment::is_fully_known(&s, th);
        let _ = segment::is_viable_prefix(&s, th);
        let _ = segment::last_word_to_delete(&s, th);
        let _ = repair::repair(&s, en, th);
        let _ = secret::classify_token(&s);
        let _ = secret::is_bip39_word(&s);
        let _ = english::is_word(&s, en);
        let _ = english::is_compound(&s);
        let _ = english::is_tech_term(&s);
        let _ = layout::en_to_th(&s);
        let _ = layout::th_to_en(&s);
        let _ = layout::swap_digits(&s);
        let _ = layout::upper_case(&s);
    }
}

#[test]
fn segments_always_add_back_up_to_the_text() {
    let th = dict::thai();
    let mut rng = Lcg(20_261_023);
    for _ in 0..30_000 {
        let s = random(&mut rng);
        let joined: String = segment::segment(&s, th)
            .into_iter()
            .map(|seg| seg.text)
            .collect();
        assert_eq!(joined, s, "segmenting {s:?} lost or changed text");
    }
}

#[test]
fn character_sets_strip_never_panics_whatever_the_position() {
    let mut rng = Lcg(20_261_024);
    for _ in 0..20_000 {
        let typed = random(&mut rng);
        let set: Vec<String> = (0..rng.below(5)).map(|_| random(&mut rng)).collect();
        for at in [0, 1, set.len(), set.len() + 1, usize::MAX / 2] {
            let _ = charsets::strip(&typed, &set, at);
        }
    }
}

#[test]
fn normalizing_twice_changes_nothing() {
    let mut rng = Lcg(20_261_025);
    for _ in 0..40_000 {
        let s = random(&mut rng);
        let once = thai_text::normalize(&s);
        assert_eq!(
            thai_text::normalize(&once),
            once,
            "normalizing {s:?} is not stable"
        );
    }
}

#[test]
fn a_tone_before_a_doubled_vowel_leaves_one_vowel_then_the_tone() {
    // `ก` + tone + vowel + the same vowel again.
    assert_eq!(
        thai_text::normalize("ก\u{e48}\u{e35}\u{e35}"),
        "ก\u{e35}\u{e48}"
    );
}

#[test]
fn a_long_run_of_misplaced_marks_is_normalized_in_one_go() {
    // More than nine marks before a vowel: each pass moves only the last
    // mark across it, so a fixed number of passes is not enough.
    let run = "ก\u{e48}\u{e49}\u{e4a}\u{e4b}\u{e48}\u{e49}\u{e4a}\u{e4b}\u{e48}\u{e49}\u{e35}";
    let once = thai_text::normalize(run);
    assert_eq!(thai_text::normalize(&once), once);
    // The vowel ends up first, then the tone marks.
    assert!(once.starts_with("ก\u{e35}"), "{once:?}");
}

#[test]
fn switching_layout_and_back_returns_the_word() {
    for w in dict::english_words().take(30_000) {
        assert_eq!(layout::th_to_en(&layout::en_to_th(w)), w, "english {w:?}");
    }
    for w in dict::thai_words().take(30_000) {
        assert_eq!(layout::en_to_th(&layout::th_to_en(w)), w, "thai {w:?}");
    }
}

#[test]
fn repair_leaves_every_correct_dictionary_word_alone() {
    let en = dict::english();
    let th = dict::thai();
    for w in dict::english_words()
        .take(30_000)
        .chain(dict::thai_words().take(30_000))
    {
        assert_eq!(repair::repair(w, en, th).text, w, "{w:?}");
    }
}
