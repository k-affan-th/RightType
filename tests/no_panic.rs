//! What the keyboard hook runs on every key must never panic: a panic there
//! takes RightType down mid-word. Random text (Latin, Thai, symbols, marks
//! out of place, emoji) through every reader of typed text added in 2.4.

use righttype::{abbrev, dict, ghost, latex, macros, mathsimple, naturalmath, snippets};

/// A small deterministic generator: the same strings on every run.
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
    "x",
    "y",
    "2",
    "10",
    "3.14",
    "^",
    "_",
    "/",
    "(",
    ")",
    "{",
    "}",
    "[",
    "]",
    "\\",
    "\\frac",
    "\\sqrt",
    "\\alpha",
    "\\",
    " ",
    " ",
    "  ",
    "+",
    "-",
    "=",
    "<=",
    "!=",
    "->",
    "*",
    ".",
    ",",
    "\"",
    "'",
    "!",
    "?",
    "pi",
    "Sigma",
    "sum",
    "sqrt",
    "squared",
    "over",
    "to",
    "from",
    "of",
    "plus",
    "times",
    "ยกกำลัง",
    "สอง",
    "บวก",
    "ลบ",
    "ส่วน",
    "รากที่สองของ",
    "จาก",
    "ถึง",
    "มี",
    "กำลัง",
    "เท่ากับ",
    "ก",
    "ข",
    "ค",
    "่",
    "้",
    "ั",
    "ำ",
    "ุ",
    "เ",
    "ๆ",
    ".",
    "ก.ค.",
    "ดร.",
    "{กด",
    "{press Ctrl+B}",
    "{wait 300}",
    "{รอ",
    "{คำสั่ง",
    "{style}",
    "{clipboard}",
    "{วันที่}",
    "{date}",
    "\n",
    "\t",
    "é",
    "²",
    "∑",
    "😀",
    "\u{200b}",
    "\u{0}",
];

fn random(rng: &mut Lcg) -> String {
    let n = rng.below(14);
    (0..n).map(|_| PIECES[rng.below(PIECES.len())]).collect()
}

#[test]
fn readers_of_typed_text_never_panic() {
    let th = dict::thai();
    let now = snippets::Now {
        year: 2026,
        month: 10,
        day: 4,
        weekday: 0,
        hour: 9,
        minute: 5,
    };
    let mut rng = Lcg(20_261_004);
    for _ in 0..60_000 {
        let s = random(&mut rng);
        let _ = latex::to_unicode(&s);
        let _ = latex::looks_like_latex(&s);
        let _ = naturalmath::to_unicode(&s);
        let _ = naturalmath::offer(&s);
        if let Some(m) = mathsimple::Math::read(&s) {
            let _ = (
                m.unicode(),
                m.unicode_math(),
                m.formatted(),
                m.fully_unicode(),
            );
        }
        let _ = macros::steps(&s);
        let _ = snippets::fill(&s, &now);
        let _ = abbrev::reads_as_abbreviation(&s, th);
        // Typed key by key: the ghost reads every prefix, kept to 48.
        let mut tail = String::new();
        for c in s.chars() {
            tail.push(c);
            while tail.chars().count() > 48 {
                tail.remove(0);
            }
            if let Some(g) = ghost::offer(&tail) {
                assert!(
                    g.replace <= tail.chars().count(),
                    "{tail:?}: replaces {} of {}",
                    g.replace,
                    tail.chars().count()
                );
            }
        }
    }
}

#[test]
fn readers_are_quick_enough_for_the_hook() {
    // The ghost reads the text before the caret on every key: well under a
    // millisecond for the longest text it keeps (release builds are ~10×
    // faster than this debug bound).
    let worst = "x ยกกำลังสองบวก y ยกกำลังสองเท่ากับ z ยกกำลังสอง บวก 1";
    let start = std::time::Instant::now();
    for _ in 0..200 {
        let _ = ghost::offer(worst);
    }
    let each = start.elapsed() / 200;
    eprintln!("ghost offer on the longest text: {each:?} per key");
    assert!(
        each < std::time::Duration::from_millis(20),
        "{each:?} per key"
    );
}
