//! Low-level keyboard hook (`WH_KEYBOARD_LL`) — Windows only.
//!
//! This is the whole input→correct pipeline, run **synchronously on the hook
//! thread**. Keys pass through normally while we accumulate the current word; in
//! Auto mode a boundary that completes a wrong-layout word is **swallowed** and we
//! inject `backspaces + correction + boundary` in one atomic batch. Doing it
//! synchronously and swallowing the trigger makes the replacement race-free: the
//! mistyped letters are already in the target app before we inject, and because
//! every keystroke is serialised through this one thread, nothing can interleave —
//! even under fast typing. (An earlier async-worker design raced and garbled.)
//!
//! Modifier state (Shift/Ctrl/Alt/Caps) is read live from `GetAsyncKeyState` /
//! `GetKeyState` rather than tracked from the event stream. Tracking it ourselves
//! let a modifier *stick* whenever a key-up was missed — e.g. the Alt+Shift used to
//! switch keyboard layout — which then captured every following letter as if Shift
//! were held. Reading the real key state each time makes sticking impossible.
//!
//! The work here is tiny (a `ToUnicodeEx` call + two hashed dictionary lookups +
//! a small `SendInput`), so the callback stays far under the `LowLevelHooksTimeout`
//! that evicts slow hooks — see `docs/PLAN.md`, Bug 1. We also ignore our own
//! injected events (tagged in `dwExtraInfo`, plus `LLKHF_INJECTED`) so a
//! correction can never feed back into itself.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use zeroize::Zeroize;

use righttype::buffer::{Key, WordBuffer};
use righttype::layout::{auto_convert, en_to_th, th_to_en};
use righttype::{detect, dict, secret, segment};

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, GetKeyboardLayoutList, ToUnicodeEx, HKL,
    VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME,
    VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE, VK_TAB,
    VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_INJECTED, WH_KEYBOARD_LL,
    WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::{inject, manual, safety};

/// Magic value stamped into `dwExtraInfo` on every event the injector sends, so
/// the hook can recognise and skip our own input with zero timing dependency. The
/// flag travels *with* the event, unlike a shared "injecting" boolean. ("RTYP")
pub const INJECT_TAG: usize = 0x5254_5950;

/// Set while we inject, as a secondary guard. The tag above is authoritative.
pub static INJECTING: AtomicBool = AtomicBool::new(false);

/// Correction mode: `false` = Manual (hotkeys only — the default, which avoids the
/// awkward press-space-to-convert workflow), `true` = Auto (correct on a boundary).
/// Toggled with Ctrl+CapsLock. Real-time Thai-segmenting auto is a later step.
static MODE_AUTO: AtomicBool = AtomicBool::new(false);

/// Master on/off, controlled from the tray. When off the hook passes every key
/// straight through and touches nothing.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// Is RightType currently enabled?
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Enable or disable all correction.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Is Auto mode on (vs Manual)?
pub fn is_auto() -> bool {
    MODE_AUTO.load(Ordering::Relaxed)
}

/// Select Auto (`true`) or Manual (`false`) mode.
pub fn set_auto(auto: bool) {
    MODE_AUTO.store(auto, Ordering::Relaxed);
}

thread_local! {
    /// Per-thread pipeline state. The hook callback always runs on the installing
    /// thread, so this persists across callbacks without locking.
    static STATE: RefCell<HookState> = RefCell::new(HookState::new());
}

struct HookState {
    buf: WordBuffer,
    seed: secret::SeedTracker,
    /// Foreground window + keyboard layout the buffer belongs to. When either
    /// changes — Alt-Tab, a click into another app, or a Thai/Eng layout switch —
    /// the buffered word no longer matches what's in front of the caret, so we
    /// drop it. This keeps the buffer honest across exactly the cases the user
    /// hit (switching layout mid-sentence left stale context behind).
    last_hwnd: isize,
    last_hkl: isize,
    /// Whether the current foreground app is blacklisted (wallet / password
    /// manager / terminal). Recomputed only when the window changes — opening the
    /// process every keystroke would be wasteful.
    sensitive_app: bool,
    /// The most recent correction, kept for one-shot Undo (Ctrl+Shift+CapsLock).
    /// Cleared after use and whenever focus/layout changes (an undo that retypes
    /// into a different window/context than the one it corrected would be wrong).
    undo: Option<UndoRecord>,
    /// The layout we requested to switch to, if we are waiting for the OS to complete it.
    pending_hkl: Option<isize>,
    /// The last completed word and the boundary key code that completed it.
    /// Used for manual Shift+Backspace correction immediately after a boundary.
    last_completed: Option<(String, u16)>,
}

impl HookState {
    fn new() -> Self {
        Self {
            buf: WordBuffer::new(),
            seed: secret::SeedTracker::new(),
            last_hwnd: 0,
            last_hkl: 0,
            sensitive_app: false,
            undo: None,
            pending_hkl: None,
            last_completed: None,
        }
    }
}

/// A reversible correction: how many characters to delete, and what to retype
/// to restore the pre-correction text exactly.
struct UndoRecord {
    /// Characters now present in the app (after the correction) to delete.
    injected_len: usize,
    /// Text to retype to restore what was there before.
    restore_text: String,
}

impl Drop for UndoRecord {
    fn drop(&mut self) {
        self.restore_text.zeroize();
    }
}

/// Record `restore_text` (what to retype) as the one-shot Undo target for the
/// correction that just replaced it with `injected_len` characters. Refuses
/// secret-shaped text — `detect::detect` already guards the automatic paths, but
/// the manual convert-word hotkey doesn't, so this is the one place that matters.
fn set_undo(injected_len: usize, restore_text: &str) {
    let record = (!secret::is_secret_token(restore_text)).then(|| UndoRecord {
        injected_len,
        restore_text: restore_text.to_string(),
    });
    STATE.with(|s| s.borrow_mut().undo = record);
}

/// Ctrl+Shift+CapsLock: revert the most recent correction, if any. One-shot —
/// the record is consumed whether or not this call finds one.
unsafe fn undo_last_correction() {
    let Some(rec) = STATE.with(|s| s.borrow_mut().undo.take()) else {
        return;
    };
    inject::apply(rec.injected_len, &rec.restore_text, None);
    crate::toast::show("Undo");
}

/// The literal character a boundary key inserts (matches what a real keypress
/// would produce as `WM_CHAR`), for reconstructing exact Undo text.
fn boundary_literal(vk: u16) -> char {
    if vk == VK_RETURN.0 {
        '\r'
    } else if vk == VK_TAB.0 {
        '\t'
    } else {
        ' '
    }
}

/// The installed hook handle, kept only so [`uninstall`] can remove it. The raw
/// handle is not `Send`; this wrapper asserts it is safe to move between threads
/// (we only ever touch it from install/uninstall, never concurrently).
struct HookHandle(HHOOK);
unsafe impl Send for HookHandle {}
static HOOK: Mutex<Option<HookHandle>> = Mutex::new(None);

/// Is `vk` physically held right now? Read from the real async key state so it
/// can never go stale (the reason we don't track modifiers from the event stream).
unsafe fn is_down(vk: VIRTUAL_KEY) -> bool {
    (GetAsyncKeyState(vk.0 as i32) as u16 & 0x8000) != 0
}

/// Is CapsLock currently toggled on?
unsafe fn caps_on() -> bool {
    (GetKeyState(VK_CAPITAL.0 as i32) & 0x0001) != 0
}

/// The modifier virtual-keys currently held down, for the injector to release
/// before a correction (the Bug 2 fix).
pub fn held_modifiers() -> Vec<u16> {
    let mut v = Vec::new();
    unsafe {
        if is_down(VK_SHIFT) {
            v.push(VK_SHIFT.0);
        }
        if is_down(VK_CONTROL) {
            v.push(VK_CONTROL.0);
        }
        if is_down(VK_MENU) {
            v.push(VK_MENU.0);
        }
    }
    v
}

/// Install the low-level keyboard hook for this thread.
///
/// # Safety
/// The calling thread must run a message loop for the duration of the hook, and
/// must call [`uninstall`] before exiting.
pub unsafe fn install() -> windows::core::Result<()> {
    let hmod = GetModuleHandleW(None)?;
    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_proc), HINSTANCE(hmod.0), 0)?;
    *HOOK.lock().unwrap() = Some(HookHandle(hook));
    // RAM hardening: pin the word buffer's (already-stable) allocation in
    // physical RAM so a typed secret can never be paged to disk. Locking it
    // here, once, is safe precisely because `WordBuffer` pre-reserves its
    // capacity and never reallocates for its lifetime (see `stable_region`).
    STATE.with(|s| {
        let (ptr, len) = s.borrow().buf.stable_region();
        crate::ram::lock_region(ptr, len);
    });
    Ok(())
}

/// Remove the hook if installed.
///
/// # Safety
/// Must be called on the same thread that called [`install`].
pub unsafe fn uninstall() {
    if let Some(h) = HOOK.lock().unwrap().take() {
        let _ = UnhookWindowsHookEx(h.0);
    }
}

/// Tear down and re-establish the hook — the recovery action after a power/session
/// transition that may have silently evicted it (RightLang Bug 1).
///
/// # Safety
/// Same thread + message-loop requirements as [`install`].
pub unsafe fn reinstall() -> windows::core::Result<()> {
    uninstall();
    install()
}

unsafe extern "system" fn ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        // Skip anything we generated: our tag is authoritative and timing-free.
        let ours = kb.dwExtraInfo == INJECT_TAG
            || (kb.flags.0 & LLKHF_INJECTED.0) != 0
            || INJECTING.load(Ordering::Relaxed);
        if !ours && process(wparam.0 as u32, kb) {
            // We handled this key as a hotkey/correction; swallow it.
            return LRESULT(1);
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

fn is_modifier(vk: u16) -> bool {
    matches!(
        vk,
        v if v == VK_SHIFT.0
            || v == 0xA0 // VK_LSHIFT
            || v == 0xA1 // VK_RSHIFT
            || v == VK_CONTROL.0
            || v == 0xA2 // VK_LCONTROL
            || v == 0xA3 // VK_RCONTROL
            || v == VK_MENU.0
            || v == 0xA4 // VK_LMENU
            || v == 0xA5 // VK_RMENU
            || v == VK_CAPITAL.0
    )
}

unsafe fn is_layout_switch_trigger(vk: u16) -> bool {
    // 1. Grave Accent (VK_OEM_3 = 0xC0)
    if vk == 0xC0 {
        return true;
    }
    // 2. Win + Space
    if vk == VK_SPACE.0 && (is_down(VIRTUAL_KEY(0x5B)) || is_down(VIRTUAL_KEY(0x5C))) { // VK_LWIN = 0x5B, VK_RWIN = 0x5C
        return true;
    }
    // 3. Alt + Shift / Ctrl + Shift (when one of them is pressed while the other modifier is held)
    let is_shift = vk == VK_SHIFT.0 || vk == 0xA0 || vk == 0xA1;
    let is_menu = vk == VK_MENU.0 || vk == 0xA4 || vk == 0xA5;
    let is_ctrl = vk == VK_CONTROL.0 || vk == 0xA2 || vk == 0xA3;
    
    if (is_shift && (is_down(VK_MENU) || is_down(VK_CONTROL)))
        || (is_menu && is_down(VK_SHIFT))
        || (is_ctrl && is_down(VK_SHIFT))
    {
        return true;
    }
    
    false
}

/// Process one event. Returns `true` to swallow the current key (we handled a
/// hotkey or corrected a word and re-injected its boundary), `false` to let it
/// pass through normally.
unsafe fn process(msg: u32, kb: &KBDLLHOOKSTRUCT) -> bool {
    let vk = kb.vkCode as u16;
    let down = msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN;
    if !down {
        return false;
    }

    // Shift+Backspace hotkey.
    let is_shift_backspace = vk == VK_BACK.0 && is_down(VK_SHIFT) && !is_down(VK_CONTROL) && !is_down(VK_MENU);
    if !is_shift_backspace && !is_modifier(vk) {
        STATE.with(|s| s.borrow_mut().last_completed = None);
    }

    // Panic switch: Ctrl+Alt+CapsLock instantly flips master enable, either way.
    // Checked before the enabled gate and the sensitive-context guard below so
    // it always works — including turning back ON, and even from inside a
    // password field or blacklisted app.
    if vk == VK_CAPITAL.0 && is_down(VK_CONTROL) && is_down(VK_MENU) {
        let now_on = !ENABLED.fetch_xor(true, Ordering::Relaxed);
        crate::toast::show(if now_on { "RightType: ON" } else { "RightType: OFF" });
        crate::config::persist();
        return true;
    }

    // Master switch: when disabled, pass everything through untouched.
    if !ENABLED.load(Ordering::Relaxed) {
        return false;
    }

    // User initiated manual layout switch (Grave accent, Alt+Shift, Ctrl+Shift, Win+Space)
    if is_layout_switch_trigger(vk) {
        let current_is_thai = foreground_is_thai();
        let target_primary = if current_is_thai { PRIMARYLANG_EN } else { PRIMARYLANG_THAI };
        let count = GetKeyboardLayoutList(None);
        if count > 0 {
            let mut list = vec![HKL::default(); count as usize];
            let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
            for hkl in list.iter().take(got) {
                if ((hkl.0 as usize & 0x3FF) as u16) == target_primary {
                    STATE.with(|s| {
                        let mut st = s.borrow_mut();
                        st.pending_hkl = Some(hkl.0 as isize);
                        st.buf.clear();
                        st.last_completed = None;
                    });
                    break;
                }
            }
        }
    }

    // If focus or layout changed since the last key, the buffered word is stale.
    sync_context();

    // Never run where secrets are typed: blacklisted apps, or password fields
    // (native ES_PASSWORD, or UIA-detected ones in browsers/Electron/UWP).
    if STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_password_field()
        || crate::focus::is_password_field()
    {
        return false;
    }

    // CapsLock: a hotkey carrier when chorded, otherwise a normal toggle.
    if vk == VK_CAPITAL.0 {
        let ctrl = is_down(VK_CONTROL);
        let shift = is_down(VK_SHIFT);
        if ctrl && shift {
            // Ctrl+Shift+CapsLock: undo the last correction (one-shot). Swallow.
            undo_last_correction();
            return true;
        }
        if ctrl {
            // Ctrl+CapsLock: toggle Auto/Manual. Swallow so Caps never flips.
            let now_auto = !MODE_AUTO.fetch_xor(true, Ordering::Relaxed);
            crate::toast::show(if now_auto { "Auto mode" } else { "Manual mode" });
            crate::config::persist();
            return true;
        }
        if shift {
            // Shift+CapsLock: convert the current selection. Swallow.
            manual::request_convert_selection();
            return true;
        }
        return false;
    }

    // Shift+Backspace: flip the current word in place. Always swallowed — even
    // when there's nothing to convert — so the key's auto-repeat can't fall
    // through to a destructive Backspace and delete the result we just injected.
    if vk == VK_BACK.0 && is_down(VK_SHIFT) && !is_down(VK_CONTROL) && !is_down(VK_MENU) {
        convert_last_word();
        return true;
    }

    let Some(key) = classify(vk, kb.scanCode as u16) else {
        return false;
    };

    // Drive the buffer; only a boundary can return a completed word.
    let completed = STATE.with(|s| s.borrow_mut().buf.observe(key));
    let Some(mut word) = completed else {
        // No boundary yet. In Auto mode, eagerly convert a complete wrong-layout
        // Thai word the moment it's recognised — this is the no-space case that a
        // boundary trigger can't handle.
        if MODE_AUTO.load(Ordering::Relaxed) && matches!(key, Key::Char(_)) {
            return auto_convert_live();
        }
        return false;
    };

    // Auto-learn this completed word (no-op unless the user enabled learning).
    crate::learn::observe(&word);

    // Auto mode corrects on the boundary (the EN-on-Thai-layout direction, which
    // does have spaces); Manual mode waits for a hotkey.
    let swallow = if MODE_AUTO.load(Ordering::Relaxed) {
        maybe_correct(&word, vk)
    } else {
        false
    };
    if swallow {
        STATE.with(|s| s.borrow_mut().last_completed = None);
    } else {
        STATE.with(|s| s.borrow_mut().last_completed = Some((word.clone(), vk)));
    }
    word.zeroize();
    swallow
}

/// Auto mode, no-space (run-on) conversion, in whichever direction matches the
/// active layout. Returns `true` to swallow the keystroke that completed the word
/// — its character belongs to the injected text and must not also reach the app.
///
/// Only Thai-layout → EN runs eagerly without a word boundary. Thai text writes
/// words with no spaces, so we must commit at the first recognised word.
/// EN-layout → Thai is intentionally excluded from this path: English text always
/// has spaces, so the boundary path handles it safely. Running EN→Thai live caused
/// false triggers mid-word (e.g. "fu" = "ดี", "idio" = "รกรน") which disrupted
/// typing English words like "fucking" or "idiot".
unsafe fn auto_convert_live() -> bool {
    if foreground_is_thai() {
        auto_thai_layout_to_en()
    } else {
        false // EN → Thai: boundary path only, never live
    }
}

/// Typing on a non-Thai layout, producing ASCII that is really wrong-layout Thai.
unsafe fn auto_en_layout_to_thai() -> bool {
    let ascii = STATE.with(|s| s.borrow().buf.current().to_string());
    let n = ascii.chars().count();
    // ≥2 chars, pure ASCII, not a secret, not a genuine English word.
    if n < 2
        || !ascii.is_ascii()
        || secret::is_secret_token(&ascii)
        || dict::english().contains(&ascii)
    {
        return false;
    }
    let mut thai = en_to_th(&ascii);
    // Strong signal: the whole run partitions into real Thai words.
    if !segment::is_fully_known(&thai, dict::thai()) {
        thai.zeroize();
        return false;
    }
    STATE.with(|s| s.borrow_mut().buf.clear());
    inject::apply(n - 1, &thai, None);
    set_undo(thai.chars().count(), &ascii);
    crate::stats::record_auto();
    thai.zeroize();
    activate_layout(PRIMARYLANG_THAI);
    true
}

/// Typing on the Thai layout, producing Thai that is really wrong-layout English.
unsafe fn auto_thai_layout_to_en() -> bool {
    let thai = STATE.with(|s| s.borrow().buf.current().to_string());
    let n = thai.chars().count();
    // ≥4 chars: a single English-word match is a weaker signal than full Thai
    // segmentation, and short English words (the/and/in/on) would false-trigger on
    // ordinary Thai prefixes — those convert via the space-boundary path instead.
    if n < 4
        || thai.is_ascii()
        || secret::is_secret_token(&thai)
        || dict::thai().contains(&thai)
    {
        return false;
    }
    let mut eng = th_to_en(&thai);
    if !dict::english().contains(&eng) && !crate::learn::contains(&eng) {
        eng.zeroize();
        return false;
    }
    STATE.with(|s| s.borrow_mut().buf.clear());
    inject::apply(n - 1, &eng, None);
    set_undo(eng.chars().count(), &thai);
    crate::stats::record_auto();
    eng.zeroize();
    activate_layout(PRIMARYLANG_EN);
    true
}

/// PRIMARYLANGID values (the low 10 bits of a LANGID). Matching the *primary*
/// language, not the full LANGID, accepts any sub-variant the user has installed
/// (US/UK English, Thai Kedmanee/Pattachote, …).
const PRIMARYLANG_THAI: u16 = 0x1E;
const PRIMARYLANG_EN: u16 = 0x09;

/// The primary language of the foreground window's keyboard layout.
unsafe fn foreground_primary_lang() -> u16 {
    (foreground_layout().0 as usize & 0x3FF) as u16
}

/// Is the foreground window's keyboard layout Thai?
unsafe fn foreground_is_thai() -> bool {
    foreground_primary_lang() == PRIMARYLANG_THAI
}

/// Switch the foreground window's input language to the first loaded layout whose
/// primary language matches `primary`. No-op if no such layout is installed.
unsafe fn activate_layout(primary: u16) {
    if foreground_primary_lang() == primary {
        STATE.with(|s| s.borrow_mut().pending_hkl = None);
        return;
    }
    let count = GetKeyboardLayoutList(None);
    if count <= 0 {
        return;
    }
    let mut list = vec![HKL::default(); count as usize];
    let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
    for hkl in list.iter().take(got) {
        if ((hkl.0 as usize & 0x3FF) as u16) == primary {
            let _ = PostMessageW(
                GetForegroundWindow(),
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(hkl.0 as isize),
            );
            // Record that we are waiting for this HKL to activate
            STATE.with(|s| {
                s.borrow_mut().pending_hkl = Some(hkl.0 as isize);
            });
            // No toast here: a layout switch happens on every ignition, which is
            // too frequent — and Windows' own language indicator already reflects
            // it. We only toast deliberate, rare changes (mode / enabled).
            return;
        }
    }
}

/// Manual: flip the layout of the word currently in the buffer, in place. A no-op
/// when the buffer is empty (e.g. an auto-repeat after the word was already
/// converted) — the caller swallows the key either way.
unsafe fn convert_last_word() {
    let mut word = STATE.with(|s| s.borrow().buf.current().to_string());
    if word.is_empty() {
        // Try to convert the last completed word if we just hit a boundary (e.g. Space)
        let last = STATE.with(|s| s.borrow_mut().last_completed.take());
        if let Some((mut last_word, boundary_vk)) = last {
            let backspaces = last_word.chars().count() + 1; // +1 for the boundary character
            let mut converted = auto_convert(&last_word);
            let changed = converted != last_word;
            if changed {
                inject::apply(backspaces, &converted, Some(boundary_vk));
                set_undo(
                    converted.chars().count() + 1,
                    &format!("{}{}", last_word, boundary_literal(boundary_vk)),
                );
                crate::stats::record_manual();
                
                // Switch language layout to the one of the converted word
                let to_thai = converted.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
                activate_layout(if to_thai { PRIMARYLANG_THAI } else { PRIMARYLANG_EN });
            }
            converted.zeroize();
            last_word.zeroize();
        }
        return;
    }
    STATE.with(|s| {
        s.borrow_mut().buf.clear();
        s.borrow_mut().last_completed = None;
    });

    let backspaces = word.chars().count();
    let mut converted = auto_convert(&word);
    let changed = converted != word;
    if changed {
        inject::apply(backspaces, &converted, None);
        set_undo(converted.chars().count(), &word);
        crate::stats::record_manual();

        // Switch language layout to the one of the converted word
        let to_thai = converted.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
        activate_layout(if to_thai { PRIMARYLANG_THAI } else { PRIMARYLANG_EN });
    }
    word.zeroize();
    converted.zeroize();
}

/// Run the seed guard + detection on a completed `word`; inject the fix if any.
/// `boundary_vk` is the separator key that completed the word, re-emitted after
/// the correction. Returns `true` if a correction was injected (caller swallows
/// the boundary), `false` otherwise.
unsafe fn maybe_correct(word: &str, boundary_vk: u16) -> bool {
    // A run of BIP39 words is a seed phrase — never touch it.
    if STATE.with(|s| s.borrow_mut().seed.observe(word)) {
        return false;
    }
    // `detect` already refuses secret-shaped and too-short tokens internally.
    let Some(d) = detect::detect(word, dict::english(), dict::thai()) else {
        return false;
    };
    // The boundary is swallowed, so only the word's own characters are deleted.
    let backspaces = word.chars().count();
    let mut corrected = d.corrected;
    inject::apply(backspaces, &corrected, Some(boundary_vk));

    // Undo target: retype the original word plus the boundary it would have
    // gotten anyway (the boundary keystroke itself never reached the app).
    let mut restore = format!("{word}{}", boundary_literal(boundary_vk));
    set_undo(corrected.chars().count() + 1, &restore);
    crate::stats::record_auto();
    restore.zeroize();

    // Switch to whichever language we just produced, so the rest of the sentence
    // types natively — same idea as the live Thai path, now for the EN direction.
    let to_thai = corrected
        .chars()
        .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    corrected.zeroize();
    activate_layout(if to_thai {
        PRIMARYLANG_THAI
    } else {
        PRIMARYLANG_EN
    });
    true
}

/// Translate a raw key into a [`Key`] for the buffer, or `None` to ignore it.
unsafe fn classify(vk: u16, scan: u16) -> Option<Key> {
    // Any Ctrl/Alt chord is a command, not text: drop the in-progress word.
    if is_down(VK_CONTROL) || is_down(VK_MENU) {
        return Some(Key::Reset);
    }
    if vk == VK_BACK.0 {
        return Some(Key::Backspace);
    }
    if vk == VK_SPACE.0 || vk == VK_RETURN.0 || vk == VK_TAB.0 {
        return Some(Key::Boundary);
    }
    // Navigation / editing keys move the caret: the buffered word is no longer
    // contiguous with what we'd correct, so discard it.
    if matches!(
        vk,
        v if v == VK_ESCAPE.0
            || v == VK_LEFT.0
            || v == VK_RIGHT.0
            || v == VK_UP.0
            || v == VK_DOWN.0
            || v == VK_HOME.0
            || v == VK_END.0
            || v == VK_PRIOR.0
            || v == VK_NEXT.0
            || v == VK_DELETE.0
            || v == VK_INSERT.0
    ) {
        return Some(Key::Reset);
    }
    translate(vk, scan).map(Key::Char)
}

/// Reproduce the character the keystroke produced, using the foreground layout
/// and the live Shift/Caps state.
unsafe fn translate(vk: u16, scan: u16) -> Option<char> {
    let mut state = [0u8; 256];
    if is_down(VK_SHIFT) {
        state[VK_SHIFT.0 as usize] = 0x80;
    }
    if caps_on() {
        state[VK_CAPITAL.0 as usize] = 0x01;
    }

    let hkl = foreground_layout();
    let mut out = [0u16; 8];
    let n = ToUnicodeEx(vk as u32, scan as u32, &state, &mut out, 0, hkl);
    if n == 1 {
        char::from_u32(out[0] as u32).filter(|c| !c.is_control())
    } else {
        // 0 = no mapping (e.g. F-keys / modifiers), -1 = dead key, >1 = ligature.
        None
    }
}

/// Drop the buffered word if the focused window or keyboard layout changed since
/// the previous key — the buffer only describes one editing context at a time.
unsafe fn sync_context() {
    let hwnd = GetForegroundWindow();
    let tid = GetWindowThreadProcessId(hwnd, None);
    let hwnd_i = hwnd.0 as isize;

    // If we have a pending HKL switch, wait for the target application thread
    // to process the message and actually apply the layout, so the next key
    // is translated correctly under the new layout.
    let mut hkl_i = GetKeyboardLayout(tid).0 as isize;
    let pending = STATE.with(|s| s.borrow().pending_hkl);
    if let Some(target) = pending {
        if hkl_i == target {
            STATE.with(|s| s.borrow_mut().pending_hkl = None);
        } else {
            let start = std::time::Instant::now();
            let limit = std::time::Duration::from_millis(50);
            while std::time::Instant::now() - start < limit {
                std::thread::sleep(std::time::Duration::from_millis(1));
                hkl_i = GetKeyboardLayout(tid).0 as isize;
                if hkl_i == target {
                    break;
                }
            }
            // Always clear to prevent getting stuck in a perpetual 50ms delay loop
            STATE.with(|s| s.borrow_mut().pending_hkl = None);
        }
    }

    let window_changed = STATE.with(|s| {
        let mut st = s.borrow_mut();
        let changed = st.last_hwnd != hwnd_i;
        if changed || st.last_hkl != hkl_i {
            st.buf.clear();
            st.seed.reset();
            st.undo = None;
            st.last_hwnd = hwnd_i;
            st.last_hkl = hkl_i;
        }
        changed
    });
    // Re-evaluate the (heavier) app blacklist only when the window changed.
    if window_changed {
        let blacklisted = safety::is_blacklisted_app(hwnd);
        STATE.with(|s| s.borrow_mut().sensitive_app = blacklisted);
    }
}

/// The keyboard layout (HKL) of whatever window currently has focus.
unsafe fn foreground_layout() -> HKL {
    let hwnd = GetForegroundWindow();
    let tid = GetWindowThreadProcessId(hwnd, None);
    GetKeyboardLayout(tid)
}
