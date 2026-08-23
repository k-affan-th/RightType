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
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
#[cfg(debug_assertions)]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use zeroize::Zeroize;

use righttype::buffer::{Key, WordBuffer};
use righttype::layout::auto_convert;
use righttype::{dict, policy, secret};

use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
#[cfg(debug_assertions)]
use windows::Win32::UI::Input::KeyboardAndMouse::VK_PACKET;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetKeyState, GetKeyboardLayout, GetKeyboardLayoutList, ToUnicodeEx, HKL,
    VIRTUAL_KEY, VK_BACK, VK_CAPITAL, VK_CONTROL, VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME,
    VK_INSERT, VK_LEFT, VK_MENU, VK_NEXT, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_SHIFT, VK_SPACE,
    VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, PostMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, GUITHREADINFO, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, WH_KEYBOARD_LL, WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN, WM_SYSKEYDOWN,
};

use crate::{inject, manual, safety};

/// Magic value stamped into `dwExtraInfo` on every event the injector sends, so
/// the hook can recognise and skip our own input with zero timing dependency. The
/// flag travels *with* the event, unlike a shared "injecting" boolean. ("RTYP")
pub const INJECT_TAG: usize = 0x5254_5950;

/// Set while we inject, as a secondary guard. The tag above is authoritative.
pub static INJECTING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Manual = 0,
    Auto = 1,
    Suggest = 2,
}

impl Mode {
    fn next(self) -> Self {
        match self {
            Self::Manual => Self::Auto,
            Self::Auto => Self::Suggest,
            Self::Suggest => Self::Manual,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Manual => "Manual mode",
            Self::Auto => "Auto mode",
            Self::Suggest => "Suggest mode",
        }
    }
}

static MODE: AtomicU8 = AtomicU8::new(Mode::Manual as u8);

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
    if !on {
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.buf.clear();
            st.undo = None;
            st.last_completed = None;
            st.suggestion = None;
        });
    }
}

pub fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        1 => Mode::Auto,
        2 => Mode::Suggest,
        _ => Mode::Manual,
    }
}

pub fn set_mode(mode: Mode) {
    MODE.store(mode as u8, Ordering::Relaxed);
    STATE.with(|s| s.borrow_mut().suggestion = None);
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
    /// Focus generation supplied by the UIA WinEvent hook. This catches focus
    /// changes between controls in the same top-level window.
    last_focus_generation: u64,
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
    last_completed: Option<LastCompleted>,
    suggestion: Option<SuggestionRecord>,
}

impl HookState {
    fn new() -> Self {
        Self {
            buf: WordBuffer::new(),
            seed: secret::SeedTracker::new(),
            last_hwnd: 0,
            last_hkl: 0,
            last_focus_generation: 0,
            sensitive_app: false,
            undo: None,
            pending_hkl: None,
            last_completed: None,
            suggestion: None,
        }
    }
}

struct SuggestionRecord {
    original: String,
    corrected: String,
    boundary_vk: u16,
}

struct LastCompleted {
    word: String,
    boundary_vk: u16,
}

impl Drop for LastCompleted {
    fn drop(&mut self) {
        self.word.zeroize();
    }
}

impl Drop for SuggestionRecord {
    fn drop(&mut self) {
        self.original.zeroize();
        self.corrected.zeroize();
    }
}

/// A reversible correction: how many characters to delete, and what to retype
/// to restore the pre-correction text exactly.
struct UndoRecord {
    /// Characters now present in the app (after the correction) to delete.
    injected_len: usize,
    /// Text to retype to restore what was there before.
    restore_text: String,
    created_at: Instant,
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
        created_at: Instant::now(),
    });
    STATE.with(|s| s.borrow_mut().undo = record);
}

/// Ctrl+Shift+CapsLock: revert the most recent correction, if any. One-shot —
/// the record is consumed whether or not this call finds one.
unsafe fn undo_last_correction() {
    let Some(rec) = STATE.with(|s| s.borrow_mut().undo.take()) else {
        e2e_trace("undo: no record".to_string());
        return;
    };
    if rec.created_at.elapsed() > Duration::from_secs(30) {
        e2e_trace("undo: record expired".to_string());
        return;
    }
    let ok = inject::apply(rec.injected_len, &rec.restore_text, None);
    e2e_trace(format!("undo apply len={} -> {ok}", rec.injected_len));
    if ok {
        crate::toast::show("Undo");
    } else {
        crate::toast::show("RightType: undo injection failed");
    }
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
        let _ = crate::ram::lock_region(ptr, len);
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
        let externally_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        let ours = kb.dwExtraInfo == INJECT_TAG
            || (externally_injected && !debug_e2e_accepts_injected())
            || INJECTING.load(Ordering::Relaxed);
        if !ours && process(wparam.0 as u32, kb) {
            // We handled this key as a hotkey/correction; swallow it.
            return LRESULT(1);
        }
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Computer-driven Windows E2E necessarily uses `SendInput`, which Windows marks
/// as injected. A debug build may opt into processing those events so the real
/// hook pipeline can be exercised. Release builds compile this escape hatch to
/// `false` and always ignore third-party injected input.
fn debug_e2e_accepts_injected() -> bool {
    #[cfg(debug_assertions)]
    {
        static ACCEPT: OnceLock<bool> = OnceLock::new();
        *ACCEPT.get_or_init(|| std::env::var_os("RIGHTTYPE_E2E_ACCEPT_INJECTED").is_some())
    }
    #[cfg(not(debug_assertions))]
    {
        false
    }
}

#[cfg(debug_assertions)]
pub(crate) fn e2e_trace(msg: String) {
    if debug_e2e_accepts_injected() {
        eprintln!("[rt-e2e] {msg}");
    }
}

#[cfg(not(debug_assertions))]
fn e2e_trace(_: String) {}

/// D-006 instant EN→TH commit gate data: minimum token length before an
/// in-flight commit may fire. Two-character candidates are excluded because
/// valid short words (`สว`) are frequently true prefixes of longer intended
/// words (`สวัสดี`); from three characters up, a fully-known High-confidence
/// candidate plus the layout switch that follows lets the typist finish the
/// word natively without stutter.
pub const MIN_LIVE_COMMIT_CHARS: usize = 3;

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
    if vk == VK_SPACE.0 && (is_down(VIRTUAL_KEY(0x5B)) || is_down(VIRTUAL_KEY(0x5C))) {
        // VK_LWIN = 0x5B, VK_RWIN = 0x5C
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
    let is_shift_backspace =
        vk == VK_BACK.0 && is_down(VK_SHIFT) && !is_down(VK_CONTROL) && !is_down(VK_MENU);
    if !is_shift_backspace && !is_modifier(vk) {
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.last_completed = None;
            st.suggestion = None;
        });
    }

    // Panic switch: Ctrl+Alt+CapsLock instantly flips master enable, either way.
    // Checked before the enabled gate and the sensitive-context guard below so
    // it always works — including turning back ON, and even from inside a
    // password field or blacklisted app.
    if vk == VK_CAPITAL.0 && is_down(VK_CONTROL) && is_down(VK_MENU) {
        let now_on = !ENABLED.fetch_xor(true, Ordering::Relaxed);
        crate::toast::show(if now_on {
            "RightType: ON"
        } else {
            "RightType: OFF"
        });
        crate::config::persist_async();
        return true;
    }

    // Master switch: when disabled, pass everything through untouched.
    if !ENABLED.load(Ordering::Relaxed) {
        return false;
    }

    // A user-initiated layout switch invalidates buffered caret context. Windows
    // performs the switch itself; we only discard state here.
    if is_layout_switch_trigger(vk) {
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.pending_hkl = None;
            st.buf.clear();
            st.last_completed = None;
        });
    }

    // If focus or layout changed since the last key, the buffered word is stale.
    sync_context();

    // Never run where secrets are typed: blacklisted apps, or password fields
    // (native ES_PASSWORD, or UIA-detected ones in browsers/Electron/UWP).
    if STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_password_field()
        || crate::focus::is_password_field()
    {
        STATE.with(|s| s.borrow_mut().suggestion = None);
        return false;
    }

    // CapsLock: a hotkey carrier when chorded, otherwise a normal toggle.
    if vk == VK_CAPITAL.0 {
        let ctrl = is_down(VK_CONTROL);
        let shift = is_down(VK_SHIFT);
        let alt = is_down(VK_MENU);
        if ctrl && shift {
            // Ctrl+Shift+CapsLock: undo the last correction (one-shot). Swallow.
            e2e_trace("undo-hotkey received".to_string());
            if !manual::request_undo_selection(
                GetForegroundWindow().0 as isize,
                crate::focus::generation(),
            ) {
                undo_last_correction();
            }
            return true;
        }
        if ctrl {
            // Ctrl+CapsLock: cycle Manual → Auto → Suggest. Swallow so Caps
            // never flips.
            let next = mode().next();
            set_mode(next);
            crate::toast::show(next.label());
            crate::config::persist_async();
            return true;
        }
        if alt {
            accept_suggestion();
            return true;
        }
        if shift {
            // Shift+CapsLock: convert the current selection. Swallow.
            manual::request_convert_selection(
                GetForegroundWindow().0 as isize,
                crate::focus::generation(),
            );
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
        // D-006 instant EN→TH: the moment an in-flight token becomes a
        // fully-known High-confidence Thai candidate (>= MIN_LIVE_COMMIT_CHARS),
        // correct it and switch to Thai. The typist's remaining keystrokes then
        // produce real Thai natively — no stutter, and no fabricated spaces.
        if let Key::Char(_) = key {
            let pending = STATE.with(|s| s.borrow().buf.current().to_string());
            e2e_trace(format!("live-eval {pending:?}"));
            if pending.chars().count() >= MIN_LIVE_COMMIT_CHARS
                && policy::supported_layout_id(layout_id(foreground_layout()))
                    == Some(policy::InputLayout::UsQwerty)
            {
                let d = policy::detect_token(
                    &pending,
                    policy::InputLayout::UsQwerty,
                    dict::english(),
                    dict::thai(),
                );
                if let Some(d) = d {
                    let tripped = STATE.with(|s| {
                        s.borrow_mut()
                            .seed
                            .observe_candidate(&pending, Some(d.corrected.as_str()))
                    });
                    e2e_trace(format!(
                        "live pending={pending:?} det={:?} seed={tripped}",
                        d.corrected.clone()
                    ));
                    if !tripped
                        && mode() == Mode::Auto
                        && policy::allows_live_thai_commit(Some(policy::InputLayout::UsQwerty), &d)
                        && maybe_correct(&pending, None, d)
                    {
                        STATE.with(|s| s.borrow_mut().buf.clear());
                        return true;
                    }
                }
            }
        }
        // Backspace and friends simply pass through; the buffer already shrank.
        return false;
    };

    let active_layout = policy::supported_layout_id(layout_id(foreground_layout()));
    let mut detection = active_layout
        .and_then(|layout| policy::detect_token(&word, layout, dict::english(), dict::thai()));
    e2e_trace(format!(
        "layout={active_layout:?} det={:?} mode={:?}",
        detection.as_ref().map(|d| d.corrected.clone()),
        mode()
    ));

    // Track the meaningful English stream, including a wrong-layout candidate.
    // This cannot retroactively protect the first words of a phrase (ordinary
    // English overlaps BIP39), but once the run reaches the threshold it blocks
    // Auto, Suggest and learning for the current and following seed words.
    let seed_run = STATE.with(|s| {
        s.borrow_mut()
            .seed
            .observe_candidate(&word, detection.as_ref().map(|d| d.corrected.as_str()))
    });
    if seed_run {
        detection = None;
    }

    // Learning sees only ordinary US-QWERTY input for which the production
    // policy found no wrong-layout candidate. This keeps converted candidates
    // and unsupported layouts out of the persistence path.
    if !seed_run && policy::allows_learning(active_layout, detection.is_some()) {
        crate::learn::observe(&word);
    }

    // Auto mode commits only at this boundary; Manual mode retains the token for
    // Shift+Backspace.
    let swallow = match (mode(), detection) {
        (Mode::Auto, Some(d)) => maybe_correct(&word, Some(vk), d),
        (Mode::Suggest, Some(d)) => {
            STATE.with(|s| {
                s.borrow_mut().suggestion = Some(SuggestionRecord {
                    original: word.clone(),
                    corrected: d.corrected,
                    boundary_vk: vk,
                });
            });
            crate::toast::show("Suggestion: Alt+CapsLock");
            false
        }
        _ => false,
    };
    if swallow {
        STATE.with(|s| s.borrow_mut().last_completed = None);
    } else {
        STATE.with(|s| {
            s.borrow_mut().last_completed = Some(LastCompleted {
                word: word.clone(),
                boundary_vk: vk,
            });
        });
    }
    word.zeroize();
    swallow
}

/// Exact 32-bit keyboard layout identifiers supported by the v1 mapping tables.
/// Checking the whole KLID keeps UK English and Thai Pattachote out of Auto mode;
/// sharing a primary language does not make their physical-key mapping compatible.
fn layout_id(hkl: HKL) -> u32 {
    hkl.0 as usize as u32
}

/// Switch the foreground window to one exact layout supported by the v1 mapping
/// tables. No-op if that layout is not installed.
unsafe fn activate_layout(target: policy::InputLayout) {
    if policy::supported_layout_id(layout_id(foreground_layout())) == Some(target) {
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
        if policy::supported_layout_id(layout_id(*hkl)) == Some(target) {
            let foreground = GetForegroundWindow();
            let mut gui = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            let thread_id = GetWindowThreadProcessId(foreground, None);
            let target =
                if GetGUIThreadInfo(thread_id, &mut gui).is_ok() && !gui.hwndFocus.0.is_null() {
                    gui.hwndFocus
                } else {
                    foreground
                };
            let _ = PostMessageW(
                target,
                WM_INPUTLANGCHANGEREQUEST,
                WPARAM(0),
                LPARAM(hkl.0 as isize),
            );
            // Some modern apps put focus on a custom child that does not pass
            // the request to DefWindowProc. Also notify the top-level window;
            // requesting the same explicit HKL twice is idempotent.
            if target != foreground {
                let _ = PostMessageW(
                    foreground,
                    WM_INPUTLANGCHANGEREQUEST,
                    WPARAM(0),
                    LPARAM(hkl.0 as isize),
                );
            }
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

/// Accept the one context-bound, non-destructive suggestion produced at the
/// previous boundary. Any intervening non-modifier key or context change clears
/// the record before this function can run.
unsafe fn accept_suggestion() {
    let Some(mut suggestion) = STATE.with(|s| s.borrow_mut().suggestion.take()) else {
        return;
    };
    let backspaces = suggestion.original.chars().count() + 1;
    if !inject::apply(
        backspaces,
        &suggestion.corrected,
        Some(suggestion.boundary_vk),
    ) {
        crate::toast::show("RightType: suggestion injection failed");
        return;
    }

    let mut restore = format!(
        "{}{}",
        suggestion.original,
        boundary_literal(suggestion.boundary_vk)
    );
    set_undo(suggestion.corrected.chars().count() + 1, &restore);
    restore.zeroize();
    crate::stats::record_manual();

    let to_thai = suggestion
        .corrected
        .chars()
        .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    activate_layout(if to_thai {
        policy::InputLayout::ThaiKedmanee
    } else {
        policy::InputLayout::UsQwerty
    });
    suggestion.original.zeroize();
    suggestion.corrected.zeroize();
}

/// Manual: flip the layout of the word currently in the buffer, in place. A no-op
/// when the buffer is empty (e.g. an auto-repeat after the word was already
/// converted) — the caller swallows the key either way.
unsafe fn convert_last_word() {
    let mut word = STATE.with(|s| s.borrow().buf.current().to_string());
    if word.is_empty() {
        // Try to convert the last completed word if we just hit a boundary (e.g. Space)
        let last = STATE.with(|s| s.borrow_mut().last_completed.take());
        if let Some(mut last) = last {
            let mut last_word = std::mem::take(&mut last.word);
            let boundary_vk = last.boundary_vk;
            let backspaces = last_word.chars().count() + 1; // +1 for the boundary character
            let mut converted = auto_convert(&last_word);
            let changed = converted != last_word;
            if changed {
                if !inject::apply(backspaces, &converted, Some(boundary_vk)) {
                    crate::toast::show("RightType: correction injection failed");
                    converted.zeroize();
                    last_word.zeroize();
                    return;
                }
                set_undo(
                    converted.chars().count() + 1,
                    &format!("{}{}", last_word, boundary_literal(boundary_vk)),
                );
                crate::stats::record_manual();

                // Switch language layout to the one of the converted word
                let to_thai = converted
                    .chars()
                    .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
                activate_layout(if to_thai {
                    policy::InputLayout::ThaiKedmanee
                } else {
                    policy::InputLayout::UsQwerty
                });
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
        if !inject::apply(backspaces, &converted, None) {
            crate::toast::show("RightType: correction injection failed");
            word.zeroize();
            converted.zeroize();
            return;
        }
        set_undo(converted.chars().count(), &word);
        crate::stats::record_manual();

        // Switch language layout to the one of the converted word
        let to_thai = converted
            .chars()
            .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
        activate_layout(if to_thai {
            policy::InputLayout::ThaiKedmanee
        } else {
            policy::InputLayout::UsQwerty
        });
    }
    word.zeroize();
    converted.zeroize();
}

/// Inject a completed `word` after the caller's stream and detection guards pass.
/// `boundary_vk` is the separator that completed the word (re-emitted after the
/// correction), or `None` for D-006 live EN→TH commits where nothing was typed
/// beyond the token itself. Returns `true` if a correction was injected (caller
/// swallows the triggering key), `false` otherwise.
unsafe fn maybe_correct(
    word: &str,
    boundary_vk: Option<u16>,
    d: righttype::detect::Detection,
) -> bool {
    maybe_correct_with(word, boundary_vk, d, |backspaces, text, trailing_vk| {
        inject::apply(backspaces, text, trailing_vk)
    })
}

unsafe fn maybe_correct_with<F>(
    word: &str,
    boundary_vk: Option<u16>,
    d: righttype::detect::Detection,
    apply: F,
) -> bool
where
    F: FnOnce(usize, &str, Option<u16>) -> bool,
{
    // The triggering key (when any) is swallowed, so only the word's own
    // characters are deleted.
    let backspaces = word.chars().count();
    let mut corrected = d.corrected;
    if !apply(backspaces, &corrected, boundary_vk) {
        crate::toast::show("RightType: correction injection failed");
        corrected.zeroize();
        return false;
    }

    // Undo target: retype the original word plus the boundary it would have
    // gotten anyway (the boundary keystroke itself never reached the app).
    let mut restore = match boundary_vk {
        Some(vk) => format!("{word}{}", boundary_literal(vk)),
        None => word.to_string(),
    };
    set_undo(
        corrected.chars().count() + usize::from(boundary_vk.is_some()),
        &restore,
    );
    crate::stats::record_auto();
    restore.zeroize();

    // Switch to whichever language we just produced, so the rest of the sentence
    // types natively — same idea as the live Thai path, now for the EN direction.
    let to_thai = corrected
        .chars()
        .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c));
    corrected.zeroize();
    activate_layout(if to_thai {
        policy::InputLayout::ThaiKedmanee
    } else {
        policy::InputLayout::UsQwerty
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
    #[cfg(debug_assertions)]
    if vk == VK_PACKET.0 && debug_e2e_accepts_injected() {
        return char::from_u32(scan as u32).map(Key::Char);
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

    // A layout-switch request is asynchronous. Never wait for it inside the
    // global low-level hook: a slow target window used to add up to 50 ms of
    // latency to the next physical key. If the layout has not changed yet, the
    // normal context comparison below keeps the buffer conservative.
    let hkl_i = GetKeyboardLayout(tid).0 as isize;
    if STATE.with(|s| s.borrow().pending_hkl).is_some() {
        STATE.with(|s| s.borrow_mut().pending_hkl = None);
    }

    let focus_generation = crate::focus::generation();
    let window_changed = STATE.with(|s| {
        let mut st = s.borrow_mut();
        let changed = st.last_hwnd != hwnd_i;
        let lang_changed = st.last_hkl != hkl_i;
        let focus_changed = st.last_focus_generation != focus_generation;
        if changed || lang_changed || focus_changed {
            // A layout switch alone must NOT drop the pending Undo: it is
            // usually our own correction switching the layout, and the very
            // next keypress (e.g. the Undo hotkey itself) would otherwise
            // erase the record it is about to use. Window/focus changes are
            // genuine context loss.
            if changed || focus_changed {
                st.undo = None;
            }
            st.buf.clear();
            st.seed.reset();
            st.last_completed = None;
            st.suggestion = None;
            st.last_hwnd = hwnd_i;
            st.last_hkl = hkl_i;
            st.last_focus_generation = focus_generation;
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

#[cfg(test)]
mod tests {
    use super::{Mode, STATE};
    use righttype::detect::{Confidence, Detection, Evidence};

    #[test]
    fn mode_cycle_is_manual_auto_suggest() {
        assert_eq!(Mode::Manual.next(), Mode::Auto);
        assert_eq!(Mode::Auto.next(), Mode::Suggest);
        assert_eq!(Mode::Suggest.next(), Mode::Manual);
    }

    #[test]
    fn failed_auto_injection_has_no_success_side_effects() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.undo = None;
            state.pending_hkl = None;
        });
        let stats_before = crate::stats::snapshot();
        let detection = Detection {
            corrected: "สวัสดี".to_string(),
            confidence: Confidence::High,
            evidence: Evidence::ExactDictionary,
        };

        let committed = unsafe {
            super::maybe_correct_with(
                "l;ylfu",
                Some(0x20),
                detection,
                |_backspaces, _text, _vk| false,
            )
        };

        assert!(!committed);
        assert_eq!(crate::stats::snapshot(), stats_before);
        STATE.with(|state| {
            let state = state.borrow();
            assert!(state.undo.is_none());
            assert!(state.pending_hkl.is_none());
        });
    }
}
