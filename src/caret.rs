//! Where the text cursor is, and the hints shown next to it — Windows only.
//!
//! When RightType switches the keyboard layout (after fixing a word, or after
//! a flip) a small `TH` / `EN` tag flashes just below the text cursor, where
//! the eyes already are, instead of only in the taskbar. The Suggest hint is
//! shown there too. Only the layout name or the suggested word is drawn —
//! never the text around the cursor — and nothing is recorded.
//!
//! The position comes from the system caret (`GetGUIThreadInfo`), which
//! classic Win32 controls, Office and Chromium-based apps keep up to date, and
//! otherwise from UI Automation's text pattern (apps that draw their own
//! cursor). Both are looked up after the keyboard hook has returned. Where
//! neither says, there is no tag (the taskbar still shows the language) and
//! the Suggest hint goes to the screen corner.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

use righttype::policy::InputLayout;

use crate::overlay::{self, Anchor};

/// Show hints next to the text cursor (Settings → General).
static ENABLED: AtomicBool = AtomicBool::new(true);

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// The text cursor of the foreground window, in screen pixels.
pub fn caret_rect() -> Option<RECT> {
    unsafe {
        let foreground = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(foreground, None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut gui).is_err() || gui.hwndCaret.0.is_null() {
            return None;
        }
        let rc = gui.rcCaret;
        if rc.bottom <= rc.top {
            return None;
        }
        let mut top_left = POINT {
            x: rc.left,
            y: rc.top,
        };
        let mut bottom_right = POINT {
            x: rc.right,
            y: rc.bottom,
        };
        if !ClientToScreen(gui.hwndCaret, &mut top_left).as_bool()
            || !ClientToScreen(gui.hwndCaret, &mut bottom_right).as_bool()
        {
            return None;
        }
        Some(RECT {
            left: top_left.x,
            top: top_left.y,
            right: bottom_right.x.max(top_left.x + 1),
            bottom: bottom_right.y,
        })
    }
}

/// The text cursor from the system caret or, failing that, UI Automation.
/// UI thread, outside the keyboard hook.
pub fn find_caret() -> Option<RECT> {
    caret_rect().or_else(crate::focus::uia_caret_rect)
}

/// RightType just switched the layout to `layout`: flash its tag at the caret
/// (found after the hook returns; no caret, no tag).
pub fn layout_switched(layout: InputLayout) {
    if !is_enabled() {
        return;
    }
    overlay::badge_at_caret(match layout {
        InputLayout::ThaiKedmanee => "TH",
        InputLayout::UsQwerty => "EN",
    });
}

/// Where the Suggest hint goes: next to the caret when hints there are on
/// (found after the hook returns, the corner if there is none).
pub fn hint_anchor() -> Anchor {
    if is_enabled() {
        Anchor::Caret
    } else {
        Anchor::Corner
    }
}
