//! The tour — Windows only.
//!
//! A few short chapters that walk through the Settings window: the window is
//! dimmed, a hole is cut around the thing being explained, and a small window
//! beside it says what it does, with Back, Next and Skip. The dimming window is
//! click-through (a layered window with the hole cut by per-pixel alpha), so it
//! never gets in the way of the app; the explaining window is an ordinary
//! window, so Tab and the mouse work in it.
//!
//! Nothing is read from other programs: the things pointed at are controls of
//! RightType's own windows, found by their label. Settings is moved to the
//! right page by clicking its real sidebar entry.

use std::cell::RefCell;
use std::ffi::c_void;
use std::rc::Rc;

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{BOOL, COLORREF, HWND, LPARAM, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC,
    MonitorFromRect, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HGDIOBJ, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Graphics::GdiPlus::{
    CompositingModeSourceCopy, CompositingModeSourceOver, GdipCreateBitmapFromScan0,
    GdipCreatePen1, GdipCreateSolidFill, GdipDeleteBrush, GdipDeleteGraphics, GdipDeletePath,
    GdipDeletePen, GdipDisposeImage, GdipDrawPath, GdipFillPath, GdipGetImageGraphicsContext,
    GdipGraphicsClear, GdipSetCompositingMode, GdipSetSmoothingMode, GpBitmap, GpBrush, GpGraphics,
    GpImage, GpPen, GpSolidFill, SmoothingModeAntiAlias, UnitPixel,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, EnumChildWindows, FindWindowW, GetClassNameW, GetWindowRect,
    GetWindowTextW, IsWindowVisible, SendMessageW, SetWindowPos, UpdateLayeredWindow, HMENU,
    HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, ULW_ALPHA, WINDOW_EX_STYLE, WINDOW_STYLE,
};

use crate::icons;
use crate::ui::{self, pal, Gfx, Surface, TextStyle};

/// The chapters, in the order the tour offers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chapter {
    /// Fixing wrong-layout text: the modes, the off switch, the hotkeys.
    Fixing,
    /// Your words, your short codes, a mode for each app.
    Yours,
    /// The keyboard tools and the health check.
    Keyboard,
    /// What it never does, and updates.
    Privacy,
}

impl Chapter {
    pub const ALL: [Chapter; 4] = [
        Chapter::Fixing,
        Chapter::Yours,
        Chapter::Keyboard,
        Chapter::Privacy,
    ];
}

/// What a step points at, in the Settings window.
#[derive(Clone, Copy)]
enum Spot {
    /// The visible controls labelled with these texts, together.
    Controls(&'static [T]),
    /// A region of the page (x, y, width, height in 96-DPI client units): for
    /// what is painted on the page, not a control.
    Area(i32, i32, i32, i32),
}

struct Step {
    /// The sidebar entry to click first (its label), if the step is on
    /// another page.
    page: Option<T>,
    spot: Spot,
    title: T,
    body: T,
}

const FIXING: &[Step] = &[
    Step {
        page: Some(T::NavGeneral),
        spot: Spot::Controls(&[T::ModeAuto, T::ModeSuggest, T::ModeManual]),
        title: T::TourA1Title,
        body: T::TourA1Body,
    },
    Step {
        page: Some(T::NavGeneral),
        spot: Spot::Controls(&[T::RowEnabled]),
        title: T::TourA2Title,
        body: T::TourA2Body,
    },
    Step {
        page: Some(T::NavHotkeys),
        spot: Spot::Controls(&[T::BtnChange]),
        title: T::TourA3Title,
        body: T::TourA3Body,
    },
];

const YOURS: &[Step] = &[
    Step {
        page: Some(T::NavLearned),
        spot: Spot::Controls(&[
            T::BtnImport,
            T::BtnExport,
            T::BtnClearAll,
            T::BtnSaveLearned,
        ]),
        title: T::TourB1Title,
        body: T::TourB1Body,
    },
    Step {
        page: Some(T::NavSnippets),
        spot: Spot::Area(232, 320, 504, 146),
        title: T::TourB2Title,
        body: T::TourB2Body,
    },
    Step {
        page: Some(T::NavBlocked),
        spot: Spot::Controls(&[T::BtnAddApp, T::BtnKeepMode, T::BtnRemoveApp]),
        title: T::TourB3Title,
        body: T::TourB3Body,
    },
];

const KEYBOARD: &[Step] = &[
    Step {
        page: Some(T::NavTools),
        spot: Spot::Controls(&[T::ToolClean, T::ToolTest, T::KeyMapTitle, T::PracticeTitle]),
        title: T::TourC1Title,
        body: T::TourC1Body,
    },
    Step {
        page: Some(T::NavTools),
        spot: Spot::Controls(&[T::BtnCheckAgain]),
        title: T::TourC2Title,
        body: T::TourC2Body,
    },
];

const PRIVACY: &[Step] = &[
    Step {
        page: Some(T::NavAbout),
        // The promises card of the Privacy page.
        spot: Spot::Area(232, 146, 504, 186),
        title: T::TourD1Title,
        body: T::TourD1Body,
    },
    Step {
        page: Some(T::NavAbout),
        spot: Spot::Controls(&[T::BtnCheckUpdates]),
        title: T::TourD2Title,
        body: T::TourD2Body,
    },
];

fn steps(chapter: Chapter) -> &'static [Step] {
    match chapter {
        Chapter::Fixing => FIXING,
        Chapter::Yours => YOURS,
        Chapter::Keyboard => KEYBOARD,
        Chapter::Privacy => PRIVACY,
    }
}

// --------------------------------------------------------------- the dimming

/// A window over the one being toured: dark, with a hole, and click-through.
struct Dim {
    hwnd: HWND,
}

impl Dim {
    fn new() -> Option<Dim> {
        const WS_POPUP: u32 = 0x8000_0000;
        const WS_EX_TOPMOST: u32 = 0x8;
        const WS_EX_TRANSPARENT: u32 = 0x20;
        const WS_EX_TOOLWINDOW: u32 = 0x80;
        const WS_EX_LAYERED: u32 = 0x8_0000;
        const WS_EX_NOACTIVATE: u32 = 0x800_0000;
        let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(
                    WS_EX_TOPMOST
                        | WS_EX_TRANSPARENT
                        | WS_EX_TOOLWINDOW
                        | WS_EX_LAYERED
                        | WS_EX_NOACTIVATE,
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
        }
        .ok()?;
        Some(Dim { hwnd })
    }

    /// Cover `over` (screen pixels), with a hole at `hole` (screen pixels).
    fn show(&self, over: RECT, hole: Option<RECT>) {
        if !ui::gdiplus() {
            return;
        }
        let (w, h) = (over.right - over.left, over.bottom - over.top);
        if w <= 0 || h <= 0 {
            return;
        }
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(screen);
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
            let Ok(dib) = CreateDIBSection(mem, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(mem);
                ReleaseDC(None, screen);
                return;
            };
            let old = SelectObject(mem, HGDIOBJ(dib.0));

            // GDI+ draws on the DIB's pixels as premultiplied alpha.
            let mut bitmap: *mut GpBitmap = std::ptr::null_mut();
            GdipCreateBitmapFromScan0(
                w,
                h,
                w * 4,
                0x000E_200B,
                Some(bits as *const u8),
                &mut bitmap,
            );
            let mut g: *mut GpGraphics = std::ptr::null_mut();
            GdipGetImageGraphicsContext(bitmap as *mut GpImage, &mut g);
            GdipSetSmoothingMode(g, SmoothingModeAntiAlias);
            GdipGraphicsClear(g, 0xB4_0E_10_14);
            if let Some(hole) = hole {
                let (x, y) = ((hole.left - over.left) as f32, (hole.top - over.top) as f32);
                let (hw, hh) = (
                    (hole.right - hole.left) as f32,
                    (hole.bottom - hole.top) as f32,
                );
                let radius = ui::px(8) as f32;
                // The hole: copied over the dimming as fully transparent.
                GdipSetCompositingMode(g, CompositingModeSourceCopy);
                let path = Gfx::path(x, y, hw, hh, radius);
                let mut clear: *mut GpSolidFill = std::ptr::null_mut();
                GdipCreateSolidFill(0x0000_0000, &mut clear);
                GdipFillPath(g, clear as *mut GpBrush, path);
                GdipDeleteBrush(clear as *mut GpBrush);
                GdipSetCompositingMode(g, CompositingModeSourceOver);
                // And a ring in the accent colour around it.
                let mut pen: *mut GpPen = std::ptr::null_mut();
                GdipCreatePen1(
                    0xFF00_0000 | pal().accent,
                    ui::px(2) as f32,
                    UnitPixel,
                    &mut pen,
                );
                GdipDrawPath(g, pen, path);
                GdipDeletePen(pen);
                GdipDeletePath(path);
            }
            GdipDeleteGraphics(g);
            GdipDisposeImage(bitmap as *mut GpImage);

            let at = POINT {
                x: over.left,
                y: over.top,
            };
            let size = SIZE { cx: w, cy: h };
            let from = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                screen,
                Some(&at),
                Some(&size),
                mem,
                Some(&from),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                over.left,
                over.top,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            SelectObject(mem, old);
            let _ = DeleteObject(HGDIOBJ(dib.0));
            let _ = DeleteDC(mem);
            ReleaseDC(None, screen);
        }
    }
}

impl Drop for Dim {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

// ------------------------------------------------------------ the explaining

const W: i32 = 390;
const H: i32 = 214;
const PAD: i32 = 22;

struct Callout {
    window: nwg::Window,
    surface: Rc<Surface>,
    handler: RefCell<Option<nwg::EventHandler>>,
    count: u16,
    title: u16,
    body: u16,
    back: u16,
    next: u16,
    skip: u16,
}

struct Running {
    chapter: Chapter,
    at: usize,
    callout: Rc<Callout>,
    dim: Option<Dim>,
}

thread_local! {
    static TOUR: RefCell<Option<Running>> = const { RefCell::new(None) };
}

/// Begin `chapter` (opening Settings if it is not open).
pub fn start(chapter: Chapter) {
    end();
    crate::settings::open();
    ui::refresh();
    let started = std::time::Instant::now();
    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, H))
        .title(tr(T::TourTitle))
        .topmost(true)
        .build(&mut window);
    let surface = Surface::attach(&window, 0x5254_0030, Box::new(|_g, _hdc, _rc, _page| {}));
    let s = &surface;
    let p = pal();
    let count = s.label("", TextStyle::Small, (PAD, 16, W - 2 * PAD, 18), p.bg, 0);
    let title = s.label("", TextStyle::Subtitle, (PAD, 38, W - 2 * PAD, 30), p.bg, 0);
    let body = s.label("", TextStyle::Dim, (PAD, 74, W - 2 * PAD, 78), p.bg, 0);
    let skip = s.button_icon(
        tr(T::BtnTourSkip),
        icons::CLOSE,
        false,
        (PAD, H - 56, 112, 36),
        p.bg,
        0,
    );
    let back = s.button_icon(
        tr(T::BtnTourBack),
        icons::BACK,
        false,
        (W - PAD - 118 - 8 - 100, H - 56, 100, 36),
        p.bg,
        0,
    );
    let next = s.button_icon(
        tr(T::BtnTourNext),
        icons::CHEVRON,
        true,
        (W - PAD - 118, H - 56, 118, 36),
        p.bg,
        0,
    );
    ui::size_and_center(surface.hwnd, W, H);
    let callout = Rc::new(Callout {
        window,
        surface,
        handler: RefCell::new(None),
        count,
        title,
        body,
        back,
        next,
        skip,
    });
    if let Some(h) = callout.window.handle.hwnd() {
        ui::present(HWND(h as _), "tour", started);
    } else {
        callout.window.set_visible(true);
    }

    callout.surface.on_click(move |id| {
        let (at, last) = TOUR.with(|t| {
            t.borrow()
                .as_ref()
                .map(|r| (r.at, steps(r.chapter).len() - 1))
                .unwrap_or((0, 0))
        });
        let (back, next, skip) = TOUR.with(|t| {
            t.borrow()
                .as_ref()
                .map(|r| (r.callout.back, r.callout.next, r.callout.skip))
                .unwrap_or((0, 0, 0))
        });
        if id == skip {
            end();
        } else if id == next {
            if at >= last {
                // The next chapter carries on; the last one ends the tour.
                match Chapter::ALL.iter().position(|c| *c == chapter) {
                    Some(n) if n + 1 < Chapter::ALL.len() => start(Chapter::ALL[n + 1]),
                    _ => end(),
                }
            } else {
                go_to(at + 1);
            }
        } else if id == back && at > 0 {
            go_to(at - 1);
        }
    });
    let for_close = callout.clone();
    let handler =
        nwg::full_bind_event_handler(&callout.window.handle, move |evt, _data, handle| {
            if matches!(evt, nwg::Event::OnWindowClose) && handle == for_close.window.handle {
                end();
            }
        });
    *callout.handler.borrow_mut() = Some(handler);

    TOUR.with(|t| {
        *t.borrow_mut() = Some(Running {
            chapter,
            at: 0,
            callout,
            dim: Dim::new(),
        })
    });
    go_to(0);
}

/// Leave the tour: the dimming and the explaining window go.
pub fn end() {
    let running = TOUR.with(|t| t.borrow_mut().take());
    let Some(running) = running else {
        return;
    };
    drop(running.dim);
    let callout = running.callout;
    callout.surface.detach();
    if let Some(h) = callout.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    callout.window.close();
}

/// Show step `i` of the running chapter.
pub(crate) fn go_to(i: usize) {
    let Some((chapter, callout)) = TOUR.with(|t| {
        t.borrow_mut().as_mut().map(|r| {
            r.at = i;
            (r.chapter, r.callout.clone())
        })
    }) else {
        return;
    };
    let list = steps(chapter);
    let step = &list[i];
    let Some(settings) = find_settings() else {
        // Settings was closed: nothing left to point at.
        end();
        return;
    };
    if let Some(page) = step.page {
        click(settings, tr(page));
    }
    let hole = spot_rect(settings, step.spot);
    let over = window_rect(settings);
    TOUR.with(|t| {
        if let Some(r) = t.borrow().as_ref() {
            if let Some(dim) = &r.dim {
                dim.show(over, hole.map(|h| grow(h, ui::px(6), over)));
            }
        }
    });

    let s = &callout.surface;
    s.set_text(
        callout.count,
        &trf(
            T::TourStepOf,
            &[
                ("n", &(i + 1).to_string()),
                ("total", &list.len().to_string()),
            ],
        ),
    );
    s.set_text(callout.title, tr(step.title));
    s.set_text(callout.body, tr(step.body));
    s.set_text(
        callout.next,
        tr(
            if i + 1 == list.len() && chapter == Chapter::ALL[Chapter::ALL.len() - 1] {
                T::BtnTourDone
            } else {
                T::BtnTourNext
            },
        ),
    );
    place(
        callout.surface.hwnd,
        hole.map(|h| grow(h, ui::px(6), over)),
        over,
    );
    unsafe {
        let _ = SetWindowPos(
            callout.surface.hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE
                | SWP_NOACTIVATE,
        );
    }
    // Nothing to go back to on the first step.
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
            callout.surface.hwnd_of(callout.back),
            if i == 0 {
                windows::Win32::UI::WindowsAndMessaging::SW_HIDE
            } else {
                windows::Win32::UI::WindowsAndMessaging::SW_SHOW
            },
        );
    }
    ui::redraw_all(callout.surface.hwnd);
}

// ------------------------------------------------------------------- helpers

fn find_settings() -> Option<HWND> {
    let title: Vec<u16> = format!("{}\0", tr(T::SettingsTitle))
        .encode_utf16()
        .collect();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) }.ok()?;
    (!hwnd.0.is_null()).then_some(hwnd)
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut rc = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut rc);
    }
    rc
}

/// A visible child of `parent`: its class, label and place on screen.
struct Child {
    hwnd: HWND,
    class: String,
    text: String,
    rect: RECT,
}

fn children(parent: HWND) -> Vec<Child> {
    unsafe extern "system" fn each(hwnd: HWND, found: LPARAM) -> BOOL {
        let out = &mut *(found.0 as *mut Vec<Child>);
        if IsWindowVisible(hwnd).as_bool() {
            let mut class = [0u16; 32];
            let n = GetClassNameW(hwnd, &mut class);
            let mut text = [0u16; 160];
            let t = GetWindowTextW(hwnd, &mut text);
            out.push(Child {
                hwnd,
                class: String::from_utf16_lossy(&class[..n.max(0) as usize]),
                text: String::from_utf16_lossy(&text[..t.max(0) as usize]),
                rect: window_rect(hwnd),
            });
        }
        BOOL(1)
    }
    let mut out: Vec<Child> = Vec::new();
    unsafe {
        let _ = EnumChildWindows(
            parent,
            Some(each),
            LPARAM(&mut out as *mut Vec<Child> as isize),
        );
    }
    out
}

/// Click the sidebar entry with this label, as a person would.
fn click(settings: HWND, label: &str) {
    const BM_CLICK: u32 = 0x00F5;
    let entry = children(settings)
        .into_iter()
        .find(|c| c.class == "Button" && c.text == label);
    if let Some(c) = entry {
        unsafe {
            SendMessageW(c.hwnd, BM_CLICK, WPARAM(0), LPARAM(0));
        }
    }
}

fn spot_rect(settings: HWND, spot: Spot) -> Option<RECT> {
    match spot {
        Spot::Controls(labels) => {
            let wanted: Vec<&str> = labels.iter().map(|l| tr(*l)).collect();
            let mut found: Option<RECT> = None;
            for c in children(settings) {
                if c.class == "Button" && wanted.contains(&c.text.as_str()) {
                    found = Some(match found {
                        Some(f) => union(f, c.rect),
                        None => c.rect,
                    });
                }
            }
            found
        }
        Spot::Area(x, y, w, h) => {
            let mut at = POINT {
                x: ui::px(x),
                y: ui::px(y),
            };
            unsafe {
                let _ = ClientToScreen(settings, &mut at);
            }
            Some(RECT {
                left: at.x,
                top: at.y,
                right: at.x + ui::px(w),
                bottom: at.y + ui::px(h),
            })
        }
    }
}

fn union(a: RECT, b: RECT) -> RECT {
    RECT {
        left: a.left.min(b.left),
        top: a.top.min(b.top),
        right: a.right.max(b.right),
        bottom: a.bottom.max(b.bottom),
    }
}

/// `r` grown by `by` on every side, kept inside `within`.
fn grow(r: RECT, by: i32, within: RECT) -> RECT {
    RECT {
        left: (r.left - by).max(within.left + 2),
        top: (r.top - by).max(within.top + 2),
        right: (r.right + by).min(within.right - 2),
        bottom: (r.bottom + by).min(within.bottom - 2),
    }
}

fn overlaps(a: RECT, b: RECT) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

/// Put the explaining window beside the hole (below, above, right of or left
/// of it, the first that fits on the screen without covering it), or at the
/// right of the toured window when there is no hole.
fn place(callout: HWND, hole: Option<RECT>, over: RECT) {
    let me = window_rect(callout);
    let (w, h) = (me.right - me.left, me.bottom - me.top);
    // The monitor of the toured window, not of the callout (which starts on
    // the cursor's): the hole is on the former.
    let wa = unsafe { ui::work_area(MonitorFromRect(&over, MONITOR_DEFAULTTONEAREST)) };
    let gap = ui::px(14);
    let fits = |x: i32, y: i32| {
        let r = RECT {
            left: x,
            top: y,
            right: x + w,
            bottom: y + h,
        };
        r.left >= wa.left && r.top >= wa.top && r.right <= wa.right && r.bottom <= wa.bottom
    };
    let (mut x, mut y) = (
        wa.left + ((wa.right - wa.left) - w) / 2,
        wa.top + ((wa.bottom - wa.top) - h) / 2,
    );
    if let Some(hole) = hole {
        let cx = (hole.left + (hole.right - hole.left) / 2 - w / 2)
            .clamp(wa.left, (wa.right - w).max(wa.left));
        let candidates = [
            (cx, hole.bottom + gap),
            (cx, hole.top - gap - h),
            (hole.right + gap, hole.top),
            (hole.left - gap - w, hole.top),
        ];
        let chosen = candidates.iter().find(|&&(px, py)| {
            fits(px, py)
                && !overlaps(
                    RECT {
                        left: px,
                        top: py,
                        right: px + w,
                        bottom: py + h,
                    },
                    hole,
                )
        });
        if let Some(&(px, py)) = chosen {
            (x, y) = (px, py);
        } else {
            // Nothing beside it fits: the corner of the toured window farthest from it.
            x = if hole.left > over.left + (over.right - over.left) / 2 {
                over.left + gap
            } else {
                over.right - w - gap
            };
            y = (over.bottom - h - gap).max(wa.top);
        }
    }
    unsafe {
        let _ = SetWindowPos(
            callout,
            HWND_TOPMOST,
            x.clamp(wa.left, (wa.right - w).max(wa.left)),
            y.clamp(wa.top, (wa.bottom - h).max(wa.top)),
            0,
            0,
            windows::Win32::UI::WindowsAndMessaging::SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_has_its_words_and_something_to_point_at() {
        for chapter in Chapter::ALL {
            let list = steps(chapter);
            assert!(!list.is_empty(), "{chapter:?}");
            for step in list {
                assert!(!tr(step.title).is_empty() && !tr(step.body).is_empty());
                if let Spot::Controls(labels) = step.spot {
                    assert!(!labels.is_empty());
                    for l in labels {
                        assert!(!tr(*l).is_empty());
                    }
                }
            }
        }
    }

    #[test]
    fn a_hole_never_leaves_the_window_it_dims() {
        let over = RECT {
            left: 100,
            top: 100,
            right: 500,
            bottom: 400,
        };
        let hole = RECT {
            left: 90,
            top: 95,
            right: 510,
            bottom: 410,
        };
        let g = grow(hole, 6, over);
        assert!(
            g.left >= over.left
                && g.top >= over.top
                && g.right <= over.right
                && g.bottom <= over.bottom
        );
    }
}
