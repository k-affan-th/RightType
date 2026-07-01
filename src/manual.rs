//! Manual hotkey actions — Windows only.
//!
//! "Convert selection" is the headline manual feature: select any text — a whole
//! sentence, with or without spaces — and one hotkey flips its layout. Unlike the
//! auto path it needs no word boundaries and no Thai segmentation, because a
//! layout flip is a reversible per-character remap of exactly the bytes selected.
//!
//! It runs on its own thread because it is inherently asynchronous: we inject
//! Ctrl+C, wait for the focused app to populate the clipboard, convert, write it
//! back, and inject Ctrl+V. The per-keystroke hook just posts a [`Command`] and
//! returns, so nothing blocks the hot path.

use std::mem::size_of;
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use righttype::layout::auto_convert;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_V,
};

use crate::clipboard;
use crate::hook::{held_modifiers, INJECTING, INJECT_TAG};

/// A one-shot manual action requested by the hook.
pub enum Command {
    /// Flip the layout of the current selection via the clipboard.
    ConvertSelection,
}

static SENDER: OnceLock<Sender<Command>> = OnceLock::new();

/// Start the manual-action worker. Call once at startup.
pub fn spawn() -> JoinHandle<()> {
    let (tx, rx) = crossbeam_channel::unbounded();
    let _ = SENDER.set(tx);
    thread::spawn(move || run(rx))
}

/// Ask the worker to convert the current selection. Non-blocking.
pub fn request_convert_selection() {
    if let Some(tx) = SENDER.get() {
        let _ = tx.send(Command::ConvertSelection);
    }
}

fn run(rx: Receiver<Command>) {
    for cmd in rx.iter() {
        match cmd {
            Command::ConvertSelection => unsafe { convert_selection() },
        }
    }
}

unsafe fn convert_selection() {
    // The hotkey chord (Shift+CapsLock) may still be physically held; release any
    // modifiers so the injected Ctrl+C/V isn't polluted by them.
    release_modifiers();

    let before = clipboard::sequence();
    let original = clipboard::get_text();

    send_chord(VK_C.0);
    // Wait for the focused app to answer the copy.
    let deadline = Instant::now() + Duration::from_millis(600);
    while clipboard::sequence() == before && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(8));
    }

    let Some(selection) = clipboard::get_text() else {
        restore(original);
        return;
    };
    if selection.is_empty() {
        restore(original);
        return;
    }

    let converted = auto_convert(&selection);
    if converted == selection {
        // Nothing to flip (e.g. selection already in the right script).
        restore(original);
        return;
    }
    if !clipboard::set_text(&converted) {
        restore(original);
        return;
    }
    send_chord(VK_V.0);
    crate::stats::record_manual();

    // Let the paste land before we put the user's clipboard back.
    thread::sleep(Duration::from_millis(30));
    restore(original);
}

fn restore(original: Option<String>) {
    if let Some(text) = original {
        unsafe {
            clipboard::set_text(&text);
        }
    }
}

/// Inject Ctrl+`vk` (down Ctrl, tap key, up Ctrl) as one tagged batch.
unsafe fn send_chord(vk: u16) {
    let inputs = [
        key(VK_CONTROL.0, false),
        key(vk, false),
        key(vk, true),
        key(VK_CONTROL.0, true),
    ];
    INJECTING.store(true, std::sync::atomic::Ordering::SeqCst);
    SendInput(&inputs, size_of::<INPUT>() as i32);
    INJECTING.store(false, std::sync::atomic::Ordering::SeqCst);
}

unsafe fn release_modifiers() {
    let ups: Vec<INPUT> = held_modifiers().into_iter().map(|vk| key(vk, true)).collect();
    if !ups.is_empty() {
        SendInput(&ups, size_of::<INPUT>() as i32);
    }
}

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
