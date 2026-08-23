//! Session resilience — Windows only. **The fix for RightLang Bug 1.**
//!
//! Windows can silently evict a `WH_KEYBOARD_LL` hook across power and session
//! transitions (sleep/resume, lock/unlock), and the original RightLang never
//! recovered — you had to kill and reopen it. RightType listens for those
//! transitions on the tray window and **reinstalls the hook**:
//!
//! - `RegisterSuspendResumeNotification` → `WM_POWERBROADCAST` on resume,
//! - `WTSRegisterSessionNotification` → `WM_WTSSESSION_CHANGE` on unlock.
//!
//! A `WM_TIMER` performs one delayed retry after such an event, in case the
//! immediate reinstall ran before the system had fully resumed. It is not an
//! independent hook-liveness probe. Feed every raw window message to
//! [`on_message`].

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::Power::RegisterSuspendResumeNotification;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer, DEVICE_NOTIFY_WINDOW_HANDLE};

use crate::hook;

// Stable Win32 message / wparam values (kept local to avoid extra crate features).
const WM_TIMER: u32 = 0x0113;
const WM_POWERBROADCAST: u32 = 0x0218;
const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
const PBT_APMRESUMESUSPEND: usize = 0x0007;
const PBT_APMRESUMEAUTOMATIC: usize = 0x0012;
const WTS_SESSION_UNLOCK: usize = 0x8;
const WATCHDOG_TIMER_ID: usize = 1;
const WATCHDOG_INTERVAL_MS: u32 = 1500;

/// Set by a resume/unlock event; the next timer tick reinstalls and clears it.
static NEEDS_REINSTALL: AtomicBool = AtomicBool::new(false);

/// Start listening for power/session transitions on `hwnd` and start the retry timer.
///
/// # Safety
/// `hwnd` must be a valid window on the calling (message-pumping) thread.
pub unsafe fn arm(hwnd: HWND) {
    let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
    let _ = RegisterSuspendResumeNotification(HANDLE(hwnd.0), DEVICE_NOTIFY_WINDOW_HANDLE);
    SetTimer(hwnd, WATCHDOG_TIMER_ID, WATCHDOG_INTERVAL_MS, None);
}

/// Stop the retry timer and session notifications (the power notification is released
/// by the OS on exit).
///
/// # Safety
/// Same `hwnd` passed to [`arm`].
pub unsafe fn disarm(hwnd: HWND) {
    let _ = KillTimer(hwnd, WATCHDOG_TIMER_ID);
    let _ = WTSUnRegisterSessionNotification(hwnd);
}

/// Handle one raw window message. On resume/unlock it reinstalls the hook now and
/// flags a follow-up reinstall; the retry `WM_TIMER` performs that follow-up.
///
/// # Safety
/// Must run on the hook's message-pumping thread.
pub unsafe fn on_message(msg: u32, wparam: usize) {
    match msg {
        WM_TIMER if wparam == WATCHDOG_TIMER_ID => {
            if NEEDS_REINSTALL.swap(false, Ordering::Relaxed) {
                let _ = hook::reinstall();
            }
        }
        WM_POWERBROADCAST if wparam == PBT_APMRESUMEAUTOMATIC || wparam == PBT_APMRESUMESUSPEND => {
            crate::hook::e2e_trace(format!("session: power resume ({wparam:#x}) reinstall"));
            let _ = hook::reinstall();
            NEEDS_REINSTALL.store(true, Ordering::Relaxed);
        }
        WM_WTSSESSION_CHANGE if wparam == WTS_SESSION_UNLOCK => {
            crate::hook::e2e_trace("session: unlock reinstall".to_string());
            let _ = hook::reinstall();
            NEEDS_REINSTALL.store(true, Ordering::Relaxed);
        }
        _ => {}
    }
}
