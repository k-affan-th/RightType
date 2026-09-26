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
//! immediate reinstall ran before the system had fully resumed. The same timer
//! is also an independent liveness check: Windows can drop a low-level hook
//! with no event at all (for example after a callback exceeded
//! `LowLevelHooksTimeout`), so when the system reports recent input but the
//! hook has heard nothing for [`SILENCE_MS`], the hook is reinstalled. Mouse
//! input also counts as input, so a mouse-only session reinstalls at most once
//! per [`SILENCE_MS`] — cheap, and harmless to a live hook. Feed every raw
//! window message to [`on_message`].

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::System::Power::RegisterSuspendResumeNotification;
use windows::Win32::System::RemoteDesktop::{
    WTSRegisterSessionNotification, WTSUnRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION,
};
use windows::Win32::System::SystemInformation::GetTickCount;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
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

/// Set by a resume/unlock event or a failed reinstall; the next timer tick
/// reinstalls and clears it.
static NEEDS_REINSTALL: AtomicBool = AtomicBool::new(false);

/// Whether the last hook (re)installation succeeded. While it is false the
/// tray shows that RightType is not working, and every timer tick retries.
static HEALTHY: AtomicBool = AtomicBool::new(true);

/// Is the keyboard hook installed? `false` after a reinstall failed, until one
/// succeeds.
pub fn is_healthy() -> bool {
    HEALTHY.load(Ordering::Relaxed)
}

/// Reinstall the hook and tell the user when that starts or stops working.
/// A failure is retried on every timer tick (1.5 s) until it succeeds.
unsafe fn reinstall() {
    use righttype::i18n::{tr, T};
    match hook::reinstall() {
        Ok(()) => {
            if !HEALTHY.swap(true, Ordering::Relaxed) {
                crate::toast::show(tr(T::ToastHookBack));
            }
        }
        Err(_) => {
            if HEALTHY.swap(false, Ordering::Relaxed) {
                crate::toast::show(tr(T::ToastHookLost));
            }
            NEEDS_REINSTALL.store(true, Ordering::Relaxed);
        }
    }
}

/// How long the hook may stay silent while the user is giving input before it
/// is presumed evicted.
const SILENCE_MS: u32 = 30_000;
/// "The user is giving input" = the system saw input this recently.
const RECENT_INPUT_MS: u32 = 3_000;

/// Tick of the last liveness-driven reinstall, for rate limiting.
static LAST_LIVENESS_REINSTALL: AtomicU32 = AtomicU32::new(0);

/// Pure decision for the liveness check; all values are `GetTickCount` ticks,
/// compared with wrapping arithmetic so the 49.7-day rollover is harmless.
fn hook_looks_evicted(now: u32, last_input: u32, last_hook: u32, last_reinstall: u32) -> bool {
    let input_is_recent = now.wrapping_sub(last_input) <= RECENT_INPUT_MS;
    let hook_is_silent = now.wrapping_sub(last_hook) >= SILENCE_MS;
    let not_just_reinstalled = now.wrapping_sub(last_reinstall) >= SILENCE_MS;
    input_is_recent && hook_is_silent && not_just_reinstalled
}

unsafe fn check_liveness() {
    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !GetLastInputInfo(&mut info).as_bool() {
        return;
    }
    let now = GetTickCount();
    if hook_looks_evicted(
        now,
        info.dwTime,
        hook::last_hook_tick(),
        LAST_LIVENESS_REINSTALL.load(Ordering::Relaxed),
    ) {
        crate::hook::e2e_trace("session: hook silent during input, reinstall".to_string());
        LAST_LIVENESS_REINSTALL.store(now, Ordering::Relaxed);
        reinstall();
    }
}

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
                reinstall();
            } else {
                check_liveness();
            }
        }
        WM_POWERBROADCAST if wparam == PBT_APMRESUMEAUTOMATIC || wparam == PBT_APMRESUMESUSPEND => {
            crate::hook::e2e_trace(format!("session: power resume ({wparam:#x}) reinstall"));
            reinstall();
            NEEDS_REINSTALL.store(true, Ordering::Relaxed);
        }
        WM_WTSSESSION_CHANGE if wparam == WTS_SESSION_UNLOCK => {
            crate::hook::e2e_trace("session: unlock reinstall".to_string());
            reinstall();
            NEEDS_REINSTALL.store(true, Ordering::Relaxed);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hook_that_hears_the_typing_is_left_alone() {
        assert!(!hook_looks_evicted(100_000, 99_900, 99_900, 0));
    }

    #[test]
    fn silence_during_input_means_evicted() {
        assert!(hook_looks_evicted(100_000, 99_000, 60_000, 0));
    }

    #[test]
    fn an_idle_machine_is_not_evidence() {
        // Nothing typed for a while: a silent hook is expected.
        assert!(!hook_looks_evicted(100_000, 50_000, 40_000, 0));
    }

    #[test]
    fn reinstalls_are_rate_limited() {
        assert!(!hook_looks_evicted(100_000, 99_000, 60_000, 90_000));
    }

    #[test]
    fn tick_rollover_is_harmless() {
        let now = 5_000u32;
        let last_input = now.wrapping_sub(1_000);
        let last_hook = now.wrapping_sub(40_000);
        assert!(hook_looks_evicted(
            now,
            last_input,
            last_hook,
            now.wrapping_sub(60_000)
        ));
        assert!(!hook_looks_evicted(
            now,
            last_input,
            now.wrapping_sub(10),
            0
        ));
    }
}
