//! Minimal Unicode clipboard access — Windows only.
//!
//! Used by the manual "convert selection" hotkey: copy the selection, read it,
//! convert it, restore the previous clipboard, and inject Unicode into the active
//! selection. Only `CF_UNICODETEXT` is handled (all we need), and reads never log
//! or persist the text.

use std::slice;
use zeroize::{Zeroize, Zeroizing};

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_UNICODETEXT` clipboard format (avoids pulling in the Ole feature).
const CF_UNICODETEXT: u32 = 13;
const CF_TEXT: u32 = 1;
const CF_OEMTEXT: u32 = 7;
const CF_LOCALE: u32 = 16;

fn is_plain_text_format(format: u32) -> bool {
    matches!(format, CF_TEXT | CF_OEMTEXT | CF_UNICODETEXT | CF_LOCALE)
}

/// A clipboard value that can be restored without silently discarding a rich
/// format, image, file list, or app-specific data object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlainTextSnapshot {
    Empty,
    UnicodeText(String),
}

impl Drop for PlainTextSnapshot {
    fn drop(&mut self) {
        if let Self::UnicodeText(text) = self {
            text.zeroize();
        }
    }
}

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

/// Snapshot the clipboard only when it is empty or holds plain Unicode text.
///
/// The selection converter temporarily replaces the clipboard. Restoring only a
/// `String` after overwriting rich clipboard data is destructive, so callers must
/// fail closed for every additional clipboard format until a full-format snapshot
/// implementation exists.
pub unsafe fn snapshot_plain_text() -> Option<PlainTextSnapshot> {
    if OpenClipboard(HWND::default()).is_err() {
        return None;
    }

    let mut format = 0;
    let mut saw_format = false;
    let mut plain_only = true;
    loop {
        format = EnumClipboardFormats(format);
        if format == 0 {
            break;
        }
        saw_format = true;
        // Windows may advertise synthesized ANSI/OEM text and locale metadata
        // alongside CF_UNICODETEXT. Those carry no independent rich content and
        // can be restored faithfully from the Unicode snapshot.
        if !is_plain_text_format(format) {
            plain_only = false;
        }
    }

    let result = if !plain_only {
        None
    } else if !saw_format {
        Some(PlainTextSnapshot::Empty)
    } else {
        read_unicode().map(PlainTextSnapshot::UnicodeText)
    };
    let _ = CloseClipboard();
    result
}

/// Restore a value returned by [`snapshot_plain_text`].
pub unsafe fn restore_snapshot(snapshot: &PlainTextSnapshot) -> bool {
    match snapshot {
        PlainTextSnapshot::Empty => clear(),
        PlainTextSnapshot::UnicodeText(text) => set_text(text),
    }
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
    let utf16 = Zeroizing::new(
        text.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>(),
    );
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

unsafe fn clear() -> bool {
    if OpenClipboard(HWND::default()).is_err() {
        return false;
    }
    let ok = EmptyClipboard().is_ok();
    let _ = CloseClipboard();
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_companion_formats_are_allowed_but_rich_formats_are_not() {
        for format in [CF_TEXT, CF_OEMTEXT, CF_UNICODETEXT, CF_LOCALE] {
            assert!(is_plain_text_format(format));
        }
        assert!(!is_plain_text_format(2)); // CF_BITMAP
        assert!(!is_plain_text_format(15)); // CF_HDROP
    }
}
