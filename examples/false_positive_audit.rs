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
//! - Thai phrases: 200,000 random runs of 2–4 dictionary words, as Thai is
//!   written (no spaces).
//! - English words, every one in the bundled dictionary, typed on the English
//!   layout, alone and with `:`, `?`, `"`, `)` after them.

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
    righttype::english::warm();
    let en = dict::english();
    let th = dict::thai();
    let thai_words: Vec<&str> = include_str!("../assets/th_words.txt")
        .lines()
        .map(str::trim)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .collect();
    let english_words: Vec<&str> = include_str!("../assets/en_words.txt")
        .lines()
        .map(|l| l.split_whitespace().next().unwrap_or(""))
        .filter(|w| !w.is_empty())
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
    if let Some(path) = dump {
        std::fs::write(path, dumped).expect("writing dump");
    }
}
