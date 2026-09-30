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
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;
#[cfg(debug_assertions)]
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use zeroize::Zeroize;

use righttype::buffer::{Key, WordBuffer};
use righttype::hotkeys::{Action, Chord, Hotkeys};
use righttype::layout::auto_convert;
use righttype::per_app::AppMode;
use righttype::recent::Recent;
use righttype::render;
use righttype::{dict, policy, secret};

use std::ffi::c_void;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
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
    LLKHF_INJECTED, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_INPUTLANGCHANGEREQUEST, WM_KEYDOWN,
    WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYDOWN, WM_XBUTTONDOWN,
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

    /// Toast text announcing this mode, in the interface language.
    pub fn label(self) -> &'static str {
        use righttype::i18n::{tr, T};
        tr(match self {
            Self::Manual => T::ToastModeManual,
            Self::Auto => T::ToastModeAuto,
            Self::Suggest => T::ToastModeSuggest,
        })
    }
}

static MODE: AtomicU8 = AtomicU8::new(Mode::Manual as u8);

/// The key last pressed and not yet released (a further key-down of it is an
/// auto-repeat).
static HELD_KEY: std::sync::atomic::AtomicU16 = std::sync::atomic::AtomicU16::new(0);

/// The hotkeys in use (Settings → Hotkeys).
static HOTKEYS: std::sync::RwLock<Option<Hotkeys>> = std::sync::RwLock::new(None);

pub fn hotkeys() -> Hotkeys {
    HOTKEYS.read().unwrap().clone().unwrap_or_default()
}

pub fn set_hotkeys(hotkeys: Hotkeys) {
    *HOTKEYS.write().unwrap() = Some(hotkeys);
}

/// Settings is waiting for the next chord for this action; the result is
/// posted to `hwnd` as [`WM_HOTKEY_CAPTURED`].
static CAPTURE: Mutex<Option<(Action, isize)>> = Mutex::new(None);
/// The captured chord (`None` inside = cancelled with Esc).
static CAPTURED: Mutex<Option<(Action, Option<Chord>)>> = Mutex::new(None);
pub const WM_HOTKEY_CAPTURED: u32 = 0x8000 + 0x530;

/// Capture the next chord pressed anywhere as the new hotkey for `action`.
pub fn begin_capture(action: Action, hwnd: isize) {
    *CAPTURE.lock().unwrap() = Some((action, hwnd));
}

pub fn cancel_capture() {
    CAPTURE.lock().unwrap().take();
}

/// The chord captured for Settings, once it has been posted.
pub fn take_captured() -> Option<(Action, Option<Chord>)> {
    CAPTURED.lock().unwrap().take()
}

/// While Settings captures a hotkey: the first non-modifier key (CapsLock
/// counts) with the modifiers held is the chord; Esc cancels. The key is
/// swallowed either way.
unsafe fn capture_key(vk: u16) -> bool {
    let Some((action, hwnd)) = *CAPTURE.lock().unwrap() else {
        return false;
    };
    if is_modifier(vk) && vk != VK_CAPITAL.0 {
        return false;
    }
    CAPTURE.lock().unwrap().take();
    let chord = (vk != VK_ESCAPE.0)
        .then(|| Chord::new(is_down(VK_CONTROL), is_down(VK_SHIFT), is_down(VK_MENU), vk));
    *CAPTURED.lock().unwrap() = Some((action, chord));
    let _ = PostMessageW(
        HWND(hwnd as *mut c_void),
        WM_HOTKEY_CAPTURED,
        WPARAM(0),
        LPARAM(0),
    );
    true
}

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
            st.owned = None;
            st.mark = TokenMark::Plain;
            st.undo = None;
            st.recent.clear();
            st.suggestion = None;
            st.live_hint = None;
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

/// The mode that applies in the app being typed in: its own per-app mode, or
/// the global one. `None` when RightType is switched off in this app.
fn mode_here() -> Option<Mode> {
    let own = STATE.with(|s| s.borrow().app_exe.as_deref().and_then(crate::apps::lookup));
    match own {
        Some(AppMode::Off) => None,
        Some(AppMode::Auto) => Some(Mode::Auto),
        Some(AppMode::Suggest) => Some(Mode::Suggest),
        Some(AppMode::Manual) => Some(Mode::Manual),
        None => Some(mode()),
    }
}

impl From<Mode> for AppMode {
    fn from(mode: Mode) -> Self {
        match mode {
            Mode::Auto => AppMode::Auto,
            Mode::Suggest => AppMode::Suggest,
            Mode::Manual => AppMode::Manual,
        }
    }
}

/// Ctrl+CapsLock: cycle the mode. In an app with its own mode that is the
/// mode cycled (so the hotkey does what it visibly does there); elsewhere the
/// global one.
unsafe fn cycle_mode() {
    use righttype::i18n::{tr, trf, T};
    let exe = safety::foreground_exe(GetForegroundWindow());
    let own = exe.as_deref().and_then(crate::apps::lookup);
    match (exe, own) {
        (Some(exe), Some(own)) => {
            let next = match own {
                AppMode::Manual => Mode::Auto,
                AppMode::Auto => Mode::Suggest,
                // Suggest, or Off (switched on again by the hotkey).
                _ => Mode::Manual,
            };
            crate::apps::set(&exe, Some(next.into()));
            crate::overlay::show(&trf(
                T::ToastAppMode,
                &[
                    (
                        "mode",
                        tr(match next {
                            Mode::Auto => T::ModeAuto,
                            Mode::Suggest => T::ModeSuggest,
                            Mode::Manual => T::ModeManual,
                        }),
                    ),
                    ("app", &exe),
                ],
            ));
            STATE.with(|s| s.borrow_mut().suggestion = None);
        }
        _ => {
            let next = mode().next();
            set_mode(next);
            crate::overlay::show(next.label());
        }
    }
    crate::config::persist_async();
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
    /// The foreground app's executable name (lower case), for its per-app mode.
    /// Recomputed with `sensitive_app`.
    app_exe: Option<String>,
    /// The most recent correction, kept for one-shot Undo (Ctrl+Shift+CapsLock).
    /// Cleared after use and whenever focus/layout changes (an undo that retypes
    /// into a different window/context than the one it corrected would be wrong).
    undo: Option<UndoRecord>,
    /// The layout we requested to switch to, while the target app has not yet
    /// reported it. Keystrokes in that window are translated with *this* layout:
    /// the app processes our posted switch before the next key's input message,
    /// so it already types in the new layout even though `GetKeyboardLayout`
    /// still reports the old one.
    pending_hkl: Option<PendingLayout>,
    /// D-008: set while we own what is on screen for the current run.
    owned: Option<OwnedRun>,
    /// D-009: who has decided what the current token is.
    mark: TokenMark,
    /// The last few completed words, as on screen, for Shift+Backspace right
    /// after a boundary (pressed again: the word before, and so on).
    recent: Recent,
    suggestion: Option<SuggestionRecord>,
    /// A Suggest hint shown while the word is still being typed (Suggest
    /// mode): Tab flips the word in progress. Only where it was shown.
    live_hint: Option<LiveHint>,
}

#[derive(Clone, Copy)]
struct LiveHint {
    hwnd: isize,
    focus_generation: u64,
    created: Instant,
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
            app_exe: None,
            undo: None,
            pending_hkl: None,
            owned: None,
            mark: TokenMark::Plain,
            recent: Recent::new(),
            suggestion: None,
            live_hint: None,
        }
    }
}

/// D-009: who has decided what the token in progress is. Reset at every
/// boundary and every time the buffer is dropped.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TokenMark {
    /// Nobody yet: Auto may render and revise it.
    Plain,
    /// RightType converted part of it on its own (an anchored run) and the rest
    /// is arriving natively in the new layout. The buffer holds the whole token
    /// as it is on screen, so its boundary can still revise all of it.
    Converted,
    /// The typist settled it by hand (withdrew our reading, undid an anchor, or
    /// flipped it). Auto leaves it alone; `learn` when what they rejected was
    /// RightType's own conversion, so the finished word is remembered.
    Decided { learn: bool },
}

/// How long a requested layout switch may stay unconfirmed before we assume
/// the app ignored it and fall back to what Windows reports.
const PENDING_LAYOUT_GRACE: Duration = Duration::from_millis(500);

#[derive(Clone, Copy)]
struct PendingLayout {
    hkl: isize,
    since: Instant,
}

/// D-008: a run whose on-screen text RightType is currently responsible for.
///
/// While this exists every keystroke is swallowed and the screen is moved to the
/// run's current best reading, so a reading chosen early can still be withdrawn.
/// It lives exactly as long as the run in [`WordBuffer`] does and is wiped at the
/// same moments, so revisability costs no extra retention of typed text.
struct OwnedRun {
    /// What we have put on screen for this run.
    rendered: String,
    /// Consecutive keystrokes the Thai reading has survived.
    stable: usize,
}

impl Drop for OwnedRun {
    fn drop(&mut self) {
        self.rendered.zeroize();
    }
}

struct SuggestionRecord {
    original: String,
    corrected: String,
    boundary_vk: u16,
    created: Instant,
    /// Where it was made: Tab accepts it only in the same window and field.
    hwnd: isize,
    focus_generation: u64,
}

/// Tab takes a Suggest hint only this soon after it appeared; later, Tab is
/// just Tab again.
const SUGGEST_TAB_WINDOW: Duration = Duration::from_secs(4);

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
    kind: UndoKind,
    created_at: Instant,
}

/// Who made a correction, which decides what undoing it teaches us.
#[derive(Clone, Copy, PartialEq, Eq)]
enum UndoKind {
    /// The typist asked for it (hotkey, suggestion): undo just reverts.
    Manual,
    /// RightType corrected a complete word on its own. Undoing it says "that
    /// was a real word", so the restored word is learned.
    AutoWord,
    /// RightType anchored a run mid-word. Undoing it hands the token back to
    /// the typist; it is learned once they finish it.
    AutoMidToken,
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
fn set_undo(injected_len: usize, restore_text: &str, kind: UndoKind) {
    let record = (!secret::is_secret_token(restore_text)).then(|| UndoRecord {
        injected_len,
        restore_text: restore_text.to_string(),
        kind,
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
        let restored = rec.restore_text.trim_end_matches(['\r', '\t', ' ']);
        // The word counted was the correction; the one kept is the original.
        habit_correction(!has_thai(restored), has_thai(restored));
        match rec.kind {
            UndoKind::Manual => {}
            UndoKind::AutoWord => crate::learn::learn_now(restored),
            UndoKind::AutoMidToken => STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.buf.replace(restored);
                st.mark = TokenMark::Decided { learn: true };
            }),
        }
        // The typist meant what they typed: keep typing it in its own layout.
        activate_layout(layout_of(restored));
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
    } else {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrUndoInject));
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

/// The boundary key that types `c` (the inverse of [`boundary_literal`]).
fn boundary_vk(c: char) -> u16 {
    match c {
        '\r' => VK_RETURN.0,
        '\t' => VK_TAB.0,
        _ => VK_SPACE.0,
    }
}

/// The installed hook handle, kept only so [`uninstall`] can remove it. The raw
/// handle is not `Send`; this wrapper asserts it is safe to move between threads
/// (we only ever touch it from install/uninstall, never concurrently).
struct HookHandle(HHOOK);
unsafe impl Send for HookHandle {}
static HOOK: Mutex<Option<HookHandle>> = Mutex::new(None);
/// The low-level mouse hook: a click can move the caret without a keystroke.
static MOUSE_HOOK: Mutex<Option<HookHandle>> = Mutex::new(None);

/// `GetTickCount` time of the last event the hook received (or of its
/// installation). The session watchdog compares it with the system's last-input
/// time to notice a hook Windows removed without any power/session event.
static LAST_HOOK_TICK: AtomicU32 = AtomicU32::new(0);

/// See [`LAST_HOOK_TICK`].
pub fn last_hook_tick() -> u32 {
    LAST_HOOK_TICK.load(Ordering::Relaxed)
}

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
    detect_keyboards();
    // Best-effort: without it a click is only noticed when focus changes.
    if let Ok(mouse) = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), HINSTANCE(hmod.0), 0) {
        *MOUSE_HOOK.lock().unwrap() = Some(HookHandle(mouse));
    }
    LAST_HOOK_TICK.store(
        windows::Win32::System::SystemInformation::GetTickCount(),
        Ordering::Relaxed,
    );
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
    if let Some(h) = MOUSE_HOOK.lock().unwrap().take() {
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

/// A mouse button went down: the click may have moved the caret, so nothing
/// recorded about the text before it can be trusted any more — the same as an
/// arrow key. Only the event type is looked at, never where the click was.
unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32
        && matches!(
            wparam.0 as u32,
            WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
        )
    {
        caret_may_have_moved();
    }
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

/// Let go of everything that assumes the caret is where typing left it: the
/// word in progress, a run we own (left on screen as it is), the Undo record,
/// the recent words and a pending suggestion.
fn caret_may_have_moved() {
    STATE.with(|s| {
        // Never re-entered from inside the keyboard path, but do not panic if
        // a nested hook call ever finds the state borrowed.
        let Ok(mut st) = s.try_borrow_mut() else {
            return;
        };
        st.buf.clear();
        st.owned = None;
        st.mark = TokenMark::Plain;
        st.undo = None;
        st.recent.clear();
        st.suggestion = None;
        st.live_hint = None;
    });
}

unsafe extern "system" fn ll_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        LAST_HOOK_TICK.store(kb.time, Ordering::Relaxed);
        // Skip anything we generated: our tag is authoritative and timing-free.
        let externally_injected = (kb.flags.0 & LLKHF_INJECTED.0) != 0;
        let ours = kb.dwExtraInfo == INJECT_TAG
            || (externally_injected && !debug_e2e_accepts_injected())
            || INJECTING.load(Ordering::Relaxed);
        if ours && wparam.0 as u32 == WM_KEYDOWN {
            e2e_trace(format!(
                "key vk={:#x} skipped (tag={} injecting={})",
                kb.vkCode,
                kb.dwExtraInfo == INJECT_TAG,
                INJECTING.load(Ordering::Relaxed)
            ));
        }
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
pub(crate) fn e2e_trace(_: String) {}

/// Debug e2e builds: report a fatal exception (code, address and the
/// faulting thread's stack) to stderr before Windows ends the process, which
/// otherwise dies with only an exit code (`0xC000041D` when it happens
/// inside a callback Windows made into us). First-chance, so some reported
/// access violations may be ones a system DLL catches itself; the last
/// report before the process ends is the one that killed it.
#[cfg(debug_assertions)]
pub fn report_fatal_exceptions() {
    use windows::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_POINTERS,
    };
    unsafe extern "system" fn on_exception(info: *mut EXCEPTION_POINTERS) -> i32 {
        static REPORTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
        let Some(record) = info.as_ref().and_then(|i| i.ExceptionRecord.as_ref()) else {
            return EXCEPTION_CONTINUE_SEARCH;
        };
        let code = record.ExceptionCode.0 as u32;
        let fatal = matches!(
            code,
            0xC000_0005 // access violation
                | 0xC000_001D // illegal instruction
                | 0xC000_0096 // privileged instruction
                | 0xC000_0374 // heap corruption
                | 0xC000_0409 // stack buffer overrun / fail fast
                | 0xC000_00FD // stack overflow
        );
        if !fatal || REPORTS.fetch_add(1, Ordering::Relaxed) >= 5 {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        eprintln!(
            "[rt-e2e] EXCEPTION {code:#010x} at {:?} (info {:x?})",
            record.ExceptionAddress,
            &record.ExceptionInformation[..record.NumberParameters.min(3) as usize]
        );
        // Too little stack left to walk it after an overflow.
        if code != 0xC000_00FD {
            eprintln!("{}", std::backtrace::Backtrace::force_capture());
        }
        EXCEPTION_CONTINUE_SEARCH
    }
    if debug_e2e_accepts_injected() {
        unsafe {
            AddVectoredExceptionHandler(1, Some(on_exception));
        }
    }
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
    // Windows repeats a held key as more key-downs; only a release ends it.
    let repeat = if down {
        HELD_KEY.swap(vk, Ordering::Relaxed) == vk
    } else {
        let _ = HELD_KEY.compare_exchange(vk, 0, Ordering::Relaxed, Ordering::Relaxed);
        false
    };
    if !down {
        return false;
    }
    e2e_trace(format!("key vk={vk:#x} repeat={repeat}"));
    if capture_key(vk) {
        return true;
    }
    let action = hotkeys().action_for(vk, is_down(VK_CONTROL), is_down(VK_SHIFT), is_down(VK_MENU));

    // Tab (alone) right after a Suggest hint takes it, like Alt+CapsLock. The
    // hint exists only until the next key, so Tab is otherwise untouched.
    // This runs ahead of the context checks below, so it checks for itself
    // that the caret is still where the hint was made: same window, same
    // focused field (a Tab that ended the word may have moved focus, so a
    // hint made at a Tab boundary is never taken by Tab), and not a password
    // field.
    if vk == VK_TAB.0
        && !is_down(VK_SHIFT)
        && !is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && STATE.with(|s| {
            s.borrow().suggestion.as_ref().is_some_and(|x| {
                x.created.elapsed() < SUGGEST_TAB_WINDOW
                    && x.boundary_vk != VK_TAB.0
                    && x.hwnd == GetForegroundWindow().0 as isize
                    && x.focus_generation == crate::focus::generation()
            })
        })
        && !safety::is_password_field()
        && !crate::focus::is_password_field()
    {
        accept_suggestion();
        return true;
    }
    // Tab while a hint is showing for the word still being typed: flip it now.
    if vk == VK_TAB.0
        && !is_down(VK_SHIFT)
        && !is_down(VK_CONTROL)
        && !is_down(VK_MENU)
        && STATE.with(|s| {
            let st = s.borrow();
            !st.buf.current().is_empty()
                && st.live_hint.is_some_and(|h| {
                    h.created.elapsed() < SUGGEST_TAB_WINDOW
                        && h.hwnd == GetForegroundWindow().0 as isize
                        && h.focus_generation == crate::focus::generation()
                })
        })
        && !safety::is_password_field()
        && !crate::focus::is_password_field()
    {
        STATE.with(|s| s.borrow_mut().live_hint = None);
        crate::overlay::dismiss();
        convert_last_word();
        return true;
    }

    // The flip hotkey (Shift+Backspace by default) keeps the recent words.
    let is_flip = action == Some(Action::Flip);
    if !is_flip && action.is_none() && !is_modifier(vk) {
        // Keys that edit or move away from the text before the caret, and any
        // command chord, make the recent words stale. Typing (a character or
        // a boundary) keeps them: it only adds after them.
        let stale = is_down(VK_CONTROL) || is_down(VK_MENU) || moves_or_edits(vk);
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.recent.end_chain();
            if stale {
                st.recent.clear();
            }
            st.suggestion = None;
            st.live_hint = None;
        });
    } else if (action.is_some() && !is_flip) || vk == VK_CAPITAL.0 {
        // Other hotkeys (convert selection, undo, ...) can rewrite text, and
        // CapsLock alone changes what the next keys type.
        // (The pending suggestion stays: the Accept hotkey is one of these.)
        STATE.with(|s| s.borrow_mut().recent.clear());
    }

    // Panic switch (Ctrl+Alt+CapsLock by default) instantly flips master
    // enable, either way. Checked before the enabled gate and the
    // sensitive-context guard below so it always works — including turning
    // back ON, and even from inside a password field or blacklisted app.
    if action == Some(Action::Panic) {
        let now_on = !ENABLED.fetch_xor(true, Ordering::Relaxed);
        crate::overlay::show(righttype::i18n::tr(if now_on {
            righttype::i18n::T::ToastOn
        } else {
            righttype::i18n::T::ToastOff
        }));
        crate::config::persist_async();
        return true;
    }

    // Master switch: when disabled, pass everything through untouched.
    if !ENABLED.load(Ordering::Relaxed) {
        e2e_trace("key passed through: disabled".to_string());
        return false;
    }

    // Ctrl+CapsLock: cycle Manual → Auto → Suggest. Swallow so Caps never
    // flips. Handled ahead of the context guards: it never touches text, and
    // Electron apps (Claude, VS Code, Slack, Discord) frequently report a UIA
    // focus we cannot classify, which the guards below must treat as a
    // password field — so the chord used to fall through there and silently
    // toggle CapsLock instead of switching mode.
    if action == Some(Action::Cycle) {
        cycle_mode();
        return true;
    }

    // The command palette never touches text either: it opens everywhere.
    // Opened after this callback returns, never inside the hook.
    if action == Some(Action::Palette) {
        if !repeat {
            crate::palette::request_open();
        }
        return true;
    }

    // A user-initiated layout switch invalidates buffered caret context. Windows
    // performs the switch itself; we only discard state here.
    //
    // This fires on the *modifier* key-down, before we can tell a layout switch
    // from the Ctrl+Shift prefix of RightType's own Undo hotkey. A run we own
    // must therefore be withdrawn rather than abandoned: dropping it would leave
    // rendered Thai on screen with nothing tracking it, and abandoning it while
    // clearing the buffer would make the next reconcile delete text it no longer
    // has a run for. Withdrawing is right under either reading of the chord.
    if is_layout_switch_trigger(vk) {
        withdraw_owned_run();
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.pending_hkl = None;
            st.buf.clear();
            st.mark = TokenMark::Plain;
            st.recent.clear();
        });
    }

    // If focus or layout changed since the last key, the buffered word is stale.
    sync_context();
    note_english_variant(effective_layout());

    // Never run where secrets are typed: blacklisted apps, or password fields
    // (native ES_PASSWORD, or UIA-detected ones in browsers/Electron/UWP).
    if STATE.with(|s| s.borrow().sensitive_app)
        || safety::is_password_field()
        || crate::focus::is_password_field()
    {
        e2e_trace(format!(
            "key passed through: sensitive_app={} native_password={} uia_protected={}",
            STATE.with(|s| s.borrow().sensitive_app),
            safety::is_password_field(),
            crate::focus::is_password_field()
        ));
        STATE.with(|s| s.borrow_mut().suggestion = None);
        return false;
    }
    // Switched off in this app (its per-app mode): touch nothing, like a
    // blocked app, but the hotkeys above (on/off, mode cycle) still work.
    let Some(mode_now) = mode_here() else {
        e2e_trace("key passed through: off in this app".to_string());
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.buf.clear();
            st.suggestion = None;
            st.live_hint = None;
            st.recent.clear();
        });
        return false;
    };

    // The text hotkeys (CapsLock chords by default).
    {
        if action == Some(Action::Undo) {
            // Undo the last correction (one-shot). Swallow.
            e2e_trace("undo-hotkey received".to_string());
            // A run we still own is the most recent correction there is, and it
            // has no Undo record yet (that is written when the run anchors), so
            // withdrawing our rendering *is* the undo.
            if withdraw_owned_run() {
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    // Keep the token but mark it decided: without this the very
                    // next keystroke re-evaluates the same text and can
                    // immediately re-apply the reading the typist just rejected.
                    st.mark = TokenMark::Decided { learn: true };
                    st.undo = None;
                });
                crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
            } else if !manual::request_undo_selection(
                GetForegroundWindow().0 as isize,
                crate::focus::generation(),
            ) {
                undo_last_correction();
            }
            return true;
        }
        if action == Some(Action::Accept) {
            accept_suggestion();
            return true;
        }
        if action == Some(Action::Selection) {
            // Convert the current selection. Swallow.
            e2e_trace("convert-selection-hotkey received".to_string());
            manual::request_convert_selection(
                GetForegroundWindow().0 as isize,
                crate::focus::generation(),
            );
            return true;
        }
        // CapsLock alone (or a chord that is not a hotkey) is a normal toggle.
        if vk == VK_CAPITAL.0 {
            return false;
        }
    }

    // Shift+Backspace: flip the current word in place. Always swallowed — even
    // when there's nothing to convert — so the key's auto-repeat can't fall
    // through to a destructive Backspace and delete the result we just injected.
    if is_flip {
        e2e_trace(format!(
            "flip: repeat={repeat} buf={} recent={}",
            STATE.with(|s| s.borrow().buf.current().chars().count()),
            STATE.with(|s| s.borrow().recent.len()),
        ));
        // Holding the keys acts once: each flip reaches one word further back,
        // so auto-repeat would run through all of them in a blink.
        if repeat {
            return true;
        }
        // While we own the run the screen does not match the buffer, so the
        // manual path's backspace count would be wrong. Withdraw our rendering
        // first; the typist asked for the raw keystrokes back.
        if withdraw_owned_run() {
            // The typist rejected our reading mid-word. Leave the rest of this
            // token alone, and learn it once it is complete.
            STATE.with(|s| s.borrow_mut().mark = TokenMark::Decided { learn: true });
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
            return true;
        }
        convert_last_word();
        return true;
    }

    let Some(key) = classify(vk, kb.scanCode as u16) else {
        e2e_trace(format!("key vk={vk:#x} not classified"));
        // Something we cannot follow (a dead key, a function key): the text
        // before the caret may not be what we recorded. A modifier pressed on
        // its own types nothing — and Shift is how Shift+Backspace starts, so
        // clearing here made it forget the word it was pressed to flip.
        if !is_modifier(vk) {
            STATE.with(|s| s.borrow_mut().recent.clear());
        }
        return false;
    };

    // Any text reaching the app moves the caret past a correction's Undo
    // window: the record counts characters from the end, so replaying it now
    // would delete what was just typed instead of what we changed.
    if matches!(key, Key::Char(_) | Key::Backspace | Key::Boundary) {
        STATE.with(|s| s.borrow_mut().undo = None);
    }

    // Drive the buffer; only a boundary can return a completed word.
    let completed = STATE.with(|s| s.borrow_mut().buf.observe(key));
    if completed.is_none() && key == Key::Boundary {
        // A boundary after nothing (a second space) is not in the record.
        STATE.with(|s| s.borrow_mut().recent.clear());
    }
    let Some(mut word) = completed else {
        // D-008 revisable rendering: reconcile the screen with the run's current
        // best reading. Only Char and Backspace change the run, and only a
        // token nobody has decided yet is RightType's to reinterpret.
        // A Backspace re-renders only a run we already own. Before we own
        // one, the screen still holds the character being deleted, so the
        // "run minus the new key" model below would be off by two and leave
        // raw keys mixed into the Thai (`mujouj1⌫` → `muที่นี่`); letting the
        // Backspace through keeps the screen and the buffer in step, and the
        // next key (or the boundary) reads the word again.
        let may_reconcile = match key {
            Key::Char(_) => true,
            Key::Backspace => STATE.with(|s| s.borrow().owned.is_some()),
            _ => false,
        };
        if matches!(key, Key::Char(_))
            && mode_now == Mode::Suggest
            && STATE.with(|s| s.borrow().mark == TokenMark::Plain)
            && policy::supported_layout_id(layout_id(effective_layout()))
                == Some(policy::InputLayout::UsQwerty)
        {
            show_live_hint();
        }
        if may_reconcile
            && mode_now == Mode::Auto
            && STATE.with(|s| s.borrow().mark == TokenMark::Plain)
            && policy::supported_layout_id(layout_id(effective_layout()))
                == Some(policy::InputLayout::UsQwerty)
            && reconcile_run()
        {
            return true;
        }
        // Navigation and focus events move the caret away from the run, so the
        // text we rendered is no longer ours to edit. Let go without touching it.
        if key == Key::Reset {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.owned = None;
                st.mark = TokenMark::Plain;
            });
        } else {
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                // Too long to describe any more, or erased back to nothing:
                // either way nothing about the token is ours any more.
                if st.buf.is_poisoned() || st.buf.current().is_empty() {
                    st.mark = TokenMark::Plain;
                }
            });
        }
        // Everything else passes through; the buffer already tracked it.
        return false;
    };
    let mark = STATE.with(|s| std::mem::replace(&mut s.borrow_mut().mark, TokenMark::Plain));

    // A boundary ends a run we own. If its reading cannot end as Thai and
    // the keys were all letters, they go back to what was typed and the word
    // is judged like any other below (see `policy::run_goes_back`).
    let back = STATE.with(|s| {
        s.borrow().owned.as_ref().is_some_and(|o| {
            policy::run_goes_back(&word, &o.rendered, dict::english(), dict::thai())
        })
    });
    if back {
        e2e_trace(format!("boundary: {word:?} cannot end as Thai, withdrawn"));
        withdraw_owned_run_to(&word);
    }
    // Otherwise its reading has already been applied to the screen, so the
    // boundary path must not correct it a second time — its backspace count
    // assumes the screen still holds the raw keystrokes.
    if let Some(mut rendered) = anchor_owned_run(&word, vk) {
        e2e_trace("boundary: owned run anchored".to_string());
        remember_completed(&rendered, vk, true);
        rendered.zeroize();
        word.zeroize();
        return false;
    }

    // The typist settled this token by hand; honour that, and learn the word
    // when what they rejected was RightType's own conversion.
    if let TokenMark::Decided { learn } = mark {
        // Still part of the typed stream: the seed-phrase guard must see it.
        let seed_run = STATE.with(|s| s.borrow_mut().seed.observe_candidate(&word, None));
        if learn && !seed_run {
            crate::learn::learn_now(&word);
        }
        remember_completed(&word, vk, false);
        word.zeroize();
        return false;
    }

    let active_layout = policy::supported_layout_id(layout_id(effective_layout()));
    let converted = mark == TokenMark::Converted;
    let mut detection = if converted {
        // D-009: the whole token, including the part converted before the
        // anchor, gets its first complete look now.
        policy::revise_converted(&word, dict::english(), dict::thai())
    } else {
        active_layout
            .and_then(|layout| policy::detect_token(&word, layout, dict::english(), dict::thai()))
    };
    e2e_trace(format!(
        "word={word:?} layout={active_layout:?} converted={converted} det={:?} mode={:?}",
        detection.as_ref().map(|d| d.corrected.clone()),
        mode_now
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
        forget_recent_text();
    }

    // Learning sees only ordinary US-QWERTY input for which the production
    // policy found no wrong-layout candidate. This keeps converted candidates
    // and unsupported layouts out of the persistence path.
    if !seed_run && policy::allows_learning(active_layout, detection.is_some()) {
        crate::learn::observe(&word);
    }

    // Auto mode commits only at this boundary; Manual mode retains the token for
    // Shift+Backspace.
    let swallow = match (mode_now, detection) {
        (Mode::Auto, Some(d)) => {
            let mut corrected = d.corrected.clone();
            let done = maybe_correct(&word, Some(vk), d);
            if done {
                // Shift+Backspace right after an automatic correction flips it
                // back — the undo gesture people reach for first.
                remember_completed(&corrected, vk, true);
            }
            corrected.zeroize();
            done
        }
        (Mode::Suggest, Some(d)) => {
            // Show *what* would be written, not just that something would:
            // a hint you cannot read is a hint you cannot judge.
            let mut hint = format!("{}  ·  Tab", d.corrected);
            STATE.with(|s| {
                s.borrow_mut().suggestion = Some(SuggestionRecord {
                    original: word.clone(),
                    corrected: d.corrected,
                    boundary_vk: vk,
                    created: Instant::now(),
                    hwnd: GetForegroundWindow().0 as isize,
                    focus_generation: crate::focus::generation(),
                });
            });
            crate::overlay::show_at(&hint, crate::caret::hint_anchor());
            hint.zeroize();
            false
        }
        _ => false,
    };
    if !swallow {
        remember_completed(&word, vk, converted);
    }
    word.zeroize();
    swallow
}

/// Suggest mode, mid-word: when the keys typed so far clearly read as Thai
/// (the same bar Auto uses before it rewrites anything), show that reading
/// next to the cursor; Tab then flips the word. Nothing changes on screen
/// unless the typist asks.
fn show_live_hint() {
    let (run, guarding) = STATE.with(|s| {
        let st = s.borrow();
        (st.buf.current().to_string(), st.seed.guarding())
    });
    if guarding || run.is_empty() {
        return;
    }
    let reading = policy::live_reading(&run, false, dict::english(), dict::thai());
    if let policy::Reading::Thai(mut thai) = reading {
        let mut hint = format!("{thai}  ·  Tab");
        crate::overlay::show_at(&hint, crate::caret::hint_anchor());
        hint.zeroize();
        thai.zeroize();
        let here = LiveHint {
            hwnd: unsafe { GetForegroundWindow() }.0 as isize,
            focus_generation: crate::focus::generation(),
            created: Instant::now(),
        };
        STATE.with(|s| s.borrow_mut().live_hint = Some(here));
        LIVE_HINT_SHOWN.with(|c| c.set(true));
    } else if LIVE_HINT_SHOWN.with(|c| c.replace(false)) {
        // The reading died: take the hint (and its "Tab") off the screen.
        crate::overlay::dismiss();
    }
    let mut run = run;
    run.zeroize();
}

thread_local! {
    /// A live hint is on screen (so it can be taken down when it no longer
    /// applies).
    static LIVE_HINT_SHOWN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A seed phrase was just recognised: drop every copy of recently typed text
/// we still hold (Undo record, last word, pending Suggest hint), so the words
/// that came before the threshold do not outlive it in this process.
fn forget_recent_text() {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.undo = None;
        st.recent.clear();
        st.suggestion = None;
        st.live_hint = None;
    });
    crate::overlay::dismiss();
}

/// Keep the word a boundary just completed (as it is on screen), for
/// Shift+Backspace right after it.
fn remember_completed(word: &str, boundary_vk: u16, converted: bool) {
    let exe = STATE.with(|s| {
        let mut st = s.borrow_mut();
        st.recent
            .push(word, boundary_literal(boundary_vk), converted);
        st.app_exe.clone()
    });
    if let Some(exe) = exe {
        crate::habits::record_word(&exe, word);
    }
}

/// Switch the focused field to `layout` (the per-field habit, on focus).
///
/// # Safety
/// UI (hook) thread only.
pub unsafe fn switch_layout(layout: policy::InputLayout) {
    // A focus event can be delivered while the keyboard path is running on
    // this thread (it pumps messages while injecting); never re-enter it.
    if STATE.with(|s| s.try_borrow_mut().is_err()) || INJECTING.load(Ordering::Relaxed) {
        return;
    }
    activate_layout(layout);
}

/// Keys that move the caret or change text other than by typing after it.
fn moves_or_edits(vk: u16) -> bool {
    [
        VK_BACK, VK_DELETE, VK_INSERT, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_UP, VK_DOWN, VK_HOME,
        VK_END, VK_PRIOR, VK_NEXT,
    ]
    .iter()
    .any(|k| k.0 == vk)
}

/// Remember which English keyboard (US or UK) the typist uses, from an
/// English layout whenever one is active — Thai text converted to English
/// then comes out in the punctuation of that keyboard.
fn note_english_variant(hkl: HKL) {
    if let Some(v) = policy::english_variant_of(layout_id(hkl)) {
        righttype::layout::set_english_variant(v);
    }
}

/// The foreground window's keyboard when RightType started.
static STARTUP_LAYOUT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Note the keyboard of the window in use, first thing at startup, before
/// any RightType window can take focus.
pub fn remember_startup_layout() {
    let hkl = unsafe { foreground_layout() };
    STARTUP_LAYOUT.store(hkl.0 as isize, std::sync::atomic::Ordering::Relaxed);
}

/// At startup: which table each Thai keyboard follows, and the English
/// keyboard among the installed layouts. Only once: `install` also runs on
/// every hook reinstall (sleep, session change, hook loss), and by then the
/// English keyboard actually used is known.
unsafe fn detect_keyboards() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let mut first = false;
    ONCE.call_once(|| first = true);
    if !first {
        return;
    }
    policy::set_thai_keyboards(thai_keyboards());
    // The keyboard in use when RightType started (read before any of its
    // windows, such as Welcome, could take focus), if it is an English one;
    // otherwise the first English keyboard installed.
    let active = HKL(STARTUP_LAYOUT.load(std::sync::atomic::Ordering::Relaxed) as _);
    if !active.0.is_null() && policy::english_variant_of(layout_id(active)).is_some() {
        note_english_variant(active);
        return;
    }
    let count = GetKeyboardLayoutList(None);
    if count <= 0 {
        return;
    }
    let mut list = vec![HKL::default(); count as usize];
    let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
    if let Some(hkl) = list
        .iter()
        .take(got)
        .find(|h| policy::english_variant_of(layout_id(**h)).is_some())
    {
        note_english_variant(*hkl);
    }
}

/// Windows' Thai keyboards other than the default one, by the high word of
/// their handle (`0xF000` + the registry's "Layout Id"), with the table each
/// follows, told apart by the layout file: `KBDTH0`/`KBDTH2` are Kedmanee
/// (with and without ShiftLock), `KBDTH1`/`KBDTH3` Pattachote.
fn thai_keyboards() -> Vec<(u16, righttype::layout::ThaiVariant)> {
    use righttype::layout::ThaiVariant;
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    fn read(key: &str, value: &str) -> Option<String> {
        let key: Vec<u16> = key.encode_utf16().chain(Some(0)).collect();
        let value: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let mut buf = [0u16; 128];
        let mut size = std::mem::size_of_val(&buf) as u32;
        let status = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key.as_ptr()),
                PCWSTR(value.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if status.is_err() {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    let mut out = Vec::new();
    for klid in ["0001041E", "0002041E", "0003041E"] {
        let key = format!("SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts\\{klid}");
        let Some(file) = read(&key, "Layout File") else {
            continue;
        };
        let file = file.to_ascii_uppercase();
        let variant = if file.starts_with("KBDTH0") || file.starts_with("KBDTH2") {
            ThaiVariant::Kedmanee
        } else if file.starts_with("KBDTH1") || file.starts_with("KBDTH3") {
            ThaiVariant::Pattachote
        } else {
            continue;
        };
        if let Some(id) =
            read(&key, "Layout Id").and_then(|id| u16::from_str_radix(id.trim(), 16).ok())
        {
            out.push((0xF000 | id, variant));
        }
        // The keyboard's own identifier, as some handles carry it.
        if let Ok(high) = u16::from_str_radix(&klid[..4], 16) {
            out.push((high, variant));
        }
    }
    out
}

/// The whole 32-bit handle: [`policy::supported_layout_id`] reads both the
/// language and the keyboard from it, since sharing a language does not make
/// two keyboards' physical-key mappings compatible.
fn layout_id(hkl: HKL) -> u32 {
    hkl.0 as usize as u32
}

/// Switch the foreground window to one exact layout supported by the v1 mapping
/// tables. No-op if that layout is not installed.
unsafe fn activate_layout(target: policy::InputLayout) {
    let layout = target;
    // Already there, or already on its way there.
    if policy::supported_layout_id(layout_id(effective_layout())) == Some(target) {
        return;
    }
    let count = GetKeyboardLayoutList(None);
    if count <= 0 {
        return;
    }
    let mut list = vec![HKL::default(); count as usize];
    let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
    list.truncate(got);
    // With several keyboards for the language (US and UK, Kedmanee and
    // Pattachote), the one whose table is in use comes first.
    list.sort_by_key(|h| !policy::is_preferred_layout(layout_id(*h)));
    for hkl in list.iter() {
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
            // Record that we are waiting for this HKL to activate, and adopt it
            // as the context's layout now: this switch is ours, so it must not
            // read as the context change that drops the token in progress.
            STATE.with(|s| {
                let mut st = s.borrow_mut();
                st.pending_hkl = Some(PendingLayout {
                    hkl: hkl.0 as isize,
                    since: Instant::now(),
                });
                st.last_hkl = hkl.0 as isize;
            });
            // No toast here: a layout switch happens on every ignition, which is
            // too frequent for a message. Just a small TH/EN tag at the caret,
            // where the eyes are (shown after this callback returns).
            crate::caret::layout_switched(layout);
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
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrSuggestInject));
        return;
    }

    let mut restore = format!(
        "{}{}",
        suggestion.original,
        boundary_literal(suggestion.boundary_vk)
    );
    set_undo(
        suggestion.corrected.chars().count() + 1,
        &restore,
        UndoKind::Manual,
    );
    restore.zeroize();
    crate::stats::record_manual();
    habit_correction(
        has_thai(&suggestion.original),
        has_thai(&suggestion.corrected),
    );

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

/// A counted word in this app changed language (see `habits`).
fn habit_correction(was_thai: bool, now_thai: bool) {
    if let Some(exe) = STATE.with(|s| s.borrow().app_exe.clone()) {
        crate::habits::correct_word(&exe, was_thai, now_thai);
    }
}

fn has_thai(text: &str) -> bool {
    text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c))
}

/// Shift+Backspace with no word in progress: flip one more of the recent
/// words back to the other layout (see [`Recent`]). The whole span from that
/// word to the caret is retyped in one injection, and it is one Undo step.
unsafe fn flip_back_recent() {
    let Some(step) = STATE.with(|s| s.borrow().recent.next_step(auto_convert)) else {
        e2e_trace("flip back: no recent word".to_string());
        // Say so: a press that does nothing looks like one that failed.
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastNothingToFlip));
        return;
    };
    if step.insert.chars().count() + 1 == step.backspaces
        && step.restore.strip_suffix(step.boundary) == Some(step.insert.as_str())
    {
        // Nothing changes on screen (a number, say): just move on to the
        // word before it on the next press.
        STATE.with(|s| s.borrow_mut().recent.commit(auto_convert));
        return;
    }
    if !inject::apply(
        step.backspaces,
        &step.insert,
        Some(boundary_vk(step.boundary)),
    ) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
        STATE.with(|s| s.borrow_mut().recent.clear());
        return;
    }
    STATE.with(|s| s.borrow_mut().recent.commit(auto_convert));
    set_undo(
        step.insert.chars().count() + 1,
        &step.restore,
        UndoKind::Manual,
    );
    crate::stats::record_manual();
    // Flipping back a word RightType converted by itself is the clearest
    // "that was a real word" there is.
    if let Some(word) = step.learn.as_deref() {
        crate::learn::learn_now(word);
    }
    habit_correction(step.was_thai, step.now_thai);
    activate_layout(layout_of(&step.newest));
    // Every press says how far back it reached, so the next press is never
    // a guess.
    let more = STATE.with(|s| s.borrow().recent.len()) > step.words;
    if step.reverts {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastUndo));
    } else if step.words > 1 {
        crate::overlay::show(&righttype::i18n::trf(
            righttype::i18n::T::ToastFlippedWords,
            &[("n", &step.words.to_string())],
        ));
    } else {
        crate::overlay::show(righttype::i18n::tr(if more {
            righttype::i18n::T::ToastFlippedOneMore
        } else {
            righttype::i18n::T::ToastFlippedOneBack
        }));
    }
}

/// Manual: flip the layout of the word currently in the buffer, in place. A no-op
/// when the buffer is empty (e.g. an auto-repeat after the word was already
/// converted) — the caller swallows the key either way.
unsafe fn convert_last_word() {
    let mut word = STATE.with(|s| s.borrow().buf.current().to_string());
    if word.is_empty() {
        // Right after a boundary: flip the word before it — and, pressed
        // again, the word before that one too (up to `Recent::CAP`).
        flip_back_recent();
        return;
    }

    let backspaces = word.chars().count();
    let mut converted = auto_convert(&word);
    let changed = converted != word;
    if changed {
        if !inject::apply(backspaces, &converted, None) {
            crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
            word.zeroize();
            converted.zeroize();
            return;
        }
        // The token stays open: the buffer now describes the flipped text, so
        // the typist can carry on and the boundary still sees the whole word.
        // It is theirs now — Auto will not reinterpret it.
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            let learn = st.mark == TokenMark::Converted;
            st.buf.replace(&converted);
            st.mark = TokenMark::Decided { learn };
        });
        set_undo(converted.chars().count(), &word, UndoKind::Manual);
        crate::stats::record_manual();
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ToastFlippedOne));

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

/// Put the raw keystrokes back and stop owning the run. Returns `true` if we
/// were owning anything (and therefore handled the key).
unsafe fn withdraw_owned_run() -> bool {
    let run = STATE.with(|s| s.borrow().buf.current().to_string());
    withdraw_owned_run_to(&run)
}

/// Put `run` (the keys as typed) back in place of the run we own.
unsafe fn withdraw_owned_run_to(run: &str) -> bool {
    let Some(owned) = STATE.with(|s| s.borrow_mut().owned.take()) else {
        return false;
    };
    let delta = render::delta(&owned.rendered, run);
    if !delta.is_empty() && !inject::apply(delta.backspaces, &delta.insert, None) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
    }
    true
}

/// A run we own has reached a word boundary: keep what is on screen, record it
/// for Undo, and let the Thai layout carry the rest of the sentence. Returns
/// the text now on screen for the run.
///
/// The boundary key itself passes through to the app after this, so the Undo
/// record covers it too — otherwise Undo would delete the boundary plus the
/// last rendered character and leave the first one behind.
unsafe fn anchor_owned_run(run: &str, boundary_vk: u16) -> Option<String> {
    let owned = STATE.with(|s| s.borrow_mut().owned.take())?;
    let mut restore = format!("{run}{}", boundary_literal(boundary_vk));
    set_undo(
        owned.rendered.chars().count() + 1,
        &restore,
        UndoKind::AutoWord,
    );
    restore.zeroize();
    crate::stats::record_auto();
    activate_layout(policy::InputLayout::ThaiKedmanee);
    Some(owned.rendered.clone())
}

/// The layout a piece of text is written in, for switching to after a flip.
fn layout_of(text: &str) -> policy::InputLayout {
    if text.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        policy::InputLayout::ThaiKedmanee
    } else {
        policy::InputLayout::UsQwerty
    }
}

/// Move the screen to the current run's best reading, taking ownership of the
/// run's characters if the reading is not simply "as typed".
///
/// Returns `true` when the triggering key was consumed (we rendered the run
/// ourselves and the key must not also reach the app).
unsafe fn reconcile_run() -> bool {
    reconcile_run_with(|backspaces, text, trailing_vk| inject::apply(backspaces, text, trailing_vk))
}

/// [`reconcile_run`] with the injector supplied, so the partial-failure seam
/// can be exercised without Win32.
unsafe fn reconcile_run_with<F>(apply: F) -> bool
where
    F: FnOnce(usize, &str, Option<u16>) -> bool,
{
    let (run, poisoned, holding) = STATE.with(|s| {
        let st = s.borrow();
        (
            st.buf.current().to_string(),
            st.buf.is_poisoned(),
            st.owned.is_some(),
        )
    });

    // An over-long token is dropped unanalysed; we cannot describe the screen
    // any more, so let go of it rather than editing text we cannot account for.
    if poisoned {
        if holding {
            STATE.with(|s| s.borrow_mut().owned = None);
        }
        return false;
    }
    if !holding && run.is_empty() {
        return false;
    }

    let mut reading = policy::live_reading(&run, holding, dict::english(), dict::thai());

    // The seed-phrase stream guard outranks any reading. Mid-word this only
    // asks whether a seed phrase could be in progress; the completed token is
    // counted once, at its boundary.
    if matches!(reading, policy::Reading::Thai(_)) && STATE.with(|s| s.borrow().seed.guarding()) {
        reading = policy::Reading::AsTyped;
    }
    e2e_trace(format!(
        "reconcile run={run:?} holding={holding} -> {reading:?}"
    ));

    let target = match &reading {
        policy::Reading::AsTyped => run.clone(),
        policy::Reading::Thai(thai) => thai.clone(),
    };

    // What the app is showing right now. Before we own the run the current key
    // has not reached the app yet, so the screen holds the run minus that key.
    let on_screen = if holding {
        STATE.with(|s| {
            s.borrow()
                .owned
                .as_ref()
                .map(|o| o.rendered.clone())
                .unwrap_or_default()
        })
    } else {
        if matches!(reading, policy::Reading::AsTyped) {
            // Nothing to do and nothing to own: let the key through untouched.
            return false;
        }
        run.chars()
            .take(run.chars().count().saturating_sub(1))
            .collect()
    };

    let delta = render::delta(&on_screen, &target);
    if !delta.is_empty() && !apply(delta.backspaces, &delta.insert, None) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
        STATE.with(|s| s.borrow_mut().owned = None);
        return false;
    }

    match reading {
        policy::Reading::AsTyped => {
            // The reading was withdrawn: the screen again equals the keystrokes,
            // so the app owns the run once more.
            STATE.with(|s| s.borrow_mut().owned = None);
        }
        policy::Reading::Thai(_) => {
            let anchor = STATE.with(|s| {
                let mut st = s.borrow_mut();
                let stable = st.owned.as_ref().map(|o| o.stable).unwrap_or(0) + 1;
                st.owned = Some(OwnedRun {
                    rendered: target.clone(),
                    stable,
                });
                stable >= policy::COMMIT_HORIZON
            });
            if anchor {
                // Stable long enough to stop second-guessing: hand the rest of
                // the sentence to the Thai layout and release the run.
                //
                // D-009: release the *run*, not the *token*. The buffer is
                // rewritten to what is now on screen, and the native Thai
                // keystrokes that follow extend it, so the boundary still sees
                // the whole word. Clearing it here is what used to leave
                // `กรดดำrent`: the boundary judged only the tail.
                set_undo(target.chars().count(), &run, UndoKind::AutoMidToken);
                crate::stats::record_auto();
                STATE.with(|s| {
                    let mut st = s.borrow_mut();
                    st.owned = None;
                    st.buf.replace(&target);
                    st.mark = TokenMark::Converted;
                });
                activate_layout(policy::InputLayout::ThaiKedmanee);
            }
        }
    }
    true
}

/// Inject a completed `word` after the caller's stream and detection guards pass.
/// `boundary_vk` is the separator that completed the word, re-emitted after the
/// correction. Since D-008 the in-flight path renders through
/// [`reconcile_run`] instead, so this is the boundary path only. Returns `true`
/// if a correction was injected (caller swallows the triggering key).
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
    // Every token character reached the app (the separator itself was
    // swallowed), so delete exactly `word.len()`. The `None` case is kept for
    // callers that swallowed the triggering character before it landed.
    let backspaces = word.chars().count() - usize::from(boundary_vk.is_none());
    let mut corrected = d.corrected;
    if !apply(backspaces, &corrected, boundary_vk) {
        crate::overlay::show(righttype::i18n::tr(righttype::i18n::T::ErrCorrectionInject));
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
        UndoKind::AutoWord,
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

    let hkl = effective_layout();
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
    // latency to the next physical key. While our own request is in flight the
    // context is already on the requested layout (see `effective_layout`);
    // once it lands it is confirmed, and if the app ignores it past the grace
    // period the real layout wins and reads as the context change it then is.
    let actual = GetKeyboardLayout(tid).0 as isize;
    let hkl_i = STATE.with(|s| {
        let mut st = s.borrow_mut();
        match st.pending_hkl {
            Some(p) if p.hkl != actual && p.since.elapsed() < PENDING_LAYOUT_GRACE => p.hkl,
            Some(_) => {
                st.pending_hkl = None;
                actual
            }
            None => actual,
        }
    });

    let focus_generation = crate::focus::generation();
    let window_changed = STATE.with(|s| {
        let mut st = s.borrow_mut();
        let changed = st.last_hwnd != hwnd_i;
        let lang_changed = st.last_hkl != hkl_i;
        let focus_changed = st.last_focus_generation != focus_generation;
        if changed || lang_changed || focus_changed {
            e2e_trace(format!(
                "context changed: window={changed} layout={lang_changed} ({:X} -> {hkl_i:X}) focus={focus_changed}",
                st.last_hkl
            ));
            // A layout switch alone must NOT drop the pending Undo: it is
            // usually our own correction switching the layout, and the very
            // next keypress (e.g. the Undo hotkey itself) would otherwise
            // erase the record it is about to use. Window/focus changes are
            // genuine context loss.
            if changed || focus_changed {
                st.undo = None;
            }
            st.buf.clear();
            st.owned = None;
            st.mark = TokenMark::Plain;
            st.seed.reset();
            st.recent.clear();
            st.suggestion = None;
            st.live_hint = None;
            st.last_hwnd = hwnd_i;
            st.last_hkl = hkl_i;
            st.last_focus_generation = focus_generation;
        }
        changed
    });
    // Re-evaluate the (heavier) app blacklist only when the window changed.
    if window_changed {
        let exe = safety::foreground_exe(hwnd);
        // Unknown process identity is not evidence that a context is safe.
        let blacklisted = exe.as_deref().map_or(true, safety::is_blacklisted_name);
        if let Some(exe) = exe.as_deref() {
            crate::apps::note_typing_in(exe);
        }
        STATE.with(|s| {
            let mut st = s.borrow_mut();
            st.sensitive_app = blacklisted;
            st.app_exe = exe;
        });
    }
}

/// The layout the focused app is typing in *now*: our own switch request while
/// it is in flight, otherwise what Windows reports.
///
/// The target app handles the posted `WM_INPUTLANGCHANGEREQUEST` before the
/// next keystroke's input message (posted messages are retrieved first), so a
/// key pressed right after an anchor already produces Thai there, while
/// `GetKeyboardLayout` can still say English for a moment. Translating with
/// the stale answer put Latin letters in a buffer describing Thai text.
unsafe fn effective_layout() -> HKL {
    let actual = foreground_layout();
    match STATE.with(|s| s.borrow().pending_hkl) {
        Some(p) if p.hkl != actual.0 as isize && p.since.elapsed() < PENDING_LAYOUT_GRACE => {
            HKL(p.hkl as _)
        }
        _ => actual,
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
    use super::{Mode, OwnedRun, STATE};
    use righttype::detect::{Confidence, Detection, Evidence};

    #[test]
    fn mode_cycle_is_manual_auto_suggest() {
        assert_eq!(Mode::Manual.next(), Mode::Auto);
        assert_eq!(Mode::Auto.next(), Mode::Suggest);
        assert_eq!(Mode::Suggest.next(), Mode::Manual);
    }

    /// A failed injection while we own a run must let go of it. Keeping
    /// ownership would leave the next keystroke diffing against text we were
    /// never able to put on screen, and every edit after that would be
    /// computed from a screen state that does not exist.
    #[test]
    fn failed_reconcile_injection_releases_ownership() {
        use righttype::buffer::Key;

        STATE.with(|state| {
            let mut st = state.borrow_mut();
            st.buf.clear();
            for c in "l;ylfu".chars() {
                st.buf.observe(Key::Char(c));
            }
            // Own the run, but with nothing yet rendered, so the reconciler has
            // a non-empty delta and must call the injector.
            st.owned = Some(OwnedRun {
                rendered: String::new(),
                stable: 1,
            });
        });
        let stats_before = crate::stats::snapshot();

        let consumed = unsafe { super::reconcile_run_with(|_backspaces, _text, _vk| false) };

        assert!(!consumed, "a failed injection must not swallow the key");
        STATE.with(|state| {
            assert!(
                state.borrow().owned.is_none(),
                "ownership must be released when the screen could not be updated"
            );
        });
        assert_eq!(crate::stats::snapshot(), stats_before);
        STATE.with(|state| state.borrow_mut().buf.clear());
    }

    #[test]
    fn failed_auto_injection_has_no_success_side_effects() {
        STATE.with(|state| {
            let mut state = state.borrow_mut();
            state.undo = None;
            state.pending_hkl = None;
            state.owned = None;
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
