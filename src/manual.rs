//! Manual hotkey actions — Windows only.
//!
//! "Convert selection" is the headline manual feature: select any text — a whole
//! sentence, with or without spaces — and one hotkey flips its layout. Unlike the
//! auto path it needs no word boundaries and no Thai segmentation, because a
//! layout flip is a reversible per-character remap of exactly the bytes selected.
//!
//! It runs on its own thread because it is inherently asynchronous: we inject
//! Ctrl+C, wait for the focused app to populate the clipboard, read/restore it,
//! then inject the converted selection as Unicode. The per-keystroke hook just
//! posts a [`Command`] and returns, so nothing blocks the hot path.

use std::mem::size_of;
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use righttype::layout::auto_convert;
use zeroize::Zeroize;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_Z,
};

use crate::hook::{held_modifiers, INJECTING, INJECT_TAG};
use crate::{clipboard, focus, toast};
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// A one-shot manual action requested by the hook.
pub enum Command {
    /// Flip the layout of the current selection via the clipboard.
    ConvertSelection {
        hwnd: isize,
        focus_generation: u64,
        requested_at: Instant,
    },
    UndoSelection {
        hwnd: isize,
        focus_generation: u64,
        requested_at: Instant,
    },
}

static SENDER: OnceLock<Sender<Command>> = OnceLock::new();
static SELECTION_UNDO: Mutex<Option<SelectionUndo>> = Mutex::new(None);

const MAX_COMMAND_AGE: Duration = Duration::from_secs(1);
const MAX_UNDO_AGE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
struct SelectionUndo {
    hwnd: isize,
    focus_generation: u64,
    completed_at: Instant,
}

/// Start the manual-action worker. Call once at startup.
pub fn spawn() -> JoinHandle<()> {
    let (tx, rx) = crossbeam_channel::bounded(1);
    let _ = SENDER.set(tx);
    thread::spawn(move || run(rx))
}

/// Ask the worker to convert the current selection. Non-blocking.
pub fn request_convert_selection(hwnd: isize, focus_generation: u64) {
    *SELECTION_UNDO.lock().unwrap() = None;
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::ConvertSelection {
            hwnd,
            focus_generation,
            requested_at: Instant::now(),
        });
    }
}

/// Queue app-native Ctrl+Z for the most recent selection paste, if it still
/// belongs to the same focused control. Returns whether a command was queued.
pub fn request_undo_selection(hwnd: isize, focus_generation: u64) -> bool {
    let mut undo = SELECTION_UNDO.lock().unwrap();
    let Some(record) = *undo else {
        return false;
    };
    if record.hwnd != hwnd
        || record.focus_generation != focus_generation
        || record.completed_at.elapsed() > MAX_UNDO_AGE
    {
        *undo = None;
        return false;
    }
    let Some(tx) = SENDER.get() else {
        return false;
    };
    if tx
        .try_send(Command::UndoSelection {
            hwnd,
            focus_generation,
            requested_at: Instant::now(),
        })
        .is_ok()
    {
        *undo = None;
        true
    } else {
        false
    }
}

fn run(rx: Receiver<Command>) {
    for cmd in rx.iter() {
        match cmd {
            Command::ConvertSelection {
                hwnd,
                focus_generation,
                requested_at,
            } if requested_at.elapsed() <= MAX_COMMAND_AGE => unsafe {
                convert_selection(hwnd, focus_generation)
            },
            Command::UndoSelection {
                hwnd,
                focus_generation,
                requested_at,
            } if requested_at.elapsed() <= MAX_COMMAND_AGE => unsafe {
                undo_selection(hwnd, focus_generation)
            },
            _ => {}
        }
    }
}

unsafe fn convert_selection(hwnd: isize, focus_generation: u64) {
    if !same_context(hwnd, focus_generation) {
        return;
    }

    let Some(original) = clipboard::snapshot_plain_text() else {
        crate::hook::e2e_trace(
            "selection: selection conversion needs a plain-text clipboard".to_string(),
        );
        toast::show("RightType: selection conversion needs a plain-text clipboard");
        return;
    };

    // The hotkey chord (Shift+CapsLock) may still be physically held; release any
    // modifiers so the injected Ctrl+C/V isn't polluted by them.
    if !release_modifiers() {
        crate::hook::e2e_trace("selection: could not release held modifiers".to_string());
        toast::show("RightType: could not release held modifiers");
        return;
    }
    if !same_context(hwnd, focus_generation) {
        return;
    }

    let before = clipboard::sequence();
    if !send_chord(VK_C.0) {
        let _ = clipboard::restore_snapshot(&original);
        crate::hook::e2e_trace("selection: could not copy the selection".to_string());
        toast::show("RightType: could not copy the selection");
        return;
    }
    // Wait for the focused app to answer the copy.
    let deadline = Instant::now() + Duration::from_millis(600);
    while clipboard::sequence() == before && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(8));
    }

    if clipboard::sequence() == before {
        let _ = clipboard::restore_snapshot(&original);
        crate::hook::e2e_trace("selection: no text selection was copied".to_string());
        toast::show("RightType: no text selection was copied");
        return;
    }

    // A clipboard sequence change only means the owner announced new data; apps
    // such as modern Notepad may render CF_UNICODETEXT a few milliseconds later.
    // Retry the actual read within the same bounded window instead of treating
    // that normal delay as a non-Unicode selection.
    let mut copied_text = None;
    while Instant::now() < deadline && same_context(hwnd, focus_generation) {
        if let Some(text) = clipboard::get_text() {
            copied_text = Some(text);
            break;
        }
        thread::sleep(Duration::from_millis(8));
    }
    let Some(mut selection) = copied_text else {
        let _ = clipboard::restore_snapshot(&original);
        crate::hook::e2e_trace("selection: selection is not Unicode text".to_string());
        toast::show("RightType: selection is not Unicode text");
        return;
    };
    if selection.is_empty() {
        crate::hook::e2e_trace("selection: copied selection was empty".to_string());
        let _ = clipboard::restore_snapshot(&original);
        return;
    }

    let mut converted = auto_convert(&selection);
    if converted == selection {
        crate::hook::e2e_trace("selection: conversion was a no-op".to_string());
        // Nothing to flip (e.g. selection already in the right script).
        let _ = clipboard::restore_snapshot(&original);
        selection.zeroize();
        converted.zeroize();
        return;
    }
    if !same_context(hwnd, focus_generation) {
        let _ = clipboard::restore_snapshot(&original);
        selection.zeroize();
        converted.zeroize();
        return;
    }
    // Restore the user's clipboard before changing the document. Unicode input
    // replaces the active selection directly, so there is no asynchronous paste
    // racing a fixed-delay clipboard restore (notably in Word and browsers).
    if !clipboard::restore_snapshot(&original) {
        crate::hook::e2e_trace("selection: could not restore the clipboard".to_string());
        toast::show("RightType: could not restore the clipboard");
        selection.zeroize();
        converted.zeroize();
        return;
    }
    if !same_context(hwnd, focus_generation) {
        selection.zeroize();
        converted.zeroize();
        return;
    }
    if !crate::inject::apply(0, &converted, None) {
        toast::show("RightType: could not inject the conversion");
        selection.zeroize();
        converted.zeroize();
        return;
    }
    crate::stats::record_manual();
    *SELECTION_UNDO.lock().unwrap() = Some(SelectionUndo {
        hwnd,
        focus_generation,
        completed_at: Instant::now(),
    });
    selection.zeroize();
    converted.zeroize();
}

unsafe fn undo_selection(hwnd: isize, focus_generation: u64) {
    if !same_context(hwnd, focus_generation) {
        return;
    }
    if !release_modifiers() || !same_context(hwnd, focus_generation) {
        return;
    }
    if send_chord(VK_Z.0) {
        toast::show("Undo");
    } else {
        toast::show("RightType: selection undo failed");
    }
}

unsafe fn same_context(hwnd: isize, focus_generation: u64) -> bool {
    let fg = GetForegroundWindow().0 as isize;
    let gen = focus::generation();
    let ok = fg == hwnd && gen == focus_generation;
    if !ok {
        crate::hook::e2e_trace(format!(
            "selection: context lost (hwnd {hwnd:#x} -> {fg:#x}, generation {focus_generation} -> {gen})"
        ));
    }
    ok
}

/// Inject Ctrl+`vk` (down Ctrl, tap key, up Ctrl) as one tagged batch.
unsafe fn send_chord(vk: u16) -> bool {
    let inputs = [
        key(VK_CONTROL.0, false),
        key(vk, false),
        key(vk, true),
        key(VK_CONTROL.0, true),
    ];
    INJECTING.store(true, std::sync::atomic::Ordering::SeqCst);
    let sent = SendInput(&inputs, size_of::<INPUT>() as i32);
    INJECTING.store(false, std::sync::atomic::Ordering::SeqCst);
    sent as usize == inputs.len()
}

unsafe fn release_modifiers() -> bool {
    let ups: Vec<INPUT> = held_modifiers()
        .into_iter()
        .map(|vk| key(vk, true))
        .collect();
    if !ups.is_empty() {
        return SendInput(&ups, size_of::<INPUT>() as i32) as usize == ups.len();
    }
    true
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
