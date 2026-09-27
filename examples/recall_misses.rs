//! Lists the Thai dictionary words that do not come back when typed on the
//! English layout, most frequent first when a `word<TAB>count` file is given
//! (for example PyThaiNLP's CC0 `tnc_freq.txt`): `cargo run --release --example
//! recall_misses -- tnc_freq.txt`.
use righttype::dict;
use righttype::layout::th_to_en;
use righttype::policy::{detect_token, InputLayout};
use std::collections::HashMap;

fn main() {
    let freq: HashMap<String, u64> = std::env::args()
        .nth(1)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .filter_map(|l| {
                    let (w, n) = l.split_once('\t')?;
                    Some((w.to_string(), n.trim().parse().ok()?))
                })
                .collect()
        })
        .unwrap_or_default();
    let en = dict::english();
    let th = dict::thai();
    let mut misses = Vec::new();
    for w in include_str!("../assets/th_words.txt")
        .lines()
        .map(str::trim)
    {
        if w.is_empty() || w.starts_with('#') {
            continue;
        }
        let typed = th_to_en(w);
        let got = detect_token(&typed, InputLayout::UsQwerty, en, th).map(|d| d.corrected);
        if got.as_deref() != Some(w) {
            misses.push((freq.get(w).copied().unwrap_or(0), w, typed, got));
        }
    }
    misses.sort_by_key(|m| std::cmp::Reverse(m.0));
    println!("{} misses", misses.len());
    for (f, w, t, g) in misses.iter().take(60) {
        println!("{f}\t{w}\t{t}\t{g:?}\ten={}", en.contains(t));
    }
}
