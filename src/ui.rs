//! RightType's look — Windows only.
//!
//! A small retained UI kit shared by the Settings, Welcome and Statistics
//! windows:
//!
//! - **DPI**: the process is per-monitor-DPI-aware (v2, see `main`), and every
//!   length here is written in 96-DPI units and scaled with [`px`], so text and
//!   shapes are crisp at 125–200 %. A window opens at the DPI of the monitor
//!   under the mouse; dragged to a monitor with another scale it re-lays itself
//!   out (`WM_DPICHANGED`) instead of being bitmap-stretched.
//! - **Theme**: a light and a dark palette; [`refresh`] follows the Windows
//!   "app mode" setting each time a window opens.
//! - **Controls** are real Win32 `BUTTON`/`STATIC`/`EDIT` children — toggles are
//!   genuine check boxes, the mode picker and navigation are genuine radio
//!   buttons — so keyboard navigation (Tab, arrows, Space) and screen readers
//!   keep working. Only their *painting* is ours: buttons through
//!   `NM_CUSTOMDRAW`, text through `SS_OWNERDRAW`, shapes anti-aliased with
//!   GDI+ and text with ClearType GDI.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::OnceLock;

use native_windows_gui as nwg;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWINDOWATTRIBUTE,
};
use windows::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, SRCCOPY,
};
use windows::Win32::Graphics::Gdi::{
    CreateFontW, CreateSolidBrush, DeleteObject, DrawTextW, FillRect, InvalidateRect, SelectObject,
    SetBkMode, SetTextColor, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET,
    DRAW_TEXT_FORMAT, DT_CALCRECT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HBRUSH, HDC, HFONT, HGDIOBJ, OUT_DEFAULT_PRECIS,
    TRANSPARENT,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Graphics::GdiPlus::{
    FillModeAlternate, GdipAddPathArc, GdipClosePathFigure, GdipCreateFromHDC, GdipCreatePath,
    GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath,
    GdipDeletePen, GdipDrawPath, GdipFillEllipse, GdipFillPath, GdipSetPixelOffsetMode,
    GdipSetSmoothingMode, GdiplusStartup, GdiplusStartupInput, GpBrush, GpGraphics, GpPath, GpPen,
    GpSolidFill, PixelOffsetModeHalf, SmoothingModeAntiAlias, UnitPixel,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Controls::SetWindowTheme;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForMonitor, GetDpiForSystem, MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, GetClientRect, GetCursorPos, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, SendMessageW, SetWindowPos, SetWindowTextW, ShowWindow, SystemParametersInfoW,
    GWL_EXSTYLE, GWL_STYLE, HMENU, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_EX_STYLE, WINDOW_STYLE,
};
use zeroize::Zeroize;

// ------------------------------------------------------------------ palette

/// A colour as `0xRRGGBB`.
pub type Rgb = u32;

/// `a` moved `percent` (0–100) of the way to `b`.
pub fn blend(a: Rgb, b: Rgb, percent: u8) -> Rgb {
    let t = percent.min(100) as u32;
    let ch = |c: Rgb, s: u32| (c >> s) & 0xFF;
    [0, 8, 16]
        .iter()
        .map(|&sh| ((ch(a, sh) * (100 - t) + ch(b, sh) * t) / 100) << sh)
        .sum()
}

/// Every colour the windows use.
#[derive(Clone, PartialEq, Eq)]
pub struct Palette {
    /// Window background.
    pub bg: Rgb,
    /// Cards and the navigation's selected item.
    pub surface: Rgb,
    /// A card row under the mouse.
    pub surface_hover: Rgb,
    /// Inset fields: the segmented picker's track, text boxes.
    pub inset: Rgb,
    /// Hairlines around cards and fields.
    pub border: Rgb,
    pub text: Rgb,
    pub text_dim: Rgb,
    pub accent: Rgb,
    pub accent_hover: Rgb,
    pub accent_pressed: Rgb,
    /// Text and knobs drawn on the accent colour.
    pub on_accent: Rgb,
    /// Secondary buttons.
    pub button: Rgb,
    pub button_hover: Rgb,
    pub button_pressed: Rgb,
    /// An "off" switch's outline and knob.
    pub toggle_off: Rgb,
    /// Key caps on the hotkeys page.
    pub keycap: Rgb,
    pub keycap_border: Rgb,
}

const DARK: Palette = Palette {
    bg: 0x202020,
    surface: 0x2B2B2B,
    surface_hover: 0x323232,
    inset: 0x1C1C1C,
    border: 0x3A3A3A,
    text: 0xF3F3F3,
    text_dim: 0xABABAB,
    accent: 0x4CC2FF,
    accent_hover: 0x62CBFF,
    accent_pressed: 0x3AA6DD,
    on_accent: 0x0B1A24,
    button: 0x373737,
    button_hover: 0x3F3F3F,
    button_pressed: 0x2F2F2F,
    toggle_off: 0xC5C5C5,
    keycap: 0x383838,
    keycap_border: 0x4A4A4A,
};

const LIGHT: Palette = Palette {
    bg: 0xF3F3F3,
    surface: 0xFFFFFF,
    surface_hover: 0xF7F7F7,
    inset: 0xF3F3F3,
    border: 0xE3E3E3,
    text: 0x1B1B1B,
    text_dim: 0x5E5E5E,
    accent: 0x005FB8,
    accent_hover: 0x1A6FC0,
    accent_pressed: 0x0B4F94,
    on_accent: 0xFFFFFF,
    button: 0xFBFBFB,
    button_hover: 0xF4F4F4,
    button_pressed: 0xEDEDED,
    toggle_off: 0x7A7A7A,
    keycap: 0xF7F7F7,
    keycap_border: 0xD5D5D5,
};

static DARK_MODE: AtomicBool = AtomicBool::new(true);
/// Windows' High Contrast is on: every colour comes from the system's
/// contrast theme instead (see [`contrast_palette`]).
static HIGH_CONTRAST: AtomicBool = AtomicBool::new(false);
/// The palette built from the contrast theme last time it was read. Kept
/// (leaked) for the `'static` borrows of [`pal`]; a new one is made only when
/// the theme's colours change.
static CONTRAST_PAL: std::sync::atomic::AtomicPtr<Palette> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
static DPI: AtomicU32 = AtomicU32::new(96);

/// Re-read the Windows theme and DPI. Call when a window opens: it will open
/// on the monitor under the mouse (see [`size_and_center`]), at that monitor's
/// DPI.
pub fn refresh() {
    let contrast = high_contrast();
    HIGH_CONTRAST.store(contrast, Ordering::Relaxed);
    if contrast {
        let fresh = contrast_palette();
        let old = CONTRAST_PAL.load(Ordering::Acquire);
        if old.is_null() || unsafe { &*old } != &fresh {
            CONTRAST_PAL.store(Box::into_raw(Box::new(fresh)), Ordering::Release);
        }
        DARK_MODE.store(luminance(pal().bg) < 0x80, Ordering::Relaxed);
        DPI.store(monitor_dpi(cursor_monitor()), Ordering::Relaxed);
        return;
    }
    DARK_MODE.store(windows_prefers_dark(), Ordering::Relaxed);
    DPI.store(monitor_dpi(cursor_monitor()), Ordering::Relaxed);
}

/// The monitor under the mouse pointer.
pub fn cursor_monitor() -> HMONITOR {
    unsafe {
        let mut pt = windows::Win32::Foundation::POINT::default();
        let _ = GetCursorPos(&mut pt);
        MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
    }
}

/// The effective DPI of `monitor` (the system DPI if Windows cannot say).
pub fn monitor_dpi(monitor: HMONITOR) -> u32 {
    let (mut x, mut y) = (0u32, 0u32);
    let ok = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) }.is_ok();
    let dpi = if ok && x != 0 {
        x
    } else {
        unsafe { GetDpiForSystem() }
    };
    if dpi == 0 {
        96
    } else {
        dpi
    }
}

/// The work area (the screen minus the taskbar) of `monitor`, in physical
/// pixels.
pub fn work_area(monitor: HMONITOR) -> RECT {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return info.rcWork;
    }
    let mut wa = RECT::default();
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut wa as *mut RECT as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
    }
    wa
}

/// Is Windows' High Contrast on? Read live (cheap).
pub fn high_contrast() -> bool {
    use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
    use windows::Win32::UI::WindowsAndMessaging::SPI_GETHIGHCONTRAST;
    let mut hc = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some(&mut hc as *mut HIGHCONTRASTW as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
        .is_ok()
            && hc.dwFlags.contains(HCF_HIGHCONTRASTON)
    }
}

/// A system colour as `0xRRGGBB`.
pub fn sys_rgb(index: windows::Win32::Graphics::Gdi::SYS_COLOR_INDEX) -> Rgb {
    let c = unsafe { windows::Win32::Graphics::Gdi::GetSysColor(index) };
    ((c & 0xFF) << 16) | (c & 0xFF00) | ((c >> 16) & 0xFF)
}

fn luminance(rgb: Rgb) -> u32 {
    let (r, g, b) = ((rgb >> 16) & 0xFF, (rgb >> 8) & 0xFF, rgb & 0xFF);
    (r * 299 + g * 587 + b * 114) / 1000
}

/// The contrast theme's own colours, used as they are: text on window
/// colour, the selection colours for anything picked or primary, and plain
/// outlines instead of subtle fills (which a contrast theme does not have).
fn contrast_palette() -> Palette {
    use windows::Win32::Graphics::Gdi::{
        COLOR_BTNFACE, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    };
    let window = sys_rgb(COLOR_WINDOW);
    let text = sys_rgb(COLOR_WINDOWTEXT);
    let hi = sys_rgb(COLOR_HIGHLIGHT);
    Palette {
        bg: window,
        surface: window,
        surface_hover: window,
        inset: window,
        border: text,
        text,
        text_dim: text,
        accent: hi,
        accent_hover: hi,
        accent_pressed: hi,
        on_accent: sys_rgb(COLOR_HIGHLIGHTTEXT),
        button: sys_rgb(COLOR_BTNFACE),
        button_hover: sys_rgb(COLOR_BTNFACE),
        button_pressed: sys_rgb(COLOR_BTNFACE),
        toggle_off: text,
        keycap: window,
        keycap_border: text,
    }
}

/// The palette for the current theme.
pub fn pal() -> &'static Palette {
    if HIGH_CONTRAST.load(Ordering::Relaxed) {
        let p = CONTRAST_PAL.load(Ordering::Acquire);
        if !p.is_null() {
            return unsafe { &*p };
        }
    }
    if is_dark() {
        &DARK
    } else {
        &LIGHT
    }
}

pub fn is_dark() -> bool {
    DARK_MODE.load(Ordering::Relaxed)
}

/// A DWORD of the current user's registry, if it is there.
pub fn user_dword(key: &str, value: &str) -> Option<u32> {
    let key: Vec<u16> = format!("{key}\0").encode_utf16().collect();
    let value: Vec<u16> = format!("{value}\0").encode_utf16().collect();
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.is_ok().then_some(data)
}

/// Windows' own autocorrect for the hardware keyboard (Settings → Time &
/// language → Typing) is on: it may change an English word again after
/// RightType wrote it.
pub fn windows_autocorrects() -> bool {
    user_dword(
        "Software\\Microsoft\\Input\\Settings",
        "EnableHwkbAutocorrection",
    ) == Some(1)
}

/// `AppsUseLightTheme` = 0 means the user chose dark apps. Missing = light,
/// the Windows default.
fn windows_prefers_dark() -> bool {
    let key: Vec<u16> = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize\0"
        .encode_utf16()
        .collect();
    let value: Vec<u16> = "AppsUseLightTheme\0".encode_utf16().collect();
    let mut data: u32 = 1;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    status.is_ok() && data == 0
}

/// Scale a 96-DPI length to the display.
pub fn px(v: i32) -> i32 {
    px_at(v, DPI.load(Ordering::Relaxed))
}

/// Scale a 96-DPI length to a display of `dpi`.
pub fn px_at(v: i32, dpi: u32) -> i32 {
    (v * dpi as i32 + 48) / 96
}

fn pxf(v: f32) -> f32 {
    v * DPI.load(Ordering::Relaxed) as f32 / 96.0
}

pub fn colorref(rgb: Rgb) -> COLORREF {
    COLORREF(((rgb & 0xFF) << 16) | (rgb & 0xFF00) | ((rgb >> 16) & 0xFF))
}

fn argb(rgb: Rgb) -> u32 {
    0xFF00_0000 | rgb
}

// ------------------------------------------------------------------ fonts

/// The fonts one window uses, sized for the current DPI.
pub struct Fonts {
    pub body: HFONT,
    pub body_strong: HFONT,
    pub small: HFONT,
    pub subtitle: HFONT,
    pub title: HFONT,
    pub display: HFONT,
    /// Windows' icon font for palette rows and headings; `None` where the
    /// system has none (before Windows 10).
    pub icons: Option<HFONT>,
    pub icons_small: Option<HFONT>,
}

impl Fonts {
    pub fn new() -> Fonts {
        Fonts {
            body: make_font(14, 400),
            body_strong: make_font(14, 600),
            small: make_font(12, 400),
            subtitle: make_font(17, 600),
            title: make_font(24, 600),
            display: make_font(34, 600),
            icons: make_icon_font(16),
            icons_small: make_icon_font(13),
        }
    }
}

impl Drop for Fonts {
    fn drop(&mut self) {
        for font in [
            self.body,
            self.body_strong,
            self.small,
            self.subtitle,
            self.title,
            self.display,
        ]
        .into_iter()
        .chain(self.icons)
        .chain(self.icons_small)
        {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
    }
}

/// The interface typeface: IBM Plex Sans Thai (SIL OFL, `assets/fonts`),
/// embedded in the binary. One family draws both Thai and Latin, so a mixed
/// line such as `เปิดพร้อม Windows` is set in one consistent design instead of
/// Segoe UI with a Thai fallback font. Two weights are bundled; GDI names the
/// semibold one as its own family.
static FONT_REGULAR: &[u8] = include_bytes!("../assets/fonts/IBMPlexSansThai-Regular.ttf");
static FONT_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/IBMPlexSansThai-SemiBold.ttf");
const FACE_REGULAR: &str = "IBM Plex Sans Thai";
const FACE_SEMIBOLD: &str = "IBM Plex Sans Thai SmBld";
/// Used if the embedded fonts could not be registered.
const FACE_FALLBACK: &str = "Segoe UI";

static FONTS_LOADED: AtomicBool = AtomicBool::new(false);

/// Make the embedded typeface available to this process (only). Call once at
/// startup, before any window or toast is created.
pub fn load_fonts() {
    let mut ok = true;
    for data in [FONT_REGULAR, FONT_SEMIBOLD] {
        let mut count = 0u32;
        let handle = unsafe {
            windows::Win32::Graphics::Gdi::AddFontMemResourceEx(
                data.as_ptr() as *const c_void,
                data.len() as u32,
                None,
                // Written by the call despite the `*const` in the binding.
                std::ptr::addr_of_mut!(count).cast_const(),
            )
        };
        ok &= !handle.is_invalid() && count > 0;
    }
    FONTS_LOADED.store(ok, Ordering::Relaxed);
}

/// A font of the interface typeface at `size` 96-DPI pixels of character
/// height; `weight` ≥ 600 selects the semibold cut.
pub fn make_font(size: i32, weight: i32) -> HFONT {
    make_font_at(size, weight, DPI.load(Ordering::Relaxed))
}

/// [`make_font`] for a display of `dpi`.
pub fn make_font_at(size: i32, weight: i32, dpi: u32) -> HFONT {
    let (face, weight) = if !FONTS_LOADED.load(Ordering::Relaxed) {
        (FACE_FALLBACK, weight)
    } else if weight >= 600 {
        (FACE_SEMIBOLD, 400)
    } else {
        (FACE_REGULAR, 400)
    };
    let face: Vec<u16> = format!("{face}\0").encode_utf16().collect();
    unsafe {
        CreateFontW(
            -px_at(size, dpi),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(face.as_ptr()),
        )
    }
}

/// The icon font this Windows has: Segoe Fluent Icons (Windows 11), else
/// Segoe MDL2 Assets (Windows 10). The glyphs used are in both, at the same
/// code points.
fn icon_face() -> Option<&'static str> {
    static FACE: OnceLock<Option<&'static str>> = OnceLock::new();
    *FACE.get_or_init(|| {
        ["Segoe Fluent Icons", "Segoe MDL2 Assets"]
            .into_iter()
            .find(|&face| installed(face))
    })
}

/// Whether GDI gives `face` itself when asked for it, rather than a stand-in.
fn installed(face: &str) -> bool {
    let wide: Vec<u16> = format!("{face}\0").encode_utf16().collect();
    unsafe {
        let font = CreateFontW(
            -16,
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(wide.as_ptr()),
        );
        let hdc = windows::Win32::Graphics::Gdi::GetDC(None);
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        let mut got = [0u16; 64];
        let n = windows::Win32::Graphics::Gdi::GetTextFaceW(hdc, Some(&mut got));
        SelectObject(hdc, old);
        windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
        let _ = DeleteObject(HGDIOBJ(font.0));
        let n = (n.max(1) as usize - 1).min(got.len());
        String::from_utf16_lossy(&got[..n]).eq_ignore_ascii_case(face)
    }
}

/// The icon font at `size` 96-DPI pixels, if Windows has one.
fn make_icon_font(size: i32) -> Option<HFONT> {
    let face: Vec<u16> = format!("{}\0", icon_face()?).encode_utf16().collect();
    Some(unsafe {
        CreateFontW(
            -px(size),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            DEFAULT_CHARSET.0 as u32,
            OUT_DEFAULT_PRECIS.0 as u32,
            CLIP_DEFAULT_PRECIS.0 as u32,
            CLEARTYPE_QUALITY.0 as u32,
            0,
            PCWSTR(face.as_ptr()),
        )
    })
}

// ------------------------------------------------------------------ show

/// Show a window that has just been built, whole. Every label and button is
/// its own child window that paints separately, so a plain show let the
/// frame appear first and the controls fill in one by one. The window is
/// cloaked (DWM keeps it off screen), painted completely, then uncloaked.
/// `started` is when building it began: the debug trace reports how long
/// building and painting took.
pub fn present(hwnd: HWND, name: &str, started: std::time::Instant) {
    const DWMWA_CLOAK: i32 = 13;
    let built = started.elapsed();
    unsafe {
        let on: i32 = 1;
        let cloaked = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(DWMWA_CLOAK),
            &on as *const i32 as _,
            4,
        )
        .is_ok();
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
            hwnd,
            None,
            None,
            windows::Win32::Graphics::Gdi::RDW_ERASE
                | windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN
                | windows::Win32::Graphics::Gdi::RDW_UPDATENOW,
        );
        if cloaked {
            let off: i32 = 0;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWINDOWATTRIBUTE(DWMWA_CLOAK),
                &off as *const i32 as _,
                4,
            );
        }
        let _ = windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow(hwnd);
    }
    crate::hook::e2e_trace(format!(
        "window {name}: built in {} ms, shown painted at {} ms",
        built.as_millis(),
        started.elapsed().as_millis()
    ));
}

// ------------------------------------------------------------------ frame

/// Title bar matching the theme, rounded corners on Windows 11.
pub fn apply_frame(hwnd: HWND) {
    unsafe {
        let dark: i32 = is_dark() as i32;
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
        // Windows 11: caption the same colour as the window, so the title bar
        // and the content read as one surface. Older builds ignore it.
        let caption = colorref(pal().bg);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(35), // DWMWA_CAPTION_COLOR
            &caption as *const COLORREF as _,
            4,
        );
    }
}

/// Resize `hwnd` so its client area is `w`×`h` (96-DPI units) and centre it in
/// the work area of the monitor under the mouse — the monitor whose DPI
/// [`refresh`] picked.
pub fn size_and_center(hwnd: HWND, w: i32, h: i32) {
    unsafe {
        let style = WINDOW_STYLE(GetWindowLongPtrW(hwnd, GWL_STYLE) as u32);
        let ex = WINDOW_EX_STYLE(GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32);
        let mut rc = RECT {
            left: 0,
            top: 0,
            right: px(w),
            bottom: px(h),
        };
        let _ = AdjustWindowRectExForDpi(&mut rc, style, false, ex, DPI.load(Ordering::Relaxed));
        let (ww, wh) = (rc.right - rc.left, rc.bottom - rc.top);
        let wa = work_area(cursor_monitor());
        let x = wa.left + ((wa.right - wa.left) - ww).max(0) / 2;
        let y = wa.top + ((wa.bottom - wa.top) - wh).max(0) / 2;
        let _ = SetWindowPos(hwnd, None, x, y, ww, wh, SWP_NOZORDER | SWP_NOACTIVATE);
        redraw_all(hwnd);
    }
}

/// Repaint a window and its controls in full. A window drawn before it
/// reached its final size and place can otherwise keep showing the plain
/// background where the first drawing was lost (seen at 144 DPI: the
/// typing practice text and keyboard missing until something changed).
pub fn redraw_all(hwnd: HWND) {
    use windows::Win32::Graphics::Gdi::{
        RedrawWindow, RDW_ALLCHILDREN, RDW_ERASE, RDW_FRAME, RDW_INVALIDATE,
    };
    unsafe {
        let _ = RedrawWindow(
            hwnd,
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_FRAME,
        );
    }
}

// ------------------------------------------------------------------ GDI+

fn gdiplus() -> bool {
    static STARTED: OnceLock<bool> = OnceLock::new();
    *STARTED.get_or_init(|| unsafe {
        let mut token = 0usize;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        GdiplusStartup(&mut token, &input, std::ptr::null_mut()).0 == 0
    })
}

/// Anti-aliased shapes on one device context.
pub struct Gfx {
    g: *mut GpGraphics,
}

impl Gfx {
    pub fn new(hdc: HDC) -> Option<Gfx> {
        if !gdiplus() {
            return None;
        }
        let mut g = std::ptr::null_mut();
        unsafe {
            if GdipCreateFromHDC(hdc, &mut g).0 != 0 || g.is_null() {
                return None;
            }
            GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            GdipSetPixelOffsetMode(g, PixelOffsetModeHalf);
        }
        Some(Gfx { g })
    }

    fn path(x: f32, y: f32, w: f32, h: f32, r: f32) -> *mut GpPath {
        let mut path = std::ptr::null_mut();
        unsafe {
            GdipCreatePath(FillModeAlternate, &mut path);
            let d = (r * 2.0).min(w).min(h).max(0.0);
            if d <= 0.5 {
                GdipAddPathArc(path, x, y, 0.0, 0.0, 180.0, 90.0);
                GdipAddPathArc(path, x + w, y, 0.0, 0.0, 270.0, 90.0);
                GdipAddPathArc(path, x + w, y + h, 0.0, 0.0, 0.0, 90.0);
                GdipAddPathArc(path, x, y + h, 0.0, 0.0, 90.0, 90.0);
            } else {
                GdipAddPathArc(path, x, y, d, d, 180.0, 90.0);
                GdipAddPathArc(path, x + w - d, y, d, d, 270.0, 90.0);
                GdipAddPathArc(path, x + w - d, y + h - d, d, d, 0.0, 90.0);
                GdipAddPathArc(path, x, y + h - d, d, d, 90.0, 90.0);
            }
            GdipClosePathFigure(path);
        }
        path
    }

    /// Fill a rounded rectangle; coordinates are device pixels, `r` too.
    pub fn fill_round(&self, rc: RECT, r: f32, color: Rgb) {
        unsafe {
            let path = Self::path(
                rc.left as f32,
                rc.top as f32,
                (rc.right - rc.left) as f32,
                (rc.bottom - rc.top) as f32,
                r,
            );
            let mut brush: *mut GpSolidFill = std::ptr::null_mut();
            GdipCreateSolidFill(argb(color), &mut brush);
            GdipFillPath(self.g, brush as *mut GpBrush, path);
            GdipDeleteBrush(brush as *mut GpBrush);
            GdipDeletePath(path);
        }
    }

    /// Outline a rounded rectangle with a `width`-pixel line inside `rc`.
    pub fn stroke_round(&self, rc: RECT, r: f32, width: f32, color: Rgb) {
        unsafe {
            let inset = width / 2.0;
            let path = Self::path(
                rc.left as f32 + inset,
                rc.top as f32 + inset,
                (rc.right - rc.left) as f32 - width,
                (rc.bottom - rc.top) as f32 - width,
                (r - inset).max(0.0),
            );
            let mut pen: *mut GpPen = std::ptr::null_mut();
            GdipCreatePen1(argb(color), width, UnitPixel, &mut pen);
            GdipDrawPath(self.g, pen, path);
            GdipDeletePen(pen);
            GdipDeletePath(path);
        }
    }

    pub fn fill_circle(&self, cx: f32, cy: f32, radius: f32, color: Rgb) {
        unsafe {
            let mut brush: *mut GpSolidFill = std::ptr::null_mut();
            GdipCreateSolidFill(argb(color), &mut brush);
            GdipFillEllipse(
                self.g,
                brush as *mut GpBrush,
                cx - radius,
                cy - radius,
                radius * 2.0,
                radius * 2.0,
            );
            GdipDeleteBrush(brush as *mut GpBrush);
        }
    }
}

impl Drop for Gfx {
    fn drop(&mut self) {
        unsafe {
            GdipDeleteGraphics(self.g);
        }
    }
}

// ------------------------------------------------------------------ text

pub fn fill(hdc: HDC, rc: RECT, color: Rgb) {
    unsafe {
        let brush = CreateSolidBrush(colorref(color));
        FillRect(hdc, &rc, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

/// Draw `text` in `rc`. Returns the height it needed.
pub fn text(
    hdc: HDC,
    s: &str,
    mut rc: RECT,
    font: HFONT,
    color: Rgb,
    flags: DRAW_TEXT_FORMAT,
) -> i32 {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    if wide.is_empty() {
        return 0;
    }
    unsafe {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(color));
        let h = DrawTextW(hdc, &mut wide, &mut rc, flags | DT_NOPREFIX);
        SelectObject(hdc, old);
        h
    }
}

/// A single symbol (e.g. a check mark) centred in `rc`, sized to fit it.
pub fn glyph(hdc: HDC, s: &str, rc: RECT, color: Rgb) {
    let size = (rc.bottom - rc.top) * 7 / 10;
    // `make_font` takes 96-DPI units; `rc` is already in device pixels.
    let dpi = DPI.load(Ordering::Relaxed) as i32;
    let font = make_font(size * 96 / dpi.max(1), 700);
    text(
        hdc,
        s,
        rc,
        font,
        color,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    unsafe {
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}

/// Width in device pixels of `s` on one line in the interface typeface, on a
/// display of `dpi`.
pub fn text_width_at(s: &str, size: i32, weight: i32, dpi: u32) -> i32 {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    let font = make_font_at(size, weight, dpi);
    let mut rc = RECT::default();
    unsafe {
        let hdc = windows::Win32::Graphics::Gdi::GetDC(None);
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        DrawTextW(
            hdc,
            &mut wide,
            &mut rc,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(hdc, old);
        windows::Win32::Graphics::Gdi::ReleaseDC(None, hdc);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
    wide.zeroize();
    rc.right - rc.left
}

/// Height `s` needs when wrapped to `width` device pixels.
/// The width of `s` on one line in `font`.
pub fn measure_width(hdc: HDC, s: &str, font: HFONT) -> i32 {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    let mut m = RECT::default();
    unsafe {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        DrawTextW(
            hdc,
            &mut wide,
            &mut m,
            DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(hdc, old);
    }
    m.right - m.left
}

pub fn measure(hdc: HDC, s: &str, width: i32, font: HFONT) -> i32 {
    let mut wide: Vec<u16> = s.encode_utf16().collect();
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: 0,
    };
    unsafe {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        DrawTextW(
            hdc,
            &mut wide,
            &mut rc,
            DT_WORDBREAK | DT_CALCRECT | DT_NOPREFIX,
        );
        SelectObject(hdc, old);
    }
    rc.bottom
}

fn window_text(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, &mut buf);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

// ------------------------------------------------------------------ controls

/// How a text control is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TextStyle {
    Display,
    Title,
    Subtitle,
    Body,
    BodyStrong,
    Dim,
    Small,
    /// `Shift + Backspace` drawn as key caps.
    Keys,
    /// A palette heading: small dim text after an icon (a glyph of the icon
    /// font, kept out of the text so a screen reader does not read it).
    Heading(char),
}

#[derive(Clone)]
pub enum Kind {
    Text(TextStyle),
    /// Filled accent button — the one action a window is about.
    Primary,
    /// Neutral button.
    Secondary,
    /// A whole card row that is a check box: title, optional description,
    /// switch at the right.
    Toggle {
        sub: String,
    },
    /// One option of a segmented picker (a radio button).
    Segment,
    /// A navigation entry (a radio button).
    Nav,
    Edit,
    /// A list with columns (a report-view list view); drawn by Windows in
    /// the window's colours.
    Table,
    /// A palette row: left-aligned text with an optional number key cap in
    /// front (`"3\tFix text"`; `"\tFix text"` without), and `hint` in dim
    /// text at the right (a state, or what a key does). `icon` (a glyph of
    /// the icon font) goes between the key cap and the text.
    Row {
        hint: String,
        icon: char,
    },
}

/// Something done to a table row with the mouse or keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableEvent {
    /// Right-click (or the context-menu key): show the row's actions here,
    /// in screen pixels.
    Menu { x: i32, y: i32 },
    /// Delete pressed on the selected row.
    Delete,
    /// The selection moved.
    Selected,
    /// A row double-clicked, or Enter pressed on it.
    Activated,
}

/// Receives table events.
type TableHandler = Rc<dyn Fn(u16, TableEvent)>;

pub struct Control {
    pub hwnd: HWND,
    pub id: u16,
    kind: Kind,
    /// The colour under this control.
    bg: Rgb,
    /// 0 = shown on every page.
    page: u8,
    /// Where it sits, in 96-DPI units (re-scaled on a DPI change).
    rc96: (i32, i32, i32, i32),
}

/// Background painter: draws cards and other decoration for the given page
/// into the window's client area.
pub type Painter = Box<dyn Fn(&Gfx, HDC, RECT, u8)>;

/// Receives the id of a clicked control.
type ClickHandler = Rc<dyn Fn(u16)>;

/// One themed window's controls and fonts.
pub struct Surface {
    pub hwnd: HWND,
    fonts: RefCell<Fonts>,
    /// The DPI this window is laid out for.
    dpi: std::cell::Cell<u32>,
    pub controls: RefCell<Vec<Control>>,
    page: std::cell::Cell<u8>,
    painter: Painter,
    brushes: RefCell<HashMap<Rgb, HBRUSH>>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
    on_click: RefCell<Option<ClickHandler>>,
    on_table: RefCell<Option<TableHandler>>,
    /// How far the shown page is scrolled down, in 96-DPI units.
    scroll: std::cell::Cell<i32>,
    /// The height of pages taller than the window, in 96-DPI units: those
    /// scroll with the mouse wheel.
    page_heights: RefCell<HashMap<u8, i32>>,
    /// Buttons whose label is faded toward their fill, 0–100 (percent).
    faded: RefCell<HashMap<isize, u8>>,
}

thread_local! {
    /// The scroll of the page being painted, for its painter.
    static PAINT_SCROLL: std::cell::Cell<i32> = const { std::cell::Cell::new(0) };
}

thread_local! {
    /// The open themed windows (UI thread), for [`focus_moved_to`].
    static SURFACES: RefCell<Vec<std::rc::Weak<Surface>>> = const { RefCell::new(Vec::new()) };
}

/// Keyboard focus went to `hwnd` (a focus event on the UI thread): if it is
/// a control of a scrolled page, bring it into view (Tab through a page
/// taller than the window).
pub fn focus_moved_to(hwnd: HWND) {
    let surfaces: Vec<Rc<Surface>> =
        SURFACES.with(|v| v.borrow().iter().filter_map(|w| w.upgrade()).collect());
    for s in surfaces {
        s.scroll_into_view(hwnd);
    }
}

/// While a [`Painter`] runs: how far its page is scrolled down, in 96-DPI
/// units (0 for pages that do not scroll).
pub fn page_scroll() -> i32 {
    PAINT_SCROLL.with(|s| s.get())
}

/// One wheel notch scrolls this far (96-DPI units): three lines of text.
const WHEEL_STEP: i32 = 60;
const WM_MOUSEWHEEL: u32 = 0x020A;

const WM_ERASEBKGND: u32 = 0x0014;
const WM_COMMAND: u32 = 0x0111;
const BN_CLICKED: usize = 0;
const WM_DRAWITEM: u32 = 0x002B;
const WM_NOTIFY: u32 = 0x004E;
const WM_CTLCOLOREDIT: u32 = 0x0133;
const WM_CTLCOLORSTATIC: u32 = 0x0138;
const WM_CTLCOLORBTN: u32 = 0x0135;
const WM_SETFONT: u32 = 0x0030;
const WM_DPICHANGED: u32 = 0x02E0;
const BM_GETCHECK: u32 = 0x00F0;
const BM_SETCHECK: u32 = 0x00F1;
const CDDS_PREPAINT: u32 = 1;
const CDRF_DODEFAULT: isize = 0;
const CDRF_SKIPDEFAULT: isize = 4;
const CDIS_SELECTED: u32 = 0x1;
const CDIS_DISABLED: u32 = 0x4;
const CDIS_FOCUS: u32 = 0x10;
const CDIS_HOT: u32 = 0x40;
const CDIS_SHOWKEYBOARDCUES: u32 = 0x200;

#[repr(C)]
struct NmHdr {
    hwnd_from: HWND,
    id_from: usize,
    code: u32,
}

#[repr(C)]
struct NmCustomDraw {
    hdr: NmHdr,
    draw_stage: u32,
    hdc: HDC,
    rc: RECT,
    item_spec: usize,
    item_state: u32,
    item_lparam: isize,
}

#[repr(C)]
struct DrawItem {
    ctl_type: u32,
    ctl_id: u32,
    item_id: u32,
    item_action: u32,
    item_state: u32,
    hwnd_item: HWND,
    hdc: HDC,
    rc: RECT,
    item_data: usize,
}

const WS_CHILD: u32 = 0x4000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
const WS_TABSTOP: u32 = 0x0001_0000;
const WS_GROUP: u32 = 0x0002_0000;
const WS_VSCROLL: u32 = 0x0020_0000;
const WS_CLIPCHILDREN: u32 = 0x0200_0000;
const BS_PUSHBUTTON: u32 = 0x0;
const BS_AUTOCHECKBOX: u32 = 0x3;
const BS_AUTORADIOBUTTON: u32 = 0x9;
const SS_OWNERDRAW: u32 = 0xD;
const SS_NOPREFIX: u32 = 0x80;
const ES_MULTILINE: u32 = 0x4;
const ES_AUTOVSCROLL: u32 = 0x40;
const ES_WANTRETURN: u32 = 0x1000;

impl Surface {
    /// Take over painting of `window` (its client area, children and text).
    pub fn attach(window: &nwg::Window, handler_id: usize, painter: Painter) -> Rc<Surface> {
        let hwnd = window
            .handle
            .hwnd()
            .map(|h| HWND(h as _))
            .unwrap_or_default();
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_STYLE) as u32;
            windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW(
                hwnd,
                GWL_STYLE,
                (style | WS_CLIPCHILDREN) as isize,
            );
        }
        apply_frame(hwnd);
        let surface = Rc::new(Surface {
            hwnd,
            fonts: RefCell::new(Fonts::new()),
            dpi: std::cell::Cell::new(DPI.load(Ordering::Relaxed)),
            controls: RefCell::new(Vec::new()),
            page: std::cell::Cell::new(1),
            painter,
            brushes: RefCell::new(HashMap::new()),
            handler: RefCell::new(None),
            on_click: RefCell::new(None),
            on_table: RefCell::new(None),
            scroll: std::cell::Cell::new(0),
            page_heights: RefCell::new(HashMap::new()),
            faded: RefCell::new(HashMap::new()),
        });
        let weak = Rc::downgrade(&surface);
        let handler =
            nwg::bind_raw_event_handler(&window.handle, handler_id, move |_h, msg, w, l| {
                let s = weak.upgrade()?;
                unsafe { s.on_message(msg, w, l) }.map(|r| r.0)
            })
            .ok();
        *surface.handler.borrow_mut() = handler;
        SURFACES.with(|v| {
            let mut v = v.borrow_mut();
            v.retain(|w| w.strong_count() > 0);
            v.push(Rc::downgrade(&surface));
        });
        surface
    }

    /// Stop handling messages (call before closing the window).
    pub fn detach(&self) {
        self.on_click.borrow_mut().take();
        self.on_table.borrow_mut().take();
        if let Some(h) = self.handler.borrow_mut().take() {
            let _ = nwg::unbind_raw_event_handler(&h);
        }
    }

    /// Called with the control id when a button, toggle or radio is clicked.
    pub fn on_click(&self, f: impl Fn(u16) + 'static) {
        *self.on_click.borrow_mut() = Some(Rc::new(f));
    }

    /// Called with the table's id when one of its rows is acted on.
    pub fn on_table(&self, f: impl Fn(u16, TableEvent) + 'static) {
        *self.on_table.borrow_mut() = Some(Rc::new(f));
    }

    fn brush(&self, color: Rgb) -> HBRUSH {
        *self
            .brushes
            .borrow_mut()
            .entry(color)
            .or_insert_with(|| unsafe { CreateSolidBrush(colorref(color)) })
    }

    fn find(&self, hwnd: HWND) -> Option<(Kind, Rgb)> {
        self.controls
            .borrow()
            .iter()
            .find(|c| c.hwnd == hwnd)
            .map(|c| (c.kind.clone(), c.bg))
    }

    unsafe fn on_message(&self, msg: u32, w: usize, l: isize) -> Option<LRESULT> {
        // `px` and `make_font` read the shared DPI; make it this window's
        // while it handles a message (another window may be on another
        // monitor).
        DPI.store(self.dpi.get(), Ordering::Relaxed);
        match msg {
            WM_DPICHANGED => {
                let dpi = (w & 0xFFFF) as u32;
                if dpi != 0 && dpi != self.dpi.get() {
                    self.rescale(dpi, &*(l as *const RECT));
                }
                Some(LRESULT(0))
            }
            WM_COMMAND if (w >> 16) & 0xFFFF == BN_CLICKED && l != 0 => {
                let id = (w & 0xFFFF) as u16;
                // Clone out of the cell: the callback may rebuild this window.
                let cb = self.on_click.borrow().clone();
                if let Some(cb) = cb {
                    cb(id);
                }
                Some(LRESULT(0))
            }
            WM_ERASEBKGND => {
                // Drawn off screen, then copied at once: a window repainted
                // often (practice, 30 times a second) never shows half a
                // frame.
                let screen = HDC(w as *mut c_void);
                let mut rc = RECT::default();
                let _ = GetClientRect(self.hwnd, &mut rc);
                let (cw, ch) = (rc.right - rc.left, rc.bottom - rc.top);
                let mem = CreateCompatibleDC(screen);
                let bmp = CreateCompatibleBitmap(screen, cw.max(1), ch.max(1));
                let (hdc, old) = if mem.is_invalid() || bmp.is_invalid() {
                    (screen, None)
                } else {
                    (mem, Some(SelectObject(mem, HGDIOBJ(bmp.0))))
                };
                fill(hdc, rc, pal().bg);
                if let Some(g) = Gfx::new(hdc) {
                    PAINT_SCROLL.with(|s| s.set(self.scroll.get()));
                    (self.painter)(&g, hdc, rc, self.page.get());
                    PAINT_SCROLL.with(|s| s.set(0));
                    self.paint_scroll_thumb(&g, rc);
                }
                if let Some(old) = old {
                    let _ = BitBlt(screen, 0, 0, cw, ch, mem, 0, 0, SRCCOPY);
                    SelectObject(mem, old);
                }
                if !bmp.is_invalid() {
                    let _ = DeleteObject(HGDIOBJ(bmp.0));
                }
                if !mem.is_invalid() {
                    let _ = DeleteDC(mem);
                }
                Some(LRESULT(1))
            }
            WM_MOUSEWHEEL if self.max_scroll() > 0 => {
                let notches = ((w >> 16) & 0xFFFF) as u16 as i16 as i32 / 120;
                self.scroll_to(self.scroll.get() - notches * WHEEL_STEP);
                Some(LRESULT(0))
            }
            WM_DRAWITEM => {
                let di = &*(l as *const DrawItem);
                let (kind, bg) = self.find(di.hwnd_item)?;
                if let Kind::Text(style) = kind {
                    self.paint_text(di.hdc, di.hwnd_item, di.rc, style, bg);
                    return Some(LRESULT(1));
                }
                None
            }
            WM_NOTIFY => {
                let nm = &*(l as *const NmCustomDraw);
                if let Some((Kind::Table, _)) = self.find(nm.hdr.hwnd_from) {
                    self.table_notify(nm.hdr.hwnd_from, nm.hdr.id_from as u16, nm.hdr.code, l);
                    return None;
                }
                if nm.hdr.code != windows::Win32::UI::Controls::NM_CUSTOMDRAW {
                    return None;
                }
                let (kind, bg) = self.find(nm.hdr.hwnd_from)?;
                if nm.draw_stage != CDDS_PREPAINT {
                    return Some(LRESULT(CDRF_DODEFAULT));
                }
                self.paint_button(nm, &kind, bg);
                Some(LRESULT(CDRF_SKIPDEFAULT))
            }
            WM_CTLCOLOREDIT => {
                let hdc = HDC(w as *mut c_void);
                SetTextColor(hdc, colorref(pal().text));
                windows::Win32::Graphics::Gdi::SetBkColor(hdc, colorref(pal().inset));
                Some(LRESULT(self.brush(pal().inset).0 as isize))
            }
            WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
                let child = HWND(l as *mut c_void);
                let bg = self.find(child).map(|(_, bg)| bg).unwrap_or(pal().bg);
                let hdc = HDC(w as *mut c_void);
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, colorref(pal().text));
                Some(LRESULT(self.brush(bg).0 as isize))
            }
            _ => None,
        }
    }

    /// Re-lay the window out for a new DPI: fonts, every control, and the
    /// window itself at the size Windows suggests.
    unsafe fn rescale(&self, dpi: u32, suggested: &RECT) {
        self.dpi.set(dpi);
        DPI.store(dpi, Ordering::Relaxed);
        // The controls keep using the old fonts until each is given a new
        // one, so the old set is freed only after the loop.
        let old_fonts = self.fonts.replace(Fonts::new());
        for c in self.controls.borrow().iter() {
            self.place(c);
            let font = self.font_for(&c.kind);
            SendMessageW(c.hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(0));
        }
        drop(old_fonts);
        let _ = SetWindowPos(
            self.hwnd,
            None,
            suggested.left,
            suggested.top,
            suggested.right - suggested.left,
            suggested.bottom - suggested.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
        let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
            self.hwnd,
            None,
            None,
            windows::Win32::Graphics::Gdi::RDW_ERASE
                | windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
        );
    }

    /// Put a control where it belongs, the shown page's scroll taken off.
    unsafe fn place(&self, c: &Control) {
        let (x, y, w, h) = c.rc96;
        let dy = if c.page != 0 && c.page == self.page.get() {
            self.scroll.get()
        } else {
            0
        };
        let _ = SetWindowPos(
            c.hwnd,
            None,
            px(x),
            px(y - dy),
            px(w),
            px(h),
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }

    /// Let `page` scroll: its content is `height` tall (96-DPI units).
    pub fn set_page_height(&self, page: u8, height: i32) {
        self.page_heights.borrow_mut().insert(page, height);
    }

    /// How far the shown page can scroll (96-DPI units; 0 if it fits).
    fn max_scroll(&self) -> i32 {
        let Some(&height) = self.page_heights.borrow().get(&self.page.get()) else {
            return 0;
        };
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let shown = (rc.bottom - rc.top) * 96 / self.dpi.get().max(1) as i32;
        (height - shown).max(0)
    }

    /// Scroll the shown page to `y` (clamped), moving its controls with it.
    fn scroll_to(&self, y: i32) {
        let y = y.clamp(0, self.max_scroll());
        if y == self.scroll.get() {
            return;
        }
        self.scroll.set(y);
        const WM_SETREDRAW: u32 = 0x000B;
        unsafe {
            SendMessageW(self.hwnd, WM_SETREDRAW, WPARAM(0), LPARAM(0));
            for c in self.controls.borrow().iter() {
                if c.page != 0 && c.page == self.page.get() {
                    self.place(c);
                }
            }
            SendMessageW(self.hwnd, WM_SETREDRAW, WPARAM(1), LPARAM(0));
            let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
                self.hwnd,
                None,
                None,
                windows::Win32::Graphics::Gdi::RDW_ERASE
                    | windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                    | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
            );
        }
    }

    /// Bring a control of the shown page into view (keyboard focus moved to
    /// it): scroll as little as needed.
    pub fn scroll_into_view(&self, hwnd: HWND) {
        let Some((y, h)) = self
            .controls
            .borrow()
            .iter()
            .find(|c| c.hwnd == hwnd && c.page == self.page.get() && c.page != 0)
            .map(|c| (c.rc96.1, c.rc96.3))
        else {
            return;
        };
        let mut rc = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rc);
        }
        let shown = (rc.bottom - rc.top) * 96 / self.dpi.get().max(1) as i32;
        let top = self.scroll.get();
        const MARGIN: i32 = 16;
        if y - MARGIN < top {
            self.scroll_to(y - MARGIN);
        } else if y + h + MARGIN > top + shown {
            self.scroll_to(y + h + MARGIN - shown);
        }
    }

    /// A slim bar at the right edge while the shown page is scrolled or can
    /// be: where in the page the window is, and that there is more.
    fn paint_scroll_thumb(&self, g: &Gfx, rc: RECT) {
        let max = self.max_scroll();
        if max <= 0 {
            return;
        }
        let shown = rc.bottom - rc.top;
        let total = shown + px(max);
        let thumb = (shown * shown / total).max(px(32));
        let top = (shown - thumb) * px(self.scroll.get()) / px(max).max(1);
        let w = px(3);
        let x = rc.right - w - px(3);
        let bar = RECT {
            left: x,
            top: rc.top + top + px(4),
            right: x + w,
            bottom: rc.top + top + thumb - px(4),
        };
        g.fill_round(bar, w as f32 / 2.0, pal().toggle_off);
    }

    fn font_for(&self, kind: &Kind) -> HFONT {
        let f = self.fonts.borrow();
        match kind {
            Kind::Edit | Kind::Secondary | Kind::Toggle { .. } | Kind::Table | Kind::Row { .. } => {
                f.body
            }
            _ => f.body_strong,
        }
    }

    unsafe fn paint_text(&self, hdc: HDC, hwnd: HWND, rc: RECT, style: TextStyle, bg: Rgb) {
        fill(hdc, rc, bg);
        let s = window_text(hwnd);
        let f = self.fonts.borrow();
        let p = pal();
        let wrap = DT_WORDBREAK | DT_LEFT;
        match style {
            TextStyle::Display => text(hdc, &s, rc, f.display, p.text, wrap),
            TextStyle::Title => text(hdc, &s, rc, f.title, p.text, wrap),
            TextStyle::Subtitle => text(hdc, &s, rc, f.subtitle, p.text, wrap),
            TextStyle::Body => text(hdc, &s, rc, f.body, p.text, wrap),
            TextStyle::BodyStrong => text(hdc, &s, rc, f.body_strong, p.text, wrap),
            TextStyle::Dim => text(hdc, &s, rc, f.body, p.text_dim, wrap),
            TextStyle::Small => text(hdc, &s, rc, f.small, p.text_dim, wrap),
            TextStyle::Keys => {
                self.paint_keys(hdc, &s, rc);
                0
            }
            TextStyle::Heading(icon) => {
                let mut t = rc;
                if let Some(font) = f.icons_small {
                    let ic = RECT {
                        right: rc.left + px(16),
                        ..rc
                    };
                    text(
                        hdc,
                        &icon.to_string(),
                        ic,
                        font,
                        p.text_dim,
                        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
                    );
                    t.left += px(22);
                }
                text(
                    hdc,
                    &s,
                    t,
                    f.small,
                    p.text_dim,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                )
            }
        };
    }

    /// `Ctrl + Shift + CapsLock` as right-aligned key caps. With a name
    /// before a tab (`Undo\tCtrl + Z`), the name fills the space left of the
    /// caps, cut short with an ellipsis rather than hidden under them.
    unsafe fn paint_keys(&self, hdc: HDC, s: &str, rc: RECT) {
        let Some(g) = Gfx::new(hdc) else { return };
        let p = pal();
        let font = self.fonts.borrow().small;
        let (name, s) = s.split_once('\t').unwrap_or(("", s));
        let keys: Vec<&str> = s.split(" + ").collect();
        let cap_h = px(24);
        let pad = px(8);
        let gap = px(6);
        let plus_w = px(10);
        let widths: Vec<i32> = keys
            .iter()
            .map(|k| {
                let mut wide: Vec<u16> = k.encode_utf16().collect();
                let mut m = RECT::default();
                let old = SelectObject(hdc, HGDIOBJ(self.fonts.borrow().body_strong.0));
                DrawTextW(
                    hdc,
                    &mut wide,
                    &mut m,
                    DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
                );
                SelectObject(hdc, old);
                (m.right - m.left) + pad * 2
            })
            .collect();
        let total: i32 =
            widths.iter().sum::<i32>() + (keys.len() as i32 - 1).max(0) * (gap * 2 + plus_w);
        let mut x = rc.right - total;
        if !name.is_empty() {
            let room = RECT {
                right: x - px(12),
                ..rc
            };
            text(
                hdc,
                name,
                room,
                self.fonts.borrow().body,
                p.text,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
        let top = rc.top + ((rc.bottom - rc.top) - cap_h) / 2;
        for (i, (k, w)) in keys.iter().zip(&widths).enumerate() {
            if i > 0 {
                let plus = RECT {
                    left: x + gap,
                    top,
                    right: x + gap + plus_w,
                    bottom: top + cap_h,
                };
                text(
                    hdc,
                    "+",
                    plus,
                    font,
                    p.text_dim,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                x += gap * 2 + plus_w;
            }
            let cap = RECT {
                left: x,
                top,
                right: x + w,
                bottom: top + cap_h,
            };
            g.fill_round(cap, pxf(5.0), p.keycap_border);
            let inner = RECT {
                left: cap.left + px(1),
                top: cap.top + px(1),
                right: cap.right - px(1),
                bottom: cap.bottom - px(2),
            };
            g.fill_round(inner, pxf(4.0), p.keycap);
            text(
                hdc,
                k,
                inner,
                self.fonts.borrow().body_strong,
                p.text,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            x += w;
        }
    }

    unsafe fn paint_button(&self, nm: &NmCustomDraw, kind: &Kind, bg: Rgb) {
        let hdc = nm.hdc;
        let rc = nm.rc;
        let state = nm.item_state;
        let hot = state & CDIS_HOT != 0;
        let pressed = state & CDIS_SELECTED != 0;
        let disabled = state & CDIS_DISABLED != 0;
        let focus = state & CDIS_FOCUS != 0 && state & CDIS_SHOWKEYBOARDCUES != 0;
        let checked = SendMessageW(nm.hdr.hwnd_from, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;
        let label = window_text(nm.hdr.hwnd_from);
        let p = pal();
        let f = self.fonts.borrow();
        fill(hdc, rc, bg);
        let Some(g) = Gfx::new(hdc) else { return };
        let radius = pxf(4.0);
        let center = DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS;

        match kind {
            Kind::Primary => {
                let fillc = if pressed {
                    p.accent_pressed
                } else if hot {
                    p.accent_hover
                } else {
                    p.accent
                };
                g.fill_round(rc, radius, if disabled { p.button } else { fillc });
                text(hdc, &label, rc, f.body_strong, p.on_accent, center);
            }
            Kind::Secondary => {
                let fillc = if pressed {
                    p.button_pressed
                } else if hot {
                    p.button_hover
                } else {
                    p.button
                };
                let fade = self
                    .faded
                    .borrow()
                    .get(&(nm.hdr.hwnd_from.0 as isize))
                    .copied()
                    .unwrap_or(0);
                let edge = blend(p.border, fillc, fade);
                g.fill_round(rc, radius, edge);
                g.fill_round(inset(rc, px(1)), radius, fillc);
                let ink = if hot || pressed {
                    p.text
                } else {
                    blend(p.text, fillc, fade)
                };
                text(hdc, &label, rc, f.body, ink, center);
            }
            Kind::Toggle { sub } => {
                if hot {
                    g.fill_round(rc, pxf(6.0), p.surface_hover);
                }
                let sw_w = px(40);
                let sw_h = px(20);
                let right = rc.right - px(16);
                let sw = RECT {
                    left: right - sw_w,
                    top: rc.top + ((rc.bottom - rc.top) - sw_h) / 2,
                    right,
                    bottom: rc.top + ((rc.bottom - rc.top) - sw_h) / 2 + sw_h,
                };
                let knob_r = if pressed { pxf(7.0) } else { pxf(6.0) };
                let cy = (sw.top + sw.bottom) as f32 / 2.0;
                if checked {
                    let c = if hot { p.accent_hover } else { p.accent };
                    g.fill_round(sw, sw_h as f32 / 2.0, c);
                    g.fill_circle(sw.right as f32 - pxf(10.0), cy, knob_r, p.on_accent);
                } else {
                    g.stroke_round(sw, sw_h as f32 / 2.0, pxf(1.0).max(1.0), p.toggle_off);
                    g.fill_circle(sw.left as f32 + pxf(10.0), cy, knob_r, p.toggle_off);
                }
                // Title and description, left of the switch.
                let text_rc = RECT {
                    left: rc.left + px(16),
                    top: rc.top,
                    right: sw.left - px(16),
                    bottom: rc.bottom,
                };
                if sub.is_empty() {
                    text(
                        hdc,
                        &label,
                        text_rc,
                        f.body,
                        p.text,
                        DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                    );
                } else {
                    let w = text_rc.right - text_rc.left;
                    let h1 = measure(hdc, &label, w, f.body);
                    let h2 = measure(hdc, sub, w, f.small);
                    let top = rc.top + ((rc.bottom - rc.top) - (h1 + px(2) + h2)) / 2;
                    let r1 = RECT {
                        top,
                        bottom: top + h1,
                        ..text_rc
                    };
                    text(hdc, &label, r1, f.body, p.text, DT_WORDBREAK);
                    let r2 = RECT {
                        top: top + h1 + px(2),
                        bottom: rc.bottom,
                        ..text_rc
                    };
                    text(hdc, sub, r2, f.small, p.text_dim, DT_WORDBREAK);
                }
                if focus {
                    g.stroke_round(rc, pxf(6.0), pxf(2.0), p.text);
                }
                return;
            }
            Kind::Segment => {
                let pill = inset(rc, px(3));
                if checked {
                    g.fill_round(pill, radius, p.accent);
                    text(hdc, &label, pill, f.body_strong, p.on_accent, center);
                } else {
                    if hot {
                        g.fill_round(pill, radius, p.surface_hover);
                    }
                    text(hdc, &label, pill, f.body, p.text, center);
                }
            }
            Kind::Nav => {
                if checked || hot {
                    g.fill_round(
                        rc,
                        radius,
                        if checked { p.surface } else { p.surface_hover },
                    );
                }
                if checked {
                    let bar_h = px(16);
                    let bar = RECT {
                        left: rc.left,
                        top: rc.top + ((rc.bottom - rc.top) - bar_h) / 2,
                        right: rc.left + px(3),
                        bottom: rc.top + ((rc.bottom - rc.top) - bar_h) / 2 + bar_h,
                    };
                    g.fill_round(bar, pxf(1.5), p.accent);
                }
                let t = RECT {
                    left: rc.left + px(14),
                    ..rc
                };
                text(
                    hdc,
                    &label,
                    t,
                    if checked { f.body_strong } else { f.body },
                    p.text,
                    DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            Kind::Row { hint, icon } => {
                if focus || hot || pressed {
                    g.fill_round(rc, radius, p.surface_hover);
                }
                if focus {
                    let bar_h = (rc.bottom - rc.top) - px(14);
                    let bar = RECT {
                        left: rc.left + px(2),
                        top: rc.top + px(7),
                        right: rc.left + px(5),
                        bottom: rc.top + px(7) + bar_h,
                    };
                    g.fill_round(bar, pxf(1.5), p.accent);
                }
                let (number, text_s) = label.split_once('\t').unwrap_or(("", &label));
                let mut x = rc.left + px(12);
                let cap_w = px(22);
                if !number.is_empty() {
                    let cap_h = px(20);
                    let top = rc.top + ((rc.bottom - rc.top) - cap_h) / 2;
                    let cap = RECT {
                        left: x,
                        top,
                        right: x + cap_w,
                        bottom: top + cap_h,
                    };
                    g.fill_round(cap, pxf(4.0), p.keycap_border);
                    g.fill_round(inset(cap, px(1)), pxf(3.0), p.keycap);
                    text(
                        hdc,
                        number,
                        cap,
                        f.small,
                        p.text,
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                }
                x += cap_w + px(10);
                if let (Some(font), false) = (f.icons, *icon == '\0') {
                    let ic = RECT {
                        left: x,
                        right: x + px(18),
                        ..rc
                    };
                    // The selected row's icon in the accent colour: the eye
                    // finds the row by it.
                    text(
                        hdc,
                        &icon.to_string(),
                        ic,
                        font,
                        if focus { p.accent } else { p.text_dim },
                        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                    );
                    x += px(18) + px(10);
                }
                let hint_w = if hint.is_empty() {
                    0
                } else {
                    measure_width(hdc, hint, f.small) + px(12)
                };
                if !hint.is_empty() {
                    let hr = RECT {
                        left: rc.right - hint_w - px(4),
                        right: rc.right - px(12),
                        ..rc
                    };
                    text(
                        hdc,
                        hint,
                        hr,
                        f.small,
                        p.text_dim,
                        DT_RIGHT | DT_VCENTER | DT_SINGLELINE,
                    );
                }
                let tr_rc = RECT {
                    left: x,
                    right: rc.right - hint_w - px(8),
                    ..rc
                };
                text(
                    hdc,
                    text_s,
                    tr_rc,
                    f.body,
                    p.text,
                    DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
                return;
            }
            Kind::Text(_) | Kind::Edit | Kind::Table => {}
        }
        if focus {
            g.stroke_round(rc, radius + pxf(1.0), pxf(2.0), p.text);
        }
    }

    // --- building --------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    fn create(
        &self,
        class: &str,
        label: &str,
        style: u32,
        rc: (i32, i32, i32, i32),
        kind: Kind,
        bg: Rgb,
        page: u8,
    ) -> u16 {
        let id = 1000 + self.controls.borrow().len() as u16;
        let class_w: Vec<u16> = format!("{class}\0").encode_utf16().collect();
        let text_w: Vec<u16> = format!("{label}\0").encode_utf16().collect();
        let visible = if page == 0 || page == self.page.get() {
            WS_VISIBLE
        } else {
            0
        };
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class_w.as_ptr()),
                PCWSTR(text_w.as_ptr()),
                WINDOW_STYLE(WS_CHILD | visible | style),
                px(rc.0),
                px(rc.1),
                px(rc.2),
                px(rc.3),
                self.hwnd,
                HMENU(id as usize as *mut c_void),
                GetModuleHandleW(None).unwrap_or_default(),
                None,
            )
        }
        .unwrap_or_default();
        let font = self.font_for(&kind);
        unsafe {
            SendMessageW(hwnd, WM_SETFONT, WPARAM(font.0 as usize), LPARAM(1));
        }
        self.controls.borrow_mut().push(Control {
            hwnd,
            id,
            kind,
            bg,
            page,
            rc96: rc,
        });
        id
    }

    /// A text label. `rc` is `(x, y, w, h)` in 96-DPI units.
    pub fn label(
        &self,
        s: &str,
        style: TextStyle,
        rc: (i32, i32, i32, i32),
        bg: Rgb,
        page: u8,
    ) -> u16 {
        self.create(
            "STATIC",
            s,
            SS_OWNERDRAW | SS_NOPREFIX,
            rc,
            Kind::Text(style),
            bg,
            page,
        )
    }

    pub fn button(
        &self,
        s: &str,
        primary: bool,
        rc: (i32, i32, i32, i32),
        bg: Rgb,
        page: u8,
    ) -> u16 {
        let kind = if primary {
            Kind::Primary
        } else {
            Kind::Secondary
        };
        self.create(
            "BUTTON",
            s,
            BS_PUSHBUTTON | WS_TABSTOP | WS_GROUP,
            rc,
            kind,
            bg,
            page,
        )
    }

    /// One row of the palette (see [`Kind::Row`]): joins the previous row's
    /// group, so the arrow keys move between them.
    /// `icon` is a glyph of the icon font, or `'\0'` for none.
    pub fn row(
        &self,
        s: &str,
        hint: &str,
        icon: char,
        first: bool,
        rc: (i32, i32, i32, i32),
        bg: Rgb,
    ) -> u16 {
        let group = if first { WS_GROUP } else { 0 };
        self.create(
            "BUTTON",
            s,
            BS_PUSHBUTTON | WS_TABSTOP | group,
            rc,
            Kind::Row {
                hint: hint.to_string(),
                icon,
            },
            bg,
            0,
        )
    }

    /// A card row that is a check box.
    pub fn toggle(
        &self,
        title: &str,
        sub: &str,
        rc: (i32, i32, i32, i32),
        bg: Rgb,
        page: u8,
    ) -> u16 {
        self.create(
            "BUTTON",
            title,
            BS_AUTOCHECKBOX | WS_TABSTOP | WS_GROUP,
            rc,
            Kind::Toggle {
                sub: sub.to_string(),
            },
            bg,
            page,
        )
    }

    /// Radio buttons; `first` starts a new group (and takes the Tab stop).
    pub fn segment(
        &self,
        s: &str,
        first: bool,
        rc: (i32, i32, i32, i32),
        bg: Rgb,
        page: u8,
    ) -> u16 {
        let group = if first { WS_GROUP | WS_TABSTOP } else { 0 };
        self.create(
            "BUTTON",
            s,
            BS_AUTORADIOBUTTON | group,
            rc,
            Kind::Segment,
            bg,
            page,
        )
    }

    pub fn nav(&self, s: &str, first: bool, rc: (i32, i32, i32, i32)) -> u16 {
        let group = if first { WS_GROUP | WS_TABSTOP } else { 0 };
        self.create(
            "BUTTON",
            s,
            BS_AUTORADIOBUTTON | group,
            rc,
            Kind::Nav,
            pal().bg,
            0,
        )
    }

    /// A one-line text box.
    pub fn line_edit(&self, s: &str, rc: (i32, i32, i32, i32), page: u8) -> u16 {
        const ES_AUTOHSCROLL: u32 = 0x80;
        let id = self.create(
            "EDIT",
            s,
            ES_AUTOHSCROLL | WS_TABSTOP | WS_GROUP,
            rc,
            Kind::Edit,
            pal().inset,
            page,
        );
        if is_dark() {
            let theme: Vec<u16> = "DarkMode_Explorer\0".encode_utf16().collect();
            unsafe {
                let _ = SetWindowTheme(self.hwnd_of(id), PCWSTR(theme.as_ptr()), PCWSTR::null());
            }
        }
        id
    }

    pub fn edit(&self, s: &str, rc: (i32, i32, i32, i32), page: u8) -> u16 {
        let id = self.create(
            "EDIT",
            s,
            ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | WS_VSCROLL | WS_TABSTOP | WS_GROUP,
            rc,
            Kind::Edit,
            pal().inset,
            page,
        );
        let hwnd = self.hwnd_of(id);
        if is_dark() {
            let theme: Vec<u16> = "DarkMode_Explorer\0".encode_utf16().collect();
            unsafe {
                let _ = SetWindowTheme(hwnd, PCWSTR(theme.as_ptr()), PCWSTR::null());
            }
        }
        id
    }

    /// A table with `columns` (title, width in 96-DPI units); rows are put in
    /// with [`Surface::set_rows`].
    pub fn table(&self, columns: &[(&str, i32)], rc: (i32, i32, i32, i32), page: u8) -> u16 {
        const LVS_REPORT: u32 = 0x1;
        const LVS_SINGLESEL: u32 = 0x4;
        const LVS_SHOWSELALWAYS: u32 = 0x8;
        const LVS_NOSORTHEADER: u32 = 0x8000;
        const WS_BORDER: u32 = 0x0080_0000;
        const LVM_SETEXTENDEDLISTVIEWSTYLE: u32 = 0x1036;
        const LVS_EX_FULLROWSELECT: usize = 0x20;
        const LVS_EX_DOUBLEBUFFER: usize = 0x0001_0000;
        const LVM_SETBKCOLOR: u32 = 0x1001;
        const LVM_SETTEXTCOLOR: u32 = 0x1024;
        const LVM_SETTEXTBKCOLOR: u32 = 0x1026;
        const LVM_INSERTCOLUMNW: u32 = 0x1061;
        let id = self.create(
            "SysListView32",
            "",
            LVS_REPORT
                | LVS_SINGLESEL
                | LVS_SHOWSELALWAYS
                | LVS_NOSORTHEADER
                | WS_BORDER
                | WS_TABSTOP
                | WS_GROUP,
            rc,
            Kind::Table,
            pal().inset,
            page,
        );
        let hwnd = self.hwnd_of(id);
        let p = pal();
        unsafe {
            let ex = LVS_EX_FULLROWSELECT | LVS_EX_DOUBLEBUFFER;
            SendMessageW(
                hwnd,
                LVM_SETEXTENDEDLISTVIEWSTYLE,
                WPARAM(ex),
                LPARAM(ex as isize),
            );
            SendMessageW(
                hwnd,
                LVM_SETBKCOLOR,
                WPARAM(0),
                LPARAM(colorref(p.inset).0 as isize),
            );
            SendMessageW(
                hwnd,
                LVM_SETTEXTBKCOLOR,
                WPARAM(0),
                LPARAM(colorref(p.inset).0 as isize),
            );
            SendMessageW(
                hwnd,
                LVM_SETTEXTCOLOR,
                WPARAM(0),
                LPARAM(colorref(p.text).0 as isize),
            );
            if is_dark() {
                let theme: Vec<u16> = "DarkMode_Explorer\0".encode_utf16().collect();
                let _ = SetWindowTheme(hwnd, PCWSTR(theme.as_ptr()), PCWSTR::null());
            }
            for (i, (title, width)) in columns.iter().enumerate() {
                let mut text: Vec<u16> = format!("{title}\0").encode_utf16().collect();
                let col = windows::Win32::UI::Controls::LVCOLUMNW {
                    mask: windows::Win32::UI::Controls::LVCF_TEXT
                        | windows::Win32::UI::Controls::LVCF_WIDTH,
                    cx: px(*width),
                    pszText: windows::core::PWSTR(text.as_mut_ptr()),
                    ..Default::default()
                };
                SendMessageW(
                    hwnd,
                    LVM_INSERTCOLUMNW,
                    WPARAM(i),
                    LPARAM(&col as *const _ as isize),
                );
            }
        }
        id
    }

    /// Replace a table's rows (each a cell per column), keeping the row
    /// `select` selected.
    pub fn set_rows(&self, id: u16, rows: &[Vec<String>], select: Option<usize>) {
        const LVM_DELETEALLITEMS: u32 = 0x1009;
        const LVM_INSERTITEMW: u32 = 0x104D;
        const LVM_SETITEMTEXTW: u32 = 0x1074;
        let hwnd = self.hwnd_of(id);
        unsafe {
            SendMessageW(hwnd, LVM_DELETEALLITEMS, WPARAM(0), LPARAM(0));
            for (r, row) in rows.iter().enumerate() {
                for (c, cell) in row.iter().enumerate() {
                    let mut text: Vec<u16> = format!("{cell}\0").encode_utf16().collect();
                    let item = windows::Win32::UI::Controls::LVITEMW {
                        mask: windows::Win32::UI::Controls::LVIF_TEXT,
                        iItem: r as i32,
                        iSubItem: c as i32,
                        pszText: windows::core::PWSTR(text.as_mut_ptr()),
                        ..Default::default()
                    };
                    let msg = if c == 0 {
                        LVM_INSERTITEMW
                    } else {
                        LVM_SETITEMTEXTW
                    };
                    let wparam = if c == 0 { 0 } else { r };
                    SendMessageW(
                        hwnd,
                        msg,
                        WPARAM(wparam),
                        LPARAM(&item as *const _ as isize),
                    );
                }
            }
        }
        if let Some(i) = select.filter(|&i| i < rows.len()) {
            self.select_row(id, i);
        }
    }

    /// Select (and scroll to) row `i` of a table.
    pub fn select_row(&self, id: u16, i: usize) {
        const LVM_SETITEMSTATE: u32 = 0x102B;
        const LVM_ENSUREVISIBLE: u32 = 0x1013;
        let hwnd = self.hwnd_of(id);
        let state = windows::Win32::UI::Controls::LIST_VIEW_ITEM_STATE_FLAGS(
            windows::Win32::UI::Controls::LVIS_SELECTED.0
                | windows::Win32::UI::Controls::LVIS_FOCUSED.0,
        );
        let item = windows::Win32::UI::Controls::LVITEMW {
            stateMask: state,
            state,
            ..Default::default()
        };
        unsafe {
            SendMessageW(
                hwnd,
                LVM_SETITEMSTATE,
                WPARAM(i),
                LPARAM(&item as *const _ as isize),
            );
            SendMessageW(hwnd, LVM_ENSUREVISIBLE, WPARAM(i), LPARAM(0));
        }
    }

    /// The selected row of a table.
    pub fn selected_row(&self, id: u16) -> Option<usize> {
        const LVM_GETNEXTITEM: u32 = 0x100C;
        const LVNI_SELECTED: isize = 0x2;
        let i = unsafe {
            SendMessageW(
                self.hwnd_of(id),
                LVM_GETNEXTITEM,
                WPARAM(usize::MAX),
                LPARAM(LVNI_SELECTED),
            )
        }
        .0;
        usize::try_from(i).ok()
    }

    /// A table told its parent something (`WM_NOTIFY`).
    unsafe fn table_notify(&self, hwnd: HWND, id: u16, code: u32, l: isize) {
        const NM_RCLICK: u32 = (-5i32) as u32;
        const NM_DBLCLK: u32 = (-3i32) as u32;
        const VK_RETURN: u16 = 0x0D;
        const LVN_KEYDOWN: u32 = (-155i32) as u32;
        const LVN_ITEMCHANGED: u32 = (-101i32) as u32;
        const VK_DELETE: u16 = 0x2E;
        const VK_APPS: u16 = 0x5D;
        #[repr(C)]
        struct NmKey {
            hdr: NmHdr,
            vkey: u16,
            flags: u32,
        }
        let event = match code {
            NM_RCLICK => {
                let mut pt = windows::Win32::Foundation::POINT::default();
                let _ = GetCursorPos(&mut pt);
                Some(TableEvent::Menu { x: pt.x, y: pt.y })
            }
            LVN_KEYDOWN => match (*(l as *const NmKey)).vkey {
                VK_DELETE => Some(TableEvent::Delete),
                VK_RETURN => Some(TableEvent::Activated),
                VK_APPS => {
                    let mut rc = RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rc);
                    Some(TableEvent::Menu {
                        x: rc.left + px(40),
                        y: rc.top + px(40),
                    })
                }
                _ => None,
            },
            LVN_ITEMCHANGED => Some(TableEvent::Selected),
            NM_DBLCLK => Some(TableEvent::Activated),
            _ => None,
        };
        let Some(event) = event else { return };
        let cb = self.on_table.borrow().clone();
        if let Some(cb) = cb {
            cb(id, event);
        }
    }

    // --- state -----------------------------------------------------------

    pub fn hwnd_of(&self, id: u16) -> HWND {
        self.controls
            .borrow()
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.hwnd)
            .unwrap_or_default()
    }

    pub fn checked(&self, id: u16) -> bool {
        unsafe { SendMessageW(self.hwnd_of(id), BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1 }
    }

    pub fn set_checked(&self, id: u16, on: bool) {
        unsafe {
            SendMessageW(
                self.hwnd_of(id),
                BM_SETCHECK,
                WPARAM(on as usize),
                LPARAM(0),
            );
            let _ = InvalidateRect(self.hwnd_of(id), None, true);
        }
    }

    /// Fade a button's label and edge toward its fill (`percent` 0–100);
    /// it shows in full again under the mouse.
    pub fn set_fade(&self, id: u16, percent: u8) {
        let hwnd = self.hwnd_of(id);
        let percent = percent.min(100);
        let old = self.faded.borrow_mut().insert(hwnd.0 as isize, percent);
        if old != Some(percent) {
            unsafe {
                let _ = InvalidateRect(hwnd, None, true);
            }
        }
    }

    pub fn text_of(&self, id: u16) -> String {
        window_text(self.hwnd_of(id))
    }

    pub fn set_text(&self, id: u16, s: &str) {
        let wide: Vec<u16> = format!("{s}\0").encode_utf16().collect();
        unsafe {
            let _ = SetWindowTextW(self.hwnd_of(id), PCWSTR(wide.as_ptr()));
            let _ = InvalidateRect(self.hwnd_of(id), None, true);
        }
    }

    /// Show the controls of `page` (and the always-visible ones).
    pub fn show_page(&self, page: u8) {
        self.page.set(page);
        // Each page opens at its top.
        if self.scroll.replace(0) != 0 {
            for c in self.controls.borrow().iter() {
                unsafe { self.place(c) };
            }
        }
        // Swap the pages' controls with drawing off, then paint once: showing
        // them one by one repainted the window dozens of times.
        const WM_SETREDRAW: u32 = 0x000B;
        unsafe {
            SendMessageW(self.hwnd, WM_SETREDRAW, WPARAM(0), LPARAM(0));
        }
        for c in self.controls.borrow().iter() {
            let show = c.page == 0 || c.page == page;
            unsafe {
                let _ = ShowWindow(c.hwnd, if show { SW_SHOW } else { SW_HIDE });
            }
        }
        unsafe {
            SendMessageW(self.hwnd, WM_SETREDRAW, WPARAM(1), LPARAM(0));
            let _ = windows::Win32::Graphics::Gdi::RedrawWindow(
                self.hwnd,
                None,
                None,
                windows::Win32::Graphics::Gdi::RDW_ERASE
                    | windows::Win32::Graphics::Gdi::RDW_INVALIDATE
                    | windows::Win32::Graphics::Gdi::RDW_ALLCHILDREN,
            );
        }
    }

    pub fn page(&self) -> u8 {
        self.page.get()
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        for (_, brush) in self.brushes.borrow_mut().drain() {
            unsafe {
                let _ = DeleteObject(HGDIOBJ(brush.0));
            }
        }
    }
}

/// `rc` shrunk by `d` on every side.
pub fn inset(rc: RECT, d: i32) -> RECT {
    RECT {
        left: rc.left + d,
        top: rc.top + d,
        right: rc.right - d,
        bottom: rc.bottom - d,
    }
}

/// A 96-DPI `(x, y, w, h)` as a device rectangle.
pub fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: px(x),
        top: px(y),
        right: px(x + w),
        bottom: px(y + h),
    }
}

/// A card: surface fill with a hairline border.
pub fn card(g: &Gfx, rc: RECT) {
    let p = pal();
    g.fill_round(rc, pxf(8.0), p.border);
    g.fill_round(inset(rc, px(1)), pxf(7.0), p.surface);
}

/// A hairline between two card rows.
pub fn divider(hdc: HDC, x: i32, y: i32, w: i32) {
    fill(
        hdc,
        rect(x, y, w, 0).with_height(px(1).max(1)),
        pal().border,
    );
}

trait WithHeight {
    fn with_height(self, h: i32) -> RECT;
}

impl WithHeight for RECT {
    fn with_height(self, h: i32) -> RECT {
        RECT {
            bottom: self.top + h,
            ..self
        }
    }
}

/// The segmented picker's track behind its radio buttons.
pub fn track(g: &Gfx, rc: RECT) {
    let p = pal();
    g.fill_round(rc, pxf(6.0), p.border);
    g.fill_round(inset(rc, px(1)), pxf(5.0), p.inset);
}

/// A text field's frame, drawn around an edit control at `rc`.
pub fn field(g: &Gfx, rc: RECT) {
    let p = pal();
    g.fill_round(rc, pxf(4.0), p.border);
    g.fill_round(inset(rc, px(1)), pxf(3.0), p.inset);
}
