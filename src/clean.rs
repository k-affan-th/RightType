//! Cleaning the keyboard, and testing it — Windows only.
//!
//! One window for both. Cleaning: pick how long, then the keyboard (and,
//! when asked, the mouse) is locked while the keys are wiped; the keys
//! pressed light up, so the wiper sees what is left; it unlocks when the
//! time is up or with the button. Testing: every key pressed lights up and
//! goes nowhere else, and the last one is named; Esc held ends it. See
//! [`crate::lock`] for the lock itself.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use righttype::keyboard::{self, KEYS};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{DeleteObject, HDC, HGDIOBJ};
use windows::Win32::UI::WindowsAndMessaging::{
    KillTimer, SetTimer, SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
};

use crate::ui::{self, pal, Gfx, Surface, TextStyle};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Clean,
    Test,
}

/// Window pages: choosing (cleaning only), running, done (cleaning only).
const SETUP: u8 = 1;
const RUNNING: u8 = 2;
const DONE: u8 = 3;

const W: i32 = 1040;
const H: i32 = 640;
const PAD: i32 = 32;
/// The keyboard drawing: top and one key's size.
const KB_Y: i32 = 216;
const UNIT: i32 = 42;
/// Where the running page's status and buttons sit, under the keyboard.
const FOOT_Y: i32 = KB_Y + (keyboard::HEIGHT * UNIT as f32) as i32 + 24;
/// The pause between the start button and the lock.
const LEAD_IN: Duration = Duration::from_secs(3);
const TICK_MS: u32 = 200;
const DURATIONS: [Duration; 3] = [
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120),
];

struct Win {
    window: nwg::Window,
    surface: Rc<Surface>,
    mode: Mode,
    durations: [u16; 3],
    lock_mouse: u16,
    start: u16,
    unlock: u16,
    start_over: u16,
    close: u16,
    close_done: u16,
    test_keys: u16,
    /// When the lock begins (the lead-in after Start).
    starting: Cell<Option<Instant>>,
    timer: Cell<usize>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Win>>> = const { RefCell::new(None) };
    /// What the painter shows (it has no access to the window).
    static SHOWN: Cell<(Mode, Option<Instant>)> = const { Cell::new((Mode::Clean, None)) };
}

/// Open the window in `mode` (from the message loop: the hook and menus
/// call this; windows are made outside the hook).
pub fn request_open(mode: Mode) {
    unsafe extern "system" fn clean(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        open(Mode::Clean);
    }
    unsafe extern "system" fn test(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        open(Mode::Test);
    }
    unsafe {
        SetTimer(
            None,
            0,
            1,
            Some(if mode == Mode::Clean { clean } else { test }),
        );
    }
}

/// Close the window (the lock ended from the keyboard: Esc held).
pub fn request_close() {
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        if let Some(win) = CURRENT.with(|c| c.borrow_mut().take()) {
            close(&win);
        }
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

fn close(win: &Rc<Win>) {
    crate::lock::stop();
    unsafe {
        let _ = KillTimer(win.surface.hwnd, win.timer.get());
    }
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    win.window.close();
}

fn open(mode: Mode) {
    if let Some(win) = CURRENT.with(|c| c.borrow_mut().take()) {
        close(&win);
    }
    ui::refresh();
    let mut window = nwg::Window::default();
    let title = tr(if mode == Mode::Clean {
        T::CleanTitle
    } else {
        T::KeyTestTitle
    });
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .size((W, H))
        .title(title)
        .build(&mut window)
        .is_err()
    {
        return;
    }
    let surface = Surface::attach(&window, 0x5254_0018, Box::new(paint));
    let s = &surface;
    let p = pal();
    s.label(title, TextStyle::Title, (PAD, 20, W - 2 * PAD, 40), p.bg, 0);
    s.label(
        tr(if mode == Mode::Clean {
            T::CleanIntro
        } else {
            T::KeyTestIntro
        }),
        TextStyle::Dim,
        (PAD, 64, W - 2 * PAD, 24),
        p.bg,
        0,
    );
    // Choosing (cleaning).
    s.label(
        tr(T::CleanFor),
        TextStyle::Body,
        (PAD, 120, 120, 24),
        p.bg,
        SETUP,
    );
    let seg_w = 112;
    let durations = [T::Clean30s, T::Clean1m, T::Clean2m];
    let durations: [u16; 3] = std::array::from_fn(|i| {
        s.segment(
            tr(durations[i]),
            i == 0,
            (PAD + 4 + i as i32 * seg_w, 146, seg_w, 36),
            p.inset,
            SETUP,
        )
    });
    s.set_checked(durations[1], true);
    let lock_mouse = s.toggle(
        tr(T::RowLockMouse),
        tr(T::SubLockMouse),
        (PAD + 3 * seg_w + 28, 128, 340, 60),
        p.bg,
        SETUP,
    );
    let start = s.button(
        tr(T::BtnStartLock),
        true,
        (W - PAD - 190, 140, 190, 40),
        p.bg,
        SETUP,
    );
    // Running.
    let unlock = s.button(
        tr(T::BtnUnlock),
        true,
        (W - PAD - 160, FOOT_Y, 160, 40),
        p.bg,
        if mode == Mode::Clean { RUNNING } else { 99 },
    );
    let start_over = s.button(
        tr(T::BtnStartOver),
        false,
        (W - PAD - 160 - 12 - 140, FOOT_Y, 140, 40),
        p.bg,
        if mode == Mode::Test { RUNNING } else { 99 },
    );
    let close_id = s.button(
        tr(T::BtnClose),
        true,
        (W - PAD - 160, FOOT_Y, 160, 40),
        p.bg,
        if mode == Mode::Test { RUNNING } else { 99 },
    );
    // Done (cleaning).
    let test_keys = s.button(
        tr(T::BtnTestKeys),
        false,
        (W - PAD - 160 - 12 - 160, FOOT_Y, 160, 40),
        p.bg,
        DONE,
    );
    let close_done = s.button(
        tr(T::BtnClose),
        true,
        (W - PAD - 160, FOOT_Y, 160, 40),
        p.bg,
        DONE,
    );
    s.label(
        tr(if mode == Mode::Clean {
            T::CleanLimits
        } else {
            T::KeyTestEsc
        }),
        TextStyle::Small,
        (PAD, H - 56, W - 2 * PAD, 40),
        p.bg,
        0,
    );
    ui::size_and_center(surface.hwnd, W, H);
    let win = Rc::new(Win {
        window,
        surface,
        mode,
        durations,
        lock_mouse,
        start,
        unlock,
        start_over,
        close: close_id,
        close_done,
        test_keys,
        starting: Cell::new(None),
        timer: Cell::new(0),
        handler: RefCell::new(None),
    });
    let hwnd = win.surface.hwnd;
    SHOWN.with(|v| v.set((mode, None)));
    if mode == Mode::Test {
        win.surface.show_page(RUNNING);
        crate::lock::start(None, false, true, hwnd.0 as isize);
    } else {
        win.surface.show_page(SETUP);
    }
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
        win.timer.set(SetTimer(hwnd, 1, TICK_MS, None));
    }
    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if let Some(win) = weak.upgrade() {
            clicked(&win, id);
        }
    });
    let weak = Rc::downgrade(&win);
    let raw =
        nwg::bind_raw_event_handler(&win.window.handle, 0x5254_0019, move |_h, msg, _w, _l| {
            const WM_TIMER: u32 = 0x0113;
            const WM_CLOSE: u32 = 0x0010;
            let win = weak.upgrade()?;
            match msg {
                WM_TIMER => {
                    tick(&win);
                    Some(0)
                }
                WM_CLOSE => {
                    CURRENT.with(|c| c.borrow_mut().take());
                    close(&win);
                    Some(0)
                }
                _ => None,
            }
        })
        .ok();
    *win.handler.borrow_mut() = raw;
    crate::hook::trace_note(if mode == Mode::Clean {
        "cleaning window: open"
    } else {
        "key tester: open"
    });
    CURRENT.with(|c| *c.borrow_mut() = Some(win));
}

fn clicked(win: &Rc<Win>, id: u16) {
    let s = &win.surface;
    if id == win.start {
        win.starting.set(Some(Instant::now() + LEAD_IN));
        SHOWN.with(|v| v.set((win.mode, Some(Instant::now() + LEAD_IN))));
        s.show_page(RUNNING);
        // The unlock button stays hidden until the lock begins.
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                s.hwnd_of(win.unlock),
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
    } else if id == win.unlock {
        crate::lock::stop();
        s.show_page(DONE);
    } else if id == win.start_over {
        crate::lock::start(None, false, true, s.hwnd.0 as isize);
        repaint(win);
    } else if id == win.close || id == win.close_done {
        CURRENT.with(|c| c.borrow_mut().take());
        close(win);
    } else if id == win.test_keys {
        CURRENT.with(|c| c.borrow_mut().take());
        close(win);
        request_open(Mode::Test);
    }
}

fn tick(win: &Rc<Win>) {
    if let Some(at) = win.starting.get() {
        if Instant::now() >= at {
            win.starting.set(None);
            SHOWN.with(|v| v.set((win.mode, None)));
            let chosen = win
                .durations
                .iter()
                .position(|&d| win.surface.checked(d))
                .unwrap_or(1);
            let mouse = win.surface.checked(win.lock_mouse);
            crate::lock::start(
                Some(Instant::now() + DURATIONS[chosen]),
                mouse,
                false,
                win.surface.hwnd.0 as isize,
            );
            if !mouse {
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        win.surface.hwnd_of(win.unlock),
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
                    );
                }
            }
        }
        repaint(win);
        return;
    }
    if win.mode == Mode::Clean && win.surface.page() == RUNNING && !crate::lock::check_deadline() {
        win.surface.show_page(DONE);
    }
    if win.mode == Mode::Clean && win.surface.page() == RUNNING {
        repaint(win);
    }
}

fn repaint(win: &Win) {
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(win.surface.hwnd, None, true);
    }
}

/// `1:05`, `0:42`.
fn clock(d: Duration) -> String {
    let s = d.as_secs_f32().ceil() as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

fn paint(g: &Gfx, hdc: HDC, rc: RECT, page: u8) {
    use windows::Win32::Graphics::Gdi::{DT_CENTER, DT_LEFT, DT_SINGLELINE, DT_VCENTER};
    let p = pal();
    let (mode, starting) = SHOWN.with(|v| v.get());
    let big = ui::make_font(26, 700);
    let body = ui::make_font(14, 400);
    let cap = ui::make_font(12, 600);
    let small = ui::make_font(10, 400);
    let line = |s: &str, y: i32, font, color| {
        ui::text(
            hdc,
            s,
            ui::rect(PAD, y, W - 2 * PAD, 40),
            font,
            color,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    };
    let pressed = crate::lock::pressed();
    let down = crate::lock::down();
    let all = KEYS.len().to_string();
    if page == SETUP {
        ui::track(g, ui::rect(PAD, 142, 3 * 112 + 8, 44));
    }
    if page == RUNNING {
        match (mode, starting) {
            (Mode::Clean, Some(at)) => line(
                &trf(
                    T::CleanStarting,
                    &[("n", &clock(at.saturating_duration_since(Instant::now())))],
                ),
                128,
                big,
                p.text,
            ),
            (Mode::Clean, None) => {
                let left = crate::lock::left().map(clock).unwrap_or_default();
                line(
                    &format!(
                        "{}  ·  {}",
                        tr(T::CleanLocked),
                        trf(T::CleanLeft, &[("t", &left)])
                    ),
                    128,
                    big,
                    p.text,
                );
                line(
                    &trf(
                        T::CleanWiped,
                        &[("n", &pressed.count().to_string()), ("all", &all)],
                    ),
                    FOOT_Y,
                    body,
                    p.text_dim,
                );
            }
            (Mode::Test, _) => {
                let last = crate::lock::last().map_or_else(
                    || tr(T::KeyTestNone).to_string(),
                    |i| {
                        let k = KEYS[i];
                        let name = if k.label.is_empty() { "Space" } else { k.label };
                        trf(
                            T::KeyTestLast,
                            &[
                                ("k", name),
                                (
                                    "code",
                                    &format!(
                                        "{}{:02X}",
                                        if k.extended { "E0 " } else { "" },
                                        k.scan
                                    ),
                                ),
                            ],
                        )
                    },
                );
                line(&last, 128, big, p.text);
                line(
                    &trf(
                        T::KeyTestCount,
                        &[("n", &pressed.count().to_string()), ("all", &all)],
                    ),
                    FOOT_Y,
                    body,
                    p.text_dim,
                );
            }
        }
    }
    if page == DONE {
        line(tr(T::CleanDone), 128, big, p.text);
        line(
            &trf(
                T::CleanWiped,
                &[("n", &pressed.count().to_string()), ("all", &all)],
            ),
            FOOT_Y,
            body,
            p.text_dim,
        );
    }
    // The keyboard.
    let x0 = (W - (keyboard::WIDTH * UNIT as f32) as i32) / 2;
    for (i, k) in KEYS.iter().enumerate() {
        let r = ui::rect(
            x0 + (k.x * UNIT as f32) as i32 + 2,
            KB_Y + (k.y * UNIT as f32) as i32 + 2,
            (k.w * UNIT as f32) as i32 - 4,
            (k.h * UNIT as f32) as i32 - 4,
        );
        let (fill, ink) = if down.is_pressed(i) {
            (p.accent_pressed, p.on_accent)
        } else if pressed.is_pressed(i) {
            (p.accent, p.on_accent)
        } else {
            (p.keycap, p.text)
        };
        g.fill_round(
            r,
            ui::px(5) as f32,
            if fill == p.keycap {
                p.keycap_border
            } else {
                fill
            },
        );
        g.fill_round(ui::inset(r, ui::px(1)), ui::px(4) as f32, fill);
        let label = if k.label.is_empty() { "Space" } else { k.label };
        let font = if label.chars().count() > 3 {
            small
        } else {
            cap
        };
        ui::text(
            hdc,
            label,
            r,
            font,
            ink,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        // The Thai character it types, small in the corner.
        if let Some(c) = k.us {
            let (typed, th) = crate::keymap::thai_of(c);
            if typed != c.to_string() {
                let corner = RECT {
                    left: r.right - ui::px(16),
                    top: r.bottom - ui::px(16),
                    right: r.right - ui::px(2),
                    bottom: r.bottom - ui::px(1),
                };
                let dim = if ink == p.text { p.text_dim } else { ink };
                ui::text(hdc, &th, corner, small, dim, DT_CENTER | DT_SINGLELINE);
            }
        }
    }
    let _ = rc;
    unsafe {
        for f in [big, body, cap, small] {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
}
