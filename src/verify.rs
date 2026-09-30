//! Check-after-write: after a correction is typed as keys, read back what the
//! app shows before the caret and compare it with what was sent.
//!
//! Some apps garble injected text when it arrives faster than they read it
//! (Windows 11 Notepad showed `ีีีีีี` for สวัสดี). Standard Windows text
//! boxes are no longer typed into (see [`crate::inject`]), but other apps
//! may do the same. When one does, the typist is told, the problem report
//! notes it, and from then on RightType waits longer between the deletions
//! and the text in that app.
//!
//! Runs on a worker thread, a moment after the keys were sent; skipped when
//! the typist has pressed a key since (what is before the caret is then not
//! only the correction), the focus moved, or the app does not share its text
//! (UI Automation text pattern, or a standard text box). The text read back
//! is only as long as the correction, and both strings are zeroized.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use zeroize::{Zeroize, Zeroizing};

/// Keys the typist pressed (not ours), counted by the hook.
pub static TYPED: AtomicU64 = AtomicU64::new(0);

/// Programs that showed a correction differently from what was sent: they get
/// [`SLOW_GAP`] between deletions and text.
static SLOW_APPS: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// How long the app gets to show the keys before they are read back.
const SETTLE: Duration = Duration::from_millis(120);

/// The pause between deletions and text in an app that garbled one.
pub const SLOW_GAP: Duration = Duration::from_millis(150);

pub fn is_slow(exe: &str) -> bool {
    SLOW_APPS
        .lock()
        .map(|s| s.as_ref().is_some_and(|s| s.contains(exe)))
        .unwrap_or(false)
}

fn mark_slow(exe: &str) {
    if let Ok(mut s) = SLOW_APPS.lock() {
        s.get_or_insert_with(HashSet::new).insert(exe.to_string());
    }
}

/// `expected` was just typed as keys into `exe`: check it on a worker thread.
pub fn after_keys(expected: &str, exe: Option<String>) {
    if expected.is_empty() {
        return;
    }
    let expected = Zeroizing::new(expected.to_string());
    let typed = TYPED.load(Ordering::SeqCst);
    let generation = crate::focus::generation();
    let foreground =
        unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0 as isize;
    let _ = std::thread::Builder::new()
        .name("verify".into())
        .spawn(move || {
            std::thread::sleep(SETTLE);
            let now = unsafe { windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow() }.0
                as isize;
            if TYPED.load(Ordering::SeqCst) != typed
                || crate::focus::generation() != generation
                || now != foreground
            {
                crate::hook::e2e_trace("verify: skipped (typing or focus moved)".to_string());
                return;
            }
            let n = expected.chars().count();
            let Some(shown) = crate::focus::text_before_caret(n) else {
                crate::hook::trace_note("verify: app does not share its text");
                return;
            };
            if *shown == *expected {
                crate::hook::trace_note("verify: correction shown as sent");
                return;
            }
            let mut sent: Vec<char> = expected.chars().collect();
            let mut got: Vec<char> = shown.chars().collect();
            let same = sent.iter().zip(&got).filter(|(a, b)| a == b).count();
            sent.zeroize();
            got.zeroize();
            crate::hook::e2e_trace(format!(
                "verify: app shows something else ({same} of {n} characters match)"
            ));
            righttype::diag::note(
                "verify: app shows something else",
                &[("length", n.into()), ("matching", same.into())],
            );
            if let Some(exe) = exe.as_deref() {
                mark_slow(exe);
            }
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastVerifyDiffers));
        });
}
