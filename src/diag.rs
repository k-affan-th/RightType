//! What RightType did lately, for a problem report — with nothing typed in it.
//!
//! The release build keeps no log, so when a correction goes wrong on a real
//! PC there is nothing to look at afterwards. This keeps the last [`CAPACITY`]
//! decisions in memory, and the typist can save them to a file of their choice
//! (tray → "Save a problem report…") to send along with a bug report.
//!
//! Redacted by construction: an entry is a compile-time message, numbers and
//! yes/no flags, plus program and window-class names that pass [`app_name`].
//! There is no way to put a typed word in — a word appears only as its
//! [`Shape`] (how many letters of each kind). Nothing is written to disk unless
//! the typist saves a report, and the ring is gone when RightType exits.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::sync::Mutex;
use std::time::Instant;

/// How many entries are kept. A few minutes of typing.
pub const CAPACITY: usize = 400;

/// One value in an entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Num(i64),
    Flag(bool),
    /// A word as counts: Thai characters, Latin letters, anything else.
    Shape(Shape),
    /// A reason written into the program (`'static`, so never typed text).
    Label(&'static str),
}

/// A word reduced to how many characters of each kind it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shape {
    pub thai: u16,
    pub latin: u16,
    pub other: u16,
}

impl Shape {
    pub fn of(word: &str) -> Shape {
        let mut s = Shape::default();
        for c in word.chars() {
            let n = if ('\u{0E00}'..='\u{0E7F}').contains(&c) {
                &mut s.thai
            } else if c.is_ascii_alphabetic() {
                &mut s.latin
            } else {
                &mut s.other
            };
            *n = n.saturating_add(1);
        }
        s
    }
}

impl From<i64> for Value {
    fn from(n: i64) -> Self {
        Value::Num(n)
    }
}
impl From<usize> for Value {
    fn from(n: usize) -> Self {
        Value::Num(i64::try_from(n).unwrap_or(i64::MAX))
    }
}
impl From<u32> for Value {
    fn from(n: u32) -> Self {
        Value::Num(i64::from(n))
    }
}
impl From<bool> for Value {
    fn from(b: bool) -> Self {
        Value::Flag(b)
    }
}
impl From<&'static str> for Value {
    fn from(s: &'static str) -> Self {
        Value::Label(s)
    }
}
impl From<Shape> for Value {
    fn from(s: Shape) -> Self {
        Value::Shape(s)
    }
}

#[derive(Debug, Clone)]
struct Entry {
    ms: u128,
    what: &'static str,
    fields: Vec<(&'static str, Value)>,
    app: Option<(String, String)>,
}

struct Ring {
    started: Instant,
    entries: VecDeque<Entry>,
}

static RING: Mutex<Option<Ring>> = Mutex::new(None);

fn push(what: &'static str, fields: Vec<(&'static str, Value)>, app: Option<(String, String)>) {
    let Ok(mut ring) = RING.lock() else {
        return;
    };
    let ring = ring.get_or_insert_with(|| Ring {
        started: Instant::now(),
        entries: VecDeque::with_capacity(CAPACITY),
    });
    if ring.entries.len() == CAPACITY {
        ring.entries.pop_front();
    }
    let ms = ring.started.elapsed().as_millis();
    ring.entries.push_back(Entry {
        ms,
        what,
        fields,
        app,
    });
}

/// Record that something happened.
pub fn note(what: &'static str, fields: &[(&'static str, Value)]) {
    push(what, fields.to_vec(), None);
}

/// Record the program and window class the caret moved to. Anything that
/// does not look like a program or class name is left out.
pub fn note_app(exe: &str, class: &str) {
    push(
        "app",
        Vec::new(),
        Some((
            app_name(exe).unwrap_or("?").into(),
            app_name(class).unwrap_or("?").into(),
        )),
    );
}

/// A program or window-class name, if it looks like one: at most 64
/// characters of ASCII letters, digits and `. _ - :`. Keeps anything typed
/// (Thai, spaces) out even if a caller passes the wrong string.
pub fn app_name(name: &str) -> Option<&str> {
    let ok = !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'));
    ok.then_some(name)
}

/// Forget everything recorded.
pub fn clear() {
    if let Ok(mut ring) = RING.lock() {
        *ring = None;
    }
}

/// The report: `about` lines (version, settings) and then every entry, oldest
/// first, as text.
pub fn report(about: &[(&str, String)]) -> String {
    let mut out = String::from("RightType problem report\r\n");
    out.push_str("No typed text is in this report: words appear only as letter counts.\r\n\r\n");
    for (k, v) in about {
        let _ = write!(out, "{k}: {v}\r\n");
    }
    out.push_str("\r\n");
    let Ok(ring) = RING.lock() else {
        return out;
    };
    let Some(ring) = ring.as_ref() else {
        out.push_str("(nothing recorded yet)\r\n");
        return out;
    };
    for e in &ring.entries {
        let _ = write!(out, "{:>9} ms  {}", e.ms, e.what);
        for (k, v) in &e.fields {
            match v {
                Value::Num(n) => {
                    let _ = write!(out, " {k}={n}");
                }
                Value::Flag(b) => {
                    let _ = write!(out, " {k}={}", if *b { "yes" } else { "no" });
                }
                Value::Label(l) => {
                    let _ = write!(out, " {k}={l:?}");
                }
                Value::Shape(s) => {
                    let _ = write!(out, " {k}=[th {} en {} other {}]", s.thai, s.latin, s.other);
                }
            }
        }
        if let Some((exe, class)) = &e.app {
            let _ = write!(out, " {exe} ({class})");
        }
        out.push_str("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // The ring is process-wide; one test covers it so tests do not race.
    #[test]
    fn a_report_has_no_typed_text() {
        clear();
        let word = "สวัสดีครับ";
        note(
            "word",
            &[
                ("keys", Shape::of("l;ylfu8iy[").into()),
                ("fixed", true.into()),
            ],
        );
        note(
            "word",
            &[
                ("shown", Shape::of(word).into()),
                ("deleted", 5usize.into()),
            ],
        );
        note("inject", &[("why", "selection did not take".into())]);
        note_app("notepad.exe", "RichEditD2DPT");
        // Someone passes typed text where a name belongs: it is dropped.
        note_app("hello world", "ส่งไฟล์");
        let r = report(&[("version", "2.0.1".into())]);
        assert!(r.contains("version: 2.0.1"));
        assert!(r.contains("keys=[th 0 en 7 other 3] fixed=yes"));
        assert!(r.contains("shown=[th 10 en 0 other 0] deleted=5"));
        assert!(r.contains("inject why=\"selection did not take\""));
        assert!(r.contains("notepad.exe (RichEditD2DPT)"));
        assert!(r.contains("app ? (?)"));
        for leaked in ["l;ylfu", "สวัส", "hello", "ส่ง"] {
            assert!(!r.contains(leaked), "{leaked} leaked into:\n{r}");
        }

        for _ in 0..CAPACITY + 10 {
            note("key", &[]);
        }
        let r = report(&[]);
        assert_eq!(r.matches("ms  key").count(), CAPACITY);
        assert!(!r.contains("ms  word"), "the oldest entries are dropped");

        clear();
        assert!(report(&[]).contains("nothing recorded yet"));
    }
}
