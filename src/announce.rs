//! Telling a screen reader what RightType did — Windows only.
//!
//! A word fixed in place changes text the typist cannot see; a screen-reader
//! user would not know it happened. Each fix, tag and message is raised as a
//! UI Automation *notification* ("Fixed: สวัสดี"), which Narrator, NVDA and
//! JAWS speak without moving focus.
//!
//! Only when a screen reader (any UI Automation client) is listening. The
//! text goes to that program on this PC and nowhere else; it is not kept.
//! Raised on a thread of its own so a slow screen reader never holds up the
//! keyboard hook.

use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;

use windows::core::{implement, IUnknown, BSTR, VARIANT};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::{
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, NotificationKind_ActionCompleted,
    NotificationProcessing_ImportantMostRecent, ProviderOptions,
    ProviderOptions_ServerSideProvider, ProviderOptions_UseComThreading, UIA_NamePropertyId,
    UiaClientsAreListening, UiaHostProviderFromHwnd, UiaRaiseNotificationEvent, UIA_PATTERN_ID,
    UIA_PROPERTY_ID,
};
use zeroize::Zeroize;

/// The element the notifications come from: RightType's tag window.
#[implement(IRawElementProviderSimple)]
struct Source {
    hwnd: HWND,
}

impl IRawElementProviderSimple_Impl for Source_Impl {
    fn ProviderOptions(&self) -> windows::core::Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    fn GetPatternProvider(&self, _: UIA_PATTERN_ID) -> windows::core::Result<IUnknown> {
        // No patterns: S_OK with nothing.
        Err(windows::core::Error::empty())
    }

    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> windows::core::Result<VARIANT> {
        if id == UIA_NamePropertyId {
            return Ok(VARIANT::from(BSTR::from("RightType")));
        }
        Ok(VARIANT::default())
    }

    fn HostRawElementProvider(&self) -> windows::core::Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

static SAY: OnceLock<SyncSender<(isize, String)>> = OnceLock::new();

/// Have a screen reader say `text`, coming from the window `hwnd`. Does
/// nothing when none is listening. Never waits.
pub fn say(hwnd: isize, text: &str) {
    if hwnd == 0 || text.is_empty() || !unsafe { UiaClientsAreListening() }.as_bool() {
        return;
    }
    let tx = SAY.get_or_init(|| {
        let (tx, rx) = sync_channel::<(isize, String)>(4);
        let _ = std::thread::Builder::new()
            .name("announce".into())
            .spawn(move || {
                unsafe {
                    let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
                }
                while let Ok((hwnd, mut text)) = rx.recv() {
                    raise(hwnd, &text);
                    text.zeroize();
                }
            });
        tx
    });
    // Full: a burst of fixes; the screen reader keeps only the latest anyway.
    let _ = tx.try_send((hwnd, text.to_string()));
}

fn raise(hwnd: isize, text: &str) {
    let source: IRawElementProviderSimple = Source {
        hwnd: HWND(hwnd as *mut _),
    }
    .into();
    let said = BSTR::from(text);
    unsafe {
        let _ = UiaRaiseNotificationEvent(
            &source,
            NotificationKind_ActionCompleted,
            NotificationProcessing_ImportantMostRecent,
            &said,
            &BSTR::from("RightType"),
        );
    }
    // The BSTR held typed text: clear it before it is freed.
    let len = said.len();
    if len > 0 {
        unsafe {
            let p = said.as_ptr() as *mut u16;
            std::ptr::write_bytes(p, 0, len);
        }
    }
    drop(said);
}
