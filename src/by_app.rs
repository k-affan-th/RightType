//! "How each app is doing" (2.3 P4) — Windows only. A table of this
//! session's counts per program ([`righttype::app_quality`]): words fixed,
//! read back as sent or shown differently, and undone, with a note on what
//! to do. Opened from the statistics window.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;
use righttype::app_quality::{verdict, Verdict};
use righttype::i18n::{tr, T};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::HDC;

use crate::ui::{self, pal, Gfx, Surface, TextStyle};

const W: i32 = 840;
const H: i32 = 480;
const PAD: i32 = 24;

struct ByApp {
    window: nwg::Window,
    surface: Rc<Surface>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<ByApp>>> = const { RefCell::new(None) };
}

fn note(v: Verdict) -> &'static str {
    tr(match v {
        Verdict::Good => T::ByAppGood,
        Verdict::OftenUndone => T::ByAppUndone,
        Verdict::ShownWrong => T::ByAppShownWrong,
        Verdict::Unknown => T::ByAppUnknown,
    })
}

fn close(b: &Rc<ByApp>) {
    b.surface.detach();
    if let Some(h) = b.handler.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    b.window.close();
}

pub fn open() {
    if let Some(b) = CURRENT.with(|c| c.borrow_mut().take()) {
        close(&b);
    }
    ui::refresh();
    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .size((W, H))
        .title(tr(T::ByAppTitle))
        .topmost(true)
        .build(&mut window)
        .is_err()
    {
        return;
    }
    let surface = Surface::attach(&window, 0x5254_0020, Box::new(paint));
    let s = &surface;
    let p = pal();
    s.label(
        tr(T::ByAppTitle),
        TextStyle::Title,
        (PAD, 16, W - 2 * PAD, 36),
        p.bg,
        0,
    );
    let num = 84;
    let table = s.table(
        &[
            (tr(T::ByAppApp), 150),
            (tr(T::ByAppFixed), num),
            (tr(T::ByAppRight), num),
            (tr(T::ByAppWrong), num),
            (tr(T::ByAppUndoneCol), num),
            (tr(T::ByAppNote), W - 2 * PAD - 150 - 4 * num - 28),
        ],
        (PAD, 62, W - 2 * PAD, H - 62 - 70),
        0,
    );
    s.label(
        tr(T::ByAppFoot),
        TextStyle::Small,
        (PAD, H - 58, W - 2 * PAD, 40),
        p.bg,
        0,
    );
    let rows: Vec<Vec<String>> = crate::stats::app_rows()
        .into_iter()
        .map(|(exe, q)| {
            vec![
                exe,
                q.fixed.to_string(),
                q.shown_right.to_string(),
                q.shown_wrong.to_string(),
                q.undone.to_string(),
                note(verdict(q)).to_string(),
            ]
        })
        .collect();
    let rows = if rows.is_empty() {
        vec![vec![
            tr(T::ByAppEmpty).to_string(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        ]]
    } else {
        rows
    };
    s.set_rows(table, &rows, None);
    ui::size_and_center(surface.hwnd, W, H);
    let b = Rc::new(ByApp {
        window,
        surface,
        handler: RefCell::new(None),
    });
    let weak = Rc::downgrade(&b);
    let raw = nwg::bind_raw_event_handler(&b.window.handle, 0x5254_0021, move |_h, msg, w, _| {
        const WM_CLOSE: u32 = 0x0010;
        const WM_COMMAND: u32 = 0x0111;
        let b = weak.upgrade()?;
        if msg == WM_CLOSE || (msg == WM_COMMAND && w & 0xFFFF == 2) {
            CURRENT.with(|c| c.borrow_mut().take());
            close(&b);
            return Some(0);
        }
        None
    })
    .ok();
    *b.handler.borrow_mut() = raw;
    CURRENT.with(|c| *c.borrow_mut() = Some(b));
}

fn paint(_g: &Gfx, _hdc: HDC, _rc: RECT, _page: u8) {}
