//! Typing practice — Windows only (lessons and scoring are in
//! [`righttype::trainer`]).
//!
//! One window: pick the keyboard (Kedmanee, Pattachote, Manoonchai,
//! English), the lesson, and the mode — a lesson, 60 seconds, or the
//! falling-words game. While it is in front the keyboard hook hands it the
//! keys by their place on the keyboard ([`crate::hook::WM_PRACTICE_KEY`]),
//! so a keyboard not installed can be practised, and nothing typed here
//! reaches any app or RightType's corrections. The keyboard drawing lights
//! the next key, dims the keys not learnt yet, and tints the keys missed
//! most. Day scores are kept only when the typist turns that on; the text
//! never is.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use righttype::keyboard::KEYS;
use righttype::layout::ThaiVariant;
use righttype::trainer::{self, Board, DayScore, KeyStats, Lesson, Rng, Session, Typed};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HDC, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

use crate::ui::{self, pal, Gfx, Rgb, Surface, TextStyle};

const W: i32 = 1040;
const H: i32 = 700;
const PAD: i32 = 32;
const ROW1_Y: i32 = 66;
const ROW2_Y: i32 = 112;
const TEXT_Y: i32 = 162;
const TEXT_H: i32 = 108;
const STATS_Y: i32 = 280;
const KB_Y: i32 = 316;
const UNIT: i32 = 40;
const FOOT_Y: i32 = KB_Y + 6 * UNIT + 30;
const TIMED: Duration = Duration::from_secs(60);
const BOARDS: [Board; 4] = [
    Board::Thai(ThaiVariant::Kedmanee),
    Board::Thai(ThaiVariant::Pattachote),
    Board::Thai(ThaiVariant::Manoonchai),
    Board::English,
];
const LESSON_LABELS: [T; 8] = [
    T::LessonHome,
    T::LessonTop,
    T::LessonBottom,
    T::LessonNumbers,
    T::LessonEdges,
    T::LessonShift,
    T::LessonMarks,
    T::LessonDigits,
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Lesson,
    Timed,
    Game,
}

struct Fall {
    text: Vec<char>,
    /// Left edge (96-DPI units) and top (fraction of the game area).
    x: i32,
    y: f32,
    typed: usize,
}

struct Game {
    falling: Vec<Fall>,
    score: u32,
    lives: u8,
    /// When the next word appears, and the gap after it (ms).
    next_at: u64,
    gap: u64,
    /// How far a word falls per second (fraction of the area).
    speed: f32,
    last_ms: u64,
    over: bool,
    /// Words typed right in a row (a miss or a word reaching the ground
    /// ends it): each word scores more the longer it runs.
    combo: u32,
    best_combo: u32,
    /// Every 8 words: a level, and faster.
    words: u32,
    level: u32,
    /// Points rising from where a word was finished: (x, y, text, when).
    pops: Vec<(i32, f32, String, u64)>,
    /// When the last miss was, for a short shake of the word.
    missed_at: Option<u64>,
}

/// How long a word's points rise and fade (ms).
const POP_MS: u64 = 700;
/// Red for what is about to land, and for a miss.
const DANGER: Rgb = 0xE0_4F_4F;

struct State {
    board: Board,
    lesson: usize,
    mode: Mode,
    words: Vec<String>,
    text: String,
    session: Session,
    /// Misses per character, over every session since the window opened
    /// (and before, when scores are kept).
    misses: HashMap<char, u32>,
    rng: Rng,
    clock: Instant,
    timed_until: Option<Instant>,
    result: Option<(u32, u8)>,
    game: Option<Game>,
    scores: Vec<DayScore>,
}

impl State {
    fn now_ms(&self) -> u64 {
        self.clock.elapsed().as_millis() as u64
    }

    fn lesson_value(&self) -> Lesson {
        let all = trainer::lessons(self.board);
        all[self.lesson.min(all.len() - 1)]
    }

    /// Fresh words for the lesson, and a new text or game.
    fn new_round(&mut self) {
        let lesson = self.lesson_value();
        self.words = trainer::words_for(self.board, lesson);
        let count = if self.mode == Mode::Timed { 160 } else { 24 };
        // The whole keyboard, or a minute against the clock: real sentences.
        let whole_keyboard = lesson == *trainer::lessons(self.board).last().unwrap_or(&lesson)
            && !matches!(lesson, Lesson::ThaiDigits | Lesson::ThaiMarks);
        let sentences = (self.mode == Mode::Timed || whole_keyboard)
            .then(|| trainer::sentence_text(self.board, count.min(60), &mut self.rng))
            .flatten();
        self.text = sentences.unwrap_or_else(|| {
            trainer::practice_text(
                self.board,
                lesson,
                &self.words,
                &self.misses,
                count,
                &mut self.rng,
            )
        });
        self.session = Session::new(&self.text);
        self.timed_until = None;
        self.result = None;
        self.game = (self.mode == Mode::Game).then(|| Game {
            falling: Vec::new(),
            score: 0,
            lives: 3,
            next_at: self.now_ms(),
            gap: 2200,
            speed: 0.06,
            last_ms: self.now_ms(),
            over: false,
            combo: 0,
            best_combo: 0,
            words: 0,
            level: 1,
            pops: Vec::new(),
            missed_at: None,
        });
    }

    /// One word for the game.
    fn game_word(&mut self) -> Vec<char> {
        let text = trainer::practice_text(
            self.board,
            self.lesson_value(),
            &self.words,
            &self.misses,
            1,
            &mut self.rng,
        );
        text.chars().collect()
    }
}

struct Practice {
    window: nwg::Window,
    surface: Rc<Surface>,
    boards: [u16; 4],
    modes: [u16; 3],
    lessons: [u16; 8],
    keep: u16,
    on_screen: u16,
    timer: Cell<usize>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Practice>>> = const { RefCell::new(None) };
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    /// How each key went in practice (kept beside the scores when they
    /// are kept; else only while RightType runs). `None`: not read yet.
    static KEY_STATS: RefCell<Option<KeyStats>> = const { RefCell::new(None) };
}

fn with_key_stats<R>(f: impl FnOnce(&mut KeyStats) -> R) -> R {
    KEY_STATS.with(|k| {
        let mut k = k.borrow_mut();
        let stats = k.get_or_insert_with(|| {
            if crate::hook::practice_keeps_scores() {
                read_file()
                    .map(|t| trainer::key_stats_from_text(&t))
                    .unwrap_or_default()
            } else {
                KeyStats::new()
            }
        });
        f(stats)
    })
}

/// Practice counts for the keyboard-map render (debug builds).
#[cfg(debug_assertions)]
pub fn seed_demo() {
    let board = Board::Thai(righttype::layout::thai_variant());
    with_key_stats(|k| {
        for (i, us) in "asdfjkl;ghqwer".chars().enumerate() {
            k.insert(
                (board.name(), us),
                trainer::KeyStat {
                    hits: 140 - i as u32 * 9,
                    misses: 0,
                },
            );
        }
    });
}

/// How well key `us` (by its US character) of the Thai keyboard in use is
/// known, 0 to 1: the keyboard map fades the keys learnt (T5).
pub fn mastery_of(variant: ThaiVariant, us: char) -> f32 {
    let board = Board::Thai(variant);
    with_key_stats(|k| {
        k.get(&(board.name(), us))
            .map(|s| trainer::mastery(*s))
            .unwrap_or(0.0)
    })
}

/// Open the practice window (from the message loop).
pub fn request_open() {
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        open();
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

/// Open on the game in full swing (debug renders).
#[cfg(debug_assertions)]
pub fn open_game_demo() {
    open();
    STATE.with(|st| {
        let mut st = st.borrow_mut();
        let Some(s) = st.as_mut() else {
            return;
        };
        s.mode = Mode::Game;
        s.new_round();
        let now = s.now_ms();
        if let Some(g) = s.game.as_mut() {
            // Far in the future: the demo stays still for the screenshot.
            g.next_at = u64::MAX;
            g.speed = 0.0;
            g.score = 27;
            g.combo = 7;
            g.level = 3;
            g.lives = 2;
            for (text, x, y, typed) in [
                ("ขอบคุณ", 120, 0.15, 0),
                ("สวัสดี", 430, 0.42, 3),
                ("พรุ่งนี้", 700, 0.86, 0),
            ] {
                g.falling.push(Fall {
                    text: text.chars().collect(),
                    x,
                    y,
                    typed,
                });
            }
            g.pops.push((600, 0.6, "+2".into(), now));
        }
    });
    if let Some(p) = CURRENT.with(|c| c.borrow().clone()) {
        repaint(&p);
    }
}

fn scores_path() -> Option<std::path::PathBuf> {
    let mut p = crate::data_dir::righttype_dir()?;
    p.push("practice.txt");
    Some(p)
}

fn read_file() -> Option<String> {
    scores_path().and_then(|p| std::fs::read_to_string(p).ok())
}

fn load_scores() -> Vec<DayScore> {
    read_file()
        .map(|t| trainer::scores_from_text(&t))
        .unwrap_or_default()
}

fn today() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
}

fn close(p: &Rc<Practice>) {
    crate::hook::PRACTICE_WINDOW.store(0, std::sync::atomic::Ordering::Release);
    unsafe {
        let _ = KillTimer(p.surface.hwnd, p.timer.get());
    }
    p.surface.detach();
    if let Some(h) = p.handler.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    p.window.close();
    if crate::hook::practice_keeps_scores() {
        save_scores();
    }
    STATE.with(|s| s.borrow_mut().take());
}

fn open() {
    if let Some(p) = CURRENT.with(|c| c.borrow().clone()) {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(p.surface.hwnd);
        }
        return;
    }
    ui::refresh();
    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .size((W, H))
        .title(tr(T::PracticeTitle))
        .build(&mut window)
        .is_err()
    {
        return;
    }
    let surface = Surface::attach(&window, 0x5254_001C, Box::new(paint));
    let s = &surface;
    let pl = pal();
    s.label(
        tr(T::PracticeTitle),
        TextStyle::Title,
        (PAD, 16, 400, 36),
        pl.bg,
        0,
    );
    let seg = |labels: &[T], x: i32, y: i32, w: i32| -> Vec<u16> {
        labels
            .iter()
            .enumerate()
            .map(|(i, l)| s.segment(tr(*l), i == 0, (x + i as i32 * w, y, w, 32), pl.inset, 0))
            .collect()
    };
    let boards: [u16; 4] = seg(
        &[
            T::PracticeKedmanee,
            T::PracticePattachote,
            T::PracticeManoonchai,
            T::PracticeEnglish,
        ],
        PAD + 4,
        ROW1_Y,
        112,
    )
    .try_into()
    .unwrap_or([0; 4]);
    let modes: [u16; 3] = seg(
        &[T::PracticeLesson, T::PracticeTimed, T::PracticeGame],
        W - PAD - 4 - 3 * 118,
        ROW1_Y,
        118,
    )
    .try_into()
    .unwrap_or([0; 3]);
    let lessons: [u16; 8] = seg(&LESSON_LABELS, PAD + 4, ROW2_Y, (W - 2 * PAD - 8) / 8)
        .try_into()
        .unwrap_or([0; 8]);
    let on_screen = s.button(
        tr(T::PracticeOnScreen),
        false,
        (W - PAD - 260, 18, 260, 34),
        pl.bg,
        0,
    );
    let keep = s.toggle(
        tr(T::RowKeepScores),
        tr(T::SubKeepScores),
        (PAD, FOOT_Y, 520, 56),
        pl.bg,
        0,
    );
    s.label(
        tr(T::PracticeHelp),
        TextStyle::Small,
        (PAD + 540, FOOT_Y + 8, W - 2 * PAD - 540, 44),
        pl.bg,
        0,
    );
    ui::size_and_center(surface.hwnd, W, H);
    let keeping = crate::hook::practice_keeps_scores();
    s.set_checked(keep, keeping);
    let board = match righttype::layout::thai_variant() {
        ThaiVariant::Kedmanee => 0,
        ThaiVariant::Pattachote => 1,
        ThaiVariant::Manoonchai => 2,
    };
    s.set_checked(boards[board], true);
    s.set_checked(modes[0], true);
    s.set_checked(lessons[1], true);
    let scores = if keeping { load_scores() } else { Vec::new() };
    let mut state = State {
        board: BOARDS[board],
        lesson: 1,
        mode: Mode::Lesson,
        words: Vec::new(),
        text: String::new(),
        session: Session::new(""),
        misses: HashMap::new(),
        rng: Rng::new(Instant::now().elapsed().as_nanos() as u64 ^ 0x9E37_79B9_7F4A_7C15),
        clock: Instant::now(),
        timed_until: None,
        result: None,
        game: None,
        scores,
    };
    state.new_round();
    STATE.with(|st| *st.borrow_mut() = Some(state));
    let p = Rc::new(Practice {
        window,
        surface,
        boards,
        modes,
        lessons,
        keep,
        on_screen,
        timer: Cell::new(0),
        handler: RefCell::new(None),
    });
    show_lessons(&p);
    let hwnd = p.surface.hwnd;
    crate::hook::PRACTICE_WINDOW.store(hwnd.0 as isize, std::sync::atomic::Ordering::Release);
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
        p.timer.set(SetTimer(hwnd, 1, 33, None));
    }
    // Now that there is something to draw.
    ui::redraw_all(hwnd);
    let weak = Rc::downgrade(&p);
    p.surface.on_click(move |id| {
        if let Some(p) = weak.upgrade() {
            clicked(&p, id);
        }
    });
    let weak = Rc::downgrade(&p);
    let raw = nwg::bind_raw_event_handler(&p.window.handle, 0x5254_001D, move |_h, msg, w, l| {
        const WM_TIMER: u32 = 0x0113;
        const WM_CLOSE: u32 = 0x0010;
        let p = weak.upgrade()?;
        match msg {
            crate::hook::WM_PRACTICE_KEY => {
                let us = char::from_u32(w as u32).filter(|c| *c != '\0');
                key(&p, us, l as u16);
                Some(0)
            }
            WM_TIMER => {
                tick(&p);
                Some(0)
            }
            WM_CLOSE => {
                CURRENT.with(|c| c.borrow_mut().take());
                close(&p);
                Some(0)
            }
            _ => None,
        }
    })
    .ok();
    *p.handler.borrow_mut() = raw;
    crate::hook::trace_note("typing practice: open");
    CURRENT.with(|c| *c.borrow_mut() = Some(p));
}

/// English has no Thai lessons: their buttons go.
fn show_lessons(p: &Practice) {
    let thai = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .is_some_and(|s| s.board != Board::English)
    });
    for id in &p.lessons[6..] {
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                p.surface.hwnd_of(*id),
                if thai {
                    windows::Win32::UI::WindowsAndMessaging::SW_SHOW
                } else {
                    windows::Win32::UI::WindowsAndMessaging::SW_HIDE
                },
            );
        }
    }
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.borrow_mut().as_mut().map(f))
}

fn repaint(p: &Practice) {
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(p.surface.hwnd, None, true);
    }
}

fn clicked(p: &Rc<Practice>, id: u16) {
    if let Some(i) = p.boards.iter().position(|&b| b == id) {
        with_state(|s| {
            s.board = BOARDS[i];
            let n = trainer::lessons(s.board).len();
            if s.lesson >= n {
                s.lesson = n - 1;
            }
            s.new_round();
        });
        show_lessons(p);
        let lesson = with_state(|s| s.lesson).unwrap_or(0);
        for (j, b) in p.lessons.iter().enumerate() {
            p.surface.set_checked(*b, j == lesson);
        }
    } else if let Some(i) = p.modes.iter().position(|&b| b == id) {
        with_state(|s| {
            s.mode = [Mode::Lesson, Mode::Timed, Mode::Game][i];
            s.new_round();
        });
    } else if let Some(i) = p.lessons.iter().position(|&b| b == id) {
        with_state(|s| {
            s.lesson = i;
            s.new_round();
        });
    } else if id == p.on_screen {
        // T5: the map of the Thai keyboard in use, on top while working;
        // the keys learnt here fade.
        let variant = match with_state(|s| s.board) {
            Some(Board::Thai(v)) => v,
            _ => righttype::layout::thai_variant(),
        };
        crate::keymap::request_show(variant);
        crate::overlay::show(tr(T::PracticeOnScreenTip));
    } else if id == p.keep {
        let on = p.surface.checked(p.keep);
        crate::hook::set_practice_keeps_scores(on);
        crate::config::persist();
        if on {
            let loaded = load_scores();
            with_state(|s| s.scores = loaded);
            // Counts from before, added to this run's (unless not read yet:
            // they are read with the file then).
            let read = KEY_STATS.with(|k| k.borrow().is_some());
            let kept = read
                .then(read_file)
                .flatten()
                .map(|t| trainer::key_stats_from_text(&t))
                .unwrap_or_default();
            with_key_stats(|k| {
                for (key, s) in kept {
                    let e = k.entry(key).or_default();
                    e.hits = e.hits.saturating_add(s.hits);
                    e.misses = e.misses.saturating_add(s.misses);
                }
            });
            save_scores();
        } else if let Some(path) = scores_path() {
            // Turned off: what was kept goes.
            let _ = std::fs::remove_file(path);
        }
    }
    repaint(p);
    // Typing goes to the window, not the button just clicked.
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(p.surface.hwnd);
    }
}

fn key(p: &Rc<Practice>, us: Option<char>, vk: u16) {
    let keep = p.surface.checked(p.keep);
    let finished = with_state(|s| {
        match vk {
            0x1B => {
                // Esc: the same text again.
                s.session = Session::new(&s.text);
                s.timed_until = None;
                s.result = None;
                return None;
            }
            0x09 => {
                s.new_round();
                return None;
            }
            _ => {}
        }
        let us = us?;
        let c = if us == ' ' { ' ' } else { s.board.char_of(us)? };
        let now = s.now_ms();
        if let (Mode::Game, Some(game)) = (s.mode, s.game.as_mut()) {
            if game.over {
                return None;
            }
            // The word in progress, else the lowest that starts with it.
            let target = game.falling.iter().position(|f| f.typed > 0).or_else(|| {
                game.falling
                    .iter()
                    .enumerate()
                    .filter(|(_, f)| f.text.first() == Some(&c))
                    .max_by(|a, b| a.1.y.total_cmp(&b.1.y))
                    .map(|(i, _)| i)
            });
            match target {
                Some(i) if game.falling[i].text.get(game.falling[i].typed) == Some(&c) => {
                    count_key(s.board, c, true);
                    game.falling[i].typed += 1;
                    if game.falling[i].typed == game.falling[i].text.len() {
                        let done = game.falling.remove(i);
                        game.combo += 1;
                        game.best_combo = game.best_combo.max(game.combo);
                        // One point a word, one more for every 5 in a row.
                        let points = 1 + (game.combo / 5).min(4);
                        game.score += points;
                        game.pops.push((done.x, done.y, format!("+{points}"), now));
                        game.words += 1;
                        if game.words % 8 == 0 {
                            game.level += 1;
                            game.speed = (game.speed * 1.12).min(0.3);
                            game.gap = (game.gap * 88 / 100).max(700);
                        } else {
                            game.speed = (game.speed * 1.02).min(0.3);
                        }
                    }
                }
                Some(i) => {
                    let due = game.falling[i].text[game.falling[i].typed];
                    *s.misses.entry(due).or_insert(0) += 1;
                    count_key(s.board, due, false);
                    game.combo = 0;
                    game.missed_at = Some(now);
                }
                None => {}
            }
            return None;
        }
        if s.result.is_some() {
            return None;
        }
        if s.mode == Mode::Timed && s.timed_until.is_none() {
            s.timed_until = Some(Instant::now() + TIMED);
        }
        match s.session.typed(c, now) {
            Typed::Done => Some(finish(s)),
            _ => None,
        }
    })
    .flatten();
    if finished.is_some() && keep {
        save_scores();
    }
    repaint(p);
}

/// The round is over: its score, and the misses added to the rest.
fn finish(s: &mut State) -> (u32, u8) {
    let now = s.now_ms();
    let score = (s.session.per_minute(now), s.session.accuracy());
    let board = s.board;
    with_key_stats(|k| trainer::add_key_stats(k, board, &s.session.hit, &s.session.missed));
    s.session.hit.clear();
    for (c, n) in s.session.missed.drain() {
        *s.misses.entry(c).or_insert(0) += n;
    }
    s.result = Some(score);
    trainer::add_score(
        &mut s.scores,
        DayScore {
            date: today(),
            board: s.board,
            lesson: s.lesson as u8,
            per_minute: score.0,
            accuracy: score.1,
        },
    );
    score
}

/// One key of the game, right or missed.
fn count_key(board: Board, c: char, right: bool) {
    let one = HashMap::from([(c, 1)]);
    let none = HashMap::new();
    let (hit, missed) = if right { (&one, &none) } else { (&none, &one) };
    with_key_stats(|k| trainer::add_key_stats(k, board, hit, missed));
}

/// Write the day scores and the key counts (only when they are kept).
fn save_scores() {
    let scores = STATE.with(|s| {
        s.borrow()
            .as_ref()
            .map(|s| trainer::scores_to_text(&s.scores))
    });
    let Some(scores) = scores
        .or_else(|| read_file().map(|t| trainer::scores_to_text(&trainer::scores_from_text(&t))))
    else {
        return;
    };
    let keys = with_key_stats(|k| trainer::key_stats_to_text(k));
    if let Some(path) = scores_path() {
        let _ = std::fs::write(path, scores + &keys);
    }
}

fn tick(p: &Rc<Practice>) {
    let keep = p.surface.checked(p.keep);
    let (changed, finished) = with_state(|s| {
        let now = s.now_ms();
        if s.mode == Mode::Timed && s.result.is_none() {
            if let Some(until) = s.timed_until {
                if Instant::now() >= until {
                    finish(s);
                    return (true, true);
                }
                return (true, false);
            }
            return (false, false);
        }
        let Some(game) = s.game.as_mut() else {
            return (false, false);
        };
        if game.over {
            return (false, false);
        }
        let dt = now.saturating_sub(game.last_ms) as f32 / 1000.0;
        game.last_ms = now;
        for f in &mut game.falling {
            f.y += game.speed * dt;
        }
        let fallen = game.falling.iter().filter(|f| f.y >= 1.0).count() as u8;
        game.falling.retain(|f| f.y < 1.0);
        game.lives = game.lives.saturating_sub(fallen);
        if fallen > 0 {
            game.combo = 0;
        }
        game.pops.retain(|p| now.saturating_sub(p.3) < POP_MS);
        if game.lives == 0 {
            game.over = true;
            return (true, false);
        }
        if now >= game.next_at {
            game.next_at = now + game.gap;
            let x = PAD + 20 + s.rng.below((W - 2 * PAD - 200) as usize) as i32;
            let text = s.game_word();
            if let Some(game) = s.game.as_mut() {
                game.falling.push(Fall {
                    text,
                    x,
                    y: 0.0,
                    typed: 0,
                });
            }
        }
        (true, false)
    })
    .unwrap_or((false, false));
    if finished && keep {
        save_scores();
    }
    if changed {
        repaint(p);
    }
}

/// Thai marks that sit on the character before them.
fn is_mark(c: char) -> bool {
    c == '\u{0E31}'
        || ('\u{0E34}'..='\u{0E3A}').contains(&c)
        || ('\u{0E47}'..='\u{0E4E}').contains(&c)
}

/// `1:05`.
fn clock(d: Duration) -> String {
    let s = d.as_secs_f32().ceil() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// Mix `b` into `a` by `t` (0–1).
fn blend(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let ch = |c: Rgb, s: u32| ((c >> s) & 0xFF) as f32;
    let mix = |s: u32| ((ch(a, s) * (1.0 - t) + ch(b, s) * t).round() as u32) << s;
    mix(16) | mix(8) | mix(0)
}

fn paint(g: &Gfx, hdc: HDC, _rc: RECT, _page: u8) {
    let p = pal();
    STATE.with(|st| {
        let Ok(st) = st.try_borrow() else {
            return;
        };
        let Some(s) = st.as_ref() else {
            return;
        };
        ui::track(g, ui::rect(PAD, ROW1_Y - 4, 4 * 112 + 8, 40));
        ui::track(
            g,
            ui::rect(W - PAD - 8 - 3 * 118, ROW1_Y - 4, 3 * 118 + 8, 40),
        );
        let lesson_count = trainer::lessons(s.board).len() as i32;
        let lw = (W - 2 * PAD - 8) / 8;
        ui::track(g, ui::rect(PAD, ROW2_Y - 4, lesson_count * lw + 8, 40));
        let big = ui::make_font(28, 400);
        let body = ui::make_font(14, 400);
        let strong = ui::make_font(14, 600);
        if s.mode == Mode::Game {
            paint_game(g, hdc, s, big, body);
        } else {
            paint_text(g, hdc, s, big, body, strong);
            paint_keyboard(g, hdc, s);
        }
        unsafe {
            for f in [big, body, strong] {
                let _ = DeleteObject(HGDIOBJ(f.0));
            }
        }
    });
    let _ = p;
}

fn line(
    hdc: HDC,
    s: &str,
    x: i32,
    y: i32,
    w: i32,
    font: windows::Win32::Graphics::Gdi::HFONT,
    color: Rgb,
) {
    ui::text(
        hdc,
        s,
        ui::rect(x, y, w, 28),
        font,
        color,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}

fn paint_text(
    g: &Gfx,
    hdc: HDC,
    s: &State,
    big: windows::Win32::Graphics::Gdi::HFONT,
    body: windows::Win32::Graphics::Gdi::HFONT,
    strong: windows::Win32::Graphics::Gdi::HFONT,
) {
    let p = pal();
    let area = ui::rect(PAD, TEXT_Y, W - 2 * PAD, TEXT_H);
    ui::card(g, area);
    let now = s.now_ms();
    if let Some((speed, acc)) = s.result {
        let speed_s = trf(T::PracticeSpeed, &[("n", &speed.to_string())]);
        let acc_s = trf(T::PracticeAccuracy, &[("n", &acc.to_string())]);
        line(
            hdc,
            &trf(T::PracticeDone, &[("speed", &speed_s), ("acc", &acc_s)]),
            PAD + 24,
            TEXT_Y + 22,
            560,
            big,
            p.text,
        );
        // This lesson's last days (kept scores), as bars.
        let days: Vec<&DayScore> = s
            .scores
            .iter()
            .filter(|d| d.board == s.board && d.lesson as usize == s.lesson)
            .collect();
        if let Some(best) = days.iter().find(|d| d.date == today()) {
            line(
                hdc,
                &trf(
                    T::PracticeBestToday,
                    &[(
                        "speed",
                        &trf(T::PracticeSpeed, &[("n", &best.per_minute.to_string())]),
                    )],
                ),
                PAD + 24,
                TEXT_Y + 62,
                560,
                body,
                p.text_dim,
            );
        }
        let recent = &days[days.len().saturating_sub(14)..];
        let top = recent
            .iter()
            .map(|d| d.per_minute)
            .max()
            .unwrap_or(1)
            .max(1);
        for (i, d) in recent.iter().enumerate() {
            let h = (d.per_minute * 70 / top) as i32;
            let x = W - PAD - 24 - (recent.len() - i) as i32 * 18;
            g.fill_round(
                ui::rect(x, TEXT_Y + 90 - h, 12, h.max(2)),
                ui::px(2) as f32,
                if d.date == today() {
                    p.accent
                } else {
                    p.toggle_off
                },
            );
        }
        return;
    }
    // The text around where the typist is: typed (dim), the character due
    // (with any marks on it, highlighted), what comes next.
    let text: Vec<char> = s.session.text().to_vec();
    let at = s.session.position();
    let mut start = at.saturating_sub(10);
    while start > 0 && is_mark(text[start]) {
        start -= 1;
    }
    let mut end = at + 1;
    while end < text.len() && is_mark(text[end]) {
        end += 1;
    }
    let mut cur_start = at;
    while cur_start > 0 && is_mark(text[cur_start]) && cur_start > start {
        cur_start -= 1;
    }
    let done: String = text[start..cur_start].iter().collect();
    let due: String = text[cur_start..end.min(text.len())].iter().collect();
    let rest: String = text[end.min(text.len())..(end + 40).min(text.len())]
        .iter()
        .collect();
    let y = TEXT_Y + 30;
    let mut x = ui::px(PAD + 24);
    let right = ui::px(W - PAD - 24);
    let draw = |s: &str, x: &mut i32, color: Rgb, under: Option<Rgb>| {
        if s.is_empty() {
            return;
        }
        let w = ui::measure_width(hdc, s, big);
        let rc = RECT {
            left: *x,
            top: ui::px(y),
            right: (*x + w).min(right),
            bottom: ui::px(y + 44),
        };
        if let Some(bg) = under {
            g.fill_round(
                RECT {
                    left: rc.left - ui::px(3),
                    top: rc.top,
                    right: rc.left + w + ui::px(3),
                    bottom: rc.bottom,
                },
                ui::px(6) as f32,
                bg,
            );
        }
        ui::text(
            hdc,
            s,
            rc,
            big,
            color,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        *x += w;
    };
    draw(&done.replace(' ', "·"), &mut x, p.text_dim, None);
    let due_shown = if due == " " { "␣".to_string() } else { due };
    draw(&due_shown, &mut x, p.on_accent, Some(p.accent));
    draw(&rest, &mut x, p.text, None);
    // Speed, accuracy, time left.
    let mut stats = vec![
        trf(
            T::PracticeSpeed,
            &[("n", &s.session.per_minute(now).to_string())],
        ),
        trf(
            T::PracticeAccuracy,
            &[("n", &s.session.accuracy().to_string())],
        ),
    ];
    if s.mode == Mode::Timed {
        let left = s
            .timed_until
            .map_or(TIMED, |u| u.saturating_duration_since(Instant::now()));
        stats.push(trf(T::PracticeLeft, &[("t", &clock(left))]));
    }
    line(
        hdc,
        &stats.join("   ·   "),
        PAD + 4,
        STATS_Y,
        W - 2 * PAD,
        strong,
        p.text,
    );
}

fn paint_keyboard(g: &Gfx, hdc: HDC, s: &State) {
    let p = pal();
    let lesson = s.lesson_value();
    let unlocked = trainer::lesson_keys(lesson);
    // The key due, and whether Shift is needed for it.
    let due_key = s.session.due().and_then(|c| {
        if c == ' ' {
            return Some((' ', false));
        }
        let us = s.board.key_of(c)?;
        let base = KEYS
            .iter()
            .filter_map(|k| k.us)
            .find(|&k| k == us || trainer::shifted(k) == Some(us))?;
        Some((base, base != us))
    });
    let x0 = (W - crate::kbdraw::size(UNIT).0) / 2;
    crate::kbdraw::draw(g, hdc, x0, KB_Y, UNIT, |i| {
        let k = &KEYS[i];
        let is_space = k.scan == 0x39;
        let is_shift = k.scan == 0x2A || k.scan == 0x36;
        let due = match due_key {
            Some((' ', _)) => is_space,
            Some((base, shift)) => k.us == Some(base) || (shift && is_shift),
            None => false,
        };
        let mut look = if due {
            crate::kbdraw::Look::lit(p.accent)
        } else {
            crate::kbdraw::Look::plain()
        };
        if let Some(us) = k.us {
            let typed = s.board.char_of(us);
            // Thai: the Thai character big, the US key in the corner.
            if s.board != Board::English {
                if let Some(c) = typed {
                    look.label = Some(if is_mark(c) {
                        format!("◌{c}")
                    } else {
                        c.to_string()
                    });
                    look.corner = Some(us.to_ascii_uppercase().to_string());
                }
            }
            let learnt =
                unlocked.contains(&us) || matches!(lesson, Lesson::ThaiMarks | Lesson::ThaiDigits);
            if !due && !learnt {
                look.fill = p.inset;
                look.ink = p.text_dim;
            } else if !due {
                // Missed often: warmer, up to half the accent.
                let missed = typed.and_then(|c| s.misses.get(&c)).copied().unwrap_or(0);
                if missed > 0 {
                    look.fill = blend(p.keycap, p.accent, (missed as f32 / 12.0).min(0.5));
                }
            }
        } else if !due && !is_space && !is_shift {
            look.fill = p.inset;
            look.ink = p.text_dim;
        }
        look
    });
}

fn paint_game(
    g: &Gfx,
    hdc: HDC,
    s: &State,
    big: windows::Win32::Graphics::Gdi::HFONT,
    body: windows::Win32::Graphics::Gdi::HFONT,
) {
    let p = pal();
    let Some(game) = s.game.as_ref() else {
        return;
    };
    let now = s.now_ms();
    let top = TEXT_Y;
    let height = FOOT_Y - 20 - top;
    ui::card(g, ui::rect(PAD, top, W - 2 * PAD, height));
    let one = DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX;
    // A miss shakes the word being typed for a moment.
    let shake = game
        .missed_at
        .map(|at| now.saturating_sub(at))
        .filter(|&ms| ms < 240)
        .map_or(0, |ms| if (ms / 40) % 2 == 0 { 4 } else { -4 });
    for f in &game.falling {
        let y = top + 8 + (f.y * (height - 52) as f32) as i32;
        let done: String = f.text[..f.typed].iter().collect();
        let rest: String = f.text[f.typed..].iter().collect();
        let dx = if f.typed > 0 { shake } else { 0 };
        let mut x = ui::px(f.x + dx);
        // Redder the closer it is to the ground.
        let near = ((f.y - 0.6).max(0.0) / 0.4 * 100.0) as u8;
        let ink = ui::blend(p.text, DANGER, near);
        for (part, color) in [(done, p.accent), (rest, ink)] {
            if part.is_empty() {
                continue;
            }
            let w = ui::measure_width(hdc, &part, big);
            ui::text(
                hdc,
                &part,
                RECT {
                    left: x,
                    top: ui::px(y),
                    right: x + w,
                    bottom: ui::px(y + 44),
                },
                big,
                color,
                one,
            );
            x += w;
        }
    }
    // Points rising and fading where each word was finished.
    for (x, fy, text, at) in &game.pops {
        let age = now.saturating_sub(*at).min(POP_MS);
        let rise = (age * 30 / POP_MS) as i32;
        let y = top + 8 + (fy * (height - 52) as f32) as i32 - rise;
        let color = ui::blend(p.accent, p.surface, (age * 100 / POP_MS) as u8);
        ui::text(hdc, text, ui::rect(*x, y, 80, 36), big, color, one);
    }
    // The ground.
    g.fill_round(
        ui::rect(PAD + 12, top + height - 10, W - 2 * PAD - 24, 3),
        1.5,
        p.toggle_off,
    );
    let status = if game.over {
        format!(
            "{}   ·   {}",
            trf(T::PracticeGameOver, &[("n", &game.score.to_string())]),
            trf(T::PracticeBestCombo, &[("n", &game.best_combo.to_string())])
        )
    } else {
        let hearts: String = "♥".repeat(game.lives as usize);
        let mut line = format!(
            "{}   ·   {}   ·   {}",
            trf(T::PracticeGameScore, &[("n", &game.score.to_string())]),
            trf(T::PracticeLevel, &[("n", &game.level.to_string())]),
            hearts
        );
        if game.combo >= 2 {
            line = format!(
                "{}   ·   {line}",
                trf(T::PracticeCombo, &[("n", &game.combo.to_string())])
            );
        }
        line
    };
    // Top-right of the game area, out of the words' way.
    ui::text(
        hdc,
        &status,
        ui::rect(PAD + 16, top + 10, W - 2 * PAD - 32, 28),
        body,
        if game.combo >= 5 {
            p.accent
        } else {
            p.text_dim
        },
        windows::Win32::Graphics::Gdi::DT_RIGHT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
}
