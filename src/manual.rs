//! Manual hotkey actions — Windows only.
//!
//! "Convert selection" is the headline manual feature: select any text — a whole
//! sentence, with or without spaces — and one hotkey flips its layout. Unlike the
//! auto path it needs no word boundaries and no Thai segmentation, because a
//! layout flip is a reversible per-character remap of exactly the bytes selected.
//!
//! The selection is read from the app through UI Automation, never through
//! the clipboard: Windows may keep what is copied in its clipboard history,
//! sync it to the user's other devices (an Android phone), and any program
//! can watch the clipboard. Only when the app does not share its selection
//! that way, and the user has turned `selection_via_clipboard` on, does it
//! fall back to Ctrl+C: wait for the app to fill the clipboard, read it,
//! restore it. Either way the converted text is typed over the selection as
//! Unicode.
//!
//! It runs on its own thread because it is inherently asynchronous; the
//! per-keystroke hook just posts a [`Command`] and returns, so nothing
//! blocks the hot path.

use std::mem::size_of;
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};
use righttype::i18n::{tr, T};

use righttype::layout::auto_convert;
use zeroize::Zeroize;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    VIRTUAL_KEY, VK_C, VK_CONTROL, VK_Z,
};

use crate::hook::{held_modifiers, INJECTING, INJECT_TAG};
use crate::{clipboard, focus, overlay};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

/// A one-shot manual action requested by the hook.
pub enum Command {
    /// Flip the layout of the current selection.
    ConvertSelection {
        hwnd: isize,
        focus_generation: u64,
        requested_at: Instant,
    },
    /// Fix every wrong-layout word in the focused field (from the palette).
    FixField { hwnd: isize, requested_at: Instant },
    /// Rewrite the selection with `transform` (from the palette: Thai
    /// digits, letter case).
    Transform {
        hwnd: isize,
        requested_at: Instant,
        transform: fn(&str) -> String,
    },
    UndoSelection {
        hwnd: isize,
        focus_generation: u64,
        requested_at: Instant,
    },
    /// Fix the words of the review the palette showed: `keep` says which
    /// (by their place in it).
    ApplyReview {
        keep: Vec<bool>,
        requested_at: Instant,
    },
    /// Type `text` (a special character from the palette).
    TypeText {
        hwnd: isize,
        text: String,
        /// Wait for the palette to close and focus to come back first.
        wait: bool,
        requested_at: Instant,
    },
    /// Type the copied text key by key (remote desktops, VMs).
    TypeClipboard { hwnd: isize, requested_at: Instant },
}

/// The review the palette is showing: the field it is for, and each word
/// (as typed, fixed). Wiped when dropped.
struct Pending {
    hwnd: isize,
    pairs: Vec<(String, String)>,
}

impl Drop for Pending {
    fn drop(&mut self) {
        for (a, b) in &mut self.pairs {
            a.zeroize();
            b.zeroize();
        }
    }
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);

static SENDER: OnceLock<Sender<Command>> = OnceLock::new();
/// See [`set_clipboard_fallback`].
static CLIPBOARD_FALLBACK: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Let "convert selection" copy the selection (Ctrl+C) in apps that do not
/// share it through UI Automation. Off by default; see the module docs.
pub fn set_clipboard_fallback(on: bool) {
    CLIPBOARD_FALLBACK.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn clipboard_fallback() -> bool {
    CLIPBOARD_FALLBACK.load(std::sync::atomic::Ordering::Relaxed)
}
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

/// Ask the worker to fix the whole focused field of `hwnd` (the palette's
/// "Fix this field"). Non-blocking.
pub fn request_fix_field(hwnd: isize) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::FixField {
            hwnd,
            requested_at: Instant::now(),
        });
    }
}

/// Ask the worker to fix the reviewed words `keep` says yes to.
pub fn request_apply_review(keep: Vec<bool>) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::ApplyReview {
            keep,
            requested_at: Instant::now(),
        });
    }
}

/// Ask the worker to type `text` into `hwnd` (once the palette has closed
/// and focus is back, when `wait`).
pub fn request_type(hwnd: isize, text: String, wait: bool) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::TypeText {
            hwnd,
            text,
            wait,
            requested_at: Instant::now(),
        });
    }
}

/// Ask the worker to type the copied text into `hwnd`, key by key.
pub fn request_type_clipboard(hwnd: isize) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::TypeClipboard {
            hwnd,
            requested_at: Instant::now(),
        });
    }
}

/// Ask the worker to rewrite the selection of `hwnd` with `transform`.
pub fn request_transform(hwnd: isize, transform: fn(&str) -> String) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::Transform {
            hwnd,
            requested_at: Instant::now(),
            transform,
        });
    }
}

/// Longest field "Fix this field" retypes (characters).
const MAX_FIELD_CHARS: usize = 4000;

/// More words than this are fixed at once rather than listed to check.
const MAX_REVIEW: usize = 12;

/// Read the whole field from the app (UI Automation or the text box itself
/// — never the clipboard, and without selecting it), find the words in the
/// wrong layout (as the Fix text window does), and show them in the palette,
/// ticked and tinted in the field, to fix with Enter ([`apply_review`]).
/// Many words are fixed at once instead. An app that does not share its
/// text gets the Fix text window.
unsafe fn fix_field(hwnd: isize) {
    // The palette has just closed: let focus land back in the field.
    thread::sleep(Duration::from_millis(200));
    if GetForegroundWindow().0 as isize != hwnd {
        return;
    }
    let Some(text) = focus::field_text(MAX_FIELD_CHARS) else {
        crate::hook::trace_note("fix field: the app does not share its text");
        // Google Docs shares it once its screen-reader support is on.
        if righttype::compat::is_google_docs(&window_title(hwnd)) {
            overlay::show(tr(T::ToastGoogleDocsTip));
            return;
        }
        crate::tray::on_ui(crate::tray::UI_FIX_WINDOW);
        overlay::show(tr(T::ToastOpenedFixWindow));
        return;
    };
    if text.chars().count() > MAX_FIELD_CHARS {
        overlay::show(tr(T::ErrFieldTooLong));
        return;
    }
    let mut repaired =
        righttype::repair::repair(&text, righttype::dict::english(), righttype::dict::thai());
    if repaired.changes.is_empty() {
        overlay::show(tr(T::ToastNothingToFix));
    } else if repaired.changes.len() > MAX_REVIEW {
        let pairs = pairs_of(&repaired);
        *PENDING.lock().unwrap() = Some(Pending { hwnd, pairs });
        apply_review(vec![true; repaired.changes.len()]);
    } else {
        // Where each word is in the field as typed (the changes count in
        // the fixed text).
        let mut shift: isize = 0;
        let spans: Vec<(usize, usize)> = repaired
            .changes
            .iter()
            .map(|c| {
                let len = c.original.chars().count();
                let at = (c.start as isize - shift).max(0) as usize;
                shift += c.fixed.chars().count() as isize - len as isize;
                (at, len)
            })
            .collect();
        let boxes = focus::field_boxes(&spans);
        let pairs = pairs_of(&repaired);
        *PENDING.lock().unwrap() = Some(Pending {
            hwnd,
            pairs: pairs.clone(),
        });
        crate::palette::request_review(crate::palette::Review {
            changes: pairs,
            boxes,
        });
    }
    repaired.text.zeroize();
    for c in &mut repaired.changes {
        c.original.zeroize();
        c.fixed.zeroize();
    }
}

/// A window's title (only looked at, never kept).
fn window_title(hwnd: isize) -> String {
    use windows::Win32::UI::WindowsAndMessaging::GetWindowTextW;
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(HWND(hwnd as *mut _), &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

/// Each change as (typed, fixed).
fn pairs_of(r: &righttype::repair::Repaired) -> Vec<(String, String)> {
    r.changes
        .iter()
        .map(|c| (c.original.clone(), c.fixed.clone()))
        .collect()
}

/// Fix the reviewed words `keep` says yes to: select the whole field, read
/// it again, and — only if it still has the same words to fix — type it
/// back with those words fixed. Ctrl+Z in the app undoes it, like a
/// selection conversion.
unsafe fn apply_review(keep: Vec<bool>) {
    let Some(pending) = PENDING.lock().unwrap().take() else {
        return;
    };
    let hwnd = pending.hwnd;
    let n = keep.iter().filter(|&&k| k).count();
    if n == 0 {
        return;
    }
    // The palette has just closed: let focus land back in the field.
    thread::sleep(Duration::from_millis(200));
    if GetForegroundWindow().0 as isize != hwnd || !release_modifiers() {
        return;
    }
    let generation = focus::generation();
    if !send_chord(0x41) {
        return;
    }
    thread::sleep(Duration::from_millis(120));
    if !same_context(hwnd, generation) {
        return;
    }
    let Some(text) = focus::selected_text() else {
        overlay::show(tr(T::ErrSelectionNotShared));
        return;
    };
    let mut repaired =
        righttype::repair::repair(&text, righttype::dict::english(), righttype::dict::thai());
    let mut now = pairs_of(&repaired);
    if now != pending.pairs {
        crate::hook::trace_note("fix field: the text changed since it was checked");
        overlay::show(tr(T::ToastFieldChanged));
    } else if same_context(hwnd, generation) {
        let mut out = repaired.with_only(&keep);
        type_over_selection(hwnd, generation, &out);
        out.zeroize();
        overlay::show(&righttype::i18n::trf(
            T::ToastFixedWords,
            &[("n", &n.to_string())],
        ));
    }
    for (a, b) in &mut now {
        a.zeroize();
        b.zeroize();
    }
    repaired.text.zeroize();
    for c in &mut repaired.changes {
        c.original.zeroize();
        c.fixed.zeroize();
    }
}

/// Type `text` into `hwnd` (once the palette has closed and focus is back,
/// when `wait`).
unsafe fn type_text(hwnd: isize, text: &str, wait: bool) {
    if wait {
        thread::sleep(Duration::from_millis(200));
    }
    if GetForegroundWindow().0 as isize != hwnd || !release_modifiers() {
        return;
    }
    if !crate::inject::apply(0, text, None) {
        overlay::show(tr(T::ErrInjectConversion));
    }
}

/// Longest copied text typed key by key (characters).
const MAX_TYPED_CHARS: usize = 2000;

/// Type the copied text into `hwnd` as key presses, for places that take
/// no paste and no Unicode input: remote desktops, virtual machine
/// consoles, some web forms. Each character is the key that types it on
/// the keyboard layout in use (with Shift if needed); a character that
/// layout has no key for goes as Unicode. Esc, or focus moving away,
/// stops it. The clipboard is read only now, because it was asked for,
/// and the text is wiped after.
unsafe fn type_clipboard(hwnd: isize) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetKeyboardLayout, MapVirtualKeyExW, VkKeyScanExW, MAPVK_VK_TO_VSC,
        VK_ESCAPE, VK_RETURN, VK_SHIFT, VK_TAB,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    thread::sleep(Duration::from_millis(200));
    if GetForegroundWindow().0 as isize != hwnd || !release_modifiers() {
        return;
    }
    let Some(mut text) = clipboard::get_text().filter(|t| !t.is_empty()) else {
        overlay::show(tr(T::ErrClipboardEmpty));
        return;
    };
    let mut chars: Vec<char> = text.replace("\r\n", "\n").chars().collect();
    text.zeroize();
    if chars.len() > MAX_TYPED_CHARS {
        chars.iter_mut().for_each(|c| *c = '\0');
        overlay::show(&righttype::i18n::trf(
            T::ErrClipboardTooLong,
            &[("n", &MAX_TYPED_CHARS.to_string())],
        ));
        return;
    }
    overlay::show(&righttype::i18n::trf(
        T::ToastTyping,
        &[("n", &chars.len().to_string())],
    ));
    let hkl = GetKeyboardLayout(GetWindowThreadProcessId(HWND(hwnd as *mut _), None));
    let tap = |vk: u16, shift: bool| {
        let scan = MapVirtualKeyExW(vk as u32, MAPVK_VK_TO_VSC, hkl) as u16;
        let mut inputs = Vec::with_capacity(4);
        if shift {
            inputs.push(key(VK_SHIFT.0, false));
        }
        let mut down = key(vk, false);
        down.Anonymous.ki.wScan = scan;
        let mut up = key(vk, true);
        up.Anonymous.ki.wScan = scan;
        inputs.push(down);
        inputs.push(up);
        if shift {
            inputs.push(key(VK_SHIFT.0, true));
        }
        SendInput(&inputs, size_of::<INPUT>() as i32) as usize == inputs.len()
    };
    let mut stopped = false;
    for &c in &chars {
        if GetAsyncKeyState(VK_ESCAPE.0 as i32) < 0 || GetForegroundWindow().0 as isize != hwnd {
            stopped = true;
            break;
        }
        let ok = match c {
            '\n' | '\r' => tap(VK_RETURN.0, false),
            '\t' => tap(VK_TAB.0, false),
            _ => {
                let scan = VkKeyScanExW(c as u16 as _, hkl);
                // Low byte: the key; high byte: 1 Shift, 2 Ctrl, 4 Alt.
                let (vk, mods) = ((scan & 0xFF) as u16, (scan >> 8) & 0xFF);
                if c as u32 <= 0xFFFF && scan != -1 && mods & !1 == 0 {
                    tap(vk, mods & 1 != 0)
                } else {
                    let mut s = [0u8; 4];
                    crate::inject::apply(0, c.encode_utf8(&mut s), None)
                }
            }
        };
        if !ok {
            stopped = true;
            break;
        }
        // Slow enough for a remote or virtual machine to keep up.
        thread::sleep(Duration::from_millis(8));
    }
    chars.iter_mut().for_each(|c| *c = '\0');
    if stopped {
        overlay::show(tr(T::ToastTypingStopped));
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
                convert_selection(hwnd, focus_generation, auto_convert)
            },
            Command::UndoSelection {
                hwnd,
                focus_generation,
                requested_at,
            } if requested_at.elapsed() <= MAX_COMMAND_AGE => unsafe {
                undo_selection(hwnd, focus_generation)
            },
            Command::FixField { hwnd, requested_at }
                if requested_at.elapsed() <= Duration::from_secs(3) =>
            unsafe { fix_field(hwnd) },
            Command::ApplyReview { keep, requested_at }
                if requested_at.elapsed() <= Duration::from_secs(3) =>
            unsafe { apply_review(keep) },
            Command::TypeText {
                hwnd,
                text,
                wait,
                requested_at,
            } if requested_at.elapsed() <= Duration::from_secs(3) => unsafe {
                type_text(hwnd, &text, wait)
            },
            Command::TypeClipboard { hwnd, requested_at }
                if requested_at.elapsed() <= Duration::from_secs(3) =>
            unsafe { type_clipboard(hwnd) },
            Command::Transform {
                hwnd,
                requested_at,
                transform,
            } if requested_at.elapsed() <= Duration::from_secs(3) => unsafe {
                // The palette has just closed: let focus land back first.
                thread::sleep(Duration::from_millis(200));
                if GetForegroundWindow().0 as isize == hwnd {
                    convert_selection(hwnd, focus::generation(), transform)
                }
            },
            _ => {}
        }
    }
}

unsafe fn convert_selection(hwnd: isize, focus_generation: u64, convert: fn(&str) -> String) {
    if !same_context(hwnd, focus_generation) {
        return;
    }

    if let Some(selection) = focus::selected_text() {
        crate::hook::trace_note("selection: read through UI Automation");
        let mut converted = convert(&selection);
        if converted == *selection {
            crate::hook::trace_note("selection: conversion was a no-op");
            overlay::show(tr(T::ToastNothingToChange));
        } else if !release_modifiers() {
            crate::hook::trace_note("selection: could not release held modifiers");
            overlay::show(tr(T::ErrModifiers));
        } else if same_context(hwnd, focus_generation) {
            type_over_selection(hwnd, focus_generation, &converted);
        }
        converted.zeroize();
        return;
    }
    if !clipboard_fallback() {
        crate::hook::trace_note("selection: not shared by the app; clipboard not used");
        overlay::show(tr(T::ErrSelectionNotShared));
        return;
    }

    let original = match clipboard::snapshot_plain_text() {
        Ok(snapshot) => snapshot,
        Err(clipboard::SnapshotError::Busy) => {
            crate::hook::trace_note("selection: clipboard is busy");
            overlay::show(tr(T::ErrClipboardBusy));
            return;
        }
        Err(clipboard::SnapshotError::NotPlainText) => {
            crate::hook::trace_note("selection: selection conversion needs a plain-text clipboard");
            overlay::show(tr(T::ErrClipboardNotPlain));
            return;
        }
    };

    // The hotkey chord (Shift+CapsLock) may still be physically held; release any
    // modifiers so the injected Ctrl+C/V isn't polluted by them.
    if !release_modifiers() {
        crate::hook::trace_note("selection: could not release held modifiers");
        overlay::show(tr(T::ErrModifiers));
        return;
    }
    if !same_context(hwnd, focus_generation) {
        return;
    }

    let before = clipboard::sequence();
    if !send_chord(VK_C.0) {
        let _ = clipboard::restore_snapshot(&original);
        crate::hook::trace_note("selection: could not copy the selection");
        overlay::show(tr(T::ErrCopy));
        return;
    }
    // Wait for the focused app to answer the copy.
    let deadline = Instant::now() + Duration::from_millis(600);
    while clipboard::sequence() == before && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(8));
    }

    if clipboard::sequence() == before {
        let _ = clipboard::restore_snapshot(&original);
        crate::hook::trace_note("selection: no text selection was copied");
        overlay::show(tr(T::ErrNothingCopied));
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
        crate::hook::trace_note("selection: selection is not Unicode text");
        overlay::show(tr(T::ErrNotUnicode));
        return;
    };
    if selection.is_empty() {
        crate::hook::trace_note("selection: copied selection was empty");
        let _ = clipboard::restore_snapshot(&original);
        return;
    }

    let mut converted = convert(&selection);
    if converted == selection {
        crate::hook::trace_note("selection: conversion was a no-op");
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
        crate::hook::trace_note("selection: could not restore the clipboard");
        overlay::show(tr(T::ErrRestoreClipboard));
        selection.zeroize();
        converted.zeroize();
        return;
    }
    if !same_context(hwnd, focus_generation) {
        selection.zeroize();
        converted.zeroize();
        return;
    }
    type_over_selection(hwnd, focus_generation, &converted);
    selection.zeroize();
    converted.zeroize();
}

/// Type `converted` over the active selection, which replaces it, and keep
/// the app's own Undo (Ctrl+Z) for it.
unsafe fn type_over_selection(hwnd: isize, focus_generation: u64, converted: &str) {
    if !crate::inject::apply(0, converted, None) {
        overlay::show(tr(T::ErrInjectConversion));
        return;
    }
    crate::stats::record_manual();
    *SELECTION_UNDO.lock().unwrap() = Some(SelectionUndo {
        hwnd,
        focus_generation,
        completed_at: Instant::now(),
    });
}

unsafe fn undo_selection(hwnd: isize, focus_generation: u64) {
    if !same_context(hwnd, focus_generation) {
        return;
    }
    if !release_modifiers() || !same_context(hwnd, focus_generation) {
        return;
    }
    if send_chord(VK_Z.0) {
        overlay::show(tr(T::ToastUndo));
    } else {
        overlay::show(tr(T::ErrSelectionUndo));
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

/// Make sure no modifier is down before this worker injects a chord.
///
/// The hotkey that asked for this work (Ctrl+Shift+CapsLock, Shift+CapsLock)
/// is usually still being released as the worker starts. Injecting a fake
/// key-up and then our own Ctrl+Z races the typist's real key-ups: a physical
/// Ctrl-up landing between our Ctrl-down and Z turned Undo into a typed `z`,
/// and a still-held Shift turns Ctrl+Z into Ctrl+Shift+Z (redo). So first wait
/// — this is the worker thread, not the hook — for the hands to come off, and
/// only then release whatever is still held.
unsafe fn release_modifiers() -> bool {
    let deadline = Instant::now() + Duration::from_millis(1500);
    while !held_modifiers().is_empty() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    let mut ups: Vec<INPUT> = Vec::new();
    crate::inject::release_held(&mut ups);
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
