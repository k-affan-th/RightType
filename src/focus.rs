//! Focus-based password-field detection via UI Automation — Windows only.
//!
//! `ES_PASSWORD` (in `safety`) only sees native edit controls; password fields in
//! browsers, Electron, and UWP apps are not Win32 controls, so the user's question
//! "how do you even know it's a password box in a browser?" is exactly right — by
//! style alone, we can't. UI Automation *does* expose their `IsPassword` property,
//! but UIA is far too slow to call per keystroke. So we install a **WinEvent focus
//! hook** and query UIA only when focus moves, caching the answer in an atomic the
//! keyboard hook reads for free.
//!
//! Failure is conservative: if COM/UIA init or a query fails, the cache remains
//! `UNKNOWN` and the keyboard pipeline treats the field as protected until UIA
//! explicitly reports a safe focus.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, SetWinEventHook, UIA_DataItemControlTypeId,
    UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_TreeItemControlTypeId,
    UnhookWinEvent, HWINEVENTHOOK, UIA_CONTROLTYPE_ID,
};
use windows::Win32::UI::WindowsAndMessaging::{EVENT_OBJECT_FOCUS, WINEVENT_OUTOFCONTEXT};

const FIELD_UNKNOWN: u8 = 0;
const FIELD_SAFE: u8 = 1;
const FIELD_PASSWORD: u8 = 2;

/// Cached UIA result, read cheaply by the keyboard hook on every keystroke.
static FIELD_STATUS: AtomicU8 = AtomicU8::new(FIELD_UNKNOWN);
/// The focused field completes what is typed in place — a browser address
/// bar, which selects its suggestion after the caret. See [`completes_inline`].
static INLINE_COMPLETION: AtomicBool = AtomicBool::new(false);
/// Changes whenever Windows reports that the focused UI element changed.  The
/// keyboard hook uses this to invalidate text that belongs to an old caret,
/// including two controls inside the same top-level window.
static FOCUS_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The focused field's identity (a hash of its UI Automation runtime id; 0
/// when unknown), for "Off in this field".
static FIELD_KEY: AtomicU64 = AtomicU64::new(0);
/// Fields RightType was switched off in (palette → "Off in this field").
/// Identities only, in memory: gone when RightType exits.
static FIELDS_OFF: std::sync::Mutex<Vec<u64>> = std::sync::Mutex::new(Vec::new());
/// At most this many fields are remembered as off; the oldest goes first.
const MAX_FIELDS_OFF: usize = 32;

/// The focused field's identity, or 0 when UI Automation does not say.
pub fn field_key() -> u64 {
    FIELD_KEY.load(Ordering::Relaxed)
}

/// Is RightType switched off in the focused field?
pub fn field_is_off() -> bool {
    is_off(field_key())
}

/// Is RightType switched off in the field `key`?
pub fn is_off(key: u64) -> bool {
    key != 0 && FIELDS_OFF.lock().is_ok_and(|f| f.contains(&key))
}

/// Switch RightType off (or back on) in the field `key`.
pub fn set_field_off(key: u64, off: bool) {
    if key == 0 {
        return;
    }
    let Ok(mut fields) = FIELDS_OFF.lock() else {
        return;
    };
    fields.retain(|&k| k != key);
    if off {
        if fields.len() == MAX_FIELDS_OFF {
            fields.remove(0);
        }
        fields.push(key);
    }
}

/// A field's identity: its runtime id, hashed (FNV-1a). Unique while the
/// element exists; a page that rebuilds its fields gives them new ones.
unsafe fn runtime_key(element: &IUIAutomationElement) -> u64 {
    use windows::Win32::System::Ole::{
        SafeArrayDestroy, SafeArrayGetElement, SafeArrayGetLBound, SafeArrayGetUBound,
    };
    let Ok(sa) = element.GetRuntimeId() else {
        return 0;
    };
    if sa.is_null() {
        return 0;
    }
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    if let (Ok(lo), Ok(hi)) = (SafeArrayGetLBound(sa, 1), SafeArrayGetUBound(sa, 1)) {
        for i in lo..=hi {
            let mut v = 0i32;
            if SafeArrayGetElement(sa, &i, &mut v as *mut i32 as *mut _).is_ok() {
                for b in v.to_le_bytes() {
                    hash = (hash ^ u64::from(b)).wrapping_mul(0x100_0000_01b3);
                }
            }
        }
    }
    let _ = SafeArrayDestroy(sa);
    hash.max(1)
}

thread_local! {
    static UIA: RefCell<Option<IUIAutomation>> = const { RefCell::new(None) };
    static HOOK: RefCell<Option<HWINEVENTHOOK>> = const { RefCell::new(None) };
    /// The element that last took focus as a field (see [`moves_to_another_field`]).
    static FIELD: RefCell<Option<IUIAutomationElement>> = const { RefCell::new(None) };
}

/// Is the currently focused element a password field (per UIA)?
pub fn is_password_field() -> bool {
    status_is_protected(FIELD_STATUS.load(Ordering::Relaxed))
}

fn status_is_protected(status: u8) -> bool {
    status != FIELD_SAFE
}

/// Does the focused field fill in the rest of what is typed and select it
/// (a browser address bar with a matching history entry)? The first
/// Backspace of a correction would then only remove that selection and leave
/// one mistyped character behind, so [`crate::inject`] clears it first.
pub fn completes_inline() -> bool {
    INLINE_COMPLETION.load(Ordering::Relaxed)
}

/// Address bars that complete inline, by UI Automation class name
/// (Chromium: Chrome, Edge, Brave, Opera, Vivaldi) or automation id (Firefox).
fn is_inline_completing(class_name: &str, automation_id: &str) -> bool {
    class_name == "OmniboxViewViews" || automation_id == "urlbar-input"
}

/// Monotonically increasing identity for the current focused UI element.
///
/// This is deliberately cheaper than querying UI Automation from the keyboard
/// hook.  It is best-effort: native controls still have the top-level-window
/// guard in the hook if an app does not publish focus events.
pub fn generation() -> u64 {
    FOCUS_GENERATION.load(Ordering::Relaxed)
}

/// Initialise COM + UIA and install the focus hook. Best-effort.
///
/// # Safety
/// UI thread only; call [`disarm`] before exit.
pub unsafe fn arm() {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    if let Ok(uia) =
        CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
    {
        bound_waits(&uia);
        UIA.with(|u| *u.borrow_mut() = Some(uia));
    }
    start_worker();
    wake_worker();
    let _ = context_worker();
    let hook = SetWinEventHook(
        EVENT_OBJECT_FOCUS,
        EVENT_OBJECT_FOCUS,
        None,
        Some(on_focus),
        0,
        0,
        WINEVENT_OUTOFCONTEXT,
    );
    HOOK.with(|h| *h.borrow_mut() = Some(hook));
}

/// Where the focus worker reports "the caret moved to another field", so the
/// per-field habit switch runs on the UI thread (the hook's state lives
/// there). Set by the tray once its window exists.
static NOTIFY_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// Posted to [`NOTIFY_HWND`] when the caret moved to another field.
pub const WM_FOCUS_MOVED: u32 = 0x8000 + 0x551;

pub fn set_notify_window(hwnd: isize) {
    NOTIFY_HWND.store(hwnd, Ordering::Release);
}

static WORKER: std::sync::OnceLock<std::sync::mpsc::SyncSender<()>> = std::sync::OnceLock::new();
/// Focus events seen, and how many of them the worker has answered.
static ASKED: AtomicU64 = AtomicU64::new(0);
static ANSWERED: AtomicU64 = AtomicU64::new(0);

/// Before the keyboard hook handles a key: let the focus worker finish with
/// the focus events that came before it, for at most `max`. The answers (a
/// new field, a password field) must apply to the keys typed after the
/// click; done in the background they arrived a few keys late and the first
/// word of a field came out wrong (CI). Normally a few milliseconds; a hung
/// app costs `max` per key, and its keys go nowhere meanwhile anyway.
pub fn settle(max: std::time::Duration) {
    let asked = ASKED.load(Ordering::Acquire);
    if ANSWERED.load(Ordering::Acquire) >= asked {
        return;
    }
    let until = std::time::Instant::now() + max;
    while ANSWERED.load(Ordering::Acquire) < asked && std::time::Instant::now() < until {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// The focus questions (which element, is it a password field, does it
/// complete inline) go to the app through UI Automation, which waits on the
/// app — seconds for a hung one, and the cap set on the client does not
/// cover GetFocusedElement (CI's slow-window test: the UI thread, and with it
/// the keyboard hook and the tray, stuck there). So they are asked on a
/// thread of their own; the hook reads the answers from atomics.
fn start_worker() {
    let (tx, rx) = std::sync::mpsc::sync_channel::<()>(1);
    if WORKER.set(tx).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("focus".into())
        .spawn(move || {
            while rx.recv().is_ok() {
                // Focus events come in bursts: answer once for all of them.
                while rx.try_recv().is_ok() {}
                let asked = ASKED.load(Ordering::Acquire);
                unsafe { on_focus_inner() };
                ANSWERED.fetch_max(asked, Ordering::AcqRel);
            }
        });
}

/// Ask the focus worker to look again (never waits; a pending ask covers it).
fn wake_worker() {
    ASKED.fetch_add(1, Ordering::AcqRel);
    if let Some(tx) = WORKER.get() {
        let _ = tx.try_send(());
    }
}

/// Remove the focus hook.
///
/// # Safety
/// Same thread that called [`arm`].
pub unsafe fn disarm() {
    HOOK.with(|h| {
        if let Some(hook) = h.borrow_mut().take() {
            let _ = UnhookWinEvent(hook);
        }
    });
}

unsafe extern "system" fn on_focus(
    _hook: HWINEVENTHOOK,
    _event: u32,
    hwnd: HWND,
    idobj: i32,
    idchild: i32,
    _thread: u32,
    _time: u32,
) {
    thread_local!(static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) });
    let depth = DEPTH.with(|d| d.replace(d.get() + 1));
    crate::hook::e2e_trace(format!(
        "focus event from hwnd={:#x} obj={idobj} child={idchild} depth={depth}",
        hwnd.0 as usize
    ));
    wake_worker();
    DEPTH.with(|d| d.set(depth));
}

/// On the focus worker thread.
unsafe fn on_focus_inner() {
    let started = std::time::Instant::now();
    let moved = moves_to_another_field();
    if moved {
        FOCUS_GENERATION.fetch_add(1, Ordering::Relaxed);
        refresh_status();
    }
    crate::hook::e2e_trace(format!(
        "focus event handled in {} ms (moved={moved})",
        started.elapsed().as_millis()
    ));
    if !moved {
        return;
    }
    righttype::diag::note(
        "caret moved to another field",
        &[("password", is_password_field().into())],
    );
    // After the password check above: the habit switch never runs in one.
    // It changes the hook's state, so it runs on the UI thread.
    let hwnd = NOTIFY_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            HWND(hwnd as *mut _),
            WM_FOCUS_MOVED,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );
    }
}

/// Whether a focus event means the caret went to another field.
///
/// Not when the element is the field that already had focus, nor when it is
/// a row of a list or menu: a browser's address-bar suggestions give the
/// highlighted row accessibility focus (so screen readers read it out) while
/// the keys still go to the address bar. Edge does that on almost every
/// keystroke, and counting it as a move made RightType forget the word being
/// typed and convert only its end (`l;ylfu` became `l;ัสดี`). Such events
/// also leave the password status alone: it still describes the field.
unsafe fn moves_to_another_field() -> bool {
    let uia = uia_here();
    UIA.with(|_| {
        let Some(uia) = uia else {
            return true;
        };
        let Ok(element) = uia.GetFocusedElement() else {
            FIELD.with(|f| *f.borrow_mut() = None);
            FIELD_KEY.store(0, Ordering::Relaxed);
            return true;
        };
        let same = FIELD.with(|f| {
            f.borrow().as_ref().is_some_and(|last| {
                uia.CompareElements(last, &element)
                    .map(|b| b.as_bool())
                    .unwrap_or(false)
            })
        });
        let row = element.CurrentControlType().is_ok_and(is_list_row);
        crate::hook::e2e_trace(format!("focus event: same={same} row={row}"));
        if same || row {
            return false;
        }
        FIELD_KEY.store(runtime_key(&element), Ordering::Relaxed);
        FIELD.with(|f| *f.borrow_mut() = Some(element));
        true
    })
}

/// Rows of a list, menu, grid or tree: what a suggestion dropdown is made of.
fn is_list_row(control_type: UIA_CONTROLTYPE_ID) -> bool {
    [
        UIA_ListItemControlTypeId,
        UIA_MenuItemControlTypeId,
        UIA_DataItemControlTypeId,
        UIA_TreeItemControlTypeId,
    ]
    .contains(&control_type)
}

unsafe fn refresh_status() {
    let mut inline = false;
    let uia = uia_here();
    let status = UIA.with(|_| {
        uia.as_ref()
            .and_then(|uia| {
                let el = uia.GetFocusedElement().ok()?;
                let class = el
                    .CurrentClassName()
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                let id = el
                    .CurrentAutomationId()
                    .map(|b| b.to_string())
                    .unwrap_or_default();
                inline = is_inline_completing(&class, &id);
                el.CurrentIsPassword().ok().map(|b| b.as_bool())
            })
            .map(|is_password| {
                if is_password {
                    FIELD_PASSWORD
                } else {
                    FIELD_SAFE
                }
            })
            .unwrap_or(FIELD_UNKNOWN)
    });
    crate::hook::e2e_trace(format!("field status={status} inline={inline}"));
    FIELD_STATUS.store(status, Ordering::Relaxed);
    INLINE_COMPLETION.store(inline, Ordering::Relaxed);
}

/// This thread's UI Automation client, made on first use. The UI thread's
/// is made by [`arm`]; the selection worker gets its own.
/// How long one UI Automation call may wait on an app. Windows' default is
/// several seconds per call: a hung app then held RightType's UI thread (and
/// with it the keyboard hook and the tray) for that long on every focus
/// change — CI's slow-window test caught it now and then. Nothing RightType
/// asks of an app is worth more than this.
const UIA_WAIT_MS: u32 = 500;

/// Cap this client's waits ([`UIA_WAIT_MS`]); needs Windows 8 or later
/// (IUIAutomation2), and is skipped where that is missing.
fn bound_waits(uia: &IUIAutomation) {
    use windows::core::Interface;
    if let Ok(two) = uia.cast::<windows::Win32::UI::Accessibility::IUIAutomation2>() {
        unsafe {
            let _ = two.SetConnectionTimeout(UIA_WAIT_MS);
            let _ = two.SetTransactionTimeout(UIA_WAIT_MS);
        }
    }
}

fn uia_here() -> Option<IUIAutomation> {
    UIA.with(|u| {
        if u.borrow().is_none() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                if let Ok(uia) =
                    CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                {
                    bound_waits(&uia);
                    *u.borrow_mut() = Some(uia);
                }
            }
        }
        u.borrow().clone()
    })
}

/// The text selected in the focused field, asked of the app through UI
/// Automation — without the clipboard, which Windows may keep in its
/// history, sync to other devices, and show to every program watching it.
/// `None` when the app does not say (no text pattern, nothing selected, a
/// password field). Any thread.
pub fn selected_text() -> Option<zeroize::Zeroizing<String>> {
    uia_selected_text().or_else(edit_selected_text)
}

fn uia_selected_text() -> Option<zeroize::Zeroizing<String>> {
    use windows::Win32::UI::Accessibility::{IUIAutomationTextPattern, UIA_TextPatternId};
    let step = |what: &str| crate::hook::e2e_trace(format!("selection (UIA): {what}"));
    let Some(uia) = uia_here() else {
        step("no UI Automation client");
        return None;
    };
    unsafe {
        let Ok(element) = uia.GetFocusedElement() else {
            step("no focused element");
            return None;
        };
        if element.CurrentIsPassword().map_or(true, |b| b.as_bool()) {
            step("password field (or unknown)");
            return None;
        }
        let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        else {
            step("no text pattern");
            return None;
        };
        let Ok(ranges) = pattern.GetSelection() else {
            step("no selection ranges");
            return None;
        };
        let mut text = zeroize::Zeroizing::new(String::new());
        for i in 0..ranges.Length().unwrap_or(0) {
            let range = ranges.GetElement(i).ok()?;
            text.push_str(&range.GetText(-1).ok()?.to_string());
        }
        if text.is_empty() {
            step("empty selection");
            return None;
        }
        Some(text)
    }
}

/// The focused standard Windows text box (Edit, RichEdit: classic and
/// Windows 11 Notepad, WordPad, many dialogs), which can be asked and told
/// things directly with its own messages — Windows copies their text
/// between processes. Never a password box.
pub struct TextBox {
    hwnd: windows::Win32::Foundation::HWND,
    rich: bool,
}

impl TextBox {
    /// The focused window of the foreground app, when it is such a box.
    pub fn focused() -> Result<TextBox, &'static str> {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW,
            GetWindowThreadProcessId, GUITHREADINFO, GWL_STYLE,
        };
        const ES_PASSWORD: i32 = 0x0020;
        unsafe {
            let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let mut gui = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            if GetGUIThreadInfo(thread, &mut gui).is_err() || gui.hwndFocus.0.is_null() {
                return Err("no focused window");
            }
            let hwnd = gui.hwndFocus;
            let mut class = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut class) as usize;
            let class = String::from_utf16_lossy(&class[..n]);
            let rich = class.to_ascii_lowercase().starts_with("richedit");
            if !(class.eq_ignore_ascii_case("Edit") || rich) {
                return Err("not a text box");
            }
            if GetWindowLongW(hwnd, GWL_STYLE) & ES_PASSWORD != 0 {
                return Err("password box");
            }
            Ok(TextBox { hwnd, rich })
        }
    }

    /// Send `msg` and wait (bounded) for the answer.
    fn ask(&self, msg: u32, w: usize, l: isize) -> Option<usize> {
        self.ask_within(msg, w, l, 300)
    }

    fn ask_within(&self, msg: u32, w: usize, l: isize, ms: u32) -> Option<usize> {
        // Inside the keyboard hook, every wait of one replacement shares one
        // budget (see `HOOK_BUDGET`).
        let ms = match REPLACE_DEADLINE.with(|d| d.get()) {
            Some(deadline) => {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    return None;
                }
                ms.min(left.as_millis().max(1) as u32)
            }
            None => ms,
        };
        // While this waits, Windows may hand the keyboard hook the next key
        // on this thread (see `waiting_on_app`).
        struct Waiting;
        impl Drop for Waiting {
            fn drop(&mut self) {
                WAITING_ON_APP.with(|w| w.set(w.get() - 1));
            }
        }
        WAITING_ON_APP.with(|w| w.set(w.get() + 1));
        let _waiting = Waiting;
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, SMTO_ABORTIFHUNG};
        let mut result = 0usize;
        let ok = unsafe {
            SendMessageTimeoutW(
                self.hwnd,
                msg,
                WPARAM(w),
                LPARAM(l),
                SMTO_ABORTIFHUNG,
                ms,
                Some(&mut result),
            )
        };
        (ok.0 != 0).then_some(result)
    }

    /// The selection, in UTF-16 positions (16 bits each: EM_GETSEL's limit).
    fn selection(&self) -> Option<(usize, usize)> {
        const EM_GETSEL: u32 = 0x00B0;
        let sel = self.ask(EM_GETSEL, 0, 0)?;
        Some((sel & 0xFFFF, (sel >> 16) & 0xFFFF))
    }

    /// Replace the `delete` UTF-16 units before the caret with `text`, as one
    /// edit the box can undo. Only with a bare caret (nothing selected) and
    /// enough text before it; checked afterwards by where the caret ended up.
    /// `Err` before anything changed means the caller may fall back to keys.
    /// How many characters before `caret` to replace, when the last `delete`
    /// characters of `context` are what the correction replaces (see
    /// [`righttype::render::chars_on_screen`]).
    fn chars_to_replace(
        &self,
        caret: usize,
        context: &str,
        delete: usize,
    ) -> Result<usize, ReplaceError> {
        use windows::Win32::UI::WindowsAndMessaging::{WM_GETTEXT, WM_GETTEXTLENGTH};
        use zeroize::Zeroize;
        let len = self
            .ask(WM_GETTEXTLENGTH, 0, 0)
            .ok_or(ReplaceError::Untouched("no answer"))?;
        if len > 0xFFFF || caret > len {
            return Err(ReplaceError::Untouched("caret position out of reach"));
        }
        let mut units = vec![0u16; len + 1];
        let got = self
            .ask(WM_GETTEXT, units.len(), units.as_mut_ptr() as isize)
            .ok_or(ReplaceError::Untouched("no answer"))?
            .min(len);
        // RichEdit counts a line break as one position and WM_GETTEXT as two.
        let usable = caret <= got && !(self.rich && units[..caret].contains(&(b'\n' as u16)));
        let mut before = if usable {
            String::from_utf16_lossy(&units[..caret])
        } else {
            String::new()
        };
        units.zeroize();
        if !usable {
            return Err(ReplaceError::Untouched(
                "text before the caret out of reach",
            ));
        }
        let keep: String = {
            let n = context.chars().count().saturating_sub(delete);
            context.chars().take(n).collect()
        };
        let replaced: String = context.chars().skip(keep.chars().count()).collect();
        let whole = righttype::render::chars_on_screen(context, &before);
        let part = righttype::render::chars_on_screen(&replaced, &before);
        before.zeroize();
        match (whole, part) {
            (Some(_), Some(n)) => Ok(n),
            _ => Err(ReplaceError::Untouched(
                "the box has not caught up with the keys",
            )),
        }
    }

    /// The replacement was sent but not answered in time. Give the box up to
    /// 300 ms more, then read what is before the caret: the correction in
    /// place is success; the old text unchanged means nothing happened (keys
    /// may do it); anything else is unknown.
    fn settle_after_slow_replace(
        &self,
        context: Option<&str>,
        delete: usize,
        text: &str,
    ) -> Result<(), ReplaceError> {
        use zeroize::Zeroize;
        const WM_NULL: u32 = 0;
        let unknown = ReplaceError::Unknown("no answer to the replacement");
        let Some(context) = context else {
            return Err(unknown);
        };
        // This runs inside the keyboard hook: keep the wait short (Windows
        // drops a hook that holds keys too long).
        if self.ask_within(WM_NULL, 0, 0, 300).is_none() {
            return Err(unknown);
        }
        let Some((a, b)) = self.selection() else {
            return Err(unknown);
        };
        if a != b {
            return Err(unknown);
        }
        let Some(mut before) = self.text_before(a) else {
            return Err(unknown);
        };
        let kept: String = {
            let n = context.chars().count().saturating_sub(delete);
            context.chars().take(n).collect()
        };
        let mut done = format!("{kept}{text}");
        let result = if before.ends_with(done.as_str()) {
            Ok(())
        } else if before.ends_with(context) {
            Err(ReplaceError::Untouched("the replacement did not happen"))
        } else {
            Err(unknown)
        };
        before.zeroize();
        done.zeroize();
        crate::hook::e2e_trace(format!(
            "text box: slow replacement settled: {}",
            result.is_ok()
        ));
        result
    }

    /// The box's text before position `caret`, if it can be located.
    fn text_before(&self, caret: usize) -> Option<String> {
        use windows::Win32::UI::WindowsAndMessaging::{WM_GETTEXT, WM_GETTEXTLENGTH};
        use zeroize::Zeroize;
        let len = self.ask(WM_GETTEXTLENGTH, 0, 0)?;
        if len > 0xFFFF || caret > len {
            return None;
        }
        let mut units = vec![0u16; len + 1];
        let got = self
            .ask(WM_GETTEXT, units.len(), units.as_mut_ptr() as isize)?
            .min(len);
        let usable = caret <= got && !(self.rich && units[..caret].contains(&(b'\n' as u16)));
        let text = usable.then(|| String::from_utf16_lossy(&units[..caret]));
        units.zeroize();
        text
    }

    /// Debug e2e trace: the caret, and the text before it, as the box holds
    /// them right before a replacement.
    #[cfg(debug_assertions)]
    fn trace_around(&self, caret: usize, delete: usize) {
        use windows::Win32::UI::WindowsAndMessaging::{WM_GETTEXT, WM_GETTEXTLENGTH};
        let len = self.ask(WM_GETTEXTLENGTH, 0, 0).unwrap_or(0).min(0xFFFF);
        let mut units = vec![0u16; len + 1];
        let got = self
            .ask(WM_GETTEXT, units.len(), units.as_mut_ptr() as isize)
            .unwrap_or(0)
            .min(len);
        let before = String::from_utf16_lossy(&units[..caret.min(got)]);
        crate::hook::e2e_trace(format!(
            "text box: caret={caret} length={got} delete={delete} before caret={before:?}"
        ));
    }

    pub fn replace_before_caret(
        &self,
        delete: usize,
        text: &str,
        context: Option<&str>,
    ) -> Result<(), ReplaceError> {
        // In the keyboard hook, all the waits on the box together stay under
        // what Windows allows a hook: past that it lets the key through
        // itself (CI: a fresh Notepad slow to answer, and the `u` that
        // finished `l;ylfu` landed after สวัสดี).
        struct Budget;
        impl Drop for Budget {
            fn drop(&mut self) {
                REPLACE_DEADLINE.with(|d| d.set(None));
            }
        }
        let _budget = crate::hook::in_hook().then(|| {
            REPLACE_DEADLINE.with(|d| d.set(Some(std::time::Instant::now() + HOOK_BUDGET)));
            Budget
        });
        self.replace_within_budget(delete, text, context)
    }

    fn replace_within_budget(
        &self,
        delete: usize,
        text: &str,
        context: Option<&str>,
    ) -> Result<(), ReplaceError> {
        const EM_SETSEL: u32 = 0x00B1;
        const EM_REPLACESEL: u32 = 0x00C2;
        let (start, end) = self
            .selection()
            .ok_or(ReplaceError::Untouched("no answer"))?;
        if start != end {
            return Err(ReplaceError::Untouched("text is selected"));
        }
        let delete = match context {
            Some(context) => self.chars_to_replace(start, context, delete)?,
            None => delete,
        };
        if start < delete || start >= 0xFFFF {
            return Err(ReplaceError::Untouched("caret position out of reach"));
        }
        let from = start - delete;
        #[cfg(debug_assertions)]
        self.trace_around(start, delete);
        self.ask(EM_SETSEL, from, start as isize)
            .ok_or(ReplaceError::Untouched("could not select"))?;
        if self.selection() != Some((from, start)) {
            // Put the caret back where it was before giving the job to keys.
            let _ = self.ask(EM_SETSEL, start, start as isize);
            return Err(ReplaceError::Untouched("selection did not take"));
        }
        let mut units: zeroize::Zeroizing<Vec<u16>> =
            zeroize::Zeroizing::new(text.encode_utf16().chain(std::iter::once(0)).collect());
        // wParam 1: the replacement can be undone (Ctrl+Z in the app).
        if self
            .ask(EM_REPLACESEL, 1, units.as_mut_ptr() as isize)
            .is_none()
        {
            // A slow box may still do it (Windows 11 Notepad, CI): wait for
            // it, then see what it holds rather than guess.
            return self.settle_after_slow_replace(context, delete, text);
        }
        let expected = from + units.len() - 1;
        match self.selection() {
            Some((a, b)) if a == expected && b == expected => Ok(()),
            _ => Err(ReplaceError::Unknown(
                "caret not where the replacement should leave it",
            )),
        }
    }
}

thread_local! {
    static WAITING_ON_APP: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    /// When the waits of a replacement made in the keyboard hook must end.
    static REPLACE_DEADLINE: std::cell::Cell<Option<std::time::Instant>> =
        const { std::cell::Cell::new(None) };
}

/// All the waits on a text box for one replacement in the keyboard hook.
/// Windows' own limit for a hook (`LowLevelHooksTimeout`) is a few hundred
/// milliseconds; the hook's other work fits in what is left.
const HOOK_BUDGET: std::time::Duration = std::time::Duration::from_millis(200);

/// This thread is waiting for a text box to answer (SendMessageTimeout).
/// The wait lets Windows call the keyboard hook again for the next key; that
/// key must not be handled then (the word on screen is about to change).
pub fn waiting_on_app() -> bool {
    WAITING_ON_APP.with(|w| w.get() > 0)
}

/// Why [`TextBox::replace_before_caret`] did not do the job.
#[derive(Debug)]
pub enum ReplaceError {
    /// Nothing was changed: typing the correction as keys is safe.
    Untouched(&'static str),
    /// The box may have been changed: do not type the correction again.
    Unknown(&'static str),
}

/// The selection of a standard Windows text box ([`TextBox`]): `EM_GETSEL`
/// for where, `WM_GETTEXT` for the text. No clipboard. The whole text passes
/// through this process for a moment and is wiped.
fn edit_selected_text() -> Option<zeroize::Zeroizing<String>> {
    use windows::Win32::UI::WindowsAndMessaging::{WM_GETTEXT, WM_GETTEXTLENGTH};
    use zeroize::Zeroize;
    let step = |what: &str| crate::hook::e2e_trace(format!("selection (edit control): {what}"));
    let tb = match TextBox::focused() {
        Ok(tb) => tb,
        Err(why) => {
            step(why);
            return None;
        }
    };
    let (start, end) = tb.selection()?;
    if end <= start {
        step("nothing selected");
        return None;
    }
    // EM_GETSEL reports positions in 16 bits.
    let len = tb.ask(WM_GETTEXTLENGTH, 0, 0)?;
    if len > 0xFFFF {
        step("text too long to locate the selection");
        return None;
    }
    let mut units = vec![0u16; len + 1];
    let got = tb
        .ask(WM_GETTEXT, units.len(), units.as_mut_ptr() as isize)?
        .min(len);
    // RichEdit counts a line break as one position but WM_GETTEXT gives two
    // characters, so past one the positions no longer line up: refuse rather
    // than convert the wrong text.
    let shifted = tb.rich && units[..end.min(got)].contains(&(b'\n' as u16));
    let text = (end <= got && !shifted)
        .then(|| zeroize::Zeroizing::new(String::from_utf16_lossy(&units[start..end])));
    units.zeroize();
    if text.is_none() {
        step("selection cannot be located in the text");
    }
    text
}

/// A question for the context worker: how many characters, and where to
/// answer.
enum ContextAsk {
    Text(
        usize,
        std::sync::mpsc::SyncSender<Option<zeroize::Zeroizing<String>>>,
    ),
    Boxes(
        Vec<(usize, usize)>,
        std::sync::mpsc::SyncSender<Vec<Option<windows::Win32::Foundation::RECT>>>,
    ),
}
static CONTEXT_WORKER: std::sync::OnceLock<std::sync::mpsc::SyncSender<ContextAsk>> =
    std::sync::OnceLock::new();

/// Up to `n` characters before the caret, for the keyboard hook, which must not make the
/// cross-process calls itself: asked of a worker, waiting at most `max`
/// (`None` past that — the caller treats it as "cannot tell"). Code mode
/// asks this at a word's end, only for a word it would otherwise fix.
pub fn text_before_caret_within(
    n: usize,
    max: std::time::Duration,
) -> Option<zeroize::Zeroizing<String>> {
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    context_worker()
        .try_send(ContextAsk::Text(n, reply_tx))
        .ok()?;
    reply_rx.recv_timeout(max).ok().flatten()
}

fn context_worker() -> &'static std::sync::mpsc::SyncSender<ContextAsk> {
    CONTEXT_WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::sync_channel::<ContextAsk>(1);
        let _ = std::thread::Builder::new()
            .name("context".into())
            .spawn(move || {
                // Ready before the first question: making the UI Automation
                // client takes longer than the hook waits for an answer.
                let _ = uia_here();
                while let Ok(ask) = rx.recv() {
                    match ask {
                        ContextAsk::Text(n, reply) => {
                            let _ = reply.try_send(text_before_caret_up_to(n));
                        }
                        ContextAsk::Boxes(spans, reply) => {
                            let _ = reply.try_send(boxes_before_caret(&spans));
                        }
                    }
                }
            });
        tx
    })
}

/// Where pieces of the text before the caret are on screen: for each
/// `(start, len)` — starting `start` characters before the caret, `len`
/// long — its box in screen pixels, or `None`. Asked of the app through UI
/// Automation on a worker, waiting at most `max`; empty when the app does
/// not say. For showing which words a palette command would change.
pub fn boxes_before_caret_within(
    spans: Vec<(usize, usize)>,
    max: std::time::Duration,
) -> Vec<Option<windows::Win32::Foundation::RECT>> {
    let (reply_tx, reply_rx) = std::sync::mpsc::sync_channel(1);
    if context_worker()
        .try_send(ContextAsk::Boxes(spans, reply_tx))
        .is_err()
    {
        return Vec::new();
    }
    reply_rx.recv_timeout(max).unwrap_or_default()
}

fn boxes_before_caret(spans: &[(usize, usize)]) -> Vec<Option<windows::Win32::Foundation::RECT>> {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationTextPattern, TextPatternRangeEndpoint_End, TextPatternRangeEndpoint_Start,
        TextUnit_Character, UIA_TextPatternId,
    };
    let none = || vec![None; spans.len()];
    let Some(uia) = uia_here() else {
        return none();
    };
    unsafe {
        let Ok(element) = uia.GetFocusedElement() else {
            return none();
        };
        if element.CurrentIsPassword().map_or(true, |b| b.as_bool()) {
            return none();
        }
        let Ok(pattern) =
            element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
        else {
            return none();
        };
        let Some(caret) = pattern
            .GetSelection()
            .ok()
            .filter(|s| s.Length().unwrap_or(0) > 0)
            .and_then(|s| s.GetElement(0).ok())
        else {
            return none();
        };
        // An empty range at the caret.
        let _ = caret.MoveEndpointByRange(
            TextPatternRangeEndpoint_End,
            &caret,
            TextPatternRangeEndpoint_Start,
        );
        spans
            .iter()
            .map(|&(start, len)| {
                let r = caret.Clone().ok()?;
                let moved = r
                    .MoveEndpointByUnit(
                        TextPatternRangeEndpoint_Start,
                        TextUnit_Character,
                        -(start as i32),
                    )
                    .ok()?;
                if moved != -(start as i32) {
                    return None;
                }
                r.MoveEndpointByRange(
                    TextPatternRangeEndpoint_End,
                    &r,
                    TextPatternRangeEndpoint_Start,
                )
                .ok()?;
                r.MoveEndpointByUnit(TextPatternRangeEndpoint_End, TextUnit_Character, len as i32)
                    .ok()?;
                union_of_boxes(&r)
            })
            .collect()
    }
}

/// One box around every line of `range`.
fn union_of_boxes(
    range: &windows::Win32::UI::Accessibility::IUIAutomationTextRange,
) -> Option<windows::Win32::Foundation::RECT> {
    unsafe {
        let array = range.GetBoundingRectangles().ok()?;
        if array.is_null() {
            return None;
        }
        let count = (*array).rgsabound[0].cElements as usize;
        let data = (*array).pvData as *const f64;
        let mut out: Option<windows::Win32::Foundation::RECT> = None;
        if !data.is_null() {
            for k in 0..count / 4 {
                let (x, y, w, h) = (
                    *data.add(4 * k),
                    *data.add(4 * k + 1),
                    *data.add(4 * k + 2),
                    *data.add(4 * k + 3),
                );
                if w <= 0.0 || h <= 0.0 {
                    continue;
                }
                let r = windows::Win32::Foundation::RECT {
                    left: x as i32,
                    top: y as i32,
                    right: (x + w) as i32,
                    bottom: (y + h) as i32,
                };
                out = Some(match out {
                    None => r,
                    Some(o) => windows::Win32::Foundation::RECT {
                        left: o.left.min(r.left),
                        top: o.top.min(r.top),
                        right: o.right.max(r.right),
                        bottom: o.bottom.max(r.bottom),
                    },
                });
            }
        }
        let _ = windows::Win32::System::Ole::SafeArrayDestroy(array);
        out
    }
}

/// Up to `n` characters before the caret in the focused field, asked of
/// the app (UI Automation, or a standard text box's own messages): all of
/// it when the field holds fewer. `None` when the app does not say, text is
/// selected, or the field is a password field. Worker threads only
/// (cross-process calls). For checking a correction just typed
/// ([`crate::verify`]) and Code mode's look at the line. Some apps
/// count a character as a whole cluster (Chrome: ว and ั are one), so the
/// text may be longer than `n` — compare its end, not its length.
pub fn text_before_caret_up_to(n: usize) -> Option<zeroize::Zeroizing<String>> {
    uia_text_before_caret(n).or_else(|| edit_text_before_caret(n))
}

fn uia_text_before_caret(n: usize) -> Option<zeroize::Zeroizing<String>> {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationTextPattern, TextPatternRangeEndpoint_Start, TextUnit_Character,
        UIA_TextPatternId,
    };
    let uia = uia_here()?;
    unsafe {
        let element = uia.GetFocusedElement().ok()?;
        if element.CurrentIsPassword().map_or(true, |b| b.as_bool()) {
            return None;
        }
        let pattern: IUIAutomationTextPattern =
            element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let selection = pattern.GetSelection().ok()?;
        if selection.Length().ok()? != 1 {
            return None;
        }
        let range = selection.GetElement(0).ok()?;
        // Only a caret: with text selected there is nothing to compare.
        if !range.GetText(1).ok()?.is_empty() {
            return None;
        }
        let back = i32::try_from(n).ok()?;
        // Fewer characters before the caret than asked: all of them.
        range
            .MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, -back)
            .ok()?;
        let text = zeroize::Zeroizing::new(range.GetText(-1).ok()?.to_string());
        Some(text)
    }
}

fn edit_text_before_caret(n: usize) -> Option<zeroize::Zeroizing<String>> {
    use windows::Win32::UI::WindowsAndMessaging::{WM_GETTEXT, WM_GETTEXTLENGTH};
    use zeroize::Zeroize;
    let tb = TextBox::focused().ok()?;
    let (start, end) = tb.selection()?;
    if start != end {
        return None;
    }
    let len = tb.ask(WM_GETTEXTLENGTH, 0, 0)?;
    if len > 0xFFFF || end > len {
        return None;
    }
    let mut units = vec![0u16; len + 1];
    let got = tb
        .ask(WM_GETTEXT, units.len(), units.as_mut_ptr() as isize)?
        .min(len);
    // RichEdit counts a line break as one position and WM_GETTEXT as two.
    let usable = end <= got && !(tb.rich && units[..end].contains(&(b'\n' as u16)));
    let text = usable.then(|| {
        let mut before = String::from_utf16_lossy(&units[..end]);
        let skip = before.chars().count().saturating_sub(n);
        let tail = zeroize::Zeroizing::new(before.chars().skip(skip).collect::<String>());
        before.zeroize();
        tail
    });
    units.zeroize();
    text
}

/// The text cursor of the focused element, from UI Automation, in screen
/// pixels — for apps that draw their own cursor and keep no system caret
/// (many Chromium/Electron editors). Slow (a cross-process call), so never
/// called from inside the keyboard hook.
pub fn uia_caret_rect() -> Option<windows::Win32::Foundation::RECT> {
    use windows::Win32::UI::Accessibility::{
        IUIAutomationTextPattern, IUIAutomationTextRange, TextUnit_Character, UIA_TextPatternId,
    };
    fn bounds(range: &IUIAutomationTextRange) -> Option<windows::Win32::Foundation::RECT> {
        unsafe {
            let array = range.GetBoundingRectangles().ok()?;
            if array.is_null() {
                return None;
            }
            let count = (*array).rgsabound[0].cElements as usize;
            let data = (*array).pvData as *const f64;
            let rect = (count >= 4 && !data.is_null()).then(|| {
                let (x, y, w, h) = (*data, *data.add(1), *data.add(2), *data.add(3));
                windows::Win32::Foundation::RECT {
                    left: x as i32,
                    top: y as i32,
                    right: (x + w.max(1.0)) as i32,
                    bottom: (y + h) as i32,
                }
            });
            let _ = windows::Win32::System::Ole::SafeArrayDestroy(array);
            rect.filter(|r| r.bottom > r.top)
        }
    }
    UIA.with(|u| {
        let uia = u.borrow().clone()?;
        unsafe {
            let element = uia.GetFocusedElement().ok()?;
            let pattern: IUIAutomationTextPattern =
                element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
            let selection = pattern.GetSelection().ok()?;
            if selection.Length().ok()? < 1 {
                return None;
            }
            let range = selection.GetElement(0).ok()?;
            // An empty selection (just a caret) often has no rectangle: use
            // the character after it, whose left edge is the caret.
            bounds(&range).or_else(|| {
                let one = range.Clone().ok()?;
                one.ExpandToEnclosingUnit(TextUnit_Character).ok()?;
                bounds(&one).map(|r| windows::Win32::Foundation::RECT {
                    right: r.left + 1,
                    ..r
                })
            })
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{
        is_inline_completing, is_list_row, status_is_protected, FIELD_PASSWORD, FIELD_SAFE,
        FIELD_UNKNOWN,
    };

    #[test]
    fn suggestion_rows_are_not_fields() {
        use windows::Win32::UI::Accessibility::{
            UIA_DocumentControlTypeId, UIA_EditControlTypeId, UIA_ListItemControlTypeId,
            UIA_MenuItemControlTypeId,
        };
        assert!(is_list_row(UIA_ListItemControlTypeId));
        assert!(is_list_row(UIA_MenuItemControlTypeId));
        assert!(!is_list_row(UIA_EditControlTypeId));
        assert!(!is_list_row(UIA_DocumentControlTypeId));
    }

    #[test]
    fn browser_address_bars_complete_inline() {
        assert!(is_inline_completing("OmniboxViewViews", ""));
        assert!(is_inline_completing("", "urlbar-input"));
        assert!(!is_inline_completing("Chrome_RenderWidgetHostHWND", ""));
        assert!(!is_inline_completing("RichEditD2DPT", ""));
        assert!(!is_inline_completing("", ""));
    }

    #[test]
    fn unknown_and_password_statuses_fail_closed() {
        assert!(status_is_protected(FIELD_UNKNOWN));
        assert!(status_is_protected(FIELD_PASSWORD));
        assert!(!status_is_protected(FIELD_SAFE));
    }
}
