//! Typing practice: lessons that add a row of keys at a time, practice
//! text made from real dictionary words that use only the keys learnt so
//! far, a session that scores what is typed, and the daily scores the
//! typist may choose to keep. Plain data: no OS calls.
//!
//! Nothing typed in practice is kept: a session holds its text and the
//! typed keys only while it runs; what lasts (if the typist turns it on) is
//! a score per day and, per key, how often it was missed.

use std::collections::HashMap;

use crate::layout::{self, Layout, ThaiVariant};

/// The keyboard practised.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Board {
    Thai(ThaiVariant),
    English,
}

impl Board {
    pub fn name(self) -> &'static str {
        match self {
            Board::Thai(ThaiVariant::Kedmanee) => "kedmanee",
            Board::Thai(ThaiVariant::Pattachote) => "pattachote",
            Board::Thai(ThaiVariant::Manoonchai) => "manoonchai",
            Board::English => "english",
        }
    }

    pub fn parse(s: &str) -> Option<Board> {
        Some(match s {
            "kedmanee" => Board::Thai(ThaiVariant::Kedmanee),
            "pattachote" => Board::Thai(ThaiVariant::Pattachote),
            "manoonchai" => Board::Thai(ThaiVariant::Manoonchai),
            "english" => Board::English,
            _ => return None,
        })
    }

    fn table(self) -> &'static dyn Layout {
        match self {
            Board::Thai(v) => layout::thai_table(v),
            Board::English => layout::us_table(),
        }
    }

    /// What the key (by its US character) types on this keyboard.
    pub fn char_of(self, key: char) -> Option<char> {
        self.table().char_of(key)
    }

    /// The key (by its US character) that types `c` on this keyboard.
    pub fn key_of(self, c: char) -> Option<char> {
        self.table().key_of(c)
    }
}

/// The keys of each row, by their US character, unshifted.
const HOME: &str = "asdfghjkl;";
const TOP: &str = "qwertyuiop";
const BOTTOM: &str = "zxcvbnm,./";
const NUMBERS: &str = "1234567890-=";
const EDGES: &str = "[]\\'`";

/// What a lesson practises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lesson {
    /// The first `rows` of: home, top, bottom, numbers, edges; with the
    /// Shift layer when `shift`.
    Rows { rows: u8, shift: bool },
    /// Thai: a vowel above with a tone mark on it, in the order they are
    /// typed (ที่ not ท่ี).
    ThaiMarks,
    /// Thai digits ๐–๙ (Thai keyboards only).
    ThaiDigits,
}

/// The lessons in order, for a keyboard.
pub fn lessons(board: Board) -> Vec<Lesson> {
    let mut v: Vec<Lesson> = (1..=5)
        .map(|rows| Lesson::Rows { rows, shift: false })
        .collect();
    v.push(Lesson::Rows {
        rows: 5,
        shift: true,
    });
    if let Board::Thai(_) = board {
        v.push(Lesson::ThaiMarks);
        v.push(Lesson::ThaiDigits);
    }
    v
}

/// The keys (US characters) a lesson has unlocked.
pub fn lesson_keys(lesson: Lesson) -> Vec<char> {
    let Lesson::Rows { rows, shift } = lesson else {
        return Vec::new();
    };
    let mut keys: Vec<char> = [HOME, TOP, BOTTOM, NUMBERS, EDGES]
        .iter()
        .take(rows as usize)
        .flat_map(|r| r.chars())
        .collect();
    if shift {
        let shifted: Vec<char> = keys.iter().filter_map(|&k| shifted(k)).collect();
        keys.extend(shifted);
    }
    keys
}

/// The US character of `key` with Shift.
pub fn shifted(key: char) -> Option<char> {
    if key.is_ascii_lowercase() {
        return Some(key.to_ascii_uppercase());
    }
    let (plain, up) = ("1234567890-=[]\\;',./`", "!@#$%^&*()_+{}|:\"<>?~");
    plain.find(key).and_then(|i| up.chars().nth(i))
}

/// The characters a lesson's keys type on `board`.
pub fn lesson_chars(board: Board, lesson: Lesson) -> Vec<char> {
    lesson_keys(lesson)
        .into_iter()
        .filter_map(|k| board.char_of(k))
        .collect()
}

/// A small, seeded random number source (practice text, the game's words):
/// the same seed gives the same text, which tests rely on.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.max(1))
    }

    pub fn next_u64(&mut self) -> u64 {
        // xorshift64
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next_u64() % n as u64) as usize
        }
    }
}

const THAI_UPPER_VOWELS: &str = "\u{0E31}\u{0E34}\u{0E35}\u{0E36}\u{0E37}\u{0E47}";
const THAI_TONES: &str = "\u{0E48}\u{0E49}\u{0E4A}\u{0E4B}";

/// The words a lesson can use: from `words` (a dictionary), those made only
/// of characters the lesson's keys type (for the Thai lessons, those that
/// practise what the lesson is about), 2 to 10 characters long.
pub fn lesson_words<'a>(
    board: Board,
    lesson: Lesson,
    words: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    let allowed: Vec<char> = lesson_chars(board, lesson);
    words
        .filter(|w| (2..=10).contains(&w.chars().count()))
        .filter(|w| match lesson {
            Lesson::Rows { .. } => w.chars().all(|c| allowed.contains(&c)),
            Lesson::ThaiMarks => {
                let c: Vec<char> = w.chars().collect();
                c.windows(2)
                    .any(|p| THAI_UPPER_VOWELS.contains(p[0]) && THAI_TONES.contains(p[1]))
                    && w.chars().all(|ch| board.key_of(ch).is_some())
            }
            Lesson::ThaiDigits => false,
        })
        .map(str::to_string)
        .collect()
}

/// Everyday words, most useful first: the Thai ones written for practice
/// (`assets/th_common.txt`), the English ones the most frequent of the
/// bundled list.
fn everyday_words(board: Board) -> Vec<&'static str> {
    match board {
        Board::English => crate::dict::english_words().take(5000).collect(),
        Board::Thai(_) => include_str!("../assets/th_common.txt")
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .collect(),
    }
}

/// The words a lesson practises: everyday words first, so the practice is
/// words people really write (the whole dictionary gave หาดก, ทระนาว,
/// `coiiier`); the whole dictionary only when too few everyday words use
/// just the lesson's keys.
pub fn words_for(board: Board, lesson: Lesson) -> Vec<String> {
    let everyday = lesson_words(board, lesson, everyday_words(board).into_iter());
    if everyday.len() >= 12 {
        return everyday;
    }
    match board {
        Board::English => lesson_words(board, lesson, crate::dict::english_words()),
        Board::Thai(_) => lesson_words(board, lesson, crate::dict::thai_words()),
    }
}

/// Real sentences to type on `board` (chat, work, everyday), those it can
/// type every character of.
pub fn sentences(board: Board) -> Vec<&'static str> {
    let want = match board {
        Board::English => "[en]",
        Board::Thai(_) => "[th]",
    };
    let mut on = false;
    include_str!("../assets/practice_sentences.txt")
        .lines()
        .map(str::trim)
        .filter(|l| {
            if l.starts_with('[') {
                on = *l == want;
                return false;
            }
            on && !l.is_empty() && !l.starts_with('#')
        })
        .filter(|l| l.chars().all(|c| c == ' ' || board.key_of(c).is_some()))
        .collect()
}

/// Sentences for a run of about `words` words, none again before all have
/// come.
pub fn sentence_text(board: Board, words: usize, rng: &mut Rng) -> Option<String> {
    let all = sentences(board);
    if all.len() < 2 {
        return None;
    }
    // Shuffled, each once, before any comes back.
    let mut order: Vec<usize> = (0..all.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, rng.below(i + 1));
    }
    let mut out: Vec<&str> = Vec::new();
    let mut count = 0;
    for &i in order.iter().cycle() {
        if count >= words {
            break;
        }
        count += all[i].split(' ').count();
        out.push(all[i]);
    }
    Some(out.join(" "))
}

/// Practice text: `count` items from `words`, more often those holding the
/// keys missed most (`misses`, by character); when there are too few words
/// for the lesson (the first rows), short drills of its characters.
pub fn practice_text(
    board: Board,
    lesson: Lesson,
    words: &[String],
    misses: &HashMap<char, u32>,
    count: usize,
    rng: &mut Rng,
) -> String {
    if lesson == Lesson::ThaiDigits {
        let digits: Vec<char> = "๐๑๒๓๔๕๖๗๘๙".chars().collect();
        return (0..count)
            .map(|_| {
                (0..3 + rng.below(3))
                    .map(|_| digits[rng.below(10)])
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    if words.len() < 12 {
        let chars = lesson_chars(board, lesson);
        return (0..count)
            .map(|_| {
                (0..3 + rng.below(3))
                    .map(|_| chars[rng.below(chars.len())])
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    // Each word weighs 1, plus how often its characters were missed.
    let weight = |w: &str| {
        1 + w
            .chars()
            .map(|c| misses.get(&c).copied().unwrap_or(0))
            .sum::<u32>()
    };
    let total: u64 = words.iter().map(|w| weight(w) as u64).sum();
    (0..count)
        .map(|_| {
            let mut at = rng.next_u64() % total.max(1);
            for w in words {
                let wt = weight(w) as u64;
                if at < wt {
                    return w.clone();
                }
                at -= wt;
            }
            words[0].clone()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One run through a practice text. A wrong key does not move on: the
/// right one is still expected, and the miss is counted against the key
/// that was due.
#[derive(Debug, Clone)]
pub struct Session {
    target: Vec<char>,
    at: usize,
    misses: u32,
    started_ms: Option<u64>,
    finished_ms: Option<u64>,
    /// Per character due: how often it was missed this session.
    pub missed: HashMap<char, u32>,
    /// Per character: how often it was typed right this session.
    pub hit: HashMap<char, u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typed {
    Right,
    Wrong,
    /// The last character: the session is over.
    Done,
}

impl Session {
    pub fn new(text: &str) -> Self {
        Session {
            target: text.chars().collect(),
            at: 0,
            misses: 0,
            started_ms: None,
            finished_ms: None,
            missed: HashMap::new(),
            hit: HashMap::new(),
        }
    }

    /// The character typed at `now_ms` (any clock in milliseconds).
    pub fn typed(&mut self, c: char, now_ms: u64) -> Typed {
        if self.is_done() {
            return Typed::Done;
        }
        self.started_ms.get_or_insert(now_ms);
        let due = self.target[self.at];
        if c != due {
            self.misses += 1;
            *self.missed.entry(due).or_insert(0) += 1;
            return Typed::Wrong;
        }
        self.at += 1;
        *self.hit.entry(due).or_insert(0) += 1;
        if self.is_done() {
            self.finished_ms = Some(now_ms);
            return Typed::Done;
        }
        Typed::Right
    }

    pub fn is_done(&self) -> bool {
        self.at >= self.target.len()
    }

    pub fn text(&self) -> &[char] {
        &self.target
    }

    /// How many characters are typed right so far.
    pub fn position(&self) -> usize {
        self.at
    }

    /// The character due next.
    pub fn due(&self) -> Option<char> {
        self.target.get(self.at).copied()
    }

    /// Characters per minute so far (or over the whole session once done).
    pub fn per_minute(&self, now_ms: u64) -> u32 {
        let Some(start) = self.started_ms else {
            return 0;
        };
        let end = self.finished_ms.unwrap_or(now_ms);
        let ms = end.saturating_sub(start).max(1);
        (self.at as u64 * 60_000 / ms) as u32
    }

    /// Of the keys pressed, the share that were right (percent).
    pub fn accuracy(&self) -> u8 {
        let pressed = self.at as u32 + self.misses;
        if pressed == 0 {
            return 100;
        }
        (self.at as u32 * 100 / pressed) as u8
    }
}

/// One day's best for one lesson, kept only if the typist turned it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayScore {
    /// `2026-10-03`.
    pub date: String,
    pub board: Board,
    /// Index into [`lessons`].
    pub lesson: u8,
    pub per_minute: u32,
    pub accuracy: u8,
}

/// The kept scores: at most this many days' worth of lines.
pub const MAX_SCORES: usize = 365;

/// Add `score`: it replaces the same day's score for the same lesson when
/// it is better (more per minute, at no less than 90% right, or more
/// right); the oldest go past [`MAX_SCORES`].
pub fn add_score(scores: &mut Vec<DayScore>, score: DayScore) {
    if let Some(old) = scores
        .iter_mut()
        .find(|s| s.date == score.date && s.board == score.board && s.lesson == score.lesson)
    {
        let better = (score.accuracy >= 90 && score.per_minute > old.per_minute)
            || score.accuracy > old.accuracy;
        if better {
            *old = score;
        }
    } else {
        scores.push(score);
    }
    if scores.len() > MAX_SCORES {
        let extra = scores.len() - MAX_SCORES;
        scores.drain(..extra);
    }
}

/// Scores as lines: `2026-10-03 kedmanee 2 182 96`.
pub fn scores_to_text(scores: &[DayScore]) -> String {
    scores
        .iter()
        .map(|s| {
            format!(
                "{} {} {} {} {}\n",
                s.date,
                s.board.name(),
                s.lesson,
                s.per_minute,
                s.accuracy
            )
        })
        .collect()
}

pub fn scores_from_text(text: &str) -> Vec<DayScore> {
    text.lines()
        .filter(|l| !l.starts_with("key "))
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            Some(DayScore {
                date: p.next()?.to_string(),
                board: Board::parse(p.next()?)?,
                lesson: p.next()?.parse().ok()?,
                per_minute: p.next()?.parse().ok()?,
                accuracy: p.next()?.parse::<u8>().ok()?.min(100),
            })
        })
        .take(MAX_SCORES)
        .collect()
}

/// How one key has gone in practice: kept per keyboard and key position
/// (the US character of the key and layer), never what was typed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyStat {
    pub hits: u32,
    pub misses: u32,
}

/// The kept counts stop growing here (old practice then weighs as much
/// as new).
const STAT_CAP: u32 = 10_000;

/// Per keyboard, per key: how practice went.
pub type KeyStats = HashMap<(&'static str, char), KeyStat>;

/// Add a session's hits and misses (by the character due) to `stats`.
pub fn add_key_stats(
    stats: &mut KeyStats,
    board: Board,
    hit: &HashMap<char, u32>,
    missed: &HashMap<char, u32>,
) {
    let mut add = |c: char, hits: u32, misses: u32| {
        if c == ' ' {
            return;
        }
        let Some(key) = board.key_of(c) else {
            return;
        };
        let s = stats.entry((board.name(), key)).or_default();
        s.hits = (s.hits + hits).min(STAT_CAP);
        s.misses = (s.misses + misses).min(STAT_CAP);
    };
    for (&c, &n) in hit {
        add(c, n, 0);
    }
    for (&c, &n) in missed {
        add(c, 0, n);
    }
}

/// How well a key is known, from 0 (not yet) to 1 (without looking): it
/// needs some practice first, then grows with the keys typed right and
/// shrinks with the share missed (a key missed one time in ten or more is
/// not known).
pub fn mastery(s: KeyStat) -> f32 {
    const START: u32 = 20;
    const FULL: u32 = 120;
    if s.hits < START {
        return 0.0;
    }
    let practised = ((s.hits - START) as f32 / (FULL - START) as f32).min(1.0);
    let missed = s.misses as f32 / (s.hits + s.misses) as f32;
    practised * (1.0 - missed * 10.0).clamp(0.0, 1.0)
}

/// Key stats as lines: `key kedmanee d 120 3` (the key as a code point,
/// so `#` and space-like keys stay one word).
pub fn key_stats_to_text(stats: &KeyStats) -> String {
    let mut lines: Vec<String> = stats
        .iter()
        .map(|((board, key), s)| format!("key {board} {:x} {} {}\n", *key as u32, s.hits, s.misses))
        .collect();
    lines.sort();
    lines.concat()
}

pub fn key_stats_from_text(text: &str) -> KeyStats {
    text.lines()
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            if p.next()? != "key" {
                return None;
            }
            let board = Board::parse(p.next()?)?;
            let key = char::from_u32(u32::from_str_radix(p.next()?, 16).ok()?)?;
            if !key.is_ascii_graphic() {
                return None;
            }
            let hits = p.next()?.parse::<u32>().ok()?.min(STAT_CAP);
            let misses = p.next()?.parse::<u32>().ok()?.min(STAT_CAP);
            Some(((board.name(), key), KeyStat { hits, misses }))
        })
        .take(4 * 100)
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn practice_is_everyday_words_and_real_sentences() {
        use crate::layout::ThaiVariant;
        let th = Board::Thai(ThaiVariant::Kedmanee);
        let words = super::words_for(
            th,
            Lesson::Rows {
                rows: 5,
                shift: false,
            },
        );
        assert!(words.iter().any(|w| w == "ไป"), "{:?}", &words[..5]);
        assert!(!words.iter().any(|w| w == "ทระนาว"));
        let text = super::sentence_text(th, 30, &mut super::Rng::new(1)).unwrap();
        let parts: Vec<&str> = super::sentences(th)
            .into_iter()
            .filter(|s| text.contains(s))
            .collect();
        assert!(parts.len() >= 3);
        assert!(super::sentences(Board::English).len() >= 10);
    }

    use super::*;

    const KED: Board = Board::Thai(ThaiVariant::Kedmanee);

    #[test]
    fn lessons_unlock_a_row_at_a_time() {
        let first = lesson_keys(Lesson::Rows {
            rows: 1,
            shift: false,
        });
        assert_eq!(first.iter().collect::<String>(), HOME);
        // Kedmanee's home row: ฟ ห ก ด เ ้ ่ า ส ว.
        assert_eq!(
            lesson_chars(
                KED,
                Lesson::Rows {
                    rows: 1,
                    shift: false
                }
            )
            .iter()
            .collect::<String>(),
            "ฟหกดเ้่าสว"
        );
        let shift = lesson_keys(Lesson::Rows {
            rows: 5,
            shift: true,
        });
        assert!(shift.contains(&'A') && shift.contains(&'?'));
        assert_eq!(lessons(KED).len(), 8);
        assert_eq!(lessons(Board::English).len(), 6);
    }

    #[test]
    fn words_use_only_the_keys_learnt() {
        let dict = ["หา", "ดาว", "กิน", "เกา", "ที่", "นี้", "ปีก"];
        let home = lesson_words(
            KED,
            Lesson::Rows {
                rows: 1,
                shift: false,
            },
            dict.iter().copied(),
        );
        assert_eq!(home, ["หา", "ดาว", "เกา"]);
        let marks = lesson_words(KED, Lesson::ThaiMarks, dict.iter().copied());
        assert_eq!(marks, ["ที่", "นี้"]);
    }

    #[test]
    fn practice_text_is_reproducible_and_leans_on_missed_keys() {
        let words: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
        let mut misses = HashMap::new();
        let text = |misses: &HashMap<char, u32>| {
            practice_text(
                Board::English,
                Lesson::Rows {
                    rows: 5,
                    shift: false,
                },
                &words,
                misses,
                200,
                &mut Rng::new(7),
            )
        };
        assert_eq!(text(&misses), text(&misses));
        let before = text(&misses).matches("w7").count();
        misses.insert('7', 50);
        assert!(text(&misses).matches("w7").count() > before * 3);
        // Too few words: drills of the lesson's characters.
        let drill = practice_text(
            KED,
            Lesson::Rows {
                rows: 1,
                shift: false,
            },
            &[],
            &HashMap::new(),
            5,
            &mut Rng::new(1),
        );
        assert!(drill.chars().all(|c| c == ' ' || "ฟหกดเ้่าสว".contains(c)));
        let digits = practice_text(
            KED,
            Lesson::ThaiDigits,
            &[],
            &HashMap::new(),
            4,
            &mut Rng::new(3),
        );
        assert!(digits.chars().all(|c| c == ' ' || ('๐'..='๙').contains(&c)));
    }

    #[test]
    fn a_session_scores_speed_and_accuracy() {
        let mut s = Session::new("ab");
        assert_eq!(s.typed('a', 1_000), Typed::Right);
        assert_eq!(s.typed('x', 1_200), Typed::Wrong);
        assert_eq!(s.due(), Some('b'));
        assert_eq!(s.typed('b', 2_000), Typed::Done);
        assert!(s.is_done());
        assert_eq!(s.per_minute(99_999), 120); // 2 characters in 1 second
        assert_eq!(s.accuracy(), 66);
        assert_eq!(s.missed.get(&'b'), Some(&1));
    }

    #[test]
    fn scores_keep_the_days_best_and_read_back() {
        let day = |d: &str, pm, acc| DayScore {
            date: d.into(),
            board: KED,
            lesson: 2,
            per_minute: pm,
            accuracy: acc,
        };
        let mut v = Vec::new();
        add_score(&mut v, day("2026-10-03", 150, 95));
        add_score(&mut v, day("2026-10-03", 140, 97)); // more accurate
        add_score(&mut v, day("2026-10-03", 300, 60)); // fast but sloppy: not better
        assert_eq!(v, [day("2026-10-03", 140, 97)]);
        for i in 0..400 {
            add_score(&mut v, day(&format!("d{i:03}"), 100, 90));
        }
        assert_eq!(v.len(), MAX_SCORES);
        assert_eq!(scores_from_text(&scores_to_text(&v)), v);
    }

    #[test]
    fn the_dictionaries_give_every_lesson_words() {
        for board in [
            KED,
            Board::Thai(ThaiVariant::Pattachote),
            Board::Thai(ThaiVariant::Manoonchai),
            Board::English,
        ] {
            for lesson in lessons(board) {
                if lesson == Lesson::ThaiDigits {
                    continue;
                }
                let words = match board {
                    Board::English => lesson_words(board, lesson, crate::dict::english_words()),
                    _ => lesson_words(board, lesson, crate::dict::thai_words()),
                };
                eprintln!("{} {lesson:?}: {}", board.name(), words.len());
                // The first row alone may have only a few (drills then);
                // from the second row on, plenty.
                if lesson
                    != (Lesson::Rows {
                        rows: 1,
                        shift: false,
                    })
                {
                    assert!(
                        words.len() >= 50,
                        "{} {lesson:?}: {}",
                        board.name(),
                        words.len()
                    );
                }
            }
        }
    }

    #[test]
    fn key_stats_count_by_key_and_read_back_beside_scores() {
        let mut stats = KeyStats::new();
        // ก is on D in Kedmanee; ฃ (Shift) on its own layer.
        let hit = HashMap::from([('ก', 30), (' ', 9)]);
        let missed = HashMap::from([('ก', 2), ('ฅ', 1)]);
        add_key_stats(&mut stats, KED, &hit, &missed);
        assert_eq!(
            stats[&("kedmanee", 'd')],
            KeyStat {
                hits: 30,
                misses: 2
            }
        );
        assert!(!stats.keys().any(|(_, k)| *k == ' '));
        let scores = vec![DayScore {
            date: "2026-10-03".into(),
            board: KED,
            lesson: 1,
            per_minute: 120,
            accuracy: 97,
        }];
        let text = scores_to_text(&scores) + &key_stats_to_text(&stats);
        assert_eq!(scores_from_text(&text), scores);
        assert_eq!(key_stats_from_text(&text), stats);
        // Nothing but counts: no line carries a typed character.
        assert!(!text.contains('ก'));
    }

    #[test]
    fn mastery_needs_practice_and_few_misses() {
        let m = |hits, misses| mastery(KeyStat { hits, misses });
        assert_eq!(m(10, 0), 0.0);
        assert!(m(70, 0) > 0.4 && m(70, 0) < 0.6);
        assert_eq!(m(500, 0), 1.0);
        // One miss in ten: not known, however long practised.
        assert_eq!(m(450, 50), 0.0);
        assert!(m(500, 10) < m(500, 0));
    }
}
