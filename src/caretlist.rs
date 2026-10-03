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
use righttype::atcaret::{self, Kind, Row};
use righttype::i18n::{lang, tr, Lang, T};
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
/// A key handed over by the hook (wParam: the key; lParam: its character).
const WM_CARET_KEY: u32 = 0x8000 + 0x5A2;
/// Filter this long after the last key (a search can take a few ms).
const FILTER_MS: u32 = 60;
const TIMER_FILTER: usize = 1;
const TIMER_WATCH: usize = 2;

static ENABLED: AtomicBool = AtomicBool::new(true);
/// The open list's window (0: closed).
static OPEN: AtomicIsize = AtomicIsize::new(0);
/// The window the list types into.
static TARGET: AtomicIsize = AtomicIsize::new(0);

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
}

thread_local! {
    static LIST: RefCell<Option<List>> = const { RefCell::new(None) };
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Open the list (from the keyboard hook: the window is shown after the
/// hook returns).
pub fn request_open() {
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
        return false;
    }
    let fg = unsafe { GetForegroundWindow() }.0 as isize;
    if fg != TARGET.load(Ordering::Acquire) && fg != open {
        return false;
    }
    let printable = ch.is_some_and(|c| !c.is_control());
    if !printable && !matches!(vk, 0x08 | 0x09 | 0x0D | 0x1B | 0x26 | 0x28) {
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
    // Topmost, off the taskbar, never activated: the app keeps the focus.
    const EX: u32 = 0x0000_0008 | 0x0000_0080 | 0x0800_0000;
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
    if is_open() {
        return;
    }
    let target = unsafe { GetForegroundWindow() };
    if target.0.is_null() {
        return;
    }
    ui::refresh();
    let Some(hwnd) = ensure() else {
        return;
    };
    STATE.with(|s| *s.borrow_mut() = State::default());
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
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, ui::px(W), h, SWP_NOACTIVATE);
        round(hwnd, h);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, TIMER_WATCH, 300, None);
    }
    ui::redraw_all(hwnd);
    crate::hook::trace_note("caret list: open");
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
        s.selected = 0;
    });
    if hwnd != 0 {
        unsafe {
            let h = HWND(hwnd as *mut _);
            let _ = KillTimer(h, TIMER_WATCH);
            let _ = KillTimer(h, TIMER_FILTER);
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
    crate::hook::trace_note("caret list: closed");
}

fn on_key(vk: u16, ch: Option<char>) {
    let hwnd = OPEN.load(Ordering::Acquire);
    if hwnd == 0 {
        return;
    }
    let h = HWND(hwnd as *mut _);
    match (vk, ch) {
        (_, Some(c)) => {
            STATE.with(|s| s.borrow_mut().query.push(c));
            unsafe {
                SetTimer(h, TIMER_FILTER, FILTER_MS, None);
            }
        }
        (0x08, _) => {
            STATE.with(|s| {
                s.borrow_mut().query.pop();
            });
            unsafe {
                SetTimer(h, TIMER_FILTER, FILTER_MS, None);
            }
        }
        (0x0D | 0x09, _) => {
            // What was typed last may not be filtered yet.
            refilter();
            pick();
            return;
        }
        (0x1B, _) => {
            close();
            return;
        }
        (0x26 | 0x28, _) => STATE.with(|s| {
            let mut s = s.borrow_mut();
            let n = s.rows.len();
            if n > 0 {
                s.selected = if vk == 0x26 {
                    s.selected.saturating_sub(1)
                } else {
                    (s.selected + 1).min(n - 1)
                };
            }
        }),
        _ => {}
    }
    ui::redraw_all(h);
}

fn refilter() {
    let query = STATE.with(|s| s.borrow().query.clone());
    let rows = atcaret::search(&query, &crate::hook::snippets(), lang() == Lang::Th);
    let n = rows.len();
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.rows = rows;
        s.selected = 0;
    });
    let hwnd = OPEN.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            let h = HWND(hwnd as *mut _);
            let _ = SetWindowPos(
                h,
                HWND_TOPMOST,
                0,
                0,
                ui::px(W),
                height(n),
                SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE,
            );
            round(h, height(n));
            ui::redraw_all(h);
        }
    }
}

/// Type the selected row into the app and close.
fn pick() {
    let target = TARGET.load(Ordering::Acquire);
    let row = STATE.with(|s| {
        let s = s.borrow();
        s.rows.get(s.selected).cloned()
    });
    close();
    let Some(row) = row else {
        return;
    };
    let text = match row.kind {
        Kind::Snippet => righttype::snippets::fill(&row.text, &crate::hook::snippet_now()),
        Kind::Character => row.text,
    };
    crate::hook::trace_note("caret list: row typed");
    crate::manual::request_type(target, text, false);
}

fn paint(g: &Gfx, hdc: HDC, rc: RECT, _page: u8) {
    let p = pal();
    g.fill_round(rc, ui::px(8) as f32, p.border);
    g.fill_round(ui::inset(rc, ui::px(1)), ui::px(7) as f32, p.surface);
    let body = ui::make_font(14, 400);
    let dim = ui::make_font(12, 400);
    let big = ui::make_font(20, 400);
    let one = DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX;
    STATE.with(|s| {
        let s = s.borrow();
        // The search line.
        let search = ui::rect(PAD, PAD, W - 2 * PAD, SEARCH_H - PAD);
        g.fill_round(search, ui::px(6) as f32, p.inset);
        let text_rc = ui::rect(PAD + 12, PAD, W - 2 * PAD - 24, SEARCH_H - PAD);
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
        // The rows.
        if s.rows.is_empty() && !s.query.is_empty() {
            ui::text(
                hdc,
                tr(T::CaretListNone),
                ui::rect(PAD + 12, SEARCH_H, W - 2 * PAD - 24, ROW_H),
                body,
                p.text_dim,
                DT_LEFT | one,
            );
        }
        for (i, row) in s.rows.iter().enumerate() {
            let y = SEARCH_H + PAD / 2 + i as i32 * ROW_H;
            let selected = i == s.selected;
            if selected {
                g.fill_round(
                    ui::rect(PAD, y, W - 2 * PAD, ROW_H - 2),
                    ui::px(6) as f32,
                    p.accent,
                );
            }
            let (ink, faint) = if selected {
                (p.on_accent, p.on_accent)
            } else {
                (p.text, p.text_dim)
            };
            let glyph_font = if row.kind == Kind::Character {
                big
            } else {
                dim
            };
            ui::text(
                hdc,
                &row.glyph,
                ui::rect(PAD + 4, y, 56, ROW_H - 2),
                glyph_font,
                ink,
                DT_CENTER | one | DT_END_ELLIPSIS,
            );
            ui::text(
                hdc,
                &row.label,
                ui::rect(PAD + 68, y, W - 2 * PAD - 68 - 96, ROW_H - 2),
                body,
                ink,
                DT_LEFT | one | DT_END_ELLIPSIS,
            );
            ui::text(
                hdc,
                &row.detail,
                ui::rect(W - PAD - 100, y, 92, ROW_H - 2),
                dim,
                faint,
                DT_RIGHT | one | DT_END_ELLIPSIS,
            );
        }
        // How to use it, at the bottom.
        let foot_y = SEARCH_H + PAD / 2 + s.rows.len().max(1) as i32 * ROW_H;
        ui::text(
            hdc,
            tr(T::CaretListKeys),
            ui::rect(PAD + 12, foot_y, W - 2 * PAD - 24, FOOT_H),
            dim,
            p.text_dim,
            DT_LEFT | one,
        );
    });
    unsafe {
        for f in [body, dim, big] {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
}

/// Open with a search already typed (debug renders).
#[cfg(debug_assertions)]
pub fn open_demo(query: &str) {
    open();
    STATE.with(|s| s.borrow_mut().query = query.to_string());
    refilter();
}
