//! Focus-based password-field detection via UI Automation — Windows only.
//!
//! `ES_PASSWORD` (in `safety`) only sees native edit controls; password fields in
//! browsers, Electron, and UWP apps are not Win32 controls, so the user's question
//! "how do you even know it's a password box in a browser?" is exactly right — by
//! style alone, we can't. UI Automation *does* expose their `IsPassword` property,
//! but UIA is far too slow to call per keystroke. So we install a **WinEvent focus
//! hook** and query UIA only when focus moves, caching the answer in an atomic the
//! keyboard hook reads for free.
//!
//! Failure is conservative: if COM/UIA init or a query fails, the cache remains
//! `UNKNOWN` and the keyboard pipeline treats the field as protected until UIA
//! explicitly reports a safe focus.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK,
};
use windows::Win32::UI::WindowsAndMessaging::{EVENT_OBJECT_FOCUS, WINEVENT_OUTOFCONTEXT};

const FIELD_UNKNOWN: u8 = 0;
const FIELD_SAFE: u8 = 1;
const FIELD_PASSWORD: u8 = 2;

/// Cached UIA result, read cheaply by the keyboard hook on every keystroke.
static FIELD_STATUS: AtomicU8 = AtomicU8::new(FIELD_UNKNOWN);
/// Changes whenever Windows reports that the focused UI element changed.  The
/// keyboard hook uses this to invalidate text that belongs to an old caret,
/// including two controls inside the same top-level window.
static FOCUS_GENERATION: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    static HOOK: RefCell<Option<HWINEVENTHOOK>> = const { RefCell::new(None) };
}

/// Is the currently focused element a password field (per UIA)?
pub fn is_password_field() -> bool {
    status_is_protected(FIELD_STATUS.load(Ordering::Relaxed))
}

fn status_is_protected(status: u8) -> bool {
    status != FIELD_SAFE
}

/// Monotonically increasing identity for the current focused UI element.
///
/// This is deliberately cheaper than querying UI Automation from the keyboard
/// hook.  It is best-effort: native controls still have the top-level-window
/// guard in the hook if an app does not publish focus events.
pub fn generation() -> u64 {
    FOCUS_GENERATION.load(Ordering::Relaxed)
}

/// Initialise COM + UIA and install the focus hook. Best-effort.
///
/// # Safety
/// UI thread only; call [`disarm`] before exit.
pub unsafe fn arm() {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    if let Ok(uia) =
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    {
        UIA.with(|u| *u.borrow_mut() = Some(uia));
    }
    refresh_status();
    let hook = SetWinEventHook(
        EVENT_OBJECT_FOCUS,
        EVENT_OBJECT_FOCUS,
        None,
        Some(on_focus),
        0,
        0,
        WINEVENT_OUTOFCONTEXT,
    );
    HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

/// Remove the focus hook.
///
/// # Safety
/// Same thread that called [`arm`].
pub unsafe fn disarm() {
    HOOK.with(|h| {
        if let Some(hook) = h.borrow_mut().take() {
            let _ = UnhookWinEvent(hook);
        }
    });
}

unsafe extern "system" fn on_focus(
    _hook: HWINEVENTHOOK,
    _event: u32,
    _hwnd: HWND,
    _idobj: i32,
    _idchild: i32,
    _thread: u32,
    _time: u32,
) {
    FOCUS_GENERATION.fetch_add(1, Ordering::Relaxed);
    refresh_status();
}

unsafe fn refresh_status() {
    let status = UIA.with(|u| {
        u.borrow()
            .as_ref()
            .and_then(|uia| {
                let el = uia.GetFocusedElement().ok()?;
                el.CurrentIsPassword().ok().map(|b| b.as_bool())
            })
            .map(|is_password| {
                if is_password {
                    FIELD_PASSWORD
                } else {
                    FIELD_SAFE
                }
            })
            .unwrap_or(FIELD_UNKNOWN)
    });
    FIELD_STATUS.store(status, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::{status_is_protected, FIELD_PASSWORD, FIELD_SAFE, FIELD_UNKNOWN};

    #[test]
    fn unknown_and_password_statuses_fail_closed() {
        assert!(status_is_protected(FIELD_UNKNOWN));
        assert!(status_is_protected(FIELD_PASSWORD));
        assert!(!status_is_protected(FIELD_SAFE));
    }
}
