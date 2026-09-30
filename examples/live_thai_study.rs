//! What the live (mid-word) Thai conversion does with words RightType does
//! not know, typed without switching layout — the two sides of one trade-off:
//!
//! * **Unknown English** typed on the English layout must stay English.
//!   Built from the English list: prefix/suffix forms the list lacks
//!   (`re` + `login`, `upload` + `ers`, …) and one-letter typos.
//! * **Unknown Thai** typed on the English layout (the typist forgot to
//!   switch) should still arrive as Thai. Built from the Thai list with one
//!   letter changed, kept only when the result is not a known word and not
//!   fully segmentable.
//!
//! Each token goes through the full pipeline replay (`sim::Engine`), then a
//! space. It also reports, for English tokens that ended as Thai, how many
//! were started by a reading made only of one- and two-letter Thai words
//! (พำ + สน + เร for `relogi`) — the evidence the short-segment rule (ข)
//! would refuse — and how many Thai tokens that rule would lose.
//!
//!     cargo run --release --example live_thai_study

use righttype::dict::{self, Dictionary};
use righttype::layout::{en_to_th, th_to_en};
use righttype::policy::InputLayout;
use righttype::segment;
use righttype::sim::Engine;

const SAMPLE: usize = 20_000;

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Screen after typing `keys` (named by their US character) and a space.
fn type_word(keys: &str, en: &Dictionary, th: &Dictionary) -> String {
    let mut e = Engine::new(en, th, InputLayout::UsQwerty);
    for c in keys.chars() {
        e.key(c);
    }
    e.boundary(' ');
    e.screen.trim_end().to_string()
}

fn is_thai(s: &str) -> bool {
    s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// Does some prefix of `keys` read as Thai made only of known words of at
/// most two letters (the reading rule ข would refuse to act on)?
fn short_segments_only(keys: &str, th: &Dictionary) -> bool {
    let chars: Vec<char> = keys.chars().collect();
    (2..=chars.len()).any(|n| {
        let prefix: String = chars[..n].iter().collect();
        let thai = en_to_th(&prefix);
        let segs = segment::segment(&thai, th);
        segs.len() >= 2 && segs.iter().all(|s| s.known && s.text.chars().count() <= 2)
    })
}

fn main() {
    let en = dict::english();
    let th = dict::thai();
    let clean = |l: &str| l.trim().to_string();
    let en_words: Vec<String> = include_str!("../assets/en_words.txt")
        .lines()
        .map(clean)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .collect();
    let th_words: Vec<String> = include_str!("../assets/th_words.txt")
        .lines()
        .map(clean)
        .filter(|w| !w.is_empty() && !w.starts_with('#'))
        .collect();
    let mut rng = Rng(0x5eed_1234_abcd_0001);

    // Unknown English.
    let prefixes = [
        "re", "un", "pre", "de", "over", "non", "mis", "sub", "auto", "multi",
    ];
    let suffixes = ["s", "ed", "er", "ers", "ing", "able", "less"];
    let mut unknown_en = Vec::new();
    while unknown_en.len() < SAMPLE {
        let w = &en_words[rng.below(en_words.len())];
        if w.len() < 3 || !w.chars().all(|c| c.is_ascii_lowercase()) {
            continue;
        }
        let t = match rng.below(3) {
            0 => format!("{}{w}", prefixes[rng.below(prefixes.len())]),
            1 => format!("{w}{}", suffixes[rng.below(suffixes.len())]),
            _ => {
                let mut c: Vec<char> = w.chars().collect();
                let i = rng.below(c.len());
                c[i] = (b'a' + rng.below(26) as u8) as char;
                c.into_iter().collect()
            }
        };
        if !en.contains(&t) {
            unknown_en.push(t);
        }
    }

    // Unknown Thai.
    let letters: Vec<char> = "กขคงจฉชซญดตถทธนบปผพฟภมยรลวศษสหอฮะาำิีึืุูเแโใไ่้็์ั".chars().collect();
    let mut unknown_th = Vec::new();
    while unknown_th.len() < SAMPLE {
        let w = &th_words[rng.below(th_words.len())];
        let mut c: Vec<char> = w.chars().collect();
        if c.len() < 4 {
            continue;
        }
        let i = rng.below(c.len());
        c[i] = letters[rng.below(letters.len())];
        let t: String = c.into_iter().collect();
        if th.contains(&t) || segment::is_fully_known(&t, th) {
            continue;
        }
        unknown_th.push(t);
    }

    let mut en_to_thai = 0;
    let mut en_short = 0;
    let mut examples = Vec::new();
    for t in &unknown_en {
        let got = type_word(t, en, th);
        if is_thai(&got) {
            en_to_thai += 1;
            if short_segments_only(t, th) {
                en_short += 1;
            }
            if examples.len() < 12 {
                examples.push(format!("{t} → {got}"));
            }
        }
    }
    let mut th_kept = 0;
    let mut th_kept_short = 0;
    for t in &unknown_th {
        let keys = th_to_en(t);
        let got = type_word(&keys, en, th);
        if got == *t {
            th_kept += 1;
            if short_segments_only(&keys, th) {
                th_kept_short += 1;
            }
        }
    }
    let pct = |n: usize| 100.0 * n as f64 / SAMPLE as f64;
    println!(
        "Unknown English on the English layout: {en_to_thai} of {SAMPLE} ended as Thai ({:.2}%)",
        pct(en_to_thai)
    );
    println!("  … of which a reading of only 1–2-letter Thai words was available: {en_short}");
    for e in &examples {
        println!("    {e}");
    }
    println!(
        "Unknown Thai on the English layout: {th_kept} of {SAMPLE} arrived as Thai ({:.2}%)",
        pct(th_kept)
    );
    println!("  … of which a reading of only 1–2-letter Thai words was available: {th_kept_short}");
}
