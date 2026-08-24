//! Settings window — Windows only.
//!
//! Modern dark dialog: section headers instead of dated group boxes, generous
//! spacing, hotkeys listed from the same table the onboarding window uses.
//!
//! Refresh-on-reopen: opening while visible closes the old window and opens a
//! fresh one, announced with a toast. The built-in app blacklist stays shown
//! for reference but is **not editable** — only the user's *additional* list
//! is, so a settings bug can never weaken the safety baseline.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;
use nwg::{CheckBoxState as Cbs, RadioButtonState as Rbs};

use windows::Win32::Foundation::HWND;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

<<<<<<< Updated upstream
use crate::{config, hook, learn, safety, startup, toast};
=======
use crate::{config, hook, learn, onboard, safety, startup, theme, toast};
>>>>>>> Stashed changes

const WM_CLOSE: u32 = 0x0010;

/// Only one settings window at a time; reopening refreshes it.
static OPEN: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

struct SettingsWindow {
    window: nwg::Window,
    _font: nwg::Font,
    _head: nwg::Font,
    _labels: Vec<nwg::Label>,
    cb_enabled: nwg::CheckBox,
    cb_startup: nwg::CheckBox,
    rb_auto: nwg::RadioButton,
    rb_manual: nwg::RadioButton,
    rb_suggest: nwg::RadioButton,
    cb_learn: nwg::CheckBox,
    editor: nwg::TextBox,
<<<<<<< Updated upstream
    apply: nwg::Button,
    ok: nwg::Button,
    cancel: nwg::Button,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open the settings window.
pub fn open() {
=======
    clear_learned: nwg::Button,
    cancel: nwg::Button,
    apply: nwg::Button,
    ok: nwg::Button,
    _theme: Option<nwg::RawEventHandler>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open (or refresh) the settings window.
pub fn open() {
    use std::sync::atomic::Ordering;
    let existing = OPEN.load(Ordering::Acquire);
    if existing != 0 {
        unsafe {
            let _ = PostMessageW(HWND(existing as *mut _), WM_CLOSE, WPARAM(0), LPARAM(0));
        }
        toast::show("Settings refreshed");
    } else {
        toast::show("Settings");
    }

>>>>>>> Stashed changes
    let mut font = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(15)
        .build(&mut font);
    let mut head = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(16)
        .build(&mut head);

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((560, 690))
        .position((180, 120))
        .title("RightType — Settings")
        .build(&mut window);
    let hwnd = window
        .handle
        .hwnd()
        .map(|h| HWND(h as _))
        .unwrap_or(HWND(std::ptr::null_mut()));
    theme::apply_frame(hwnd);
    let themed = theme::subclass_colors(hwnd, 0x5254_0010);

    let mut labels: Vec<nwg::Label> = Vec::new();
    macro_rules! header {
        ($text:expr, $y:expr) => {{
            let mut l = nwg::Label::default();
            let _ = nwg::Label::builder()
                .text($text)
                .font(Some(&head))
                .position((24, $y))
                .size((500, 26))
                .parent(&window)
                .build(&mut l);
            labels.push(l);
        }};
    }
    macro_rules! hint {
        ($text:expr, $y:expr) => {{
            let mut l = nwg::Label::default();
            let _ = nwg::Label::builder()
                .text($text)
                .font(Some(&font))
                .position((28, $y))
                .size((496, $crate::settings::H_HINT))
                .parent(&window)
                .build(&mut l);
            labels.push(l);
        }};
    }
    macro_rules! hotrow {
        ($action:expr, $keys:expr, $y:expr) => {{
            let mut l = nwg::Label::default();
            let _ = nwg::Label::builder()
                .text($action)
                .font(Some(&font))
                .position((30, $y))
                .size((240, 20))
                .parent(&window)
                .build(&mut l);
            labels.push(l);
            let mut r = nwg::Label::default();
            let _ = nwg::Label::builder()
                .text($keys)
                .font(Some(&font))
                .position((280, $y))
                .size((240, 20))
                .parent(&window)
                .build(&mut r);
            labels.push(r);
        }};
    }

    // --- General -----------------------------------------------------------
    header!("General", 18);

    let mut cb_enabled = nwg::CheckBox::default();
    let _ = nwg::CheckBox::builder()
        .text("Enable RightType")
        .font(Some(&font))
        .position((30, 52))
        .size((300, 24))
        .parent(&window)
        .build(&mut cb_enabled);

    let mut cb_startup = nwg::CheckBox::default();
    let _ = nwg::CheckBox::builder()
        .text("Start with Windows")
        .font(Some(&font))
        .position((30, 82))
        .size((300, 24))
        .parent(&window)
        .build(&mut cb_startup);

    // --- Correction mode ---------------------------------------------------
    header!("Correction mode", 122);

    let mut rb_auto = nwg::RadioButton::default();
    let _ = nwg::RadioButton::builder()
        .text("Auto — fix as you type (instant EN→TH, boundary TH→EN)")
        .font(Some(&font))
        .position((30, 154))
        .size((500, 24))
        .parent(&window)
        .build(&mut rb_auto);

    let mut rb_manual = nwg::RadioButton::default();
    let _ = nwg::RadioButton::builder()
        .text("Manual — only when I press the hotkey")
        .font(Some(&font))
        .position((30, 184))
        .size((500, 24))
        .parent(&window)
        .build(&mut rb_manual);

    let mut rb_suggest = nwg::RadioButton::default();
    let _ = nwg::RadioButton::builder()
        .text("Suggest — show a hint, accept with Alt+CapsLock")
        .font(Some(&font))
        .position((30, 214))
        .size((500, 24))
        .parent(&window)
        .build(&mut rb_suggest);

    let mut cb_learn = nwg::CheckBox::default();
    let _ = nwg::CheckBox::builder()
        .text("Learn new words automatically")
        .font(Some(&font))
        .position((30, 246))
        .size((400, 24))
        .parent(&window)
        .build(&mut cb_learn);

    // --- Hotkeys -----------------------------------------------------------
    header!("Hotkeys (fixed in v1)", 286);
    for (i, (action, keys)) in onboard::HOTKEYS.iter().enumerate() {
        hotrow!(*action, *keys, 318 + (i as i32) * 24);
    }

    // --- Blocked apps ------------------------------------------------------
    header!("Blocked apps", 470);
    hint!(
        "Always blocked: terminals, password managers, wallets.\n\
         Add more below — one .exe name per line.",
        500
    );

    let mut editor = nwg::TextBox::default();
    let _ = nwg::TextBox::builder()
        .text(&safety::custom_list().join("\r\n"))
        .font(Some(&font))
        .flags(
            nwg::TextBoxFlags::VISIBLE
                | nwg::TextBoxFlags::VSCROLL
                | nwg::TextBoxFlags::AUTOVSCROLL,
        )
        .position((28, 548))
        .size((504, 76))
        .parent(&window)
        .build(&mut editor);

<<<<<<< Updated upstream
    // --- Buttons ---
=======
    // --- Buttons -----------------------------------------------------------
    let mut clear_learned = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Clear learned words")
        .font(Some(&font))
        .position((28, 640))
        .size((170, 30))
        .parent(&window)
        .build(&mut clear_learned);

    let mut cancel = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Cancel")
        .font(Some(&font))
        .position((322, 640))
        .size((76, 30))
        .parent(&window)
        .build(&mut cancel);

>>>>>>> Stashed changes
    let mut apply = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Apply")
        .font(Some(&font))
        .position((404, 640))
        .size((76, 30))
        .parent(&window)
        .build(&mut apply);

    let mut ok = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("OK")
        .font(Some(&font))
        .position((486, 640))
        .size((50, 30))
        .parent(&window)
        .build(&mut ok);

    // Reflect current state.
    cb_enabled.set_check_state(bool_cb(hook::is_enabled()));
    cb_startup.set_check_state(bool_cb(startup::is_enabled()));
    cb_learn.set_check_state(bool_cb(learn::is_enabled()));
    rb_auto.set_check_state(bool_rb(hook::mode() == hook::Mode::Auto));
    rb_manual.set_check_state(bool_rb(hook::mode() == hook::Mode::Manual));
    rb_suggest.set_check_state(bool_rb(hook::mode() == hook::Mode::Suggest));

    window.set_visible(true);
    OPEN.store(
        window.handle.hwnd().map(|h| h as isize).unwrap_or(0),
        std::sync::atomic::Ordering::Release,
    );

    let ui = Rc::new(SettingsWindow {
        window,
        _font: font,
        _head: head,
        _labels: labels,
        cb_enabled,
        cb_startup,
        rb_auto,
        rb_manual,
        rb_suggest,
        cb_learn,
        editor,
<<<<<<< Updated upstream
        apply,
        ok,
        cancel,
=======
        clear_learned,
        cancel,
        apply,
        ok,
        _theme: themed,
>>>>>>> Stashed changes
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
                    finish(&ui_h);
                } else if handle == ui_h.cancel.handle {
                    finish(&ui_h);
                }
            }
            E::OnWindowClose if handle == ui_h.window.handle => finish(&ui_h),
            _ => {}
        }
    });
    *ui.handler.borrow_mut() = Some(handler);

    fn finish(ui: &Rc<SettingsWindow>) {
        let my = ui.window.handle.hwnd().map(|h| h as isize).unwrap_or(0);
        // Only clear the registration if it is still OURS — a refresh may have
        // already claimed the slot with the replacement window.
        let _ = OPEN.compare_exchange(
            my,
            0,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        );
        if let Some(h) = ui.handler.borrow_mut().take() {
            nwg::unbind_event_handler(&h);
        }
        ui.window.close();
    }
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

<<<<<<< Updated upstream
fn cleanup(ui: &Rc<SettingsWindow>) {
    if let Some(h) = ui.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    ui.window.close();
}

=======
>>>>>>> Stashed changes
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

/// Create a titled BS_GROUPBOX frame — removed with the redesign; kept as a
/// no-op hook point in case a future theme wants framed sections again.
pub const H_HINT: i32 = 40;
