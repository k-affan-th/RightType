//! What the pill says, as data: a kind (for its icon and colour), a short
//! title, an optional detail line, and a short memory of the last few.
//!
//! None of this touches the screen, so Windows and a future Mac app share it.
//! The pill's text arrives as one already-translated string; [`compose`]
//! works out the kind by matching it back to the interface string it came
//! from, and cuts it into title and detail at the separator the strings
//! already use.
//!
//! The memory is for the typist, not for anyone else: it lives only in RAM,
//! holds the last [`CAPACITY`] messages, is wiped when it is cleared or the
//! app ends, and can be switched off.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use zeroize::Zeroize;

use crate::i18n::{Lang, T};

/// How many messages the memory keeps.
pub const CAPACITY: usize = 50;

/// What a message is about, which decides its icon and colour. The icon's
/// shape differs too, so colour is never the only signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Something was fixed, saved, added or switched on.
    Done,
    /// Something was undone or put back as typed.
    Undo,
    /// A mode or setting changed, or a hint.
    Info,
    /// Nothing happened, or the typist should look.
    Attention,
    /// RightType itself had a problem.
    Problem,
}

/// One message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub kind: Kind,
    pub title: String,
    pub detail: Option<String>,
    /// Seconds since 1970-01-01 UTC.
    pub at: u64,
}

impl Notice {
    /// The whole message on one line, as the typist would read it.
    pub fn line(&self) -> String {
        match &self.detail {
            Some(d) => format!("{} · {}", self.title, d),
            None => self.title.clone(),
        }
    }

    fn wipe(&mut self) {
        self.title.zeroize();
        if let Some(d) = self.detail.as_mut() {
            d.zeroize();
        }
    }
}

/// Turn the pill's text into a [`Notice`] stamped now.
pub fn compose(text: &str) -> Notice {
    let (title, detail) = split(text);
    Notice {
        kind: classify(text),
        title,
        detail,
        at: now(),
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Longest a title is allowed to be before a message with no separator is
/// cut at its colon instead.
const TITLE_SOFT_MAX: usize = 34;

/// Cut `text` into a short title and the rest. The interface strings put
/// their hint after ` · ` or ` — `; the earliest of those is the cut. Without
/// one, a long message is cut at its first `: `. Otherwise it is all title.
pub fn split(text: &str) -> (String, Option<String>) {
    let text = text.trim();
    let cut = [" · ", " — "]
        .iter()
        .filter_map(|sep| text.find(sep).map(|i| (i, sep.len())))
        .min_by_key(|&(i, _)| i)
        .or_else(|| {
            (text.chars().count() > TITLE_SOFT_MAX)
                .then(|| text.find(": ").map(|i| (i, 2)))
                .flatten()
        });
    match cut {
        Some((i, len)) if i > 0 && i + len < text.len() => {
            (text[..i].to_string(), Some(text[i + len..].to_string()))
        }
        _ => (text.to_string(), None),
    }
}

/// The kind of the interface string `text` was made from. A message that is
/// not one of the interface strings is [`Kind::Info`].
pub fn classify(text: &str) -> Kind {
    templates()
        .iter()
        .filter(|t| t.matches(text))
        .max_by_key(|t| t.weight)
        .map_or(Kind::Info, |t| t.kind)
}

/// How long the pill stays for `notice`: long enough to read, in
/// milliseconds. A problem stays a little longer.
pub fn hold_ms(notice: &Notice) -> u32 {
    let chars = notice.line().chars().count() as u32;
    let base = (1200 + chars * 45).min(6000);
    match notice.kind {
        Kind::Problem | Kind::Attention => (base + 600).min(6000),
        _ => base,
    }
}

/// An interface string with its `{placeholders}` cut out: the literal pieces
/// that must appear, in order.
struct Template {
    kind: Kind,
    pieces: Vec<String>,
    /// Total literal length, so the most specific template wins.
    weight: usize,
}

impl Template {
    fn new(kind: Kind, template: &str) -> Template {
        let mut pieces = Vec::new();
        let mut rest = template;
        while let Some(open) = rest.find('{') {
            pieces.push(rest[..open].to_string());
            match rest[open..].find('}') {
                Some(close) => rest = &rest[open + close + 1..],
                None => {
                    rest = "";
                    break;
                }
            }
        }
        pieces.push(rest.to_string());
        let weight = pieces.iter().map(|p| p.chars().count()).sum();
        Template {
            kind,
            pieces,
            weight,
        }
    }

    fn matches(&self, text: &str) -> bool {
        // A string with placeholders and almost no fixed text could be any
        // other one.
        if self.pieces.len() > 1 && self.weight < 6 {
            return false;
        }
        let last = self.pieces.len() - 1;
        let mut at = 0;
        for (i, piece) in self.pieces.iter().enumerate() {
            if piece.is_empty() {
                continue;
            }
            if i == 0 {
                if !text.starts_with(piece.as_str()) {
                    return false;
                }
                at = piece.len();
            } else if i == last {
                if text.len() < at + piece.len() || !text.ends_with(piece.as_str()) {
                    return false;
                }
            } else {
                match text[at..].find(piece.as_str()) {
                    Some(p) => at += p + piece.len(),
                    None => return false,
                }
            }
        }
        // Without placeholders the text is the string itself.
        self.pieces.len() > 1 || text == self.pieces[0]
    }
}

fn templates() -> &'static [Template] {
    static ALL: OnceLock<Vec<Template>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut out = Vec::new();
        for key in T::ALL {
            let Some(kind) = kind_of(&format!("{key:?}")) else {
                continue;
            };
            for lang in [Lang::En, Lang::Th] {
                out.push(Template::new(kind, key.get(lang)));
            }
        }
        out
    })
}

/// The kind of the interface string named `key` (its enum name), when it is
/// one that appears in the pill.
fn kind_of(key: &str) -> Option<Kind> {
    use Kind::*;
    Some(match key {
        "ToastOn"
        | "ToastSaved"
        | "ToastWordLearned"
        | "ToastCopied"
        | "ToastFixedWords"
        | "ToastFlippedWords"
        | "ToastFlippedOne"
        | "ToastFlippedOneBack"
        | "ToastFlippedOneMore"
        | "ToastSpellingFixed"
        | "ToastHookBack"
        | "ToastLearnedCleared"
        | "ToastHabitsCleared"
        | "ToastFieldOn"
        | "ToastReplaced"
        | "ToastReplacedUnknown"
        | "ToastCapsOff" => Done,
        "ToastUndo" | "ToastSpellingKept" | "ToastSwitchUndone" | "ToastCapsKept"
        | "ToastMacroStopped" | "ToastTypingStopped" => Undo,
        "ToastOff"
        | "ToastModeAuto"
        | "ToastModeSuggest"
        | "ToastModeManual"
        | "ToastModeCode"
        | "ToastLearnOn"
        | "ToastLearnOff"
        | "ToastPaused"
        | "ToastAppMode"
        | "ToastModeForNow"
        | "ToastCapsSwitchOn"
        | "ToastCapsSwitchOff"
        | "ToastFieldOff"
        | "ToastAlreadyRunning"
        | "ToastTyping"
        | "ToastGoogleDocsTip"
        | "ToastGraveTypes"
        | "ToastOfferMode"
        | "ToastOpenedFixWindow" => Info,
        "ToastNothingToFlip"
        | "ToastNothingToFix"
        | "ToastNothingToChange"
        | "ToastWordInvalid"
        | "ToastVerifyDiffers"
        | "ToastEnterHeld"
        | "ToastNumLockOff"
        | "ToastInsertPressed"
        | "ToastInsertBlocked"
        | "ToastFieldChanged"
        | "ToastMacroNoCommand" => Attention,
        "ToastHookLost"
        | "ToastRestarted"
        | "ToastFakeKeyboard"
        | "ErrCorrectionInject"
        | "ErrFile" => Problem,
        _ => return None,
    })
}

static KEEPING: AtomicBool = AtomicBool::new(true);

fn log() -> &'static Mutex<VecDeque<Notice>> {
    static LOG: OnceLock<Mutex<VecDeque<Notice>>> = OnceLock::new();
    LOG.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Whether messages are remembered (on by default).
pub fn keeping() -> bool {
    KEEPING.load(Ordering::Relaxed)
}

/// Remember messages or not. Turning it off forgets the ones kept.
pub fn set_keeping(on: bool) {
    KEEPING.store(on, Ordering::Relaxed);
    if !on {
        clear();
    }
}

/// Remember `notice`, forgetting the oldest beyond [`CAPACITY`]. A repeat of
/// the newest message only refreshes its time.
pub fn record(notice: &Notice) {
    if !keeping() {
        return;
    }
    let mut log = log().lock().unwrap();
    if let Some(last) = log.back_mut() {
        if last.kind == notice.kind && last.title == notice.title && last.detail == notice.detail {
            last.at = notice.at;
            return;
        }
    }
    if log.len() >= CAPACITY {
        if let Some(mut old) = log.pop_front() {
            old.wipe();
        }
    }
    log.push_back(notice.clone());
}

/// The remembered messages, newest first.
pub fn recent() -> Vec<Notice> {
    log().lock().unwrap().iter().rev().cloned().collect()
}

/// Forget every remembered message.
pub fn clear() {
    let mut log = log().lock().unwrap();
    for n in log.iter_mut() {
        n.wipe();
    }
    log.clear();
}

/// `HH:MM` of `at` in UTC offset `offset_minutes` (local time given by the
/// caller; this crate has no time-zone database).
pub fn clock(at: u64, offset_minutes: i32) -> String {
    let minutes = (at / 60) as i64 + offset_minutes as i64;
    let day = minutes.rem_euclid(24 * 60);
    format!("{:02}:{:02}", day / 60, day % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::trf;

    fn fixed_kind(key: T, args: &[(&str, &str)]) -> Kind {
        classify(&trf(key, args))
    }

    #[test]
    fn every_string_with_a_kind_is_found_again_in_both_languages() {
        for key in T::ALL {
            let Some(want) = kind_of(&format!("{key:?}")) else {
                continue;
            };
            for lang in [Lang::En, Lang::Th] {
                // Fill each placeholder with a plain word.
                let mut text = key.get(lang).to_string();
                while let Some(open) = text.find('{') {
                    let close = text[open..].find('}').unwrap() + open;
                    text.replace_range(open..=close, "x");
                }
                let got = classify(&text);
                // A string with almost no fixed text may be read as another
                // one of the same family; it must never come out as Info
                // when it is not one.
                if Template::new(want, key.get(lang)).weight >= 6 {
                    assert!(
                        got == want || got != Kind::Info,
                        "{key:?} ({lang:?}) is {got:?}, wanted {want:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn kinds_follow_what_happened() {
        assert_eq!(fixed_kind(T::ToastSaved, &[]), Kind::Done);
        assert_eq!(
            fixed_kind(T::ToastFixedWords, &[("n", "3")]),
            Kind::Done,
            "placeholders do not stop a match"
        );
        assert_eq!(fixed_kind(T::ToastUndo, &[]), Kind::Undo);
        assert_eq!(fixed_kind(T::ToastModeAuto, &[]), Kind::Info);
        assert_eq!(fixed_kind(T::ToastNothingToFix, &[]), Kind::Attention);
        assert_eq!(fixed_kind(T::ToastHookLost, &[]), Kind::Problem);
        assert_eq!(classify("something nobody wrote"), Kind::Info);
    }

    #[test]
    fn a_message_is_cut_where_its_hint_starts() {
        assert_eq!(
            split("Fixed 3 words · Ctrl+Z undoes it"),
            ("Fixed 3 words".into(), Some("Ctrl+Z undoes it".into()))
        );
        assert_eq!(split("Saved"), ("Saved".into(), None));
        assert_eq!(
            split("RightType lost the keyboard — retrying…"),
            (
                "RightType lost the keyboard".into(),
                Some("retrying…".into())
            )
        );
        // The earlier separator wins.
        assert_eq!(split("A · b — c"), ("A".into(), Some("b — c".into())),);
        // A long message with no separator is cut at its colon.
        let long = "Code mode: only what is clearly a slip, never names";
        assert_eq!(split(long).0, "Code mode");
        // A short one with a colon stays whole.
        assert_eq!(split("Spelling: a → b").1, None);
        // Nothing to cut off at either end.
        assert_eq!(split(" · x").1, None);
        assert_eq!(split("x · ").1, None);
    }

    #[test]
    fn hold_time_grows_with_the_text_and_is_capped() {
        let short = compose("Saved");
        let long = compose(&"word ".repeat(200));
        assert!(hold_ms(&short) >= 1200);
        assert!(hold_ms(&long) <= 6000);
        assert!(hold_ms(&compose(&"x".repeat(40))) > hold_ms(&short));
        let mut problem = compose("RightType lost the keyboard — retrying…");
        problem.kind = Kind::Problem;
        let mut info = problem.clone();
        info.kind = Kind::Info;
        assert!(hold_ms(&problem) >= hold_ms(&info));
    }

    #[test]
    fn the_memory_keeps_the_latest_and_forgets_on_request() {
        // One test owns the shared log, so it cannot race another.
        set_keeping(true);
        clear();
        for i in 0..CAPACITY + 5 {
            record(&Notice {
                kind: Kind::Done,
                title: format!("n{i}"),
                detail: None,
                at: i as u64,
            });
        }
        let all = recent();
        assert_eq!(all.len(), CAPACITY);
        assert_eq!(all[0].title, format!("n{}", CAPACITY + 4), "newest first");
        assert_eq!(all[CAPACITY - 1].title, "n5", "the oldest five are gone");

        // A repeat refreshes the newest instead of filling the list.
        record(&Notice {
            kind: Kind::Done,
            title: format!("n{}", CAPACITY + 4),
            detail: None,
            at: 9999,
        });
        assert_eq!(recent().len(), CAPACITY);
        assert_eq!(recent()[0].at, 9999);

        set_keeping(false);
        assert!(recent().is_empty(), "switching it off forgets");
        record(&compose("Saved"));
        assert!(recent().is_empty(), "and nothing is kept while it is off");
        set_keeping(true);
        record(&compose("Saved"));
        assert_eq!(recent().len(), 1);
        clear();
        assert!(recent().is_empty());
    }

    #[test]
    fn the_clock_wraps_around_midnight() {
        assert_eq!(clock(0, 0), "00:00");
        assert_eq!(clock(3600 * 23 + 41 * 60, 0), "23:41");
        assert_eq!(clock(3600 * 23, 120), "01:00");
        assert_eq!(clock(60, -120), "22:01");
    }
}
