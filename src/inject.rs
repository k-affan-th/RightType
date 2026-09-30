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
//! interleave (text after deletions goes in a second call 40 ms later: see
//! [`needs_gap`]), and the [`INJECTING`](crate::hook::INJECTING) guard plus the
//! `LLKHF_INJECTED` flag keep the hook from reprocessing our own events.

use std::mem::size_of;
use std::sync::atomic::Ordering;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_DELETE,
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
    release_held(&mut inputs);
    // 2. Delete the mistyped word (and the boundary key that triggered us).
    //    A browser address bar may have selected a completion after the
    //    caret: Delete clears it (and does nothing otherwise, since the word
    //    being replaced always ends at the caret), so each Backspace then
    //    removes a typed character.
    if backspaces > 0 && crate::focus::completes_inline() {
        inputs.push(key(VK_DELETE.0, false));
        inputs.push(key(VK_DELETE.0, true));
    }
    for _ in 0..backspaces {
        inputs.push(key(VK_BACK.0, false));
        inputs.push(key(VK_BACK.0, true));
    }
    // Windows 11 Notepad reads Unicode characters that arrive while it is
    // still handling the Backspaces as the last one sent (`สวัสดี` became
    // `ีีีีีี`, and `l;ylfu` put back over Thai `l;ylfuuuuuu`), so text waits
    // until the deletions have landed.
    let split = needs_gap(backspaces, text).then_some(inputs.len());
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

    // One atomic batch (two when the text must wait for the deletions),
    // guarded so the hook skips every event we generate. `SendInput` returns
    // once every event has passed the low-level hooks, so the pause is a
    // real gap in what the app receives.
    INJECTING.store(true, Ordering::SeqCst);
    let (first, rest) = inputs.split_at(split.unwrap_or(inputs.len()));
    let mut sent = SendInput(first, size_of::<INPUT>() as i32) as usize;
    if sent == first.len() && !rest.is_empty() {
        std::thread::sleep(DELETE_GAP);
        sent += SendInput(rest, size_of::<INPUT>() as i32) as usize;
    }
    INJECTING.store(false, Ordering::SeqCst);
    sent == inputs.len()
}

/// How long text waits after the Backspaces that precede it. The CI probe
/// on Windows 11 Notepad: Thai sent at once or right after, 0 of 3 intact;
/// 30 ms later, 3 of 3.
const DELETE_GAP: std::time::Duration = std::time::Duration::from_millis(40);

/// Does `text` have to wait for `backspaces` deletions to land first? Any
/// text after deletions: English put back over Thai was garbled the same
/// way. Text with nothing to delete (a mid-word append) has nothing to wait
/// for.
fn needs_gap(backspaces: usize, text: &str) -> bool {
    backspaces > 0 && !text.is_empty()
}

/// An unassigned virtual key, pressed to "mask" an Alt release (the same trick
/// AutoHotkey uses, vk E8).
const VK_MENU_MASK: u16 = 0xE8;

/// Append key-ups for every modifier still physically held.
///
/// A bare Alt press-and-release with nothing in between is an *Alt tap*, which
/// Windows, Chromium and Electron treat as "activate the menu bar" — after that
/// every injected key goes to the menu, not the text. Alt+CapsLock (accept a
/// suggestion) is exactly such a sequence from the app's point of view, since
/// the CapsLock is swallowed. So an unassigned key is tapped first, making the
/// release an ordinary chord release instead of a tap.
pub fn release_held(inputs: &mut Vec<INPUT>) {
    let held = held_modifiers();
    if held.contains(&0x12) {
        inputs.push(key(VK_MENU_MASK, false));
        inputs.push(key(VK_MENU_MASK, true));
    }
    for vk in held {
        inputs.push(key(vk, true));
    }
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

#[cfg(test)]
mod tests {
    use super::needs_gap;

    #[test]
    fn thai_after_deletions_waits_for_them() {
        assert!(needs_gap(5, "สวัสดี"));
        assert!(!needs_gap(0, "สวัสดี")); // a mid-word append deletes nothing
        assert!(needs_gap(6, "l;ylfu")); // English over Thai too
        assert!(!needs_gap(3, "")); // only deleting
    }
}
