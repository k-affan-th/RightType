//! The **Fix text** window — Windows only.
//!
//! Paste a paragraph typed in the wrong layout (or take it straight from the
//! clipboard) and get it back fixed, word by word, by the same rules the hook
//! uses while you type ([`righttype::repair`]). Nothing is saved: the text
//! lives in the window's two text boxes and in short-lived strings that are
//! wiped after use, and it is gone when the window closes.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use righttype::{dict, repair};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};
use zeroize::Zeroize;

use crate::ui::{self, field, pal, rect, Gfx, Surface, TextStyle};

static OPEN: AtomicIsize = AtomicIsize::new(0);

const W: i32 = 720;
const H: i32 = 574;
const X: i32 = 28;
const CW: i32 = W - 2 * X;
const IN_Y: i32 = 134;
const IN_H: i32 = 146;
const OUT_Y: i32 = 372;
const OUT_H: i32 = 126;
/// How many changed words the status line lists by name.
const LISTED: usize = 3;

struct Ids {
    input: u16,
    output: u16,
    fix: u16,
    paste_fix: u16,
    copy: u16,
    close: u16,
    status: u16,
}

struct FixerWindow {
    window: nwg::Window,
    surface: Rc<Surface>,
    ids: Ids,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open the Fix text window, or bring the open one forward.
pub fn open() {
    open_with(None);
}

/// Open with `text` already pasted and fixed (debug harness: screenshots).
#[cfg(debug_assertions)]
pub fn open_demo(text: &str) {
    open_with(Some(text));
}

fn open_with(initial: Option<&str>) {
    let existing = OPEN.load(Ordering::Acquire);
    if existing != 0 {
        unsafe {
            let hwnd = HWND(existing as *mut _);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    ui::refresh();

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, H))
        .title(tr(T::FixTitle))
        .build(&mut window);
    let surface = Surface::attach(&window, 0x5254_0013, Box::new(paint));
    let s = &surface;
    let p = pal();

    s.label(tr(T::FixHead), TextStyle::Title, (X, 18, CW, 36), p.bg, 0);
    s.label(tr(T::FixIntro), TextStyle::Dim, (X, 60, CW, 44), p.bg, 0);
    s.label(
        tr(T::FixInput),
        TextStyle::BodyStrong,
        (X, IN_Y - 24, CW, 20),
        p.bg,
        0,
    );
    let input = s.edit("", (X + 10, IN_Y + 8, CW - 20, IN_H - 16), 0);
    let paste_fix = s.button(
        tr(T::BtnPasteFix),
        false,
        (X, IN_Y + IN_H + 12, 180, 34),
        p.bg,
        0,
    );
    let fix = s.button(
        tr(T::BtnFix),
        true,
        (X + CW - 140, IN_Y + IN_H + 12, 140, 34),
        p.bg,
        0,
    );
    s.label(
        tr(T::FixOutput),
        TextStyle::BodyStrong,
        (X, OUT_Y - 24, CW, 20),
        p.bg,
        0,
    );
    let output = s.edit("", (X + 10, OUT_Y + 8, CW - 20, OUT_H - 16), 0);
    let status = s.label(
        "",
        TextStyle::Small,
        (X, OUT_Y + OUT_H + 12, CW - 270, 40),
        p.bg,
        0,
    );
    let copy = s.button(
        tr(T::BtnCopy),
        false,
        (X + CW - 256, OUT_Y + OUT_H + 12, 120, 34),
        p.bg,
        0,
    );
    let close = s.button(
        tr(T::BtnClose),
        false,
        (X + CW - 128, OUT_Y + OUT_H + 12, 128, 34),
        p.bg,
        0,
    );

    ui::size_and_center(surface.hwnd, W, H);
    let hwnd_isize = surface.hwnd.0 as isize;
    let win = Rc::new(FixerWindow {
        window,
        surface,
        ids: Ids {
            input,
            output,
            fix,
            paste_fix,
            copy,
            close,
            status,
        },
        handler: RefCell::new(None),
    });
    if let Some(text) = initial {
        win.surface.set_text(win.ids.input, text);
        fix_into_output(&win, text);
    }
    win.window.set_visible(true);
    OPEN.store(hwnd_isize, Ordering::Release);

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if let Some(win) = weak.upgrade() {
            clicked(&win, id);
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

fn clicked(win: &Rc<FixerWindow>, id: u16) {
    let ids = &win.ids;
    let s = &win.surface;
    if id == ids.fix {
        let mut text = s.text_of(ids.input);
        fix_into_output(win, &text);
        text.zeroize();
    } else if id == ids.paste_fix {
        match unsafe { crate::clipboard::get_text() } {
            Some(mut text) => {
                s.set_text(ids.input, &text);
                fix_into_output(win, &text);
                text.zeroize();
            }
            None => s.set_text(ids.status, tr(T::ErrClipboardEmpty)),
        }
    } else if id == ids.copy {
        let mut text = s.text_of(ids.output);
        let ok = !text.is_empty() && unsafe { crate::clipboard::set_text(&text) };
        text.zeroize();
        s.set_text(
            ids.status,
            tr(if ok {
                T::FixCopied
            } else {
                T::ErrClipboardBusy
            }),
        );
    } else if id == ids.close {
        finish(win);
    }
}

/// Fix `text` and show the result, with a line saying what changed.
fn fix_into_output(win: &FixerWindow, text: &str) {
    let s = &win.surface;
    let mut repaired = repair::repair(text, dict::english(), dict::thai());
    // Edit controls want CRLF; keep whatever line breaks came in, but turn a
    // bare LF into CRLF so lines do not run together.
    let mut shown = repaired.text.replace("\r\n", "\n").replace('\n', "\r\n");
    s.set_text(win.ids.output, &shown);
    shown.zeroize();
    let n = repaired.changes.len();
    let mut status = if n == 0 {
        tr(T::FixNothing).to_string()
    } else {
        let listed: Vec<String> = repaired
            .changes
            .iter()
            .take(LISTED)
            .map(|c| format!("{} → {}", c.original, c.fixed))
            .collect();
        let more = if n > LISTED { ", …" } else { "" };
        format!(
            "{} {}{more}",
            trf(T::FixChanged, &[("n", &n.to_string())]),
            listed.join(", ")
        )
    };
    s.set_text(win.ids.status, &status);
    status.zeroize();
    repaired.text.zeroize();
    for change in repaired.changes.iter_mut() {
        change.original.zeroize();
        change.fixed.zeroize();
    }
    if n > 0 {
        crate::stats::record_manual_n(n as u64);
    }
}

fn finish(win: &Rc<FixerWindow>) {
    let my = win.surface.hwnd.0 as isize;
    let _ = OPEN.compare_exchange(my, 0, Ordering::AcqRel, Ordering::Acquire);
    // Do not leave the text in the controls' memory longer than needed.
    win.surface.set_text(win.ids.input, "");
    win.surface.set_text(win.ids.output, "");
    win.surface.set_text(win.ids.status, "");
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    win.window.close();
}

fn paint(g: &Gfx, _hdc: HDC, _rc: RECT, _page: u8) {
    field(g, rect(X, IN_Y, CW, IN_H));
    field(g, rect(X, OUT_Y, CW, OUT_H));
}
