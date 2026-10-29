//! The welcome window's flight into the tray — Windows only.
//!
//! A photograph of the window is taken, a click-through layered window shows
//! it where the window was, and then that photograph is drawn smaller, nearer
//! the tray icon and fainter, frame after frame, along the path in
//! [`righttype::motion::dock`]. The real window is never resized, so none of
//! its controls have to repaint on the way. Frames follow the clock and wait
//! for the screen (`DwmFlush`), so a slow frame skips ahead instead of making
//! the whole flight longer.
//!
//! When Windows has animation switched off, the photograph only fades.

use std::ffi::c_void;
use std::time::Instant;

use righttype::motion;
use windows::core::{w, GUID, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmFlush, DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    AlphaBlend, BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
    ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{Shell_NotifyIconGetRect, NOTIFYICONIDENTIFIER};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, FindWindowExW, FindWindowW, GetWindowRect, PostMessageW, SetWindowPos,
    SystemParametersInfoW, UpdateLayeredWindow, HMENU, HWND_TOPMOST, SPI_GETCLIENTAREAANIMATION,
    SWP_NOACTIVATE, SWP_SHOWWINDOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ULW_ALPHA,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE,
};

/// Fly `window` into the tray icon, then run `then` (from another thread).
/// Returns at once; the window may be closed right after this call, since it
/// has already been photographed. If anything needed is missing, `then` runs
/// straight away and nothing is shown.
pub fn fly_to_tray(window: HWND, then: impl FnOnce() + Send + 'static) {
    let Some(flight) = (unsafe { Flight::begin(window) }) else {
        then();
        return;
    };
    let calm = !client_animation_on();
    let _ = std::thread::Builder::new()
        .name("righttype-dock".into())
        .spawn(move || {
            unsafe { flight.run(calm) };
            then();
        });
}

/// Everything the flight needs, as plain numbers so it can move to a thread.
struct Flight {
    ghost: isize,
    snapshot: isize,
    from: RECT,
    to: POINT,
}

// SAFETY: the handles are used by one thread at a time; the UI thread only
// creates them and hands them over.
unsafe impl Send for Flight {}

impl Flight {
    /// Photograph the window, put the photograph where it was and return what
    /// the flight needs. On the UI thread, while the window is still shown.
    unsafe fn begin(window: HWND) -> Option<Flight> {
        // The visible frame, without the invisible resize border.
        let mut from = RECT::default();
        let got = DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut from as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        );
        if got.is_err() && GetWindowRect(window, &mut from).is_err() {
            return None;
        }
        let (w, h) = (from.right - from.left, from.bottom - from.top);
        if w <= 0 || h <= 0 {
            return None;
        }
        let to = tray_target(from)?;

        let screen = GetDC(None);
        let mem = CreateCompatibleDC(screen);
        let (dib, bits) = dib(mem, w, h)?;
        let old = SelectObject(mem, HGDIOBJ(dib.0));
        let copied = BitBlt(mem, 0, 0, w, h, screen, from.left, from.top, SRCCOPY).is_ok();
        // The screen has no alpha: make the photograph fully opaque.
        for px in std::slice::from_raw_parts_mut(bits as *mut u32, (w * h) as usize) {
            *px |= 0xFF00_0000;
        }
        SelectObject(mem, old);
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !copied {
            let _ = DeleteObject(HGDIOBJ(dib.0));
            return None;
        }

        let ghost = ghost_window()?;
        let flight = Flight {
            ghost: ghost.0 as isize,
            snapshot: dib.0 as isize,
            from,
            to,
        };
        // Show it exactly over the window, before the window goes.
        draw(
            ghost,
            HBITMAP(flight.snapshot as *mut c_void),
            w,
            h,
            from.left,
            from.top,
            255,
        );
        let _ = SetWindowPos(
            ghost,
            HWND_TOPMOST,
            from.left,
            from.top,
            w,
            h,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        Some(flight)
    }

    /// Play the flight, then remove the ghost window and the photograph.
    unsafe fn run(self, calm: bool) {
        let ghost = HWND(self.ghost as *mut c_void);
        let snapshot = HBITMAP(self.snapshot as *mut c_void);
        let (w, h) = (
            self.from.right - self.from.left,
            self.from.bottom - self.from.top,
        );
        let (cx0, cy0) = (
            self.from.left as f32 + w as f32 / 2.0,
            self.from.top as f32 + h as f32 / 2.0,
        );
        let started = Instant::now();
        loop {
            let f = motion::dock(started.elapsed().as_millis() as u32, calm);
            let cx = cx0 + (self.to.x as f32 - cx0) * f.x;
            let cy = cy0 + (self.to.y as f32 - cy0) * f.y;
            let (dw, dh) = (
                ((w as f32 * f.scale).round() as i32).max(1),
                ((h as f32 * f.scale).round() as i32).max(1),
            );
            draw(
                ghost,
                snapshot,
                dw,
                dh,
                (cx - dw as f32 / 2.0).round() as i32,
                (cy - dh as f32 / 2.0).round() as i32,
                f.alpha,
            );
            if f.done {
                break;
            }
            // Wait for the screen to draw; never spin if that returns early.
            let before = Instant::now();
            let _ = DwmFlush();
            if before.elapsed().as_millis() < 2 {
                std::thread::sleep(std::time::Duration::from_millis(8));
            }
        }
        // Destroyed by the thread that made it, via its own handler for WM_CLOSE.
        let _ = PostMessageW(ghost, WM_CLOSE, WPARAM(0), LPARAM(0));
        let _ = DeleteObject(HGDIOBJ(snapshot.0));
    }
}

/// Draw the photograph (`snapshot`, its own size inside `Flight`) at `size`
/// pixels with its top left at (`x`, `y`), `alpha` opaque, onto `ghost`.
#[allow(clippy::too_many_arguments)]
unsafe fn draw(ghost: HWND, snapshot: HBITMAP, dw: i32, dh: i32, x: i32, y: i32, alpha: u8) {
    let screen = GetDC(None);
    let src = CreateCompatibleDC(screen);
    let old_src = SelectObject(src, HGDIOBJ(snapshot.0));
    let (sw, sh) = bitmap_size(src, snapshot);

    let dst = CreateCompatibleDC(screen);
    if let Some((frame, _)) = dib(dst, dw, dh) {
        let old_dst = SelectObject(dst, HGDIOBJ(frame.0));
        let whole = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = AlphaBlend(dst, 0, 0, dw, dh, src, 0, 0, sw, sh, whole);
        let at = POINT { x, y };
        let size = SIZE { cx: dw, cy: dh };
        let from = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            ghost,
            screen,
            Some(&at),
            Some(&size),
            dst,
            Some(&from),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        SelectObject(dst, old_dst);
        let _ = DeleteObject(HGDIOBJ(frame.0));
    }
    let _ = DeleteDC(dst);
    SelectObject(src, old_src);
    let _ = DeleteDC(src);
    ReleaseDC(None, screen);
}

/// A top-down 32-bit DIB of `w` × `h`, and where its pixels are.
unsafe fn dib(dc: HDC, w: i32, h: i32) -> Option<(HBITMAP, *mut c_void)> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
    (!bits.is_null()).then_some((bitmap, bits))
}

/// Width and height of a bitmap selected into `dc`.
unsafe fn bitmap_size(_dc: HDC, bitmap: HBITMAP) -> (i32, i32) {
    use windows::Win32::Graphics::Gdi::{GetObjectW, BITMAP};
    let mut b = BITMAP::default();
    GetObjectW(
        HGDIOBJ(bitmap.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut b as *mut BITMAP as *mut c_void),
    );
    (b.bmWidth, b.bmHeight.abs())
}

/// A click-through, layered window to show the photograph in.
unsafe fn ghost_window() -> Option<HWND> {
    const WS_POPUP: u32 = 0x8000_0000;
    const WS_EX_TOPMOST: u32 = 0x8;
    const WS_EX_TRANSPARENT: u32 = 0x20;
    const WS_EX_TOOLWINDOW: u32 = 0x80;
    const WS_EX_LAYERED: u32 = 0x8_0000;
    const WS_EX_NOACTIVATE: u32 = 0x800_0000;
    let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
    CreateWindowExW(
        WINDOW_EX_STYLE(
            WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_NOACTIVATE,
        ),
        PCWSTR(class.as_ptr()),
        PCWSTR::null(),
        WINDOW_STYLE(WS_POPUP),
        0,
        0,
        1,
        1,
        HWND::default(),
        HMENU::default(),
        GetModuleHandleW(None).unwrap_or_default(),
        None,
    )
    .ok()
}

/// Where the tray icon is (the middle of it, in screen pixels). Falls back to
/// the notification area, then the bottom right of the screen the window is on.
fn tray_target(over: RECT) -> Option<POINT> {
    unsafe {
        let hwnd = crate::tray::tray_hwnd();
        if hwnd != 0 {
            let id = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: HWND(hwnd as *mut c_void),
                uID: 0,
                guidItem: GUID::zeroed(),
            };
            if let Ok(r) = Shell_NotifyIconGetRect(&id) {
                if r.right > r.left && r.bottom > r.top {
                    return Some(middle(r));
                }
            }
        }
        // The icon may be among the hidden ones: aim for the notification area.
        if let Ok(taskbar) = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) {
            if let Ok(area) = FindWindowExW(
                taskbar,
                HWND::default(),
                w!("TrayNotifyWnd"),
                PCWSTR::null(),
            ) {
                let mut r = RECT::default();
                if GetWindowRect(area, &mut r).is_ok() && r.right > r.left {
                    return Some(middle(r));
                }
            }
        }
    }
    // No tray found: the bottom right corner of the window's own screen.
    let _ = over;
    None
}

fn middle(r: RECT) -> POINT {
    POINT {
        x: (r.left + r.right) / 2,
        y: (r.top + r.bottom) / 2,
    }
}

/// Whether Windows plays client-area animation (off under "Show animations"
/// switched off, or in remote sessions).
fn client_animation_on() -> bool {
    let mut on: i32 = 1;
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut i32 as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_err() || on != 0
}
