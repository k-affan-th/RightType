//! The list at the text cursor (2.4 A) — Windows only. Shift tapped twice
//! opens it under the caret; what is typed then searches special
//! characters and snippets ([`righttype::atcaret`]) and never reaches the
//! document: the keyboard hook hands the keys to the list, as it does for
//! the palette. Enter or Tab types the row picked into the app, Esc closes.
//!
//! One window that never takes the focus, drawn in one paint (no child
//! controls: the palette taught that a repaint per control can make the
//! keyboard hook late). The search text is wiped when the list closes.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use native_windows_gui as nwg;
use righttype::atcaret::{self, Command, Kind, Row};
use righttype::i18n::{lang, tr, trf, Lang, T};
use righttype::motion;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT, DT_SINGLELINE,
    DT_VCENTER, HDC, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, KillTimer, PostMessageW, SetTimer, SetWindowPos, ShowWindow, HWND_TOPMOST,
    SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
};
use zeroize::Zeroize;

use crate::ui::{self, pal, Gfx, Surface};

const W: i32 = 460;
const PAD: i32 = 8;
const SEARCH_H: i32 = 44;
const ROW_H: i32 = 36;
const FOOT_H: i32 = 26;
/// One character's cell in the grid shown before anything is searched.
const CELL: i32 = 44;
const GRID_COLS: usize = ((W - 2 * PAD) / CELL) as usize;
/// A key handed over by the hook (wParam: the key; lParam: its character).
const WM_CARET_KEY: u32 = 0x8000 + 0x5A2;
/// The search worker has rows (see [`RESULT`]).
const WM_CARET_ROWS: u32 = 0x8000 + 0x5A3;
/// Filter this long after the last key (a search can take a few ms).
const FILTER_MS: u32 = 60;
const TIMER_FILTER: usize = 1;
const TIMER_WATCH: usize = 2;
/// Animation frames (Windows' timer resolution, about 60 a second).
const TIMER_ANIM: usize = 3;
const FRAME_MS: u32 = 15;

static ENABLED: AtomicBool = AtomicBool::new(true);
/// The open list's window (0: closed).
static OPEN: AtomicIsize = AtomicIsize::new(0);
/// The window the list types into.
static TARGET: AtomicIsize = AtomicIsize::new(0);
/// Shift was tapped twice and the list is about to open: keys typed in
/// the meantime are the search's (CI: the `e` of `degree` reached Notepad
/// before the list was there).
static OPENING: AtomicBool = AtomicBool::new(false);
static EARLY: std::sync::Mutex<Vec<(u16, Option<char>)>> = std::sync::Mutex::new(Vec::new());

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

pub fn is_open() -> bool {
    OPEN.load(Ordering::Acquire) != 0
}

struct List {
    _window: nwg::Window,
    surface: Rc<Surface>,
    _handler: Option<nwg::RawEventHandler>,
}

#[derive(Default)]
struct State {
    query: String,
    rows: Vec<Row>,
    selected: usize,
    /// The app's commands, read when the list opened (names and keys only).
    commands: Vec<Command>,
    /// A command that could lose work, picked once: Enter again runs it.
    armed: Option<usize>,
    /// Counts changes to `query`; `shown` is the count the rows are for.
    query_gen: u64,
    shown_gen: u64,
    /// Enter (or Tab) came before the rows for the search did: pick when
    /// they come.
    pick_pending: bool,
    /// Ctrl+P: the next key typed is where the selected character goes.
    pinning: bool,
    /// The highlight slides from this row to `selected`, since `slide_at`.
    slide_from: f32,
    slide_at: Option<std::time::Instant>,
    /// Opening (fade in, rise into place) or closing (fade out), since.
    fade: Option<(motion::Phase, std::time::Instant)>,
    /// Where the window rests (screen pixels).
    rest_y: i32,
}

impl State {
    /// The highlight's row, part way through a slide.
    fn highlight(&self) -> f32 {
        let to = self.selected as f32;
        match self.slide_at {
            Some(at) => {
                let t = motion::slide(at.elapsed().as_millis() as u32);
                self.slide_from + (to - self.slide_from) * t
            }
            None => to,
        }
    }
}

thread_local! {
    static LIST: RefCell<Option<List>> = const { RefCell::new(None) };
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Open the list (from the keyboard hook: the window is shown after the
/// hook returns).
pub fn request_open() {
    OPENING.store(true, Ordering::Release);
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        open();
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

/// The keyboard hook hands the open list its keys: typing and Backspace
/// edit the search, Up/Down move, Enter/Tab type the row, Esc closes.
/// Returns whether the list took the key (the hook then swallows it).
pub fn key(vk: u16, ch: Option<char>) -> bool {
    let open = OPEN.load(Ordering::Acquire);
    if open == 0 {
        if !OPENING.load(Ordering::Acquire) {
            return false;
        }
        // Still opening: kept for it, in order.
        let printable = ch.is_some_and(|c| !c.is_control());
        if !printable && !matches!(vk, 0x08 | 0x09 | 0x0D | 0x1B | 0x25 | 0x26 | 0x27 | 0x28) {
            return false;
        }
        if let Ok(mut early) = EARLY.lock() {
            early.push((vk, ch.filter(|c| !c.is_control())));
        }
        return true;
    }
    let fg = unsafe { GetForegroundWindow() }.0 as isize;
    if fg != TARGET.load(Ordering::Acquire) && fg != open {
        crate::hook::e2e_trace(format!(
            "caret list: key {vk:#x} passed on, another window in front ({fg:#x})"
        ));
        return false;
    }
    let printable = ch.is_some_and(|c| !c.is_control());
    if !printable && !matches!(vk, 0x08 | 0x09 | 0x0D | 0x1B | 0x25 | 0x26 | 0x27 | 0x28) {
        return false;
    }
    unsafe {
        let _ = PostMessageW(
            HWND(open as *mut _),
            WM_CARET_KEY,
            WPARAM(vk as usize),
            LPARAM(if printable {
                ch.map_or(0, |c| c as isize)
            } else {
                0
            }),
        );
    }
    true
}

fn ensure() -> Option<HWND> {
    if let Some(h) = LIST.with(|l| l.borrow().as_ref().map(|l| l.surface.hwnd)) {
        return Some(h);
    }
    let mut window = nwg::Window::default();
    // Topmost, off the taskbar, never activated (the app keeps the focus),
    // layered (it fades in and out).
    const EX: u32 = 0x0000_0008 | 0x0000_0080 | 0x0800_0000 | 0x0008_0000;
    nwg::Window::builder()
        .flags(nwg::WindowFlags::POPUP)
        .ex_flags(EX)
        .size((ui::px(W), ui::px(SEARCH_H)))
        .position((-4000, -4000))
        .title(tr(T::CaretListTitle))
        .build(&mut window)
        .ok()?;
    let surface = Surface::attach(&window, 0x5254_0022, Box::new(paint));
    let hwnd = surface.hwnd;
    let handler = nwg::bind_raw_event_handler(&window.handle, 0x5254_0023, |_h, msg, w, l| {
        const WM_TIMER: u32 = 0x0113;
        const WM_MOUSEACTIVATE: u32 = 0x0021;
        const MA_NOACTIVATE: isize = 3;
        match msg {
            WM_CARET_KEY => {
                on_key(w as u16, char::from_u32(l as u32).filter(|c| *c != '\0'));
                Some(0)
            }
            WM_CARET_ROWS => {
                rows_arrived();
                Some(0)
            }
            WM_TIMER if w == TIMER_FILTER => {
                unsafe {
                    let _ = KillTimer(HWND(_h as _), TIMER_FILTER);
                }
                refilter();
                Some(0)
            }
            WM_TIMER if w == TIMER_WATCH => {
                // The typist went to another window: the list goes.
                let fg = unsafe { GetForegroundWindow() }.0 as isize;
                if fg != TARGET.load(Ordering::Acquire) && fg != OPEN.load(Ordering::Acquire) {
                    close();
                }
                Some(0)
            }
            WM_TIMER if w == TIMER_ANIM => {
                animate();
                Some(0)
            }
            WM_MOUSEACTIVATE => Some(MA_NOACTIVATE),
            _ => None,
        }
    })
    .ok();
    LIST.with(|l| {
        *l.borrow_mut() = Some(List {
            _window: window,
            surface,
            _handler: handler,
        })
    });
    Some(hwnd)
}

fn open() {
    // However this ends, it is no longer opening; keys typed meanwhile go
    // to the list (or, if it cannot open, nowhere: they were the search's).
    OPENING.store(false, Ordering::Release);
    let early: Vec<(u16, Option<char>)> = EARLY
        .lock()
        .map(|mut e| std::mem::take(&mut *e))
        .unwrap_or_default();
    if is_open() {
        return;
    }
    let started = std::time::Instant::now();
    let target = unsafe { GetForegroundWindow() };
    if target.0.is_null() {
        return;
    }
    ui::refresh();
    let Some(hwnd) = ensure() else {
        return;
    };
    let commands = crate::sheet::app_commands(target);
    STATE.with(|s| {
        *s.borrow_mut() = State {
            commands,
            ..State::default()
        }
    });
    TARGET.store(target.0 as isize, Ordering::Release);
    OPEN.store(hwnd.0 as isize, Ordering::Release);
    // Under the text cursor; without one, near the top of the window.
    let caret = crate::caret::find_caret();
    unsafe {
        let mut wr = RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(target, &mut wr);
        let (x, y) = match caret {
            Some(c) => (c.left, c.bottom + ui::px(4)),
            None => (
                wr.left + ((wr.right - wr.left) - ui::px(W)) / 2,
                wr.top + ui::px(80),
            ),
        };
        // Kept on the screen the caret is on.
        let wa = ui::work_area(windows::Win32::Graphics::Gdi::MonitorFromWindow(
            target,
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        ));
        let h = height(0);
        let x = x.clamp(wa.left, (wa.right - ui::px(W)).max(wa.left));
        let y = if y + h > wa.bottom {
            caret.map_or(wa.bottom - h, |c| c.top - h - ui::px(4))
        } else {
            y
        };
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.rest_y = y;
            s.fade = Some((motion::Phase::Enter, std::time::Instant::now()));
        });
        let first = motion::frame(motion::Phase::Enter, 0);
        set_alpha(hwnd, first.alpha);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            x,
            y + ui::px(first.drop_px.round() as i32),
            ui::px(W),
            h,
            SWP_NOACTIVATE,
        );
        round(hwnd, h);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER_WATCH, 300, None);
        SetTimer(hwnd, TIMER_ANIM, FRAME_MS, None);
    }
    // Nothing typed yet: the grid of pinned and recent characters.
    refilter();
    ui::redraw_all(hwnd);
    crate::hook::e2e_trace(format!(
        "caret list: opened in {} ms",
        started.elapsed().as_millis()
    ));
    crate::hook::trace_note("caret list: open");
    for (vk, ch) in early {
        if !is_open() {
            break;
        }
        on_key(vk, ch);
    }
}

/// Round the window's corners to match the card drawn in it.
fn round(hwnd: HWND, h: i32) {
    unsafe {
        let r = ui::px(16);
        let rgn =
            windows::Win32::Graphics::Gdi::CreateRoundRectRgn(0, 0, ui::px(W) + 1, h + 1, r, r);
        windows::Win32::Graphics::Gdi::SetWindowRgn(hwnd, rgn, true);
    }
}

/// The window's height for `rows` rows (screen pixels).
/// The grid (nothing searched yet: the characters pinned and used lately,
/// glyphs only) or the list.
fn is_grid(s: &State) -> bool {
    s.query.is_empty() && !s.rows.is_empty()
}

/// The window's height for what it shows (screen pixels).
fn height_now() -> i32 {
    STATE.with(|s| {
        let s = s.borrow();
        if is_grid(&s) {
            let lines = s.rows.len().div_ceil(GRID_COLS) as i32;
            ui::px(SEARCH_H + PAD / 2 + lines * CELL + FOOT_H + PAD)
        } else {
            height(s.rows.len())
        }
    })
}

thread_local! {
    /// Characters picked from the list lately (in memory only), newest first.
    static RECENT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// What the grid shows: the characters used lately, then every one pinned.
fn grid_rows() -> Vec<Row> {
    let mut chars: Vec<String> = RECENT.with(|r| r.borrow().clone());
    for c in crate::hook::char_sets().all_pinned() {
        if !chars.contains(&c) {
            chars.push(c);
        }
    }
    chars.truncate(GRID_COLS * 4);
    chars
        .into_iter()
        .map(|c| atcaret::Row::character(&c))
        .collect()
}

/// Ctrl+P on a character: the next key typed is the key it is pinned to
/// (its Right Alt set). True when there is a character to pin.
pub fn pin_start() -> bool {
    let hwnd = OPEN.load(Ordering::Acquire);
    if hwnd == 0 {
        return false;
    }
    let ok = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let ok = s
            .rows
            .get(s.selected)
            .is_some_and(|r| r.kind == Kind::Character);
        s.pinning = ok;
        ok
    });
    if ok {
        ui::redraw_all(HWND(hwnd as *mut _));
    }
    ok
}

fn height(rows: usize) -> i32 {
    let rows = rows.max(1) as i32;
    ui::px(SEARCH_H + rows * ROW_H + FOOT_H + PAD)
}

fn close() {
    let hwnd = OPEN.swap(0, Ordering::AcqRel);
    TARGET.store(0, Ordering::Release);
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.query.zeroize();
        s.rows.clear();
        s.commands.clear();
        s.armed = None;
        s.selected = 0;
        s.slide_at = None;
        s.fade = Some((motion::Phase::Exit, std::time::Instant::now()));
    });
    if hwnd != 0 {
        unsafe {
            let h = HWND(hwnd as *mut _);
            let _ = KillTimer(h, TIMER_WATCH);
            let _ = KillTimer(h, TIMER_FILTER);
            // It fades out (see `animate`); typing goes to the app at once.
            SetTimer(h, TIMER_ANIM, FRAME_MS, None);
        }
    }
    crate::hook::trace_note("caret list: closed");
}

fn set_alpha(hwnd: HWND, alpha: u8) {
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            alpha,
            windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
        );
    }
}

/// One animation frame: the fade (in or out) and the highlight's slide.
fn animate() {
    let Some(hwnd) = LIST.with(|l| l.borrow().as_ref().map(|l| l.surface.hwnd)) else {
        return;
    };
    let (fade, sliding, rest_y) = STATE.with(|s| {
        let mut s = s.borrow_mut();
        // A slide that has settled is over.
        if s.slide_at
            .is_some_and(|at| at.elapsed().as_millis() as u32 >= motion::SLIDE_MS)
        {
            s.slide_at = None;
        }
        (s.fade, s.slide_at.is_some(), s.rest_y)
    });
    let mut busy = sliding;
    if let Some((phase, since)) = fade {
        let f = motion::frame(phase, since.elapsed().as_millis() as u32);
        set_alpha(hwnd, f.alpha);
        unsafe {
            if phase == motion::Phase::Enter {
                let mut wr = RECT::default();
                let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut wr);
                let _ = SetWindowPos(
                    hwnd,
                    HWND_TOPMOST,
                    wr.left,
                    rest_y + ui::px(f.drop_px.round() as i32),
                    0,
                    0,
                    SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE,
                );
            }
            if f.done {
                STATE.with(|s| s.borrow_mut().fade = None);
                if phase == motion::Phase::Exit {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
            } else {
                busy = true;
            }
        }
    }
    if sliding {
        ui::redraw_all(hwnd);
    }
    if !busy {
        unsafe {
            let _ = KillTimer(hwnd, TIMER_ANIM);
        }
    }
}

fn on_key(vk: u16, ch: Option<char>) {
    let hwnd = OPEN.load(Ordering::Acquire);
    if hwnd == 0 {
        return;
    }
    let h = HWND(hwnd as *mut _);
    // Pinning: the key typed is where the character goes; Esc leaves it.
    if STATE.with(|s| s.borrow().pinning) {
        let row = STATE.with(|s| {
            let mut s = s.borrow_mut();
            s.pinning = false;
            s.rows.get(s.selected).cloned()
        });
        if let (Some(key), Some(row)) = (ch.filter(|c| !c.is_whitespace()), row) {
            let on = crate::hook::pin_char(key, &row.text);
            let (k, c) = (key.to_string(), row.text.clone());
            crate::overlay::show(&trf(
                if on {
                    T::CaretListPinned
                } else {
                    T::CaretListUnpinned
                },
                &[("ch", &c), ("key", &k)],
            ));
            if STATE.with(|s| s.borrow().query.is_empty()) {
                refilter();
            }
        }
        ui::redraw_all(h);
        return;
    }
    let grid = STATE.with(|s| is_grid(&s.borrow()));
    if grid && matches!(vk, 0x25..=0x28) {
        STATE.with(|s| {
            let mut s = s.borrow_mut();
            let n = s.rows.len();
            s.selected = match vk {
                0x25 => s.selected.saturating_sub(1),
                0x27 => (s.selected + 1).min(n - 1),
                0x26 => s.selected.saturating_sub(GRID_COLS),
                _ => (s.selected + GRID_COLS).min(n - 1),
            };
        });
        ui::redraw_all(h);
        return;
    }
    match (vk, ch) {
        (_, Some(c)) => {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.query.push(c);
                s.query_gen += 1;
            });
            unsafe {
                SetTimer(h, TIMER_FILTER, FILTER_MS, None);
            }
        }
        (0x08, _) => {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                s.query.pop();
                s.query_gen += 1;
            });
            unsafe {
                SetTimer(h, TIMER_FILTER, FILTER_MS, None);
            }
        }
        (0x0D | 0x09, _) => {
            // What was typed last may not be searched yet: pick once it is.
            unsafe {
                let _ = KillTimer(h, TIMER_FILTER);
            }
            let ready = STATE.with(|s| {
                let s = s.borrow();
                s.shown_gen == s.query_gen
            });
            if ready {
                pick();
            } else {
                STATE.with(|s| s.borrow_mut().pick_pending = true);
                refilter();
            }
            return;
        }
        (0x1B, _) => {
            close();
            return;
        }
        (0x26 | 0x28, _) => {
            STATE.with(|s| {
                let mut s = s.borrow_mut();
                let n = s.rows.len();
                if n > 0 {
                    // From wherever the highlight is now, even mid-slide.
                    s.slide_from = s.highlight();
                    s.selected = if vk == 0x26 {
                        s.selected.saturating_sub(1)
                    } else {
                        (s.selected + 1).min(n - 1)
                    };
                    s.slide_at = Some(std::time::Instant::now());
                }
            });
            unsafe {
                SetTimer(h, TIMER_ANIM, FRAME_MS, None);
            }
        }
        _ => {}
    }
    ui::redraw_all(h);
}

/// A search for the worker.
struct Job {
    query_gen: u64,
    query: zeroize::Zeroizing<String>,
    snippets: Vec<righttype::snippets::Snippet>,
    commands: Vec<Command>,
    thai: bool,
    hwnd: isize,
}

/// The worker's latest rows, and the search they are for.
static RESULT: std::sync::Mutex<Option<(u64, Vec<Row>)>> = std::sync::Mutex::new(None);

/// The search runs on a thread of its own: over every character Unicode
/// names it takes milliseconds (hundreds in a debug build), and on the UI
/// thread, which runs the keyboard hook, that long makes Windows hand keys
/// straight to the app (CI: `ae` of `replace` reached Notepad).
fn search_worker() -> &'static std::sync::Mutex<std::sync::mpsc::Sender<Job>> {
    static TX: std::sync::OnceLock<std::sync::Mutex<std::sync::mpsc::Sender<Job>>> =
        std::sync::OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        let _ = std::thread::Builder::new()
            .name("caret list search".into())
            .spawn(move || {
                while let Ok(mut job) = rx.recv() {
                    // Only the newest search matters.
                    while let Ok(newer) = rx.try_recv() {
                        job = newer;
                    }
                    let rows = atcaret::search(&job.query, &job.snippets, &job.commands, job.thai);
                    if let Ok(mut r) = RESULT.lock() {
                        *r = Some((job.query_gen, rows));
                    }
                    unsafe {
                        let _ = PostMessageW(
                            HWND(job.hwnd as *mut _),
                            WM_CARET_ROWS,
                            WPARAM(0),
                            LPARAM(0),
                        );
                    }
                }
            });
        std::sync::Mutex::new(tx)
    })
}

/// Search for what is typed now (the rows come in [`rows_arrived`]).
fn refilter() {
    let hwnd = OPEN.load(Ordering::Acquire);
    if hwnd == 0 {
        return;
    }
    // Nothing typed: the grid, at once (no search to wait for).
    let empty = STATE.with(|s| s.borrow().query.is_empty().then_some(s.borrow().query_gen));
    if let Some(generation) = empty {
        if let Ok(mut r) = RESULT.lock() {
            *r = Some((generation, grid_rows()));
        }
        rows_arrived();
        return;
    }
    let job = STATE.with(|s| {
        let s = s.borrow();
        Job {
            query_gen: s.query_gen,
            query: zeroize::Zeroizing::new(s.query.clone()),
            snippets: crate::hook::snippets(),
            commands: s.commands.clone(),
            thai: lang() == Lang::Th,
            hwnd,
        }
    });
    if let Ok(tx) = search_worker().lock() {
        let _ = tx.send(job);
    }
}

/// The worker's rows: shown if they are for the search typed now, then
/// picked from if Enter is waiting for them.
fn rows_arrived() {
    let Some((generation, rows)) = RESULT.lock().ok().and_then(|mut r| r.take()) else {
        return;
    };
    let hwnd = OPEN.load(Ordering::Acquire);
    let current = STATE.with(|s| s.borrow().query_gen == generation);
    if hwnd == 0 || !current {
        return;
    }
    let pick_now = STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.rows = rows;
        s.shown_gen = generation;
        s.armed = None;
        s.selected = 0;
        s.slide_from = 0.0;
        s.slide_at = None;
        #[cfg(debug_assertions)]
        if DEMO_ARMED.swap(false, Ordering::Relaxed) {
            s.armed = Some(0);
        }
        std::mem::take(&mut s.pick_pending)
    });
    unsafe {
        let h = HWND(hwnd as *mut _);
        let _ = SetWindowPos(
            h,
            HWND_TOPMOST,
            0,
            0,
            ui::px(W),
            height_now(),
            SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE,
        );
        round(h, height_now());
        ui::redraw_all(h);
    }
    if pick_now {
        pick();
    }
}

/// Type the selected row into the app and close.
fn pick() {
    let target = TARGET.load(Ordering::Acquire);
    let row = STATE.with(|s| {
        let s = s.borrow();
        s.rows.get(s.selected).cloned()
    });
    // A row saying why LaTeX cannot be written: stay open to fix the search.
    if row.as_ref().is_some_and(|r| r.text.is_empty()) {
        return;
    }
    // A command that could lose work takes Enter twice.
    if let Some(r) = row.as_ref().filter(|r| r.kind == Kind::Command) {
        let cmd = Command {
            name: r.label.clone(),
            keys: r.text.clone(),
            other: String::new(),
        };
        let first = STATE.with(|s| {
            let mut s = s.borrow_mut();
            let first = atcaret::needs_confirming(&cmd) && s.armed != Some(s.selected);
            if first {
                s.armed = Some(s.selected);
            }
            first
        });
        if first {
            crate::hook::trace_note("caret list: command needs Enter again");
            let hwnd = OPEN.load(Ordering::Acquire);
            if hwnd != 0 {
                ui::redraw_all(HWND(hwnd as *mut _));
            }
            return;
        }
    }
    close();
    let Some(row) = row else {
        return;
    };
    if row.kind == Kind::Command {
        crate::hook::trace_note("caret list: command run");
        if crate::sheet::press_in(target, &row.text) {
            // The keys for next time (2.4 E3).
            crate::overlay::show(&trf(T::CaretListNextTime, &[("keys", &row.text)]));
        }
        return;
    }
    if row.kind == Kind::Character {
        RECENT.with(|r| {
            let mut r = r.borrow_mut();
            r.retain(|c| *c != row.text);
            r.insert(0, row.text.clone());
            r.truncate(GRID_COLS);
        });
    }
    let text = match row.kind {
        Kind::Snippet => righttype::snippets::fill(&row.text, &crate::hook::snippet_now()),
        _ => row.text,
    };
    crate::hook::trace_note("caret list: row typed");
    crate::manual::request_type(target, text, false);
}

/// Icons (Segoe Fluent Icons / MDL2 Assets).
const ICON_SEARCH: &str = "\u{E721}";
const ICON_SNIPPET: &str = "\u{E70B}";
const ICON_EQUATION: &str = "\u{E8EF}";
const ICON_COMMAND: &str = "\u{E945}";
const ICON_WARNING: &str = "\u{E7BA}";

fn paint(g: &Gfx, hdc: HDC, rc: RECT, _page: u8) {
    let p = pal();
    g.fill_round(rc, ui::px(8) as f32, p.border);
    g.fill_round(ui::inset(rc, ui::px(1)), ui::px(7) as f32, p.surface);
    let body = ui::make_font(14, 400);
    let dim = ui::make_font(12, 400);
    let big = ui::make_font(20, 400);
    let icons = ui::make_icon_font(16);
    let one = DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX;
    // An icon, or nothing where Windows has no icon font.
    let icon = |s: &str, r: RECT, color| {
        if let Some(f) = icons {
            ui::text(hdc, s, r, f, color, DT_CENTER | one);
        }
    };
    STATE.with(|s| {
        let s = s.borrow();
        // The search line: a magnifier, then what is typed (or a hint).
        let search = ui::rect(PAD, PAD, W - 2 * PAD, SEARCH_H - PAD);
        g.fill_round(search, ui::px(6) as f32, p.inset);
        icon(
            ICON_SEARCH,
            ui::rect(PAD + 4, PAD, 32, SEARCH_H - PAD),
            p.text_dim,
        );
        let text_x = if icons.is_some() { PAD + 38 } else { PAD + 12 };
        let text_rc = ui::rect(text_x, PAD, W - PAD - 12 - text_x, SEARCH_H - PAD);
        if s.query.is_empty() {
            ui::text(
                hdc,
                tr(T::CaretListHint),
                text_rc,
                body,
                p.text_dim,
                DT_LEFT | one,
            );
        } else {
            ui::text(
                hdc,
                &s.query,
                text_rc,
                body,
                p.text,
                DT_LEFT | one | DT_END_ELLIPSIS,
            );
            // A text cursor after it (drawn: not every font has a bar glyph).
            let w = ui::measure_width(hdc, &s.query, body);
            let x = (text_rc.left + w + ui::px(2)).min(text_rc.right);
            let mid = (text_rc.top + text_rc.bottom) / 2;
            ui::fill(
                hdc,
                RECT {
                    left: x,
                    top: mid - ui::px(9),
                    right: x + ui::px(2).max(1),
                    bottom: mid + ui::px(9),
                },
                p.accent,
            );
        }
        let row_y = |i: f32| SEARCH_H + PAD / 2 + (i * ROW_H as f32).round() as i32;
        if s.rows.is_empty() && !s.query.is_empty() {
            icon(
                ICON_SEARCH,
                ui::rect(PAD + 16, SEARCH_H, 32, ROW_H),
                p.text_dim,
            );
            ui::text(
                hdc,
                tr(T::CaretListNone),
                ui::rect(PAD + 56, SEARCH_H, W - 2 * PAD - 68, ROW_H),
                body,
                p.text_dim,
                DT_LEFT | one,
            );
        }
        // Nothing searched yet: the pinned and recent characters as a grid
        // of glyphs, nothing else (names only once something is searched).
        if is_grid(&s) {
            let top = SEARCH_H + PAD / 2;
            for (i, row) in s.rows.iter().enumerate() {
                let (col, line) = ((i % GRID_COLS) as i32, (i / GRID_COLS) as i32);
                let cell = ui::rect(PAD + col * CELL, top + line * CELL, CELL - 4, CELL - 4);
                let selected = i == s.selected;
                if selected {
                    g.fill_round(cell, ui::px(6) as f32, p.accent);
                }
                ui::text(
                    hdc,
                    &row.glyph,
                    cell,
                    big,
                    if selected { p.on_accent } else { p.text },
                    DT_CENTER | one,
                );
            }
            let lines = s.rows.len().div_ceil(GRID_COLS) as i32;
            let hint = if s.pinning {
                let ch = s
                    .rows
                    .get(s.selected)
                    .map(|r| r.text.as_str())
                    .unwrap_or("");
                trf(T::CaretListPinHint, &[("ch", ch)])
            } else {
                tr(T::CaretListKeysGrid).to_string()
            };
            ui::text(
                hdc,
                &hint,
                ui::rect(PAD + 12, top + lines * CELL, W - 2 * PAD - 24, FOOT_H),
                dim,
                if s.pinning { p.accent } else { p.text_dim },
                DT_LEFT | one,
            );
            return;
        }
        // The highlight, where its slide has got to.
        if !s.rows.is_empty() {
            g.fill_round(
                ui::rect(PAD, row_y(s.highlight()), W - 2 * PAD, ROW_H - 2),
                ui::px(6) as f32,
                p.accent,
            );
        }
        for (i, row) in s.rows.iter().enumerate() {
            let y = row_y(i as f32);
            let selected = i == s.selected;
            let (ink, faint) = if selected {
                (p.on_accent, p.on_accent)
            } else {
                (p.text, p.text_dim)
            };
            let glyph_rc = ui::rect(PAD + 4, y, 56, ROW_H - 2);
            let label_rc = ui::rect(PAD + 68, y, W - 2 * PAD - 68 - 96, ROW_H - 2);
            let detail_rc = ui::rect(W - PAD - 100, y, 92, ROW_H - 2);
            match row.kind {
                Kind::Character => {
                    ui::text(hdc, &row.glyph, glyph_rc, big, ink, DT_CENTER | one);
                    ui::text(
                        hdc,
                        &row.label,
                        label_rc,
                        body,
                        ink,
                        DT_LEFT | one | DT_END_ELLIPSIS,
                    );
                    ui::text(
                        hdc,
                        &row.detail,
                        detail_rc,
                        dim,
                        faint,
                        DT_RIGHT | one | DT_END_ELLIPSIS,
                    );
                }
                Kind::Snippet => {
                    if icons.is_some() {
                        icon(ICON_SNIPPET, glyph_rc, ink);
                    } else {
                        ui::text(hdc, "…", glyph_rc, big, ink, DT_CENTER | one);
                    }
                    ui::text(
                        hdc,
                        &row.label,
                        label_rc,
                        body,
                        ink,
                        DT_LEFT | one | DT_END_ELLIPSIS,
                    );
                    // The trigger, like a key cap, at the right.
                    ui::text(
                        hdc,
                        &row.glyph,
                        detail_rc,
                        dim,
                        faint,
                        DT_RIGHT | one | DT_END_ELLIPSIS,
                    );
                }
                Kind::Command => {
                    let armed = s.armed == Some(i);
                    if icons.is_some() {
                        icon(
                            if armed { ICON_WARNING } else { ICON_COMMAND },
                            glyph_rc,
                            ink,
                        );
                    } else {
                        ui::text(hdc, "⌘", glyph_rc, big, ink, DT_CENTER | one);
                    }
                    let label = if armed {
                        trf(T::CaretListAgain, &[("what", &row.label)])
                    } else {
                        row.label.clone()
                    };
                    ui::text(
                        hdc,
                        &label,
                        label_rc,
                        body,
                        ink,
                        DT_LEFT | one | DT_END_ELLIPSIS,
                    );
                    ui::text(
                        hdc,
                        &row.detail,
                        detail_rc,
                        dim,
                        faint,
                        DT_RIGHT | one | DT_END_ELLIPSIS,
                    );
                }
                Kind::Equation => {
                    // What the LaTeX writes, large; or, dim, why it cannot.
                    let ok = !row.text.is_empty();
                    if icons.is_some() {
                        icon(
                            if ok { ICON_EQUATION } else { ICON_WARNING },
                            glyph_rc,
                            if ok { ink } else { faint },
                        );
                    } else {
                        ui::text(hdc, "∑", glyph_rc, big, ink, DT_CENTER | one);
                    }
                    ui::text(
                        hdc,
                        &row.label,
                        label_rc,
                        if ok { big } else { body },
                        if ok { ink } else { faint },
                        DT_LEFT | one | DT_END_ELLIPSIS,
                    );
                    ui::text(
                        hdc,
                        &row.detail,
                        detail_rc,
                        dim,
                        faint,
                        DT_RIGHT | one | DT_END_ELLIPSIS,
                    );
                }
            }
        }
        // How to use it, at the bottom.
        let foot_y = SEARCH_H + PAD / 2 + s.rows.len().max(1) as i32 * ROW_H;
        let on_command = s
            .rows
            .get(s.selected)
            .is_some_and(|r| r.kind == Kind::Command);
        let pin_hint = s.pinning.then(|| {
            let ch = s
                .rows
                .get(s.selected)
                .map(|r| r.text.as_str())
                .unwrap_or("");
            trf(T::CaretListPinHint, &[("ch", ch)])
        });
        ui::text(
            hdc,
            pin_hint.as_deref().unwrap_or(tr(if on_command {
                T::CaretListKeysRun
            } else {
                T::CaretListKeys
            })),
            ui::rect(PAD + 12, foot_y, W - 2 * PAD - 24, FOOT_H),
            dim,
            p.text_dim,
            DT_LEFT | one,
        );
    });
    unsafe {
        for f in [Some(body), Some(dim), Some(big), icons]
            .into_iter()
            .flatten()
        {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
}

/// Open with a search already typed (debug renders).
#[cfg(debug_assertions)]
pub fn open_demo(query: &str) {
    open();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.query = query.to_string();
        s.query_gen += 1;
    });
    refilter();
}

/// The same, with the first row's Enter-again state showing once the
/// rows are in.
#[cfg(debug_assertions)]
pub fn open_demo_armed(query: &str) {
    DEMO_ARMED.store(true, Ordering::Relaxed);
    open_demo(query);
}

#[cfg(debug_assertions)]
static DEMO_ARMED: AtomicBool = AtomicBool::new(false);
