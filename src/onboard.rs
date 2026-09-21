//! First-run onboarding + anytime hotkeys help — Windows only.
//!
//! One small, always-on-top, focus-stealing-by-design window (it is the one
//! place we *want* attention): what RightType does, the fixed v1 hotkeys, the
//! privacy stance, and a single "Get started" button. Shown automatically on
//! first launch (config `onboarded`) and reopenable from the tray menu.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;

use crate::{config, theme};

pub const HOTKEYS: &[(&str, &str)] = &[
    ("Flip or revert last word", "Shift + Backspace"),
    ("Convert selection", "Shift + CapsLock"),
    ("Cycle Manual / Auto / Suggest", "Ctrl + CapsLock"),
    ("Undo last correction", "Ctrl + Shift + CapsLock"),
    ("Accept suggestion", "Alt + CapsLock"),
    ("Panic on/off", "Ctrl + Alt + CapsLock"),
];

struct Welcome {
    window: nwg::Window,
    _font: nwg::Font,
    _labels: Vec<nwg::Label>,
    start: nwg::Button,
    handler: RefCell<Option<nwg::EventHandler>>,
    _theme: Option<nwg::RawEventHandler>,
}

/// Show the welcome/help window. `first_run` toggles the headline copy; when
/// true, dismissing marks config onboarding complete.
pub fn show(first_run: bool) {
    let mut font = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(15)
        .build(&mut font);
    let mut big = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(20)
        .build(&mut big);

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((460, 470))
        .position((200, 140))
        .title(if first_run {
            "Welcome to RightType"
        } else {
            "RightType — Hotkeys"
        })
        .topmost(true)
        .build(&mut window);

    let mut labels: Vec<nwg::Label> = Vec::new();
    macro_rules! label {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr, $font:expr) => {{
            let mut l = nwg::Label::default();
            let _ = nwg::Label::builder()
                .text($text)
                .font(Some(&$font))
                .position(($x, $y))
                .size(($w, $h))
                .parent(&window)
                .build(&mut l);
            labels.push(l);
        }};
    }

    if first_run {
        label!("พิมพ์ผิด layout? แก้ให้ทันที ไม่ต้องพิมพ์ใหม่", 24, 18, 410, 34, big);
        label!(
            "Thai Kedmanee ↔ US QWERTY · Auto / Manual / Suggest modes.\n\
             Password fields, wallets and terminals are always ignored.",
            24,
            56,
            410,
            44,
            font
        );
    } else {
        label!("RightType — fixed hotkeys (v1)", 24, 18, 410, 30, big);
    }

    label!("Hotkeys", 24, 108, 200, 24, big);
    for (i, (action, keys)) in HOTKEYS.iter().enumerate() {
        let y = 138 + (i as i32) * 26;
        label!(*action, 28, y, 220, 22, font);
        label!(*keys, 252, y, 180, 22, font);
    }

    let tip_y = 138 + (HOTKEYS.len() as i32) * 26 + 8;
    label!(
        "Nothing you type is ever written to disk.",
        24,
        tip_y,
        410,
        22,
        font
    );

    let mut start = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text(if first_run { "Get started" } else { "Close" })
        .font(Some(&big))
        .position((330, 415))
        .size((110, 32))
        .parent(&window)
        .build(&mut start);

    let themed = theme::subclass_colors(
        window
            .handle
            .hwnd()
            .map(|h| windows::Win32::Foundation::HWND(h as _))
            .unwrap_or_default(),
        0x5254_0011,
    );
    theme::apply_frame(
        window
            .handle
            .hwnd()
            .map(|h| windows::Win32::Foundation::HWND(h as _))
            .unwrap_or_default(),
    );
    window.set_visible(true);

    let ui = Rc::new(Welcome {
        window,
        _font: font,
        _labels: labels,
        start,
        handler: RefCell::new(None),
        _theme: themed,
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            E::OnButtonClick if handle == ui_h.start.handle => {
                if first_run {
                    config::mark_onboarded();
                }
                finish(&ui_h);
            }
            E::OnWindowClose if handle == ui_h.window.handle => finish(&ui_h),
            _ => {}
        }
    });
    *ui.handler.borrow_mut() = Some(handler);

    fn finish(ui: &Rc<Welcome>) {
        if let Some(h) = ui.handler.borrow_mut().take() {
            nwg::unbind_event_handler(&h);
        }
        ui.window.close();
    }
}
