//! Settings window — Windows only.
//!
//! A single, organised dialog (in the spirit of RightLang's): grouped sections
//! with titled boxes, a proper Segoe UI font on every control (without which
//! Win32 controls fall back to the dated bitmap system font), and the standard
//! Apply / OK / Cancel buttons.
//!
//! The built-in app blacklist (terminals, password managers, wallets) is shown
//! for reference but is **not editable** — only the user's *additional* list is,
//! so a settings bug can never weaken the safety baseline, only extend it.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;
use nwg::{CheckBoxState as Cbs, RadioButtonState as Rbs};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, SendMessageW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_SETFONT, WS_CHILD, WS_VISIBLE,
};

use crate::{config, hook, learn, safety, startup, toast};

/// `BS_GROUPBOX` — a BUTTON that draws a titled group frame.
const BS_GROUPBOX: u32 = 0x0000_0007;

struct SettingsWindow {
    window: nwg::Window,
    _font: nwg::Font,
    cb_enabled: nwg::CheckBox,
    cb_startup: nwg::CheckBox,
    rb_auto: nwg::RadioButton,
    rb_manual: nwg::RadioButton,
    rb_suggest: nwg::RadioButton,
    cb_learn: nwg::CheckBox,
    editor: nwg::TextBox,
    apply: nwg::Button,
    ok: nwg::Button,
    cancel: nwg::Button,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open the settings window.
pub fn open() {
    let mut font = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .build(&mut font);

    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((492, 670))
        .position((340, 180))
        .title("RightType — Settings")
        .build(&mut window)
        .is_err()
    {
        return;
    }

    // --- General ---
    let mut cb_enabled = nwg::CheckBox::default();
    let _ = nwg::CheckBox::builder()
        .text("Enable RightType")
        .font(Some(&font))
        .position((26, 30))
        .size((260, 22))
        .parent(&window)
        .build(&mut cb_enabled);

    let mut cb_startup = nwg::CheckBox::default();
    let _ = nwg::CheckBox::builder()
        .text("Start with Windows")
        .font(Some(&font))
        .position((26, 54))
        .size((260, 22))
        .parent(&window)
        .build(&mut cb_startup);

    // --- Correction mode ---
    let mut rb_auto = nwg::RadioButton::default();
    let _ = nwg::RadioButton::builder()
        .text("Auto — fix as you type")
        .font(Some(&font))
        .position((26, 116))
        .size((260, 22))
        .parent(&window)
        .build(&mut rb_auto);

    let mut rb_manual = nwg::RadioButton::default();
    let _ = nwg::RadioButton::builder()
        .text("Manual — only with a hotkey")
        .font(Some(&font))
        .position((26, 140))
        .size((260, 22))
        .parent(&window)
        .build(&mut rb_manual);

    let mut rb_suggest = nwg::RadioButton::default();
    let _ = nwg::RadioButton::builder()
        .text("Suggest — show a hint, accept manually")
        .font(Some(&font))
        .position((26, 164))
        .size((330, 22))
        .parent(&window)
        .build(&mut rb_suggest);

    let mut cb_learn = nwg::CheckBox::default();
    let _ = nwg::CheckBox::builder()
        .text("Learn new words automatically")
        .font(Some(&font))
        .position((26, 192))
        .size((300, 22))
        .parent(&window)
        .build(&mut cb_learn);

    // --- Hotkeys (reference; not remappable) ---
    let hotkeys = [
        ("Fix last word", "Shift + Backspace"),
        ("Convert selection", "Shift + CapsLock"),
        ("Cycle Manual / Auto / Suggest", "Ctrl + CapsLock"),
        ("Undo last correction", "Ctrl + Shift + CapsLock"),
        ("Accept suggestion", "Alt + CapsLock"),
        ("Turn on / off (panic)", "Ctrl + Alt + CapsLock"),
    ];
    for (i, (action, keys)) in hotkeys.iter().enumerate() {
        let y = 250 + (i as i32) * 22;
        let mut l = nwg::Label::default();
        let _ = nwg::Label::builder()
            .text(action)
            .font(Some(&font))
            .position((26, y))
            .size((190, 20))
            .parent(&window)
            .build(&mut l);
        let mut r = nwg::Label::default();
        let _ = nwg::Label::builder()
            .text(keys)
            .font(Some(&font))
            .position((220, y))
            .size((240, 20))
            .parent(&window)
            .build(&mut r);
        std::mem::forget(l);
        std::mem::forget(r);
    }

    // --- Blocked apps ---
    let mut info = nwg::Label::default();
    let _ = nwg::Label::builder()
        .text(
            "Always blocked: terminals, password managers, wallets.\n\
             Add more below — one .exe name per line.",
        )
        .font(Some(&font))
        .position((26, 424))
        .size((440, 36))
        .parent(&window)
        .build(&mut info);
    std::mem::forget(info);

    let mut editor = nwg::TextBox::default();
    let _ = nwg::TextBox::builder()
        .text(&safety::custom_list().join("\r\n"))
        .font(Some(&font))
        .flags(
            nwg::TextBoxFlags::VISIBLE
                | nwg::TextBoxFlags::VSCROLL
                | nwg::TextBoxFlags::AUTOVSCROLL,
        )
        .position((26, 462))
        .size((440, 96))
        .parent(&window)
        .build(&mut editor);

    // --- Buttons ---
    let mut apply = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Apply")
        .font(Some(&font))
        .position((214, 576))
        .size((80, 28))
        .parent(&window)
        .build(&mut apply);

    let mut ok = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("OK")
        .font(Some(&font))
        .position((300, 576))
        .size((80, 28))
        .parent(&window)
        .build(&mut ok);

    let mut cancel = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Cancel")
        .font(Some(&font))
        .position((386, 576))
        .size((80, 28))
        .parent(&window)
        .build(&mut cancel);

    // Titled group frames, drawn behind the controls (raw Win32 — nwg has no
    // GroupBox). They must use the same font or they'd render in the old font.
    if let Some(h) = window.handle.hwnd() {
        unsafe {
            let parent = HWND(h as _);
            let hfont = font.handle as usize;
            group_box(parent, hfont, "General", 12, 8, 452, 78);
            group_box(parent, hfont, "Correction mode", 12, 96, 452, 128);
            group_box(parent, hfont, "Hotkeys", 12, 232, 452, 160);
            group_box(parent, hfont, "Blocked apps", 12, 400, 452, 168);
        }
    }

    // Reflect current state.
    cb_enabled.set_check_state(bool_cb(hook::is_enabled()));
    cb_startup.set_check_state(bool_cb(startup::is_enabled()));
    cb_learn.set_check_state(bool_cb(learn::is_enabled()));
    rb_auto.set_check_state(bool_rb(hook::mode() == hook::Mode::Auto));
    rb_manual.set_check_state(bool_rb(hook::mode() == hook::Mode::Manual));
    rb_suggest.set_check_state(bool_rb(hook::mode() == hook::Mode::Suggest));

    window.set_visible(true);

    let ui = Rc::new(SettingsWindow {
        window,
        _font: font,
        cb_enabled,
        cb_startup,
        rb_auto,
        rb_manual,
        rb_suggest,
        cb_learn,
        editor,
        apply,
        ok,
        cancel,
        handler: RefCell::new(None),
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            E::OnButtonClick => {
                if handle == ui_h.rb_auto.handle {
                    ui_h.rb_manual.set_check_state(Rbs::Unchecked);
                    ui_h.rb_suggest.set_check_state(Rbs::Unchecked);
                } else if handle == ui_h.rb_manual.handle {
                    ui_h.rb_auto.set_check_state(Rbs::Unchecked);
                    ui_h.rb_suggest.set_check_state(Rbs::Unchecked);
                } else if handle == ui_h.rb_suggest.handle {
                    ui_h.rb_auto.set_check_state(Rbs::Unchecked);
                    ui_h.rb_manual.set_check_state(Rbs::Unchecked);
                } else if handle == ui_h.apply.handle {
                    apply_settings(&ui_h);
                    toast::show("Saved");
                } else if handle == ui_h.ok.handle {
                    apply_settings(&ui_h);
                    cleanup(&ui_h);
                } else if handle == ui_h.cancel.handle {
                    cleanup(&ui_h);
                }
            }
            E::OnWindowClose if handle == ui_h.window.handle => cleanup(&ui_h),
            _ => {}
        }
    });
    *ui.handler.borrow_mut() = Some(handler);
}

/// Push the dialog's state into the running app + persist it.
fn apply_settings(ui: &Rc<SettingsWindow>) {
    hook::set_enabled(ui.cb_enabled.check_state() == Cbs::Checked);
    let mode = if ui.rb_auto.check_state() == Rbs::Checked {
        hook::Mode::Auto
    } else if ui.rb_suggest.check_state() == Rbs::Checked {
        hook::Mode::Suggest
    } else {
        hook::Mode::Manual
    };
    hook::set_mode(mode);
    learn::set_enabled(ui.cb_learn.check_state() == Cbs::Checked);
    startup::set_enabled(ui.cb_startup.check_state() == Cbs::Checked);

    let entries: Vec<String> = ui
        .editor
        .text()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_lowercase)
        .collect();
    safety::set_custom_list(entries);
    config::persist();
}

fn cleanup(ui: &Rc<SettingsWindow>) {
    if let Some(h) = ui.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    ui.window.close();
}

fn bool_cb(v: bool) -> Cbs {
    if v {
        Cbs::Checked
    } else {
        Cbs::Unchecked
    }
}

fn bool_rb(v: bool) -> Rbs {
    if v {
        Rbs::Checked
    } else {
        Rbs::Unchecked
    }
}

/// Create a titled BS_GROUPBOX frame as child of `parent`, using `hfont`.
unsafe fn group_box(parent: HWND, hfont: usize, title: &str, x: i32, y: i32, w: i32, h: i32) {
    let title_w: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    let hmod = GetModuleHandleW(None).unwrap_or_default();
    if let Ok(hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE(0),
        w!("BUTTON"),
        PCWSTR(title_w.as_ptr()),
        WS_CHILD | WS_VISIBLE | WINDOW_STYLE(BS_GROUPBOX),
        x,
        y,
        w,
        h,
        parent,
        None,
        HINSTANCE(hmod.0),
        None,
    ) {
        SendMessageW(hwnd, WM_SETFONT, WPARAM(hfont), LPARAM(1));
    }
}
