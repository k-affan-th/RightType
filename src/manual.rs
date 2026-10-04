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
    /// Run a macro's steps in `hwnd` (a snippet with steps; 2.4).
    RunMacro {
        hwnd: isize,
        steps: Vec<righttype::macros::Step>,
        /// Wait for the list at the cursor to close first.
        wait: bool,
        requested_at: Instant,
    },
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

/// Ask the worker to run a macro's steps in `hwnd`.
pub fn request_macro(hwnd: isize, steps: Vec<righttype::macros::Step>, wait: bool) {
    if let Some(tx) = SENDER.get() {
        let _ = tx.try_send(Command::RunMacro {
            hwnd,
            steps,
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
    // A standard text box is read itself: its positions are what it is told.
    let text_box = focus::TextBox::focused().ok();
    let text = match &text_box {
        Some(tb) => tb.text(),
        None => focus::field_text(MAX_FIELD_CHARS),
    };
    let Some(text) = text else {
        overlay::show(tr(T::ErrSelectionNotShared));
        return;
    };
    let mut repaired =
        righttype::repair::repair(&text, righttype::dict::english(), righttype::dict::thai());
    let mut now = pairs_of(&repaired);
    if now != pending.pairs {
        crate::hook::trace_note("fix field: the text changed since it was checked");
        overlay::show(tr(T::ToastFieldChanged));
    } else {
        // Only the words picked, each found by its text and typed over on
        // its own, from the last one back so the places of the others stay
        // put. Never the whole field: retyping it all loses an editor's
        // formatting and lets its AutoFormat rewrite every quote (Word).
        let places = repaired.places_in(&text);
        let starts = repaired.starts_in(&text);
        let mut fixed = 0usize;
        for (i, change) in repaired.changes.iter().enumerate().rev() {
            if !keep.get(i).copied().unwrap_or(false) {
                continue;
            }
            if !same_context(hwnd, generation) {
                break;
            }
            let done = match &text_box {
                // A standard text box is told to replace the word itself,
                // where it is (UTF-16 positions), once it still holds it.
                Some(tb) => {
                    let from: usize = text.chars().take(starts[i]).map(char::len_utf16).sum();
                    let to = from + change.original.encode_utf16().count();
                    tb.replace_range(from, to, &change.original, &change.fixed)
                }
                // Elsewhere, found by its text and selected; typed over only
                // once the app says the selection is that word (a browser
                // moves it a moment later: CI typed one word at the end).
                // A browser rebuilds what it shares a moment after an edit:
                // the next word, found and selected too soon, was not (CI:
                // the selection stayed empty), so it is asked again.
                None => {
                    let selected = (0..3).any(|attempt| {
                        if attempt > 0 {
                            thread::sleep(Duration::from_millis(150));
                        }
                        focus::select_in_field(&change.original, places[i])
                            && selection_is(&change.original)
                    });
                    selected
                        && same_context(hwnd, generation)
                        && crate::inject::apply(0, &change.fixed, None)
                }
            };
            if !done {
                crate::hook::trace_note("fix field: a word could not be found to fix");
                break;
            }
            fixed += 1;
        }
        if fixed > 0 {
            crate::stats::record_manual();
            crate::hook::trace_note("fix field: words typed over one by one");
            overlay::show(&righttype::i18n::trf(
                T::ToastFixedWords,
                &[("n", &fixed.to_string())],
            ));
        } else {
            // The app cannot find its own text: the Fix text window, where
            // nothing is typed into the app.
            crate::tray::on_ui(crate::tray::UI_FIX_WINDOW);
            overlay::show(tr(T::ToastOpenedFixWindow));
        }
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

/// Whether the app's selection is `word`, asked for a short while (an app
/// may move it a moment after being told to).
fn selection_is(word: &str) -> bool {
    let until = Instant::now() + Duration::from_millis(400);
    loop {
        if focus::selected_text().is_some_and(|s| s.as_str() == word) {
            return true;
        }
        if Instant::now() >= until {
            return false;
        }
        thread::sleep(Duration::from_millis(25));
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

/// Run a macro's steps in `hwnd`, one after another. Esc, or another app
/// coming to the front, stops it; a step that cannot run stops it with a
/// note saying which. Dates are filled in as it runs; the copied text is
/// read only for a `{clipboard}` step, and wiped after.
unsafe fn run_macro(hwnd: isize, steps: Vec<righttype::macros::Step>, wait: bool) {
    use righttype::macros::Step;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
    if wait {
        thread::sleep(Duration::from_millis(200));
    }
    if GetForegroundWindow().0 as isize != hwnd || !release_modifiers() {
        return;
    }
    let pid = |h: HWND| {
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, Some(&mut pid));
        pid
    };
    let app = pid(HWND(hwnd as *mut _));
    let now = crate::hook::snippet_now();
    let press = |keys: &str| {
        righttype::shortcuts::virtual_keys(keys).is_some_and(|vks| crate::inject::press_chord(&vks))
    };
    let mut commands: Option<Vec<righttype::atcaret::Command>> = None;
    crate::hook::trace_note("macro: started");
    for step in steps {
        // A dialog the macro opened is still the app; another app is not.
        // Text is never typed into a password field (the focus may have
        // moved there).
        let types = matches!(step, Step::Text(_) | Step::Clipboard);
        if GetAsyncKeyState(VK_ESCAPE.0 as i32) < 0
            || pid(GetForegroundWindow()) != app
            || (types && (crate::safety::is_password_field() || focus::is_password_field()))
        {
            crate::hook::trace_note("macro: stopped");
            overlay::show(tr(T::ToastMacroStopped));
            return;
        }
        let done = match &step {
            Step::Text(text) => {
                let filled = zeroize::Zeroizing::new(righttype::snippets::fill(text, &now));
                crate::inject::apply(0, &filled, None)
            }
            Step::Keys(keys) => press(keys),
            Step::Wait(ms) => {
                thread::sleep(Duration::from_millis(u64::from(*ms)));
                true
            }
            Step::Command(name) => {
                let list = commands
                    .get_or_insert_with(|| crate::sheet::app_commands(GetForegroundWindow()));
                let wanted = name.trim().to_lowercase();
                let found = list
                    .iter()
                    .find(|c| c.name.to_lowercase() == wanted || c.other.to_lowercase() == wanted)
                    .or_else(|| {
                        list.iter()
                            .find(|c| c.name.to_lowercase().starts_with(&wanted))
                    })
                    .map(|c| c.keys.clone());
                match found {
                    Some(keys) => press(&keys),
                    None => {
                        overlay::show(&righttype::i18n::trf(
                            T::ToastMacroNoCommand,
                            &[("name", name)],
                        ));
                        return;
                    }
                }
            }
            Step::Style(name) => {
                // Word's Apply Styles box: its name field has the focus.
                press("Ctrl+Shift+S") && {
                    thread::sleep(Duration::from_millis(400));
                    press("Ctrl+A") && crate::inject::apply(0, name, None) && press("Enter")
                }
            }
            Step::Clipboard => match clipboard::get_text() {
                Some(mut text) => {
                    let mut lines = text.replace("\r\n", "\n");
                    text.zeroize();
                    let done = crate::inject::apply(0, &lines, None);
                    lines.zeroize();
                    done
                }
                None => true,
            },
        };
        if !done {
            overlay::show(tr(T::ErrInjectConversion));
            return;
        }
        // A moment for the app to take each step in.
        thread::sleep(Duration::from_millis(40));
    }
    crate::hook::trace_note("macro: finished");
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
            Command::RunMacro {
                hwnd,
                steps,
                wait,
                requested_at,
            } if requested_at.elapsed() <= Duration::from_secs(3) => unsafe {
                run_macro(hwnd, steps, wait)
            },
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
