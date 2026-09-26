//! Typing benchmark: a mixed Thai/English academic article typed through the
//! RightType pipeline by simulated typists, with and without RightType.
//!
//!     cargo run --release --example typing_benchmark [-- --out report.md]
//!
//! Every keystroke goes through `righttype::sim::Engine`, an OS-free replay of
//! the hook's Auto-mode decisions (see `src/sim.rs`). The typists:
//!
//! - type every character with the key that produces it on the layout of the
//!   word's language (Thai letters on Kedmanee, English, digits and most
//!   symbols on US QWERTY);
//! - before each word that needs the other layout, switch by hand — or, with
//!   probability `forget`, don't, and type it on the wrong layout;
//! - look at each word after its Space and fix it if it is wrong: with
//!   RightType, one Shift+Backspace when the keys were right but the script is
//!   wrong, otherwise delete and retype; without RightType, delete and retype.
//!
//! Time is estimated from counted actions with the constants below; the
//! pipeline's own processing time is measured.

use std::time::{Duration, Instant};

use righttype::dict;
use righttype::layout::en_to_th;
use righttype::policy::InputLayout;
use righttype::sim::Engine;

const ARTICLE: &str = include_str!("data/academic_th_en.txt");
const RUNS: u64 = 50;

/// Assumed human timings (seconds). 200 keystrokes per minute is a typical
/// office typist (about 40 WPM).
const T_KEY: f64 = 0.30;
const T_SWITCH: f64 = 0.60; // Alt+Shift / Win+Space, including the glance at the indicator
const T_NOTICE: f64 = 0.80; // seeing that the word just typed is wrong
const T_FLIP: f64 = 0.40; // Shift+Backspace

#[derive(Clone, Copy)]
struct Scenario {
    name: &'static str,
    righttype: bool,
    forget: f64,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "Without RightType — always switches correctly",
        righttype: false,
        forget: 0.0,
    },
    Scenario {
        name: "Without RightType — forgets to switch 20% of the time",
        righttype: false,
        forget: 0.2,
    },
    Scenario {
        name: "Without RightType — forgets 50%",
        righttype: false,
        forget: 0.5,
    },
    Scenario {
        name: "With RightType — always switches correctly",
        righttype: true,
        forget: 0.0,
    },
    Scenario {
        name: "With RightType — forgets 20%",
        righttype: true,
        forget: 0.2,
    },
    Scenario {
        name: "With RightType — forgets 50%",
        righttype: true,
        forget: 0.5,
    },
    Scenario {
        name: "With RightType — never switches (relies on RightType)",
        righttype: true,
        forget: 1.0,
    },
];

fn is_thai(c: char) -> bool {
    ('\u{0E00}'..='\u{0E7F}').contains(&c)
}

fn lang_of(word: &str) -> InputLayout {
    if word.chars().any(is_thai) {
        InputLayout::ThaiKedmanee
    } else {
        InputLayout::UsQwerty
    }
}

/// The key that types `c` on `layout`, if that layout has it.
fn key_on(c: char, layout: InputLayout) -> Option<char> {
    match layout {
        InputLayout::UsQwerty => c.is_ascii_graphic().then_some(c),
        InputLayout::ThaiKedmanee => (0x21u8..0x7F)
            .map(|b| b as char)
            .find(|k| en_to_th(&k.to_string()).starts_with(c)),
    }
}

/// How a word is typed: runs of characters, each on one layout. Thai words
/// stay on Kedmanee for punctuation it has (quotes, brackets); digits and
/// Latin letters need US QWERTY.
fn plan(word: &str) -> Vec<(InputLayout, Vec<char>)> {
    let home = lang_of(word);
    let mut runs: Vec<(InputLayout, Vec<char>)> = Vec::new();
    for c in word.chars() {
        let layout = if key_on(c, home).is_some() {
            home
        } else {
            InputLayout::UsQwerty
        };
        let key = key_on(c, layout).unwrap_or_else(|| panic!("cannot type {c:?}"));
        match runs.last_mut() {
            Some((l, keys)) if *l == layout => keys.push(key),
            _ => runs.push((layout, vec![key])),
        }
    }
    runs
}

/// The physical keys that produce `text` (Thai through Kedmanee).
fn keys_of(text: &str) -> String {
    text.chars()
        .map(|c| {
            if is_thai(c) {
                key_on(c, InputLayout::ThaiKedmanee).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

struct Rng(u64);

impl Rng {
    fn chance(&mut self, p: f64) -> bool {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64 <= p && p > 0.0
    }
}

#[derive(Default, Clone)]
struct Outcome {
    keys: usize,
    backspaces: usize,
    switches: usize,
    flips: usize,
    retypes: usize,
    notices: usize,
    /// Words wrong right after their Space (before the typist fixed them).
    wrong_words: usize,
    /// Of those: typed on the correct layout, i.e. RightType changed a word it
    /// should have left alone.
    false_changes: usize,
    /// Typed on the wrong layout and not (fully) fixed by RightType.
    missed: usize,
    /// Words that needed the other layout and RightType fixed by itself.
    auto_fixed: usize,
    wrong_layout_words: usize,
    final_ok: bool,
    examples: Vec<(String, String, bool)>,
    latencies: Vec<Duration>,
}

impl Outcome {
    fn seconds(&self) -> f64 {
        (self.keys + self.backspaces) as f64 * T_KEY
            + self.switches as f64 * T_SWITCH
            + self.notices as f64 * T_NOTICE
            + self.flips as f64 * T_FLIP
    }
}

fn last_word(screen: &str) -> &str {
    let body = screen.trim_end_matches([' ', '\n']);
    body.rsplit([' ', '\n']).next().unwrap_or(body)
}

fn run(s: Scenario, seed: u64) -> Outcome {
    let en = dict::english();
    let th = dict::thai();
    let mut e = Engine::new(en, th, InputLayout::UsQwerty);
    e.enabled = s.righttype;
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15 ^ seed.wrapping_mul(0x2545_F491_4F6C_DD1D));
    let mut o = Outcome::default();

    // Words with their separator (space or newline); blank lines are Enters.
    let mut words: Vec<(String, char)> = Vec::new();
    let mut cur = String::new();
    for c in ARTICLE.chars() {
        if c == ' ' || c == '\n' {
            if !cur.is_empty() {
                words.push((std::mem::take(&mut cur), c));
            } else if c == '\n' {
                words.push((String::new(), '\n'));
            }
        } else {
            cur.push(c);
        }
    }
    if !cur.is_empty() {
        words.push((cur, '\n'));
    }

    let timed = |o: &mut Outcome, f: &mut dyn FnMut()| {
        let t = Instant::now();
        f();
        o.latencies.push(t.elapsed());
    };

    for (word, sep) in &words {
        if word.is_empty() {
            timed(&mut o, &mut || e.boundary(*sep));
            o.keys += 1;
            continue;
        }
        let mut forgot_any = false;
        for (layout, keys) in plan(word) {
            if e.layout() != layout {
                if rng.chance(s.forget) {
                    forgot_any = true;
                } else {
                    e.switch_layout(layout);
                    o.switches += 1;
                }
            }
            for k in keys {
                timed(&mut o, &mut || e.key(k));
                o.keys += 1;
            }
        }
        timed(&mut o, &mut || e.boundary(*sep));
        o.keys += 1;
        if forgot_any {
            o.wrong_layout_words += 1;
        }

        let got = last_word(&e.screen).to_string();
        if got == *word {
            if forgot_any {
                o.auto_fixed += 1;
            }
            continue;
        }
        // The word is wrong: the typist notices and fixes it.
        o.wrong_words += 1;
        o.notices += 1;
        if forgot_any {
            o.missed += 1;
        } else {
            o.false_changes += 1;
        }
        if o.examples.len() < 400 {
            o.examples.push((word.clone(), got.clone(), forgot_any));
        }
        let tail = got.chars().count() + 1;
        if s.righttype && keys_of(&got) == keys_of(word) {
            // Right keys, wrong script: Shift+Backspace flips it in place.
            o.flips += 1;
            let keep = e.screen.chars().count() - tail;
            e.screen = e.screen.chars().take(keep).collect();
            e.screen.push_str(word);
            e.screen.push(*sep);
            e.switch_layout(lang_of(word));
        } else {
            // Delete the word and its separator, then type it again properly.
            o.retypes += 1;
            for _ in 0..tail {
                timed(&mut o, &mut || e.backspace());
                o.backspaces += 1;
            }
            for (layout, keys) in plan(word) {
                if e.layout() != layout {
                    e.switch_layout(layout);
                    o.switches += 1;
                }
                for k in keys {
                    timed(&mut o, &mut || e.key(k));
                    o.keys += 1;
                }
            }
            timed(&mut o, &mut || e.boundary(*sep));
            o.keys += 1;
            if last_word(&e.screen) != word.as_str() {
                // Still wrong after a careful retype: force it (counted once).
                let got2 = last_word(&e.screen).to_string();
                let tail2 = got2.chars().count() + 1;
                let keep = e.screen.chars().count() - tail2;
                e.screen = e.screen.chars().take(keep).collect();
                e.screen.push_str(word);
                e.screen.push(*sep);
                o.flips += 1;
            }
        }
    }
    o.final_ok = e.screen.trim_end() == ARTICLE.trim_end();
    o
}

fn mean(v: &[f64]) -> f64 {
    v.iter().sum::<f64>() / v.len() as f64
}

fn main() {
    let out = std::env::args().skip_while(|a| a != "--out").nth(1);
    righttype::english::warm();
    let chars = ARTICLE.trim_end().chars().count();
    let words = ARTICLE.split_whitespace().count();
    let thai_words = ARTICLE
        .split_whitespace()
        .filter(|w| w.chars().any(is_thai))
        .count();

    let mut report = String::new();
    let mut push = |s: String| {
        println!("{s}");
        report.push_str(&s);
        report.push('\n');
    };
    push("# RightType typing benchmark".into());
    push(String::new());
    push(format!(
        "Text: `examples/data/academic_th_en.txt` — {chars} characters, {words} space-separated words \
         ({thai_words} Thai runs, {} English/number tokens). {RUNS} runs per scenario.",
        words - thai_words
    ));
    push(String::new());
    push(format!(
        "Time model: {T_KEY:.2} s per keystroke (≈{} keystrokes/min), {T_SWITCH:.2} s per manual layout \
         switch, {T_NOTICE:.2} s to notice a wrong word, {T_FLIP:.2} s for Shift+Backspace.",
        (60.0 / T_KEY).round()
    ));
    push(String::new());
    push("| Scenario | Words wrong after Space | …of which RightType changed a correctly typed word | Fixed by RightType | Manual switches | Extra keystrokes | Est. time | Speed (chars/min) | vs. perfect typist |".into());
    push("|---|---|---|---|---|---|---|---|---|".into());

    let mut baseline = 0.0;
    let mut all_latencies: Vec<Duration> = Vec::new();
    let mut error_table: Vec<(String, String, bool, usize)> = Vec::new();
    for (i, s) in SCENARIOS.iter().enumerate() {
        let outcomes: Vec<Outcome> = (0..RUNS).map(|seed| run(*s, seed)).collect();
        let f = |g: &dyn Fn(&Outcome) -> f64| mean(&outcomes.iter().map(g).collect::<Vec<_>>());
        let secs = f(&|o| o.seconds());
        if i == 0 {
            baseline = secs;
        }
        let cpm = chars as f64 / (secs / 60.0);
        // One keystroke per character (Shift chords count once) is the floor.
        let ideal_keys = ARTICLE.trim_end().chars().count() as f64 + 1.0;
        let extra = f(&|o| (o.keys + o.backspaces) as f64) - ideal_keys;
        assert!(
            outcomes.iter().all(|o| o.final_ok),
            "{}: final text differs",
            s.name
        );
        push(format!(
            "| {} | {:.1} | {:.1} | {:.1} of {:.1} | {:.1} | {:.0} | {:.0} s | {:.0} | {:+.1}% |",
            s.name,
            f(&|o| o.wrong_words as f64),
            f(&|o| o.false_changes as f64),
            f(&|o| o.auto_fixed as f64),
            f(&|o| o.wrong_layout_words as f64),
            f(&|o| o.switches as f64),
            extra.max(0.0),
            secs,
            cpm,
            (secs / baseline - 1.0) * 100.0
        ));
        for o in &outcomes {
            if s.righttype {
                all_latencies.extend(o.latencies.iter().copied());
                for (want, got, wrong_layout) in &o.examples {
                    match error_table
                        .iter_mut()
                        .find(|(w, g, l, _)| w == want && g == got && l == wrong_layout)
                    {
                        Some(row) => row.3 += 1,
                        None => error_table.push((want.clone(), got.clone(), *wrong_layout, 1)),
                    }
                }
            }
        }
    }

    push(String::new());
    push("## Words RightType got wrong (all RightType scenarios, all runs)".into());
    push(String::new());
    if error_table.is_empty() {
        push("None.".into());
    } else {
        error_table.sort_by_key(|row| std::cmp::Reverse(row.3));
        push("| Intended | On screen after Space | Typed on | Times |".into());
        push("|---|---|---|---|".into());
        for (want, got, wrong_layout, n) in &error_table {
            push(format!(
                "| `{want}` | `{got}` | {} | {n} |",
                if *wrong_layout {
                    "wrong layout"
                } else {
                    "correct layout"
                }
            ));
        }
    }

    all_latencies.sort();
    let n = all_latencies.len();
    let pct = |p: f64| all_latencies[((n as f64 - 1.0) * p) as usize];
    let avg = all_latencies.iter().sum::<Duration>() / n as u32;
    push(String::new());
    push("## Processing time per keystroke (RightType pipeline, this machine)".into());
    push(String::new());
    push(format!(
        "{n} keystrokes: average {avg:?}, median {:?}, 99th percentile {:?}, max {:?}. \
         A keystroke arrives every ~{:.0} ms at this typing speed.",
        pct(0.5),
        pct(0.99),
        all_latencies[n - 1],
        T_KEY * 1000.0
    ));
    if let Some(path) = out {
        std::fs::write(&path, report).expect("writing report");
        eprintln!("wrote {path}");
    }
}
