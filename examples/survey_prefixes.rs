//! Half-typed names in a search box (accuracy plan P0): every prefix of every
//! name in `data/survey_names.txt`, in both cases, typed on the US layout
//! through the hook's Auto-mode replay and then ended the way a search box
//! ends a query.
//!
//!     cargo run --release --example survey_prefixes [-- --list]
//!
//! A prefix is an error if Thai shows while it is typed (a search-as-you-type
//! box queries on every keystroke) or if the ending changes it. Counts are per
//! ending: Enter, Space, Tab, and the caret moving away (an arrow key, a click),
//! where nothing is judged at all and what is on screen stays.

use righttype::dict;
use righttype::policy::InputLayout;
use righttype::sim::Engine;

fn has_thai(s: &str) -> bool {
    s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

#[derive(Clone, Copy)]
enum End {
    Enter,
    Space,
    Tab,
    CaretMoved,
}

fn main() {
    let list = std::env::args().any(|a| a == "--list");
    let (en, th) = (dict::english(), dict::thai());
    let names: Vec<&str> = include_str!("data/survey_names.txt")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .flat_map(str::split_whitespace)
        .collect();
    let ends = [
        ("Enter", End::Enter),
        ("Space", End::Space),
        ("Tab", End::Tab),
        ("caret moved", End::CaretMoved),
    ];
    let mut prefixes = 0usize;
    let mut mid_word = 0usize;
    let mut changed = [0usize; 4];
    let mut routes = righttype::ledger::Ledger::new();
    for name in names {
        for word in [name.to_string(), name.to_lowercase()] {
            let letters: Vec<char> = word.chars().collect();
            for len in 2..=letters.len() {
                let prefix: String = letters[..len].iter().collect();
                prefixes += 1;
                let mut live = Engine::new(en, th, InputLayout::UsQwerty);
                prefix.chars().for_each(|c| live.key(c));
                if has_thai(&live.screen) {
                    mid_word += 1;
                    if list {
                        println!("MID   {prefix:16} -> {}", live.screen);
                    }
                }
                for (i, (label, end)) in ends.iter().enumerate() {
                    let mut e = Engine::new(en, th, InputLayout::UsQwerty);
                    prefix.chars().for_each(|c| e.key(c));
                    match end {
                        End::Enter => e.boundary(char::from(10u8)),
                        End::Space => e.boundary(' '),
                        End::Tab => e.boundary(char::from(9u8)),
                        End::CaretMoved => e.caret_moved(),
                    }
                    for (cause, counts) in e.ledger.cells() {
                        (0..counts.total())
                            .for_each(|_| routes.record(*cause, righttype::ledger::Outcome::Kept));
                    }
                    let shown = e.screen.trim_end();
                    if shown != prefix {
                        changed[i] += 1;
                        if list {
                            println!("{label:5} {prefix:16} -> {shown}");
                        }
                    }
                }
            }
        }
    }
    println!("Name prefixes typed on the US layout: {prefixes}");
    println!("  Thai showing while it is typed: {mid_word}");
    for ((label, _), n) in ends.iter().zip(changed) {
        println!("  changed when ended with {label}: {n}");
    }

    // Which route the rewrites took, from the replay's ledger (no text in it).
    println!("  by route, over the prefixes that were rewritten:");
    for line in routes.lines() {
        println!("    {line}");
    }
}
