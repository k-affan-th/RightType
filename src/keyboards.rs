//! The keyboard each app starts with — Windows only.
//!
//! When an app with a keyboard of its own (Settings → Apps) comes to the
//! front, its keyboard is switched to, once; switching by hand while there
//! is left alone until the app is left and come back to. An app set to
//! "English outside text" gets English while focus is not on text being
//! edited (a canvas, a tool panel), and the keyboard in use back when it is.
//!
//! Nothing typed is looked at: the program's name, whether focus is on
//! editable text, and which keyboard is on.

use std::cell::{Cell, RefCell};

use righttype::per_app::{outside_text_switch, AppKeyboard};
use righttype::policy::InputLayout;
use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyboardLayout;
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

thread_local! {
    /// The app last brought to the front.
    static LAST_APP: RefCell<Option<String>> = const { RefCell::new(None) };
    /// Thai was put away here for "English outside text".
    static PUT_AWAY: Cell<bool> = const { Cell::new(false) };
}

fn thai_on() -> bool {
    unsafe {
        let hkl = GetKeyboardLayout(GetWindowThreadProcessId(GetForegroundWindow(), None));
        hkl.0 as usize & 0xFFFF == 0x041E
    }
}

/// Another window came to the front (UI thread): its app's own keyboard
/// applies again, even when its field had focus before.
pub fn on_front() {
    LAST_APP.with(|l| *l.borrow_mut() = None);
    let _ = on_focus();
}

/// Focus moved (UI thread). Returns whether the app has a keyboard of its
/// own (then the per-field guess stays out of it).
pub fn on_focus() -> bool {
    if !crate::hook::is_enabled() || crate::focus::is_password_field() {
        return false;
    }
    let Some(exe) = (unsafe { crate::safety::foreground_exe(GetForegroundWindow()) }) else {
        return false;
    };
    if crate::safety::is_blacklisted_name(&exe)
        || crate::apps::lookup(&exe) == Some(righttype::per_app::AppMode::Off)
    {
        return false;
    }
    let new_app = LAST_APP.with(|l| {
        let mut l = l.borrow_mut();
        let new = l.as_deref() != Some(exe.as_str());
        if new {
            *l = Some(exe.clone());
        }
        new
    });
    if new_app {
        PUT_AWAY.with(|p| p.set(false));
    }
    let Some(keyboard) = crate::apps::keyboard(&exe) else {
        return false;
    };
    let switch = |layout: InputLayout| {
        crate::hook::trace_note("app keyboard: switched");
        unsafe { crate::hook::switch_layout(layout) };
    };
    match keyboard {
        AppKeyboard::Thai if new_app && !thai_on() => switch(InputLayout::ThaiKedmanee),
        AppKeyboard::English if new_app && thai_on() => switch(InputLayout::UsQwerty),
        AppKeyboard::EnglishOutsideText => {
            let put_away = PUT_AWAY.with(|p| p.get());
            match outside_text_switch(crate::focus::is_editing(), thai_on(), put_away) {
                Some(false) => {
                    PUT_AWAY.with(|p| p.set(true));
                    switch(InputLayout::UsQwerty);
                }
                Some(true) => {
                    PUT_AWAY.with(|p| p.set(false));
                    switch(InputLayout::ThaiKedmanee);
                }
                None => {}
            }
        }
        _ => {}
    }
    true
}
