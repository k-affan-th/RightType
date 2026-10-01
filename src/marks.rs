//! Highlights over words on screen — Windows only.
//!
//! While the palette's list of recent words is open, the words a command
//! would change are tinted where they are in the app, so it is plain which
//! ones change before anything does. The tint windows let clicks through,
//! never take focus, and are left out of screen sharing and screenshots.

use std::cell::RefCell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH, HGDIOBJ};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetLayeredWindowAttributes,
    SetWindowDisplayAffinity, SetWindowPos, ShowWindow, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE,
    SW_HIDE, SW_SHOWNOACTIVATE, WDA_EXCLUDEFROMCAPTURE, WNDCLASSW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};

const CLASS: PCWSTR = w!("RightTypeMark");
/// How strong the tint is (0–255).
const ALPHA: u8 = 90;

thread_local! {
    static MARKS: RefCell<Vec<HWND>> = const { RefCell::new(Vec::new()) };
    static BRUSH: RefCell<Option<HBRUSH>> = const { RefCell::new(None) };
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, w, l)
}

/// The tint: the accent colour, or the selection colour in High Contrast.
fn colour() -> COLORREF {
    if crate::ui::high_contrast() {
        unsafe {
            COLORREF(windows::Win32::Graphics::Gdi::GetSysColor(
                windows::Win32::Graphics::Gdi::COLOR_HIGHLIGHT,
            ))
        }
    } else {
        crate::ui::colorref(crate::ui::pal().accent)
    }
}

unsafe fn make() -> Option<HWND> {
    // One brush while tints are up; `clear` frees it, so the next set
    // follows the theme of the moment.
    let brush = BRUSH.with(|b| *b.borrow_mut().get_or_insert_with(|| CreateSolidBrush(colour())));
    let instance = GetModuleHandleW(None).ok()?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(proc),
        hInstance: instance.into(),
        lpszClassName: CLASS,
        hbrBackground: brush,
        ..Default::default()
    };
    // A second registration fails harmlessly; the brush is set again below.
    RegisterClassW(&class);
    let hwnd = CreateWindowExW(
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
        CLASS,
        PCWSTR::null(),
        WS_POPUP,
        0,
        0,
        1,
        1,
        None,
        None,
        instance,
        None,
    )
    .ok()?;
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), ALPHA, LWA_ALPHA);
    let _ = SetWindowDisplayAffinity(hwnd, WDA_EXCLUDEFROMCAPTURE);
    // The class may be from an earlier set, with another theme's brush.
    windows::Win32::UI::WindowsAndMessaging::SetClassLongPtrW(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::GCLP_HBRBACKGROUND,
        brush.0 as isize,
    );
    Some(hwnd)
}

/// Tint exactly `boxes` (screen pixels); any other tint goes. UI thread.
pub fn show(boxes: &[RECT]) {
    MARKS.with(|m| {
        let mut marks = m.borrow_mut();
        while marks.len() < boxes.len() {
            match unsafe { make() } {
                Some(h) => marks.push(h),
                None => break,
            }
        }
        for (i, hwnd) in marks.iter().enumerate() {
            unsafe {
                match boxes.get(i) {
                    Some(r) => {
                        let pad = 2;
                        let _ = SetWindowPos(
                            *hwnd,
                            HWND_TOPMOST,
                            r.left - pad,
                            r.top - pad,
                            (r.right - r.left) + 2 * pad,
                            (r.bottom - r.top) + 2 * pad,
                            SWP_NOACTIVATE,
                        );
                        let _ = ShowWindow(*hwnd, SW_SHOWNOACTIVATE);
                    }
                    None => {
                        let _ = ShowWindow(*hwnd, SW_HIDE);
                    }
                }
            }
        }
    });
}

/// Remove every tint. UI thread.
pub fn clear() {
    MARKS.with(|m| {
        for hwnd in m.borrow_mut().drain(..) {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
    });
    BRUSH.with(|b| {
        if let Some(old) = b.borrow_mut().take() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(old.0));
            }
        }
    });
}
