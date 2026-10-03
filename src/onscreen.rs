//! Keys shown on screen (2.3 C8) — Windows only. What may be shown is in
//! [`righttype::keycast`]: shortcuts and keys that type nothing, never
//! letters; the keyboard hook shows nothing in password fields and apps on
//! the safety list. Unlike RightType's own hints, this caption is left in
//! screen recordings and sharing: that is what it is for. Off unless
//! turned on; nothing is kept once it fades.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use native_windows_gui as nwg;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    DeleteObject, DT_CENTER, DT_SINGLELINE, DT_VCENTER, HDC, HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, KillTimer, SetTimer, SetWindowPos, ShowWindow, HWND_TOPMOST,
    SWP_NOACTIVATE, SW_HIDE, SW_SHOWNOACTIVATE,
};

use crate::ui::{self, Gfx, Surface};

const W: i32 = 420;
const H: i32 = 56;
/// How long the caption stays after the last key.
const HOLD_MS: u32 = 2000;
/// Keys shown at once, newest last.
const MAX: usize = 3;
const INK: ui::Rgb = 0xF5F5F5;
const FILL: ui::Rgb = 0x262626;

static ON: AtomicBool = AtomicBool::new(false);
static HOLD: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(HOLD_MS);

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        hide();
    }
}

struct Cast {
    window: nwg::Window,
    surface: Rc<Surface>,
    _handler: Option<nwg::RawEventHandler>,
}

thread_local! {
    static CAST: RefCell<Option<Cast>> = const { RefCell::new(None) };
    /// What is shown: each key and how many times in a row.
    static SHOWN: RefCell<Vec<(String, u32)>> = const { RefCell::new(Vec::new()) };
    static PENDING: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Show `label` (from the keyboard hook: the window is touched after the
/// hook returns).
pub fn show(label: String) {
    PENDING.with(|p| *p.borrow_mut() = Some(label));
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        if let Some(label) = PENDING.with(|p| p.borrow_mut().take()) {
            show_now(label);
        }
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

/// Show what is waiting now (debug renders).
#[cfg(debug_assertions)]
pub fn flush() {
    HOLD.store(60_000, Ordering::Relaxed);
    if let Some(label) = PENDING.with(|p| p.borrow_mut().take()) {
        show_now(label);
    }
}

fn ensure() -> Option<HWND> {
    if let Some(h) = CAST.with(|c| c.borrow().as_ref().map(|c| c.surface.hwnd)) {
        return Some(h);
    }
    let mut window = nwg::Window::default();
    // Topmost, off the taskbar, never activated, layered, and clicks go
    // through to what is under it.
    const EX: u32 = 0x0000_0008 | 0x0000_0080 | 0x0800_0000 | 0x0008_0000 | 0x0000_0020;
    nwg::Window::builder()
        .flags(nwg::WindowFlags::POPUP)
        .ex_flags(EX)
        .size((ui::px(W), ui::px(H)))
        .position((-4000, -4000))
        .title("")
        .build(&mut window)
        .ok()?;
    let surface = Surface::attach(&window, 0x5254_001E, Box::new(paint));
    let hwnd = surface.hwnd;
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            235,
            windows::Win32::UI::WindowsAndMessaging::LWA_ALPHA,
        );
    }
    let handler = nwg::bind_raw_event_handler(&window.handle, 0x5254_001F, |h, msg, w, _| {
        const WM_TIMER: u32 = 0x0113;
        if msg == WM_TIMER && w == 1 {
            unsafe {
                let _ = KillTimer(HWND(h as _), 1);
            }
            hide();
            return Some(0);
        }
        None
    })
    .ok();
    CAST.with(|c| {
        *c.borrow_mut() = Some(Cast {
            window,
            surface,
            _handler: handler,
        })
    });
    Some(hwnd)
}

fn show_now(label: String) {
    if !enabled() {
        return;
    }
    crate::hook::e2e_trace(format!("keys on screen: {label}"));
    SHOWN.with(|s| {
        let mut s = s.borrow_mut();
        match s.last_mut() {
            Some((last, n)) if *last == label => *n += 1,
            _ => s.push((label, 1)),
        }
        let extra = s.len().saturating_sub(MAX);
        s.drain(..extra);
    });
    let Some(hwnd) = ensure() else {
        return;
    };
    unsafe {
        // Bottom centre of the screen in use.
        let wa = ui::work_area(windows::Win32::Graphics::Gdi::MonitorFromWindow(
            GetForegroundWindow(),
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        ));
        let (w, h) = (ui::px(W), ui::px(H));
        let x = wa.left + ((wa.right - wa.left) - w) / 2;
        let y = wa.bottom - h - ui::px(72);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, w, h, SWP_NOACTIVATE);
        let rgn = windows::Win32::Graphics::Gdi::CreateRoundRectRgn(
            0,
            0,
            w + 1,
            h + 1,
            ui::px(28),
            ui::px(28),
        );
        windows::Win32::Graphics::Gdi::SetWindowRgn(hwnd, rgn, false);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        ui::redraw_all(hwnd);
        SetTimer(hwnd, 1, HOLD.load(Ordering::Relaxed), None);
    }
}

fn hide() {
    SHOWN.with(|s| s.borrow_mut().clear());
    CAST.with(|c| {
        if let Some(c) = c.borrow().as_ref() {
            unsafe {
                let _ = ShowWindow(c.surface.hwnd, SW_HIDE);
            }
            let _ = &c.window;
        }
    });
}

/// The keys shown, as one line: `Ctrl+C   Ctrl+V ×2`.
fn line() -> String {
    SHOWN.with(|s| {
        s.borrow()
            .iter()
            .map(|(k, n)| {
                if *n > 1 {
                    format!("{k} ×{n}")
                } else {
                    k.clone()
                }
            })
            .collect::<Vec<_>>()
            .join("    ")
    })
}

fn paint(g: &Gfx, hdc: HDC, rc: RECT, _page: u8) {
    // The corners outside the pill: the colour of the pill's edge, so a
    // missing transparency shows a square, not a hole.
    ui::fill(hdc, rc, FILL);
    g.fill_round(rc, ui::px(14) as f32, FILL);
    let font = ui::make_font(20, 600);
    ui::text(
        hdc,
        &line(),
        ui::inset(rc, ui::px(12)),
        font,
        INK,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    unsafe {
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}
