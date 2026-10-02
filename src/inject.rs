//! Correction injection — Windows only. **The Bug 2 fix.**
//!
//! In a standard Windows text box (Edit, RichEdit: Notepad, WordPad, dialogs)
//! the box itself is told to replace the word (`EM_SETSEL` + `EM_REPLACESEL`,
//! one undoable edit): no keys at all, so nothing the typist presses meanwhile
//! can land in between and nothing depends on how fast the app reads keys.
//! Everywhere else the correction is typed, as below.
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
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_BACK, VK_DELETE, VK_SPACE,
};

use crate::hook::{held_modifiers, INJECTING, INJECT_TAG};
use zeroize::Zeroize;

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
    let context = CONTEXT.with(|c| c.borrow_mut().take());
    if backspaces == 0 && text.is_empty() && trailing_vk.is_none() {
        return true;
    }
    // A standard Windows text box is told to replace the word itself, in one
    // message: nothing the typist presses meanwhile can land between our
    // keys, and nothing depends on how fast the app reads them (Windows 11
    // Notepad garbled keys-typed corrections).
    let text_box = if e2e_env("RIGHTTYPE_E2E_NO_TEXTBOX") {
        Err("turned off for a test")
    } else {
        crate::focus::TextBox::focused()
    };
    if let Ok(tb) = text_box {
        let mut whole = text.to_string();
        let trailing = match trailing_vk {
            Some(vk) if vk == VK_SPACE.0 => {
                whole.push(' ');
                None
            }
            other => other,
        };
        let replaced =
            tb.replace_before_caret(backspaces, &whole, context.as_deref().map(|s| s.as_str()));
        whole.zeroize();
        match replaced {
            Ok(()) => {
                crate::hook::e2e_trace(format!("inject: text box replaced {backspaces}"));
                righttype::diag::note(
                    "replaced by the text box",
                    &[
                        ("deleted", backspaces.into()),
                        ("typed", text.chars().count().into()),
                    ],
                );
                let Some(vk) = trailing else {
                    return true;
                };
                let mut inputs = Vec::with_capacity(6);
                release_held(&mut inputs);
                inputs.push(key(vk, false));
                inputs.push(key(vk, true));
                return send(&inputs);
            }
            Err(crate::focus::ReplaceError::Untouched(why)) => {
                crate::hook::e2e_trace(format!("inject: text box not used ({why}), keys instead"));
                righttype::diag::note("text box not used", &[("why", why.into())]);
            }
            Err(crate::focus::ReplaceError::Unknown(why)) => {
                crate::hook::e2e_trace(format!("inject: text box replace unclear ({why})"));
                righttype::diag::note(
                    "text box result unclear, not retyped",
                    &[("why", why.into())],
                );
                return false;
            }
        }
    }
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
    let exe = crate::hook::current_app();
    let gap = if exe.as_deref().is_some_and(crate::verify::is_slow) {
        crate::verify::SLOW_GAP
    } else if e2e_env("RIGHTTYPE_E2E_NO_GAP") {
        std::time::Duration::ZERO
    } else {
        DELETE_GAP
    };
    let split = (needs_gap(backspaces, text) && !gap.is_zero()).then_some(inputs.len());
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
        std::thread::sleep(gap);
        sent += SendInput(rest, size_of::<INPUT>() as i32) as usize;
    }
    INJECTING.store(false, Ordering::SeqCst);
    righttype::diag::note(
        "typed as keys",
        &[
            ("deleted", backspaces.into()),
            ("typed", text.chars().count().into()),
            (
                "waited_ms",
                (split.map_or(0, |_| gap.as_millis() as usize)).into(),
            ),
            ("all_sent", (sent == inputs.len()).into()),
        ],
    );
    // Check what the app shows, when the whole correction is before the caret.
    if sent == inputs.len() && backspaces > 0 {
        match trailing_vk {
            None => crate::verify::after_keys(text, exe),
            Some(vk) if vk == VK_SPACE.0 => {
                let mut whole = format!("{text} ");
                crate::verify::after_keys(&whole, exe);
                whole.zeroize();
            }
            Some(_) => {}
        }
    }
    sent == inputs.len()
}

thread_local! {
    /// What the next [`apply`] expects just before the caret; see
    /// [`expect_before_caret`].
    static CONTEXT: std::cell::RefCell<Option<zeroize::Zeroizing<String>>> =
        const { std::cell::RefCell::new(None) };
}

/// Tell the next [`apply`] (on this thread) what the app should be showing
/// just before the caret, the deleted characters last. A standard text box is
/// then checked first: if it has not handled the latest keys yet, the
/// correction goes in as keys (they queue behind them); if it dropped Thai
/// marks it would not accept, only what is there is deleted.
pub fn expect_before_caret(text: &str) {
    CONTEXT.with(|c| *c.borrow_mut() = Some(zeroize::Zeroizing::new(text.to_string())));
}

/// Press CapsLock once (ours: the hook lets it through untouched).
pub unsafe fn toggle_capslock() {
    let _ = send(&[key(0x14, false), key(0x14, true)]);
}

/// Debug e2e builds: a switch the test harness sets. Always off in release.
fn e2e_env(name: &str) -> bool {
    cfg!(debug_assertions) && std::env::var_os(name).is_some()
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

/// Send `inputs` as one batch, guarded so the hook skips them.
unsafe fn send(inputs: &[INPUT]) -> bool {
    INJECTING.store(true, Ordering::SeqCst);
    let sent = SendInput(inputs, size_of::<INPUT>() as i32);
    INJECTING.store(false, Ordering::SeqCst);
    sent as usize == inputs.len()
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

/// Press `vk` (a modifier) again if the typist still holds it but Windows
/// no longer thinks so, after [`apply`] let go of it: the next key of a
/// held Ctrl+Backspace must still come with Ctrl.
///
/// # Safety
/// Calls `SendInput`.
pub unsafe fn hold_again(vk: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
    if GetAsyncKeyState(vk as i32) as u16 & 0x8000 == 0 {
        send(&[key(vk, false)]);
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
