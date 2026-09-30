//! Minimal Unicode clipboard access — Windows only.
//!
//! Used by the manual "convert selection" hotkey: copy the selection, read it,
//! convert it, restore the previous clipboard, and inject Unicode into the active
//! selection. Only `CF_UNICODETEXT` is handled (all we need), and reads never log
//! or persist the text.
//!
//! Everything RightType writes is marked with the formats Windows defines for
//! it ([`PRIVATE_FORMATS`]): not kept in clipboard history (Win+V), not
//! synced to the user's other devices, and skipped by clipboard monitors that
//! honour the convention. Programs that ignore it can still read it, like
//! anything else on the clipboard.

use std::slice;
use std::thread;
use std::time::Duration;
use zeroize::{Zeroize, Zeroizing};

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

/// `CF_UNICODETEXT` clipboard format (avoids pulling in the Ole feature).
const CF_UNICODETEXT: u32 = 13;
const CF_TEXT: u32 = 1;
const CF_OEMTEXT: u32 = 7;
const CF_LOCALE: u32 = 16;

/// Another program can hold the clipboard open for a moment (clipboard managers,
/// remote-desktop sync, an app mid-copy). Retry briefly instead of failing on the
/// first refusal; a clipboard still held after this is reported as busy.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY: Duration = Duration::from_millis(10);

/// Open the clipboard for this thread, retrying while another process holds it.
unsafe fn open() -> bool {
    for attempt in 0..OPEN_ATTEMPTS {
        if OpenClipboard(HWND::default()).is_ok() {
            return true;
        }
        if attempt + 1 < OPEN_ATTEMPTS {
            thread::sleep(OPEN_RETRY);
        }
    }
    false
}

/// Why the clipboard could not be snapshotted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotError {
    /// Another program kept the clipboard open.
    Busy,
    /// It holds something besides plain text, which restoring would destroy.
    NotPlainText,
}

/// Registered formats that ask Windows (and well-behaved clipboard tools) to
/// leave the text alone: <https://learn.microsoft.com/windows/win32/dataxchg/clipboard-formats#cloud-clipboard-and-clipboard-history-formats>
const PRIVATE_FORMATS: [&str; 3] = [
    "ExcludeClipboardContentFromMonitorProcessing",
    "CanIncludeInClipboardHistory",
    "CanUploadToCloudClipboard",
];

/// The ids Windows gave [`PRIVATE_FORMATS`] in this session.
fn private_format_ids() -> &'static [u32; 3] {
    static IDS: std::sync::OnceLock<[u32; 3]> = std::sync::OnceLock::new();
    IDS.get_or_init(|| {
        PRIVATE_FORMATS.map(|name| {
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(wide.as_ptr())) }
        })
    })
}

fn is_plain_text_format(format: u32) -> bool {
    matches!(format, CF_TEXT | CF_OEMTEXT | CF_UNICODETEXT | CF_LOCALE)
        || (format != 0 && private_format_ids().contains(&format))
}

/// Add [`PRIVATE_FORMATS`] to the clipboard this thread has open, each as a
/// DWORD 0 ("no"). Best-effort.
unsafe fn mark_private() {
    for &id in private_format_ids() {
        if id == 0 {
            continue;
        }
        let Ok(hmem) = GlobalAlloc(GMEM_MOVEABLE, std::mem::size_of::<u32>()) else {
            continue;
        };
        let dst = GlobalLock(hmem) as *mut u32;
        if dst.is_null() {
            let _ = GlobalFree(hmem);
            continue;
        }
        *dst = 0;
        let _ = GlobalUnlock(hmem);
        if SetClipboardData(id, HANDLE(hmem.0)).is_err() {
            let _ = GlobalFree(hmem);
        }
    }
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
    if !open() {
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
pub unsafe fn snapshot_plain_text() -> Result<PlainTextSnapshot, SnapshotError> {
    if !open() {
        return Err(SnapshotError::Busy);
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
        Err(SnapshotError::NotPlainText)
    } else if !saw_format {
        Ok(PlainTextSnapshot::Empty)
    } else {
        read_unicode()
            .map(PlainTextSnapshot::UnicodeText)
            .ok_or(SnapshotError::NotPlainText)
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

    if !open() {
        // Never handed to the system, so it is still ours to free.
        let _ = GlobalFree(hmem);
        return false;
    }
    let _ = EmptyClipboard();
    // On success the system takes ownership of `hmem`, so we must not free it.
    let ok = SetClipboardData(CF_UNICODETEXT, HANDLE(hmem.0)).is_ok();
    if ok {
        mark_private();
    } else {
        let _ = GlobalFree(hmem);
    }
    let _ = CloseClipboard();
    ok
}

unsafe fn clear() -> bool {
    if !open() {
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
