//! Modern Windows theming — dark title bar/backdrop, Win11 rounded corners,
//! and dark control colors for our raw-Win32 dialogs. Every attribute is
//! best-effort: older builds ignore what they don't know.

use std::sync::OnceLock;

use native_windows_gui as nwg;
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::Graphics::Gdi::{
    CreateSolidBrush, FillRect, SetBkMode, SetTextColor, HBRUSH, HDC, TRANSPARENT,
};
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

pub const BG: COLORREF = COLORREF(0x00201F1F); // #1F1F20 dark neutral
pub const TEXT: COLORREF = COLORREF(0x00F2F2F2);

const WM_ERASEBKGND: u32 = 0x0014;
const WM_CTLCOLORDLG: u32 = 0x0136;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLOREDIT: u32 = 0x0133;

static DARK_BRUSH: OnceLock<isize> = OnceLock::new();

fn brush() -> isize {
    *DARK_BRUSH.get_or_init(|| unsafe { CreateSolidBrush(BG).0 as isize })
}

/// Modern window frame: dark title bar, Win11 rounded corners, Mica backdrop.
pub fn apply_frame(hwnd: HWND) {
    unsafe {
        let dark: i32 = 1;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark as *const i32 as _,
            4,
        );
        let round: i32 = 2; // DWMWCP_ROUND
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &round as *const i32 as _,
            4,
        );
        // Mica backdrop omitted on purpose: it stripes our dark label brushes.
    }
}

/// Subclass `hwnd` so dialog/static/edit controls paint in the dark palette.
/// Keep the returned handler alive for the window's lifetime.
pub fn subclass_colors(hwnd: HWND, handler_id: usize) -> Option<nwg::RawEventHandler> {
    let brush_val = brush();
    let handle = native_windows_gui::ControlHandle::Hwnd(hwnd.0 as *mut _);
    unsafe {
        nwg::bind_raw_event_handler(&handle, handler_id, move |h, msg, w, _l| match msg {
            WM_ERASEBKGND => {
                // nwg Windows are class-backed, not dialogs: paint the client
                // dark ourselves or the OS white shows through.
                let hdc = HDC(w as *mut core::ffi::c_void);
                let mut rc = RECT::default();
                let _ = GetClientRect(HWND(h as *mut core::ffi::c_void), &mut rc);
                FillRect(hdc, &rc, HBRUSH(brush_val as *mut core::ffi::c_void));
                Some(1)
            }
            WM_CTLCOLORDLG | WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT => {
                let hdc = HDC(w as *mut core::ffi::c_void);
                SetTextColor(hdc, TEXT);
                SetBkMode(hdc, TRANSPARENT);
                Some(brush_val)
            }
            _ => None,
        })
        .ok()
    }
}
