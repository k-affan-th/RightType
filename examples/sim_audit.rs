//! CI gate for the accuracy surveys: replays text through the OS-free
//! `sim::Engine` (the hook's Auto-mode pipeline) and fails when a measured
//! error rate rises above its floor.
//!
//!     cargo run --release --example sim_audit [-- --check]
//!
//! Three families, all typed without ever switching layout by hand:
//!
//! 1. Half-typed names (`data/survey_names.txt`, as `survey_prefixes`): no
//!    prefix may change at Enter, Space, Tab or a caret move (floor 0).
//! 2. English typos (as `survey_typos`): the share changed at the space and the
//!    share that show Thai while being typed.
//! 3. Text corpora under `data/`, one file per category. English categories
//!    are typed on the US layout and must stay as typed; the Thai category is
//!    typed on the Thai layout and must stay Thai. Per category, counted in
//!    words: changed at the end, and shown in the wrong script mid-word.
//!
//! Ratchet only: a floor may be tightened, never loosened without a written
//! reason beside it. The floors below are what HEAD measures; the replay is
//! deterministic, so there is no slack unless a comment says so.

use righttype::dict;
use righttype::policy::InputLayout;
use righttype::sim::{key_for, Engine};

fn has_thai(s: &str) -> bool {
    s.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// Xorshift, so the same words give the same typos on every run (this and the
/// typo generator mirror `survey_typos`).
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

/// A failed floor: what was measured, against what.
struct Check {
    label: String,
    measured: String,
    floor: String,
    ok: bool,
}

struct Report {
    checks: Vec<Check>,
}

impl Report {
    /// Counts: `measured <= floor`.
    fn count(&mut self, label: &str, measured: usize, floor: usize) {
        self.checks.push(Check {
            label: label.to_string(),
            measured: measured.to_string(),
            floor: floor.to_string(),
            ok: measured <= floor,
        });
    }

    /// Percentages: `measured <= floor`.
    fn pct(&mut self, label: &str, n: usize, total: usize, floor: f64) {
        let measured = 100.0 * n as f64 / total.max(1) as f64;
        self.checks.push(Check {
            label: label.to_string(),
            measured: format!("{measured:.2}% ({n} of {total})"),
            floor: format!("{floor:.2}%"),
            ok: measured <= floor,
        });
    }
}

#[derive(Clone, Copy)]
enum End {
    Enter,
    Space,
    Tab,
    CaretMoved,
}

/// Half-typed names: how many prefixes change at each ending.
fn prefixes(report: &mut Report) {
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
        ("caret move", End::CaretMoved),
    ];
    let mut total = 0usize;
    let mut changed = [0usize; 4];
    for name in names {
        for word in [name.to_string(), name.to_lowercase()] {
            let letters: Vec<char> = word.chars().collect();
            for len in 2..=letters.len() {
                let prefix: String = letters[..len].iter().collect();
                total += 1;
                for (i, (_, end)) in ends.iter().enumerate() {
                    let mut e = Engine::new(en, th, InputLayout::UsQwerty);
                    prefix.chars().for_each(|c| e.key(c));
                    match end {
                        End::Enter => e.boundary('\n'),
                        End::Space => e.boundary(' '),
                        End::Tab => e.boundary('\t'),
                        End::CaretMoved => e.caret_moved(),
                    }
                    if e.screen.trim_end() != prefix {
                        changed[i] += 1;
                    }
                }
            }
        }
    }
    println!("Name prefixes typed on the US layout: {total}");
    for ((label, _), n) in ends.iter().zip(changed) {
        // Floor 0: a half-typed name must never be rewritten.
        report.count(&format!("name prefix changed at {label}"), n, 0);
    }
}

/// English typos (one edit applied to common words, kept when not a word).
fn typos(report: &mut Report) {
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
            mid_word += usize::from(seen_thai);
            at_space += usize::from(e.screen.trim_end() != typo);
        }
    }
    println!("English typos typed on the US layout (not words themselves): {total}");
    // Floors set in the CI-gate task: HEAD measured 0.73% and 1.30%.
    report.pct(
        "English typos turned Thai at the space",
        at_space,
        total,
        0.80,
    );
    report.pct("English typos showing Thai mid-word", mid_word, total, 1.40);
}

/// One text category: its corpus, the layout it is typed on, and the floors.
struct Category {
    name: &'static str,
    text: &'static str,
    layout: InputLayout,
    /// Most words that may differ from what was typed when the line ends.
    max_changed: usize,
    /// Most words that may show the wrong script at some point mid-word.
    max_mid_word: usize,
}

/// Floors are what HEAD measures. Where a number above 0 stands, the comment
/// says what it is.
const CATEGORIES: [Category; 8] = [
    Category {
        name: "search queries",
        text: include_str!("data/search_queries.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "English chat",
        text: include_str!("data/chat_en.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "romanized Thai",
        text: include_str!("data/romanized_thai.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "code and shell",
        text: include_str!("data/code_shell.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "@mentions and #tags",
        text: include_str!("data/mentions_tags.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "URLs and emails",
        text: include_str!("data/urls_emails.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "English sentences",
        text: include_str!("data/sentences_en.txt"),
        layout: InputLayout::UsQwerty,
        max_changed: 0,
        max_mid_word: 0,
    },
    Category {
        name: "Thai sentences",
        text: include_str!("data/sentences_th.txt"),
        layout: InputLayout::ThaiKedmanee,
        max_changed: 0,
        max_mid_word: 0,
    },
];

/// Whether `s` is in the script the line was meant to be in.
fn wrong_script(s: &str, layout: InputLayout) -> bool {
    match layout {
        InputLayout::UsQwerty => has_thai(s),
        InputLayout::ThaiKedmanee => s.chars().any(|c| c.is_ascii_alphabetic()),
    }
}

/// Returns (words, changed at the end, wrong script mid-word, lines listed).
fn run_category(cat: &Category, list: bool) -> (usize, usize, usize) {
    let (en, th) = (dict::english(), dict::thai());
    let (mut words, mut changed, mut mid_word) = (0usize, 0usize, 0usize);
    for line in cat
        .text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let expected: Vec<&str> = line.split_whitespace().collect();
        let mut e = Engine::new(en, th, cat.layout);
        let mut bad_mid = vec![false; expected.len()];
        let mut word = 0usize;
        for c in line.chars() {
            if c == ' ' {
                e.boundary(' ');
                word += 1;
                continue;
            }
            let Some((key, _)) = key_for(c) else {
                panic!("untypeable character {c:?} in {line:?}");
            };
            e.key(key);
            let tail = e.screen.rsplit(char::is_whitespace).next().unwrap_or("");
            if word < bad_mid.len() && wrong_script(tail, cat.layout) {
                bad_mid[word] = true;
            }
        }
        e.boundary('\n');
        let shown: Vec<&str> = e.screen.split_whitespace().collect();
        words += expected.len();
        mid_word += bad_mid.iter().filter(|b| **b).count();
        let line_changed = if shown.len() == expected.len() {
            shown.iter().zip(&expected).filter(|(a, b)| a != b).count()
        } else {
            expected.len()
        };
        changed += line_changed;
        if list && (line_changed > 0 || bad_mid.iter().any(|b| *b)) {
            println!("  {line:40} -> {}", e.screen.trim_end());
        }
    }
    (words, changed, mid_word)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let check = args.iter().any(|a| a == "--check");
    let list = args.iter().any(|a| a == "--list");
    let mut report = Report { checks: Vec::new() };

    prefixes(&mut report);
    typos(&mut report);
    for cat in &CATEGORIES {
        let (words, changed, mid_word) = run_category(cat, list);
        println!("{}: {words} words", cat.name);
        report.count(
            &format!("{}: changed at the end", cat.name),
            changed,
            cat.max_changed,
        );
        report.count(
            &format!("{}: wrong script mid-word", cat.name),
            mid_word,
            cat.max_mid_word,
        );
    }

    println!();
    let mut failed = 0usize;
    for c in &report.checks {
        let mark = if c.ok { "ok  " } else { "FAIL" };
        println!(
            "{mark} {:48} measured {:>18}   floor {}",
            c.label, c.measured, c.floor
        );
        failed += usize::from(!c.ok);
    }
    if check {
        if failed > 0 {
            eprintln!("\n{failed} floor(s) exceeded: a rewrite regression, or tighten/loosen with a written reason");
            std::process::exit(1);
        }
        println!("\nAll floors hold.");
    }
}
