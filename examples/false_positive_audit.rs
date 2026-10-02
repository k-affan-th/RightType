//! How often does the boundary policy change text that was typed *correctly*?
//!
//!     cargo run --release --example false_positive_audit
//!
//! Every count below is a word RightType would have rewritten although the
//! typist was on the right layout. The target is zero; anything else is
//! listed so it can be judged.
//!
//! - Thai words, every one in the bundled dictionary, typed on the Thai
//!   layout, alone and followed by `:`.
//! - Unknown Thai: each dictionary word with one letter changed, kept only if
//!   the result is neither a word nor segmentable — the names and typos the
//!   dictionary cannot vouch for.
//! - Fix text: 20,000 correct mixed Thai/English sentences through `repair`.
//! - Numbers, dates, times, prices and symbols on the English layout: 0 may change.
//! - The same dictionary checks with Thai Pattachote, and with the UK English keyboard.
//! - Thai phrases: 200,000 random runs of 2–4 dictionary words, as Thai is
//!   written (no spaces).
//! - English words, every one in the bundled dictionary, typed on the English
//!   layout, alone and with `:`, `?`, `"`, `)` after them.
//!
//! With `--check` it is CI's quality gate (RightType 2.0, M0): it exits
//! non-zero when a dictionary word or phrase typed correctly is changed, when
//! unknown-Thai changes exceed their budget, or when recall on wrong-layout
//! dictionary words drops below its floor. Raise a floor when recall
//! improves; lowering one needs a reason in `docs/TYPING_BENCHMARK.md`.

/// Unknown Thai strings (one random letter changed) that may be changed:
/// 90 of 50,800 at the time of writing (0.18 %).
const MAX_UNKNOWN_THAI_CHANGED: usize = 120;
/// Thai dictionary words typed on the English layout that must come back.
const MIN_THAI_RECALL: usize = 60_550;
/// Thai recall with the Pattachote table (2.0 M6), measured when it was added.
const MIN_PATTACHOTE_THAI_RECALL: usize = 60_150;
/// English recall with the Pattachote table (2.0 M6), measured 86,833.
const MIN_PATTACHOTE_ENGLISH_RECALL: usize = 86_750;
/// Thai recall with the Manoonchai table (2.2), measured 60,146 when it was
/// added.
const MIN_MANOONCHAI_THAI_RECALL: usize = 60_050;
/// English dictionary words typed on the Thai layout that must come back.
const MIN_ENGLISH_RECALL: usize = 86_600;

use righttype::dict;
use righttype::policy::{detect_token, InputLayout};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn main() {
    let check = std::env::args().any(|a| a == "--check");
    let mut failures: Vec<String> = Vec::new();
    righttype::english::warm();
    let en = dict::english();
    let th = dict::thai();
    let thai_words: Vec<&str> = include_str!("../assets/th_words.txt")
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .collect();
    // The bundled technical terms are part of the English dictionary too.
    let english_words: Vec<&str> = include_str!("../assets/en_words.txt")
        .lines()
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .filter(|w| !w.is_empty())
        .chain(dict::tech_terms())
        .collect();
    let mut rng = Rng(0xC0FF_EE11_2233_4455);
    let dump = std::env::var("AUDIT_DUMP").ok();
    let mut dumped = String::new();
    let mut report = |name: &str, total: usize, hits: &[(String, String)]| {
        println!("{name}: {} of {total} changed", hits.len());
        for (from, to) in hits {
            dumped.push_str(&format!("{name}\t{from}\t{to}\n"));
        }
        for (from, to) in hits.iter().take(15) {
            println!("    {from:?} -> {to:?}");
        }
    };

    let th_layout =
        |t: &str| detect_token(t, InputLayout::ThaiKedmanee, en, th).map(|d| d.corrected);
    let us_layout = |t: &str| detect_token(t, InputLayout::UsQwerty, en, th).map(|d| d.corrected);

    // Thai dictionary words on the Thai layout.
    let mut hits = Vec::new();
    let mut total = 0;
    for w in &thai_words {
        for t in [w.to_string(), format!("{w}:")] {
            total += 1;
            if let Some(c) = th_layout(&t) {
                hits.push((t, c));
            }
        }
    }
    report(
        "Thai dictionary words (and `word:`) on the Thai layout",
        total,
        &hits,
    );
    if !hits.is_empty() {
        failures.push(format!(
            "{} Thai dictionary words changed (must be 0)",
            hits.len()
        ));
    }

    // Unknown Thai words.
    let letters: Vec<char> = "กขคงจฉชซญดตถทธนบปผพฟภมยรลวศษสหอฮะาำิีึืุูเแโใไ่้๊๋็์ั".chars().collect();
    let mut hits = Vec::new();
    let mut total = 0;
    for w in &thai_words {
        let mut chars: Vec<char> = w.chars().collect();
        if chars.len() < 3 {
            continue;
        }
        let i = rng.below(chars.len());
        chars[i] = letters[rng.below(letters.len())];
        let t: String = chars.into_iter().collect();
        if th.contains(&t) || righttype::segment::is_fully_known(&t, th) {
            continue;
        }
        total += 1;
        if let Some(c) = th_layout(&t) {
            hits.push((t, c));
        }
    }
    report(
        "Unknown Thai words (one letter changed) on the Thai layout",
        total,
        &hits,
    );
    if hits.len() > MAX_UNKNOWN_THAI_CHANGED {
        failures.push(format!(
            "{} unknown Thai words changed (budget {MAX_UNKNOWN_THAI_CHANGED})",
            hits.len()
        ));
    }

    // Thai phrases.
    let mut hits = Vec::new();
    let total = 200_000;
    for _ in 0..total {
        let n = 2 + rng.below(3);
        let t: String = (0..n)
            .map(|_| thai_words[rng.below(thai_words.len())])
            .collect();
        if let Some(c) = th_layout(&t) {
            hits.push((t, c));
        }
    }
    report("Thai phrases (2–4 words) on the Thai layout", total, &hits);
    if !hits.is_empty() {
        failures.push(format!("{} Thai phrases changed (must be 0)", hits.len()));
    }

    // English words on the English layout.
    let mut hits = Vec::new();
    let mut total = 0;
    for w in &english_words {
        for t in [
            w.to_string(),
            format!("{w}:"),
            format!("{w}?"),
            format!("\"{w}\""),
            format!("({w})"),
        ] {
            total += 1;
            if let Some(c) = us_layout(&t) {
                hits.push((t, c));
            }
        }
    }
    report(
        "English dictionary words (with punctuation) on the English layout",
        total,
        &hits,
    );
    if !hits.is_empty() {
        failures.push(format!("{} English words changed (must be 0)", hits.len()));
    }

    // Letter-less text typed on the English layout (numbers, dates, times,
    // prices, emoticons, code punctuation) must stay as typed, even where its
    // keys spell a Thai word on Kedmanee (`86` is คุ, `5,` is จม).
    let mut hits = Vec::new();
    let mut samples: Vec<String> = Vec::new();
    for n in 0..100_000u32 {
        for s in [
            n.to_string(),
            format!("{n}%"),
            format!("{n}."),
            format!("{n},"),
            format!("${n}"),
            format!("({n})"),
            format!("#{n}"),
        ] {
            samples.push(s);
        }
    }
    for a in 0..100u32 {
        for b in 0..100u32 {
            for s in [
                format!("{a}.{b}"),
                format!("{a}:{b:02}"),
                format!("{a}/{b}"),
                format!("{a}-{b}"),
                format!("{a},{b:03}"),
            ] {
                samples.push(s);
            }
        }
    }
    for s in [
        ":)", ":(", ";)", ":-)", ":-(", "<3", "^^", "^_^", "...", "..", "--", "!!", "??", "?!",
        "->", "<-", "=>", "(:", "):", "//", "/*", "*/", "&&", "||", "==", "!=", "<=", ">=", "++",
        "::", ";;", "[]", "{}", "()", "<>", "''", "\"\"", "``", "~~", "**", "__", "##", ".0",
    ] {
        samples.push(s.to_string());
    }
    for t in &samples {
        if let Some(c) = us_layout(t) {
            hits.push((t.clone(), c));
        }
    }
    report(
        "Numbers, dates and symbols on the English layout",
        samples.len(),
        &hits,
    );
    if !hits.is_empty() {
        failures.push(format!(
            "{} numbers/symbols changed (must be 0)",
            hits.len()
        ));
    }

    // Fix text (2.0): correctly typed mixed Thai/English sentences pasted into
    // the Fix text window must come back unchanged.
    let mut hits = Vec::new();
    let total = 20_000;
    for _ in 0..total {
        let n = 4 + rng.below(7);
        let sentence: Vec<&str> = (0..n)
            .map(|_| {
                if rng.below(2) == 0 {
                    thai_words[rng.below(thai_words.len())]
                } else {
                    english_words[rng.below(english_words.len())]
                }
            })
            .collect();
        let text = sentence.join(" ");
        let fixed = righttype::repair::repair(&text, en, th);
        if !fixed.changes.is_empty() {
            hits.push((text, fixed.text));
        }
    }
    report("Fix text: mixed correct sentences", total, &hits);
    if !hits.is_empty() {
        failures.push(format!(
            "{} Fix text sentences changed (must be 0)",
            hits.len()
        ));
    }

    // Recall: the same dictionaries typed on the *wrong* layout.
    let mut fixed = 0;
    for w in &thai_words {
        let typed = righttype::layout::th_to_en(w);
        if us_layout(&typed).as_deref() == Some(*w) {
            fixed += 1;
        }
    }
    println!(
        "Recall: Thai dictionary words typed on the English layout, fixed: {fixed} of {}",
        thai_words.len()
    );
    if fixed < MIN_THAI_RECALL {
        failures.push(format!("Thai recall {fixed} below floor {MIN_THAI_RECALL}"));
    }
    let mut fixed = 0;
    for w in &english_words {
        let typed = righttype::layout::en_to_th(w);
        if th_layout(&typed).as_deref() == Some(*w) {
            fixed += 1;
        }
    }
    println!(
        "Recall: English dictionary words typed on the Thai layout, fixed: {fixed} of {}",
        english_words.len()
    );
    if fixed < MIN_ENGLISH_RECALL {
        failures.push(format!(
            "English recall {fixed} below floor {MIN_ENGLISH_RECALL}"
        ));
    }
    if let Some(path) = dump {
        std::fs::write(path, dumped).expect("writing dump");
    }
    // The same core checks with Thai Pattachote as the Thai layout (2.0 M6):
    // no correctly typed Thai or English dictionary word may change, and
    // recall has its own floor.
    righttype::layout::set_thai_variant(righttype::layout::ThaiVariant::Pattachote);
    let changed_thai = thai_words.iter().filter(|w| th_layout(w).is_some()).count();
    let changed_english = english_words
        .iter()
        .filter(|w| us_layout(w).is_some())
        .count();
    let recall_thai = thai_words
        .iter()
        .filter(|w| us_layout(&righttype::layout::th_to_en(w)).as_deref() == Some(**w))
        .count();
    let recall_english = english_words
        .iter()
        .filter(|w| th_layout(&righttype::layout::en_to_th(w)).as_deref() == Some(**w))
        .count();
    righttype::layout::set_thai_variant(righttype::layout::ThaiVariant::Kedmanee);
    println!(
        "Pattachote: Thai dictionary words changed {changed_thai}, English dictionary words changed {changed_english}; recall Thai {recall_thai} of {}, English {recall_english} of {}",
        thai_words.len(),
        english_words.len()
    );
    if changed_thai + changed_english > 0 {
        failures.push(format!(
            "Pattachote: {} dictionary words changed (must be 0)",
            changed_thai + changed_english
        ));
    }
    if recall_thai < MIN_PATTACHOTE_THAI_RECALL {
        failures.push(format!(
            "Pattachote Thai recall {recall_thai} below floor {MIN_PATTACHOTE_THAI_RECALL}"
        ));
    }
    if recall_english < MIN_PATTACHOTE_ENGLISH_RECALL {
        failures.push(format!(
            "Pattachote English recall {recall_english} below floor {MIN_PATTACHOTE_ENGLISH_RECALL}"
        ));
    }
    // And with Thai Manoonchai (2.2): no dictionary word may change. Its
    // recall is printed; the floor is set from the first measurement.
    righttype::layout::set_thai_variant(righttype::layout::ThaiVariant::Manoonchai);
    let changed_thai = thai_words.iter().filter(|w| th_layout(w).is_some()).count();
    let changed_english = english_words
        .iter()
        .filter(|w| us_layout(w).is_some())
        .count();
    let recall_thai = thai_words
        .iter()
        .filter(|w| us_layout(&righttype::layout::th_to_en(w)).as_deref() == Some(**w))
        .count();
    let recall_english = english_words
        .iter()
        .filter(|w| th_layout(&righttype::layout::en_to_th(w)).as_deref() == Some(**w))
        .count();
    righttype::layout::set_thai_variant(righttype::layout::ThaiVariant::Kedmanee);
    println!(
        "Manoonchai: Thai dictionary words changed {changed_thai}, English dictionary words changed {changed_english}; recall Thai {recall_thai} of {}, English {recall_english} of {}",
        thai_words.len(),
        english_words.len()
    );
    if changed_thai + changed_english > 0 {
        failures.push(format!(
            "Manoonchai: {} dictionary words changed (must be 0)",
            changed_thai + changed_english
        ));
    }
    if recall_thai < MIN_MANOONCHAI_THAI_RECALL {
        failures.push(format!(
            "Manoonchai Thai recall {recall_thai} below floor {MIN_MANOONCHAI_THAI_RECALL}"
        ));
    }

    // UK English keyboard (2.0 M6): English typed correctly must not change,
    // with or without the punctuation the UK layout moves.
    righttype::layout::set_english_variant(righttype::layout::EnglishVariant::Uk);
    let changed_uk = english_words
        .iter()
        .flat_map(|w| {
            [
                w.to_string(),
                format!("\"{w}\""),
                format!("{w}@"),
                format!("£{w}"),
            ]
        })
        .filter(|t| us_layout(t).is_some())
        .count();
    righttype::layout::set_english_variant(righttype::layout::EnglishVariant::Us);
    println!("UK English keyboard: English dictionary words changed {changed_uk}");
    if changed_uk > 0 {
        failures.push(format!(
            "UK: {changed_uk} English words changed (must be 0)"
        ));
    }

    if check {
        if failures.is_empty() {
            println!("quality gate: PASS");
        } else {
            for f in &failures {
                eprintln!("quality gate: FAIL — {f}");
            }
            std::process::exit(1);
        }
    }
}
