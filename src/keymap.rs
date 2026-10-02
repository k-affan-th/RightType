//! The keyboard map — Windows only.
//!
//! The Thai keyboard in use (Kedmanee, Pattachote or Manoonchai), key by
//! key, in a small window that never takes the focus: clicking a key types
//! its character where the typist is. A Shift button shows (and types) the
//! shifted layer. Opened by its hotkey (Ctrl+Alt+K) or from the palette;
//! the hotkey again, or ×, closes it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};

use native_windows_gui as nwg;
use righttype::i18n::{tr, T};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, KillTimer, SetTimer, SetWindowPos, HWND_TOPMOST, SWP_NOMOVE, SWP_NOSIZE,
};

use crate::ui::{self, pal, Gfx, Surface, TextStyle};

/// The keys, row by row, by the US character they type.
const ROWS: [&str; 4] = [
    "`1234567890-=",
    "qwertyuiop[]\\",
    "asdfghjkl;'",
    "zxcvbnm,./",
];
/// The same keys with Shift.
const SHIFTED: [&str; 4] = [
    "~!@#$%^&*()_+",
    "QWERTYUIOP{}|",
    "ASDFGHJKL:\"",
    "ZXCVBNM<>?",
];
/// How far each row starts in from the left, in keys (as on a keyboard).
const INDENT: [i32; 4] = [0, 1, 1, 2];

const KEY: i32 = 40;
const GAP: i32 = 4;
const PAD: i32 = 12;
const HEAD: i32 = 40;
const W: i32 = PAD * 2 + 15 * (KEY + GAP);
const H: i32 = PAD * 2 + HEAD + 4 * (KEY + GAP);

static OPEN: AtomicIsize = AtomicIsize::new(0);

struct Map {
    window: nwg::Window,
    surface: Rc<Surface>,
    /// (button, US key, US key with Shift)
    keys: Vec<(u16, char, char)>,
    shift: u16,
    close: u16,
    shifted: Cell<bool>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Map>>> = const { RefCell::new(None) };
}

/// What a key types on the Thai keyboard in use, and how it is shown (a
/// mark above or below a letter on a dotted circle).
fn thai_of(key: char) -> (String, String) {
    let typed = righttype::layout::en_to_th(&key.to_string());
    let shown = match typed.chars().next() {
        Some(c) if is_mark(c) => format!("◌{c}"),
        _ => typed.clone(),
    };
    (typed, shown)
}

fn is_mark(c: char) -> bool {
    c == '\u{0E31}'
        || ('\u{0E34}'..='\u{0E3A}').contains(&c)
        || ('\u{0E47}'..='\u{0E4E}').contains(&c)
}

/// Open the map, or close it if it is open (from the message loop: the
/// keyboard hook calls this and must not create windows itself).
pub fn request_toggle() {
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        toggle();
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

fn toggle() {
    if let Some(map) = CURRENT.with(|c| c.borrow_mut().take()) {
        close(&map);
        return;
    }
    open();
}

fn close(map: &Rc<Map>) {
    OPEN.store(0, Ordering::Release);
    map.surface.detach();
    if let Some(h) = map.handler.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    map.window.close();
}

fn open() {
    ui::refresh();
    let mut window = nwg::Window::default();
    // Topmost, a tool window, and never activated: clicks leave the focus
    // where the typist is.
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::POPUP)
        .ex_flags(0x0000_0008 | 0x0000_0080 | WS_EX_NOACTIVATE)
        .size((W, H))
        .title(tr(T::KeyMapTitle))
        .build(&mut window)
        .is_err()
    {
        return;
    }
    let surface = Surface::attach(&window, 0x5254_0016, Box::new(paint));
    let p = pal();
    surface.label(
        tr(T::KeyMapHead),
        TextStyle::Small,
        (PAD + 4, PAD + 8, W - 2 * PAD - 150, 22),
        p.surface,
        0,
    );
    let shift = surface.segment("Shift", false, (W - PAD - 140, PAD, 90, 30), p.inset, 0);
    let close_id = surface.button("×", false, (W - PAD - 44, PAD, 44, 30), p.surface, 0);
    let mut keys = Vec::new();
    for (r, (row, up)) in ROWS.iter().zip(SHIFTED).enumerate() {
        let y = PAD + HEAD + r as i32 * (KEY + GAP);
        for (i, (k, s)) in row.chars().zip(up.chars()).enumerate() {
            let x = PAD + (INDENT[r] * (KEY + GAP)) / 2 + i as i32 * (KEY + GAP);
            let (_, shown) = thai_of(k);
            let id = surface.button(&shown, false, (x, y, KEY, KEY), p.surface, 0);
            keys.push((id, k, s));
        }
    }
    ui::size_and_center(surface.hwnd, W, H);
    unsafe {
        // Near the bottom of the screen, centred.
        let wa = ui::work_area(windows::Win32::Graphics::Gdi::MonitorFromWindow(
            GetForegroundWindow(),
            windows::Win32::Graphics::Gdi::MONITOR_DEFAULTTONEAREST,
        ));
        let (w, h) = (ui::px(W), ui::px(H));
        let x = wa.left + ((wa.right - wa.left) - w) / 2;
        let y = wa.bottom - h - ui::px(24);
        let _ = SetWindowPos(surface.hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE);
    }
    let map = Rc::new(Map {
        window,
        surface,
        keys,
        shift,
        close: close_id,
        shifted: Cell::new(false),
        handler: RefCell::new(None),
    });
    // Shown without being activated.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
            map.surface.hwnd,
            windows::Win32::UI::WindowsAndMessaging::SW_SHOWNOACTIVATE,
        );
        let _ = SetWindowPos(
            map.surface.hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        );
    }
    OPEN.store(map.surface.hwnd.0 as isize, Ordering::Release);
    let weak = Rc::downgrade(&map);
    map.surface.on_click(move |id| {
        let Some(map) = weak.upgrade() else {
            return;
        };
        if id == map.close {
            CURRENT.with(|c| c.borrow_mut().take());
            close(&map);
            return;
        }
        if id == map.shift {
            let on = !map.shifted.get();
            map.shifted.set(on);
            map.surface.set_checked(map.shift, on);
            for (button, k, s) in &map.keys {
                let (_, shown) = thai_of(if on { *s } else { *k });
                map.surface.set_text(*button, &shown);
            }
            return;
        }
        if let Some((_, k, s)) = map.keys.iter().find(|(b, _, _)| *b == id) {
            let (typed, _) = thai_of(if map.shifted.get() { *s } else { *k });
            let target = unsafe { GetForegroundWindow() }.0 as isize;
            crate::manual::request_type(target, typed, false);
        }
    });
    // Clicks must not activate it (WM_MOUSEACTIVATE → MA_NOACTIVATE).
    let raw = nwg::bind_raw_event_handler(&map.window.handle, 0x5254_0017, |_h, msg, _w, _l| {
        const WM_MOUSEACTIVATE: u32 = 0x0021;
        const MA_NOACTIVATE: isize = 3;
        (msg == WM_MOUSEACTIVATE).then_some(MA_NOACTIVATE)
    })
    .ok();
    *map.handler.borrow_mut() = raw;
    CURRENT.with(|c| *c.borrow_mut() = Some(map));
}

fn paint(g: &Gfx, _hdc: HDC, rc: RECT, _page: u8) {
    let p = pal();
    g.fill_round(rc, ui::px(8) as f32, p.border);
    g.fill_round(ui::inset(rc, ui::px(1)), ui::px(7) as f32, p.surface);
}
