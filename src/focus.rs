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
        UIA.with(|u| *u.borrow_mut() = Some(uia));
    }
    refresh_status();
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
    // A field that already has focus at startup gets no focus event.
    crate::habits::on_focus();
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
    on_focus_inner();
    DEPTH.with(|d| d.set(depth));
}

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
    // After the password check above: the habit switch never runs in one.
    crate::habits::on_focus();
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
    UIA.with(|u| {
        let Some(uia) = u.borrow().clone() else {
            return true;
        };
        let Ok(element) = uia.GetFocusedElement() else {
            FIELD.with(|f| *f.borrow_mut() = None);
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
    let status = UIA.with(|u| {
        u.borrow()
            .as_ref()
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
fn uia_here() -> Option<IUIAutomation> {
    UIA.with(|u| {
        if u.borrow().is_none() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                if let Ok(uia) =
                    CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
                {
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
    use windows::Win32::UI::Accessibility::{IUIAutomationTextPattern, UIA_TextPatternId};
    let uia = uia_here()?;
    unsafe {
        let element = uia.GetFocusedElement().ok()?;
        if element.CurrentIsPassword().map_or(true, |b| b.as_bool()) {
            return None;
        }
        let pattern: IUIAutomationTextPattern =
            element.GetCurrentPatternAs(UIA_TextPatternId).ok()?;
        let ranges = pattern.GetSelection().ok()?;
        let mut text = zeroize::Zeroizing::new(String::new());
        for i in 0..ranges.Length().ok()? {
            let range = ranges.GetElement(i).ok()?;
            text.push_str(&range.GetText(-1).ok()?.to_string());
        }
        (!text.is_empty()).then_some(text)
    }
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
