//! English typos typed on the US layout (accuracy plan P0): one edit (a
//! neighbouring key, two letters swapped, or one dropped) applied to the
//! commonest English words, kept only when the result is not itself a word,
//! then typed through the hook's Auto-mode replay.
//!
//!     cargo run --release --example survey_typos [-- --list]
//!
//! The false-positive audit checks `detect_token` alone; this goes through the
//! live path too, where about two in five of the changes come from.

use righttype::dict;
use righttype::policy::InputLayout;
use righttype::sim::Engine;

fn has_thai(s: &str) -> bool {
    s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// Xorshift, so the same words give the same typos on every run.
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

/// The keys left and right of `c` on its row of a QWERTY keyboard.
fn neighbours(c: char) -> Vec<char> {
    let mut out = Vec::new();
    for row in ["qwertyuiop", "asdfghjkl", "zxcvbnm"] {
        if let Some(i) = row.find(c) {
            let keys = row.as_bytes();
            if i > 0 {
                out.push(keys[i - 1] as char);
            }
            if i + 1 < keys.len() {
                out.push(keys[i + 1] as char);
            }
        }
    }
    out
}

fn main() {
    let list = std::env::args().any(|a| a == "--list");
    let (en, th) = (dict::english(), dict::thai());
    let words: Vec<&str> = dict::english_words()
        .take(5000)
        .filter(|w| w.len() >= 4 && w.chars().all(|c| c.is_ascii_lowercase()))
        .collect();
    let mut rng = Rng(0x1234_5678_9abc_def1);
    let (mut total, mut mid_word, mut at_space) = (0usize, 0usize, 0usize);
    for word in &words {
        for _ in 0..4 {
            let mut keys: Vec<char> = word.chars().collect();
            let i = rng.below(keys.len());
            match rng.below(3) {
                0 => {
                    let near = neighbours(keys[i]);
                    if near.is_empty() {
                        continue;
                    }
                    keys[i] = near[rng.below(near.len())];
                }
                1 => {
                    if i + 1 >= keys.len() {
                        continue;
                    }
                    keys.swap(i, i + 1);
                }
                _ => {
                    keys.remove(i);
                }
            }
            let typo: String = keys.into_iter().collect();
            if en.contains(&typo) {
                continue;
            }
            total += 1;
            let mut e = Engine::new(en, th, InputLayout::UsQwerty);
            let mut seen_thai = false;
            for c in typo.chars() {
                e.key(c);
                seen_thai |= has_thai(&e.screen);
            }
            e.boundary(' ');
            let changed = e.screen.trim_end() != typo;
            mid_word += usize::from(seen_thai);
            at_space += usize::from(changed);
            if list && (changed || seen_thai) {
                println!("{typo:14} -> {}", e.screen.trim_end());
            }
        }
    }
    let pct = |n: usize| 100.0 * n as f64 / total.max(1) as f64;
    println!("English typos typed on the US layout (not words themselves): {total}");
    println!(
        "  Thai showing at some point while typed: {mid_word} ({:.2}%)",
        pct(mid_word)
    );
    println!("  changed at the space: {at_space} ({:.2}%)", pct(at_space));
}
