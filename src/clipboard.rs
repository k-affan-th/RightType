//! Minimal Unicode clipboard access — Windows only.
//!
//! Used by the manual "convert selection" hotkey: copy the selection, read it,
//! convert it, write it back, paste. Only `CF_UNICODETEXT` is handled (all we
//! need), and reads never log or persist the text.

use std::slice;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_UNICODETEXT` clipboard format (avoids pulling in the Ole feature).
const CF_UNICODETEXT: u32 = 13;

/// A monotonically increasing counter that changes whenever the clipboard does —
/// used to wait for an app to finish responding to an injected Ctrl+C.
pub unsafe fn sequence() -> u32 {
    GetClipboardSequenceNumber()
}

/// Read the clipboard as text, or `None` if it holds no Unicode text.
pub unsafe fn get_text() -> Option<String> {
    if OpenClipboard(HWND::default()).is_err() {
        return None;
    }
    let result = read_unicode();
    let _ = CloseClipboard();
    result
}

unsafe fn read_unicode() -> Option<String> {
    let handle = GetClipboardData(CF_UNICODETEXT).ok()?;
    let hglobal = HGLOBAL(handle.0);
    let ptr = GlobalLock(hglobal) as *const u16;
    if ptr.is_null() {
        return None;
    }
    let mut len = 0usize;
    while *ptr.add(len) != 0 {
        len += 1;
    }
    let text = String::from_utf16_lossy(slice::from_raw_parts(ptr, len));
    let _ = GlobalUnlock(hglobal);
    Some(text)
}

/// Replace the clipboard contents with `text`. Returns `false` on failure.
pub unsafe fn set_text(text: &str) -> bool {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = utf16.len() * std::mem::size_of::<u16>();

    let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, bytes) else {
        return false;
    };
    let dst = GlobalLock(hmem) as *mut u16;
    if dst.is_null() {
        return false;
    }
    std::ptr::copy_nonoverlapping(utf16.as_ptr(), dst, utf16.len());
    let _ = GlobalUnlock(hmem);

    if OpenClipboard(HWND::default()).is_err() {
        return false;
    }
    let _ = EmptyClipboard();
    // On success the system takes ownership of `hmem`, so we must not free it.
    let ok = SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0)).is_ok();
    let _ = CloseClipboard();
    ok
}
