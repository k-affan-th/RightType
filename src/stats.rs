//! Usage counters + themed statistics window — Windows only.
//! **In-memory only, never written to disk.**
//!
//! RightType's no-persistence guarantee covers *content* (nothing typed is ever
//! saved), not counts — but to keep that guarantee unambiguous, these counters
//! reset every restart rather than accumulating in a file.
//!
//! Opening the window while one is already visible replaces it with a fresh
//! copy, so the numbers are always current.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};

use native_windows_gui as nwg;
use righttype::i18n::{tr, T};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::learn;
use crate::ui::{self, card, pal, rect, Gfx, Surface, TextStyle};

static AUTO: AtomicU64 = AtomicU64::new(0);
static MANUAL: AtomicU64 = AtomicU64::new(0);
static OPEN_STATS: AtomicIsize = AtomicIsize::new(0);

const WM_CLOSE: u32 = 0x0010;

/// Record one correction made automatically (Auto mode: boundary or live).
pub fn record_auto() {
    AUTO.fetch_add(1, Ordering::Relaxed);
}

/// Record one correction made via a manual hotkey (fix-word or fix-selection).
pub fn record_manual() {
    MANUAL.fetch_add(1, Ordering::Relaxed);
}

/// `(auto corrections, manual corrections)` since this run started.
pub fn snapshot() -> (u64, u64) {
    (AUTO.load(Ordering::Relaxed), MANUAL.load(Ordering::Relaxed))
}

const W: i32 = 520;
const H: i32 = 330;
const X: i32 = 28;
const TILE_W: i32 = 144;
const TILE_GAP: i32 = 16;
const TILE_Y: i32 = 76;
const TILE_H: i32 = 118;

struct StatsWindow {
    window: nwg::Window,
    surface: Rc<Surface>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open (or refresh) the statistics window.
pub fn open() {
    let prev = OPEN_STATS.load(Ordering::Acquire);
    if prev != 0 {
        unsafe {
            let _ = PostMessageW(
                HWND(prev as *mut core::ffi::c_void),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
    ui::refresh();

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, H))
        .title(tr(T::StatsTitle))
        .topmost(true)
        .build(&mut window);
    let surface = Surface::attach(&window, 0x5254_0012, Box::new(paint));
    let s = &surface;
    let p = pal();

    s.label(
        tr(T::StatsHead),
        TextStyle::Title,
        (X, 22, W - 2 * X, 36),
        p.bg,
        0,
    );
    let (auto, manual) = snapshot();
    let tiles = [
        (auto.to_string(), T::StatsAuto),
        (manual.to_string(), T::StatsManual),
        (learn::count().to_string(), T::StatsLearned),
    ];
    for (i, (value, caption)) in tiles.iter().enumerate() {
        let x = X + i as i32 * (TILE_W + TILE_GAP);
        s.label(
            value,
            TextStyle::Display,
            (x + 18, TILE_Y + 14, TILE_W - 36, 48),
            p.surface,
            0,
        );
        s.label(
            tr(*caption),
            TextStyle::Small,
            (x + 18, TILE_Y + 66, TILE_W - 36, 40),
            p.surface,
            0,
        );
    }
    s.label(
        tr(T::StatsNote),
        TextStyle::Small,
        (X, TILE_Y + TILE_H + 18, W - 2 * X, 36),
        p.bg,
        0,
    );
    let close = s.button(
        tr(T::BtnClose),
        false,
        (W - X - 110, H - 28 - 34, 110, 34),
        p.bg,
        0,
    );

    ui::size_and_center(surface.hwnd, W, H);
    let win = Rc::new(StatsWindow {
        window,
        surface,
        handler: RefCell::new(None),
    });
    win.window.set_visible(true);
    OPEN_STATS.store(win.surface.hwnd.0 as isize, Ordering::Release);

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if id == close {
            if let Some(win) = weak.upgrade() {
                finish(&win);
            }
        }
    });
    let win_h = win.clone();
    let handler = nwg::full_bind_event_handler(&win.window.handle, move |evt, _data, handle| {
        if matches!(evt, nwg::Event::OnWindowClose) && handle == win_h.window.handle {
            finish(&win_h);
        }
    });
    *win.handler.borrow_mut() = Some(handler);
}

fn finish(win: &Rc<StatsWindow>) {
    let my = win.surface.hwnd.0 as isize;
    let _ = OPEN_STATS.compare_exchange(my, 0, Ordering::AcqRel, Ordering::Acquire);
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    win.window.close();
}

fn paint(g: &Gfx, _hdc: HDC, _rc: RECT, _page: u8) {
    for i in 0..3 {
        let x = X + i * (TILE_W + TILE_GAP);
        card(g, rect(x, TILE_Y, TILE_W, TILE_H));
    }
}
