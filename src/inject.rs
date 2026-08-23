//! Correction injection (`SendInput`) — Windows only. **The Bug 2 fix.**
//!
//! RightLang's manual switch sometimes emitted `ggg…` garbage because it replayed
//! virtual keys while a physical key/modifier was still held, so the OS saw
//! auto-repeat or inherited a stuck modifier. RightType never replays VKs for the
//! text: it
//!
//! 1. **releases any held modifiers** (Shift/Ctrl/Alt) first,
//! 2. deletes the mistyped word with backspaces, then
//! 3. injects the correction as a **single atomic Unicode `SendInput` batch**
//!    (`KEYEVENTF_UNICODE`), so apps receive `WM_CHAR` codepoints directly — no
//!    layout switch, no race, and Thai combining order is preserved.
//!
//! The whole batch is sent in one `SendInput` call so no other input can
//! interleave, and the [`INJECTING`](crate::hook::INJECTING) guard plus the
//! `LLKHF_INJECTED` flag keep the hook from reprocessing our own events.

use std::mem::size_of;
use std::sync::atomic::Ordering;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK,
};

use crate::hook::{held_modifiers, INJECTING, INJECT_TAG};

/// Replace the just-typed word with `text`.
///
/// `backspaces` characters are deleted first (the mistyped word plus the boundary
/// key that completed it), then `text` is injected, then `trailing_vk` — the
/// boundary key (space/enter/tab) — is re-pressed so the separator the user typed
/// is preserved.
///
/// # Safety
/// Calls `SendInput`; must run while the keyboard hook is installed so its own
/// events are recognised (via `LLKHF_INJECTED`) and ignored.
pub unsafe fn apply(backspaces: usize, text: &str, trailing_vk: Option<u16>) -> bool {
    let mut inputs: Vec<INPUT> = Vec::with_capacity(backspaces * 2 + text.len() * 2 + 4);

    // 1. Release any modifier still physically held, so it can't taint the batch.
    for vk in held_modifiers() {
        inputs.push(key(vk, true));
    }
    // 2. Delete the mistyped word (and the boundary key that triggered us).
    for _ in 0..backspaces {
        inputs.push(key(VK_BACK.0, false));
        inputs.push(key(VK_BACK.0, true));
    }
    // 3. Inject the correction as raw Unicode code units (handles non-BMP too).
    let mut units = [0u16; 2];
    for ch in text.chars() {
        for &unit in ch.encode_utf16(&mut units).iter() {
            inputs.push(unicode(unit, false));
            inputs.push(unicode(unit, true));
        }
    }
    // 4. Re-emit the boundary key the user pressed.
    if let Some(vk) = trailing_vk {
        inputs.push(key(vk, false));
        inputs.push(key(vk, true));
    }

    if inputs.is_empty() {
        return true;
    }

    // One atomic batch, guarded so the hook skips every event we generate.
    INJECTING.store(true, Ordering::SeqCst);
    let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
    INJECTING.store(false, Ordering::SeqCst);
    sent as usize == inputs.len()
}

/// A virtual-key press or release.
fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: INJECT_TAG,
            },
        },
    }
}

/// A single UTF-16 code unit injected as a Unicode character event.
fn unicode(unit: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: if up {
                    KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_UNICODE
                },
                time: 0,
                dwExtraInfo: INJECT_TAG,
            },
        },
    }
}
