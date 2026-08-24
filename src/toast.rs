//! Tiny, modern status toast — Windows only.
//!
//! Deliberately minimal: it flashes a small dark pill in the **bottom-right
//! corner** only on rare, deliberate state changes — switching Auto/Manual mode
//! (via the Ctrl+CapsLock shortcut or the tray) and enable/disable — then fades.
//! It does NOT announce auto layout switches (too frequent — Windows' own language
//! indicator already shows those) nor individual corrections (you see the word
//! change). Custom-painted (GDI), borderless, and it never steals focus.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;

use native_windows_gui as nwg;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW,
    EndPaint, FillRect, FrameRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    SetWindowRgn, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DT_CENTER,
    DT_SINGLELINE, DT_VCENTER, HGDIOBJ, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, KillTimer, PostMessageW, SetLayeredWindowAttributes, SetTimer, SetWindowPos,
    ShowWindow, SystemParametersInfoW, HWND_TOPMOST, LWA_ALPHA, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_SHOWWINDOW, SW_HIDE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

const TIMER_ID: usize = 7;
const FADE_TIMER_ID: usize = 8;
const SHOW_MS_BASE: u32 = 900;
const FADE_STEP_MS: u32 = 30;
const W: i32 = 116;
const H: i32 = 34;
const MARGIN: i32 = 12;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_TIMER: u32 = 0x0113;
const WM_SHOW_TOAST: u32 = 0x8000 + 0x525;

static TOAST_HWND: AtomicIsize = AtomicIsize::new(0);
static UI_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static PENDING_TEXT: Mutex<Option<String>> = Mutex::new(None);
static ALPHA: AtomicU32 = AtomicU32::new(255);

/// `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED` —
/// float above everything, stay off the taskbar, never take focus, and allow
/// the fade-out animation.
const EX_FLAGS: u32 = 0x0000_0008 | 0x0000_0080 | 0x0800_0000 | 0x0008_0000;

struct Toast {
    _window: nwg::Window,
    _raw: Option<nwg::RawEventHandler>,
}

thread_local! {
    static TOAST: RefCell<Option<Toast>> = const { RefCell::new(None) };
    static TEXT: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Create the (hidden) toast window. Call once, on the UI thread, after `nwg::init`.
pub fn init() {
    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::POPUP)
        .ex_flags(EX_FLAGS)
        .size((W, H))
        .position((-4000, -4000))
        .title("")
        .build(&mut window)
        .is_err()
    {
        return;
    }

    if let Some(h) = window.handle.hwnd() {
        TOAST_HWND.store(h as isize, Ordering::Release);
        UI_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        unsafe {
            // Layered window: enable per-pixel alpha for the fade-out.
            let _ = SetLayeredWindowAttributes(HWND(h as _), COLORREF(0), 255_u8, LWA_ALPHA);
        }
    }

    // We custom-paint the window (dark pill + white text) and hide on the timer.
    let raw = nwg::bind_raw_event_handler(&window.handle, 0x5254_0002, move |hwnd, msg, w, _l| {
        let hwnd = HWND(hwnd as _);
        match msg {
            WM_ERASEBKGND => Some(1), // painted fully in WM_PAINT; skip default erase
            WM_PAINT => {
                unsafe { paint(hwnd) };
                Some(0)
            }
            WM_TIMER if w == TIMER_ID => {
                // Hold period over: begin the fade-out animation.
                unsafe {
                    let _ = KillTimer(hwnd, TIMER_ID);
                    ALPHA.store(220, Ordering::Relaxed);
                    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 220_u8, LWA_ALPHA);
                    SetTimer(hwnd, FADE_TIMER_ID, FADE_STEP_MS, None);
                }
                Some(0)
            }
            WM_TIMER if w == FADE_TIMER_ID => {
                unsafe {
                    let next = ALPHA.fetch_sub(28, Ordering::Relaxed).saturating_sub(28);
                    if next == 0 {
                        let _ = KillTimer(hwnd, FADE_TIMER_ID);
                        hide(hwnd);
                        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255_u8, LWA_ALPHA);
                    } else {
                        let _ =
                            SetLayeredWindowAttributes(hwnd, COLORREF(0), next as u8, LWA_ALPHA);
                    }
                }
                Some(0)
            }
            WM_SHOW_TOAST => {
                if let Some(text) = PENDING_TEXT.lock().unwrap().take() {
                    unsafe { show_on_ui(hwnd, &text) };
                }
                Some(0)
            }
            _ => None,
        }
    })
    .ok();

    TOAST.with(|t| {
        *t.borrow_mut() = Some(Toast {
            _window: window,
            _raw: raw,
        })
    });
}

/// Flash `text` briefly in the bottom-right corner. Calls from workers are posted
/// back to the UI thread that owns the toast window.
pub fn show(text: &str) {
    let raw = TOAST_HWND.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    let hwnd = HWND(raw as *mut c_void);
    if unsafe { GetCurrentThreadId() } == UI_THREAD_ID.load(Ordering::Acquire) {
        unsafe { show_on_ui(hwnd, text) };
    } else {
        *PENDING_TEXT.lock().unwrap() = Some(text.to_string());
        unsafe {
            let _ = PostMessageW(hwnd, WM_SHOW_TOAST, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn show_on_ui(hwnd: HWND, text: &str) {
    TEXT.with(|t| *t.borrow_mut() = text.to_string());
<<<<<<< Updated upstream
    let (x, y) = bottom_right();
=======
    // Width grows with the message (suggestion previews are longer than the
    // original mode labels) but stays a compact pill. The rounded region must
    // be recomputed too — it was sized for the default width at creation.
    let units: Vec<u16> = text.encode_utf16().collect();
    let w = (units.len() as i32 * 7 + 32).clamp(W, 520);
    let (x, y) = bottom_right(w);
>>>>>>> Stashed changes
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x,
        y,
        W,
        H,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    let rgn = CreateRoundRectRgn(0, 0, w + 1, H + 1, H, H);
    SetWindowRgn(hwnd, rgn, true);
    // Reset alpha in case a previous toast was mid-fade, then hold.
    ALPHA.store(255, Ordering::Relaxed);
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255_u8, LWA_ALPHA);
    let hold = (SHOW_MS_BASE + units.len() as u32 * 18).min(2400);
    let _ = KillTimer(hwnd, TIMER_ID);
    let _ = KillTimer(hwnd, FADE_TIMER_ID);
    SetTimer(hwnd, TIMER_ID, hold, None);
    let _ = InvalidateRect(hwnd, None, true);
}

/// Bottom-right of the work area (so it sits above the taskbar, wherever it is).
unsafe fn bottom_right() -> (i32, i32) {
    let mut wa = RECT::default();
    let _ = SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some(&mut wa as *mut RECT as *mut c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    );
    (wa.right - W - MARGIN, wa.bottom - H - MARGIN)
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // Dark pill background (COLORREF is 0x00BBGGRR) + hairline border.
    let brush = CreateSolidBrush(COLORREF(0x002A_2A2A));
    FillRect(hdc, &rc, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let border = CreateSolidBrush(COLORREF(0x0045_4545));
    FrameRect(hdc, &rc, border);
    let _ = DeleteObject(HGDIOBJ(border.0));

    // White, centred Segoe UI text.
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(0x00FF_FFFF));
    let face: Vec<u16> = "Segoe UI\0".encode_utf16().collect();
    let font = CreateFontW(
        -15,
        0,
        0,
        0,
        600,
        0,
        0,
        0,
        DEFAULT_CHARSET.0 as u32,
        OUT_DEFAULT_PRECIS.0 as u32,
        CLIP_DEFAULT_PRECIS.0 as u32,
        CLEARTYPE_QUALITY.0 as u32,
        0,
        PCWSTR(face.as_ptr()),
    );
    let old = SelectObject(hdc, HGDIOBJ(font.0));
    let mut text: Vec<u16> = TEXT.with(|t| t.borrow().encode_utf16().collect());
    DrawTextW(
        hdc,
        &mut text,
        &mut rc,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    SelectObject(hdc, old);
    let _ = DeleteObject(HGDIOBJ(font.0));

    let _ = EndPaint(hwnd, &ps);
}

unsafe fn hide(hwnd: HWND) {
    let _ = KillTimer(hwnd, TIMER_ID);
    let _ = ShowWindow(hwnd, SW_HIDE);
}
