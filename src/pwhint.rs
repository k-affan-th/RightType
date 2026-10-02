//! The TH / CAPS tag at a password field — Windows only.
//!
//! A password typed with the Thai keyboard on, or with CapsLock on, is
//! refused without saying why. When focus moves into a password field and
//! either is the case, a small tag says so next to the field, and changes
//! as the keyboard or CapsLock changes while focus stays there.
//!
//! Only the keyboard's language and CapsLock are looked at — never a key
//! typed into the field (the keyboard hook passes those through untouched,
//! as before). Not in the apps on the safety list: RightType does nothing
//! at all there.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, KillTimer, SetTimer,
};

static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// How often the keyboard and CapsLock are looked at while focus is in a
/// password field.
const TICK_MS: u32 = 300;

thread_local! {
    /// What the tag last said here (Thai keyboard, CapsLock), and the timer.
    static LAST: Cell<Option<(bool, bool)>> = const { Cell::new(None) };
    static TIMER: Cell<usize> = const { Cell::new(0) };
}

/// The tag for a password field with the Thai keyboard on (`thai`) and
/// CapsLock on (`caps`); `None` when nothing is worth saying.
pub fn tag(thai: bool, caps: bool) -> Option<&'static str> {
    match (thai, caps) {
        (true, true) => Some("TH · CAPS"),
        (true, false) => Some("TH"),
        (false, true) => Some("CAPS"),
        (false, false) => None,
    }
}

/// Focus moved (UI thread): start watching if it is in a password field.
pub fn on_focus() {
    LAST.with(|l| l.set(None));
    stop();
    if !is_enabled() || crate::focus::password_box().is_none() {
        return;
    }
    let exe = unsafe { crate::safety::foreground_exe(GetForegroundWindow()) };
    if exe
        .as_deref()
        .map_or(true, crate::safety::is_blacklisted_name)
    {
        return;
    }
    check();
    unsafe extern "system" fn fire(_: HWND, _: u32, _: usize, _: u32) {
        check();
    }
    let id = unsafe { SetTimer(None, 0, TICK_MS, Some(fire)) };
    TIMER.with(|t| t.set(id));
}

fn stop() {
    let id = TIMER.with(|t| t.replace(0));
    if id != 0 {
        unsafe {
            let _ = KillTimer(None, id);
        }
    }
}

/// Look at the keyboard and CapsLock; show the tag when they changed.
fn check() {
    let Some(field) = crate::focus::password_box() else {
        stop();
        return;
    };
    let (thai, caps) = unsafe {
        let fg = GetForegroundWindow();
        let hkl = GetKeyboardLayout(GetWindowThreadProcessId(fg, None));
        (
            hkl.0 as usize & 0xFFFF == 0x041E,
            GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0,
        )
    };
    let now = (thai, caps);
    if LAST.with(|l| l.replace(Some(now))) == Some(now) {
        return;
    }
    if let Some(text) = tag(thai, caps) {
        // At the field's right end, inside it.
        let at = windows::Win32::Foundation::RECT {
            left: field.right - 4,
            top: field.top,
            right: field.right - 2,
            bottom: field.bottom,
        };
        crate::hook::e2e_trace(format!("password tag: {text}"));
        crate::overlay::badge_at(text, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_says_what_would_spoil_a_password() {
        assert_eq!(tag(true, false), Some("TH"));
        assert_eq!(tag(false, true), Some("CAPS"));
        assert_eq!(tag(true, true), Some("TH · CAPS"));
        assert_eq!(tag(false, false), None);
    }
}
