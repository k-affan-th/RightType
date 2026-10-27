//! The overlay: a small floating pill for status and hints — Windows only.
//!
//! It rises a few pixels into place while fading in, holds, then fades out
//! (see [`righttype::motion`]; each frame comes from the time elapsed, so a
//! late timer tick skips a frame instead of stretching the animation). [`show`] puts it in the **bottom-right
//! corner of the monitor you are working on** (the one holding the foreground
//! window), for rare, deliberate state changes — mode, on/off, errors — and the
//! Suggest hint. [`show_at`] puts it next to a rectangle instead (the text
//! cursor, for the caret HUD). It does NOT announce auto layout switches (too
//! frequent — Windows' own language indicator already shows those) nor
//! individual corrections (you see the word change).
//!
//! Custom-painted (GDI), borderless, never takes focus, and sized for the DPI
//! of the monitor it appears on. What it shows is wiped from memory when it
//! hides: a Suggest hint is typed content.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::Mutex;

use native_windows_gui as nwg;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateEllipticRgn, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, DrawTextW,
    EndPaint, FillRect, FillRgn, FrameRect, GetDC, InvalidateRect, ReleaseDC, SelectObject,
    SetBkMode, SetTextColor, SetWindowRgn, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HDC, HGDIOBJ, HMONITOR, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::Graphics::Gdi::{
    MonitorFromPoint, MonitorFromWindow, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClientRect, GetForegroundWindow, KillTimer, PostMessageW, SetLayeredWindowAttributes,
    SetTimer, SetWindowPos, ShowWindow, HWND_TOPMOST, LWA_ALPHA, SWP_NOACTIVATE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE,
};
use zeroize::Zeroize;

use crate::icons;
use crate::ui::{px, px_at};
use righttype::motion;
use righttype::notice::{self, Kind};

const ANIM_TIMER_ID: usize = 9;
const SHOW_MS_BASE: u32 = 900;
/// One animation frame: Windows' timer resolution, about 60 frames a second.
const FRAME_MS: u32 = 15;
// Sizes in 96-DPI units; scaled for the target monitor when shown.
const W: i32 = 116;
const H: i32 = 34;
const MARGIN: i32 = 12;
/// Gap between an anchor rectangle (the caret) and the pill.
const GAP: i32 = 6;
const FONT_SIZE: i32 = 15;
const FONT_WEIGHT: i32 = 600;
/// The detail line under a title.
const DETAIL_SIZE: i32 = 13;
const WM_PAINT: u32 = 0x000F;
const WM_ERASEBKGND: u32 = 0x0014;
const WM_TIMER: u32 = 0x0113;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_SHOW_TOAST: u32 = 0x8000 + 0x525;
const WM_HIDE_TOAST: u32 = 0x8000 + 0x526;

static TOAST_HWND: AtomicIsize = AtomicIsize::new(0);
static UI_THREAD_ID: AtomicU32 = AtomicU32::new(0);
static PENDING: Mutex<Option<(String, Anchor, Style)>> = Mutex::new(None);
/// Whether the pill shows a tag (TH / EN / CAPS), for its colour.
static STYLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The DPI of the monitor the pill is on, for painting.
static DPI: AtomicU32 = AtomicU32::new(96);
/// Whether the text is wrapped onto several lines (it did not fit one).
static WRAPPED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How the pill looks and how long it stays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    /// A message: at least [`W`] wide, held longer the longer the text.
    Pill,
    /// A short tag (`TH` / `EN`) next to the caret: as narrow as its text and
    /// gone quickly, so it never sits over what is being typed.
    Badge,
    /// Something Tab takes (a ghost suggestion): looks like [`Style::Pill`]
    /// and stays exactly as long as Tab can take it ([`OFFER_MS`]).
    Offer,
}

/// Widest the pill gets before its text wraps onto more lines (96 dpi pixels).
const MAX_W: i32 = 360;
/// Side and top/bottom padding of a wrapped pill.
const PAD_X: i32 = 16;
const PAD_Y: i32 = 9;
/// Most lines a wrapped pill shows; the rest is cut with an ellipsis.
const MAX_LINES: i32 = 4;
/// Corner diameter of a wrapped pill: a full pill only suits one line.
const WRAP_CORNER: i32 = 24;
const BADGE_MIN_W: i32 = 44;
const BADGE_MS: u32 = 450;
/// How long an offer for Tab is shown, and can be taken.
pub const OFFER_MS: u32 = 2000;

/// Where the pill appears.
#[derive(Clone, Copy, Debug)]
pub enum Anchor {
    /// Bottom-right corner of the work area of the monitor holding the
    /// foreground window.
    Corner,
    /// Just below `rect` (screen pixels; e.g. the text cursor), or just above
    /// it when there is no room below; kept inside that monitor's work area.
    Near(RECT),
    /// Near the text cursor, looked up when the pill is shown (always after
    /// the caller returns — the lookup can be slow). No cursor: a message
    /// goes to the corner, a badge is not shown.
    Caret,
}

/// `WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE` — float above everything,
/// stay off the taskbar, and (crucially) never take focus from what's being typed.
const EX_FLAGS: u32 = 0x0000_0008 | 0x0000_0080 | 0x0800_0000 | 0x0008_0000;

struct Toast {
    _window: nwg::Window,
    _raw: Option<nwg::RawEventHandler>,
}

thread_local! {
    static TOAST: RefCell<Option<Toast>> = const { RefCell::new(None) };
    static TEXT: RefCell<String> = const { RefCell::new(String::new()) };
    static RICH: RefCell<Option<Rich>> = const { RefCell::new(None) };
    static ANIM: std::cell::Cell<Option<Anim>> = const { std::cell::Cell::new(None) };
}

/// The pill's animation, on the UI thread.
#[derive(Clone, Copy)]
struct Anim {
    phase: motion::Phase,
    since: std::time::Instant,
    hold_ms: u32,
    /// Resting top-left, screen pixels.
    x: i32,
    y: i32,
    dpi: u32,
    /// Slides in from the right (a corner pill) rather than rising.
    slide: bool,
    /// Windows has animation turned off: fade only.
    calm: bool,
}

/// Record the calling thread as the one that owns the overlay window. Call once
/// from the UI thread at startup: [`show`] creates the window lazily only on
/// this thread, and posts to it from any other.
pub fn register_ui_thread() {
    UI_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
}

/// Create the toast window lazily on the UI thread. Returns true when the
/// window exists. Deliberately NOT called at startup: creating a layered,
/// region-shaped window while an exclusive-fullscreen game owns the display
/// can raise a fatal DWM user callback — so under fullscreen we simply run
/// toast-less for the session instead of crashing.
fn ensure_created() -> bool {
    if TOAST_HWND.load(Ordering::Acquire) != 0 {
        return true;
    }
    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::POPUP)
        .ex_flags(EX_FLAGS)
        .size((px(W), px(H)))
        .position((-4000, -4000))
        .title("")
        .build(&mut window)
        .is_err()
    {
        return false;
    }

    if let Some(h) = window.handle.hwnd() {
        TOAST_HWND.store(h as isize, Ordering::Release);
        UI_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
        unsafe {
            // Layered window: enables per-pixel alpha for the fade-out.
            let _ = SetLayeredWindowAttributes(HWND(h as _), COLORREF(0), 255_u8, LWA_ALPHA);
            // Not in screen sharing, recordings or screenshots (Windows 10
            // 2004 and later): a Suggest hint or preview is typed content,
            // and the tags are noise on someone else's screen. The typist
            // still sees them.
            // (Debug builds: RIGHTTYPE_CAPTURE_OVERLAY lets documentation
            // screenshots include it.)
            let capture =
                cfg!(debug_assertions) && std::env::var_os("RIGHTTYPE_CAPTURE_OVERLAY").is_some();
            if !capture {
                let _ = windows::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity(
                    HWND(h as _),
                    windows::Win32::UI::WindowsAndMessaging::WDA_EXCLUDEFROMCAPTURE,
                );
            }
            // Rounded "pill" corners.
            let rgn = CreateRoundRectRgn(0, 0, px(W) + 1, px(H) + 1, px(H), px(H));
            SetWindowRgn(HWND(h as _), rgn, true);
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
            WM_TIMER if w == ANIM_TIMER_ID => {
                unsafe { tick(hwnd) };
                Some(0)
            }
            WM_LBUTTONUP => {
                // A click on a message opens the list of recent ones.
                if RICH.with(|r| r.borrow().is_some()) {
                    unsafe { dismiss_on_ui(hwnd) };
                    crate::settings::open_activity();
                }
                Some(0)
            }
            WM_HIDE_TOAST => {
                unsafe { dismiss_on_ui(hwnd) };
                Some(0)
            }
            WM_SHOW_TOAST => {
                if let Some((mut text, anchor, style)) = PENDING.lock().unwrap().take() {
                    unsafe { show_on_ui(hwnd, &text, anchor, style) };
                    text.zeroize();
                }
                Some(0)
            }
            _ => None,
        }
    })
    .ok();

    let hwnd_isz = window.handle.hwnd().map(|h| h as isize).unwrap_or(0);
    TOAST.with(|t| {
        *t.borrow_mut() = Some(Toast {
            _window: window,
            _raw: raw,
        });
    });
    TOAST_HWND.store(hwnd_isz, Ordering::Release);
    UI_THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Release);
    true
}

/// Flash `text` briefly in the bottom-right corner of the monitor in use.
/// Calls from workers are posted back to the UI thread that owns the window.
pub fn show(text: &str) {
    show_at(text, Anchor::Corner);
}

/// Flash `text` briefly at `anchor`.
pub fn show_at(text: &str, anchor: Anchor) {
    show_styled(text, anchor, Style::Pill, false);
}

/// Show an offer Tab takes at `anchor`, for [`OFFER_MS`].
pub fn offer_at(text: &str, anchor: Anchor) {
    show_styled(text, anchor, Style::Offer, false);
}

/// Flash a short tag (`TH` / `EN`) just below `caret`. Always shown after the
/// caller returns — the keyboard hook calls this, and must not wait for the
/// window to move and repaint.
#[cfg_attr(not(debug_assertions), allow(dead_code))] // debug harness
pub fn badge_at(text: &str, caret: RECT) {
    show_styled(text, Anchor::Near(caret), Style::Badge, true);
}

/// Flash a short tag at the text cursor, wherever it turns out to be.
pub fn badge_at_caret(text: &str) {
    show_styled(text, Anchor::Caret, Style::Badge, true);
}

fn show_styled(text: &str, anchor: Anchor, style: Style, defer: bool) {
    let defer = defer || matches!(anchor, Anchor::Caret);
    if TOAST_HWND.load(Ordering::Acquire) == 0 {
        if unsafe { GetCurrentThreadId() } != UI_THREAD_ID.load(Ordering::Acquire) {
            return; // worker thread + no window yet (e.g. fullscreen) — skip
        }
        if !ensure_created() {
            return;
        }
    }
    let raw = TOAST_HWND.load(Ordering::Acquire);
    // Read out what is shown, but not the preview of a word still being
    // typed (a tag next to it, on every key).
    if style != Style::Badge || !matches!(anchor, Anchor::Near(_)) {
        crate::announce::say(raw, text);
    }
    let hwnd = HWND(raw as *mut c_void);
    if !defer && unsafe { GetCurrentThreadId() } == UI_THREAD_ID.load(Ordering::Acquire) {
        unsafe { show_on_ui(hwnd, text, anchor, style) };
    } else {
        let pending = (text.to_string(), anchor, style);
        if let Some((mut old, _, _)) = PENDING.lock().unwrap().replace(pending) {
            old.zeroize();
        }
        unsafe {
            let _ = PostMessageW(hwnd, WM_SHOW_TOAST, WPARAM(0), LPARAM(0));
        }
    }
}

/// Have a screen reader say `text` without showing anything (a word fixed
/// with no message on screen). UI thread, or after the window exists.
pub fn announce(text: &str) {
    if TOAST_HWND.load(Ordering::Acquire) == 0
        && (unsafe { GetCurrentThreadId() } != UI_THREAD_ID.load(Ordering::Acquire)
            || !ensure_created())
    {
        return;
    }
    crate::announce::say(TOAST_HWND.load(Ordering::Acquire), text);
}

/// Hide the toast now and wipe its text — used when what it shows may be
/// sensitive (a Suggest hint made just before a seed phrase was recognised).
pub fn dismiss() {
    if let Some((mut text, _, _)) = PENDING.lock().unwrap().take() {
        text.zeroize();
    }
    let raw = TOAST_HWND.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    let hwnd = HWND(raw as *mut c_void);
    unsafe {
        if GetCurrentThreadId() == UI_THREAD_ID.load(Ordering::Acquire) {
            dismiss_on_ui(hwnd);
        } else {
            let _ = PostMessageW(hwnd, WM_HIDE_TOAST, WPARAM(0), LPARAM(0));
        }
    }
}

unsafe fn dismiss_on_ui(hwnd: HWND) {
    hide(hwnd);
}

/// Whether Windows plays client-area animation (off under "Show animations"
/// switched off, or in remote sessions).
fn client_animation_on() -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{
        SystemParametersInfoW, SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    };
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

/// Whether the mouse pointer is over the pill.
unsafe fn pointer_on(hwnd: HWND) -> bool {
    use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetWindowRect};
    let (mut p, mut r) = (POINT::default(), RECT::default());
    GetCursorPos(&mut p).is_ok()
        && GetWindowRect(hwnd, &mut r).is_ok()
        && p.x >= r.left
        && p.x < r.right
        && p.y >= r.top
        && p.y < r.bottom
}

/// One animation frame.
unsafe fn tick(hwnd: HWND) {
    let Some(mut a) = ANIM.with(|c| c.get()) else {
        let _ = KillTimer(hwnd, ANIM_TIMER_ID);
        return;
    };
    let elapsed = a.since.elapsed().as_millis().min(u32::MAX as u128) as u32;
    // The pointer on the pill keeps it there (and brings it back if it was
    // already leaving): a message takes as long to read as it takes.
    if a.phase != motion::Phase::Enter && pointer_on(hwnd) {
        if a.phase == motion::Phase::Exit {
            let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255, LWA_ALPHA);
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                a.x,
                a.y,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE,
            );
        }
        a.phase = motion::Phase::Hold;
        a.since = std::time::Instant::now();
        ANIM.with(|c| c.set(Some(a)));
        return;
    }
    if a.phase == motion::Phase::Hold {
        if elapsed >= a.hold_ms {
            a.phase = motion::Phase::Exit;
            a.since = std::time::Instant::now();
            ANIM.with(|c| c.set(Some(a)));
        }
        return;
    }
    let f = motion::frame_for(a.phase, elapsed, a.calm);
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), f.alpha, LWA_ALPHA);
    let drop = (f.drop_px * a.dpi as f32 / 96.0).round() as i32;
    let slide = if a.slide {
        (f.slide_px * a.dpi as f32 / 96.0).round() as i32
    } else {
        0
    };
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        a.x + slide,
        a.y + drop,
        0,
        0,
        SWP_NOACTIVATE | SWP_NOSIZE,
    );
    if f.done {
        match a.phase {
            motion::Phase::Enter => {
                a.phase = motion::Phase::Hold;
                a.since = std::time::Instant::now();
                ANIM.with(|c| c.set(Some(a)));
            }
            _ => hide(hwnd),
        }
    }
}

unsafe fn show_on_ui(hwnd: HWND, text: &str, anchor: Anchor, style: Style) {
    let anchor = match anchor {
        // Tags (TH / EN / CAPS, the preview) come often and use the system
        // caret only: the UI Automation fallback waits on the app, on this
        // (the hook's) thread. A message may take that wait.
        Anchor::Caret if style == Style::Badge => match crate::caret::caret_rect() {
            Some(caret) => Anchor::Near(caret),
            None => return,
        },
        Anchor::Caret => match crate::caret::find_caret() {
            Some(caret) => Anchor::Near(caret),
            None => Anchor::Corner,
        },
        other => other,
    };
    TEXT.with(|t| {
        let mut t = t.borrow_mut();
        t.zeroize();
        *t = text.to_string();
    });
    // Width grows with the message (suggestion previews are longer than the
    // original mode labels) but stays a compact pill.
    // Measured, not estimated: Thai tone marks and vowels take no width.
    let monitor = match anchor {
        // (Caret was resolved above.)
        Anchor::Corner | Anchor::Caret => {
            MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTOPRIMARY)
        }
        Anchor::Near(rc) => MonitorFromPoint(
            POINT {
                x: rc.left,
                y: rc.bottom,
            },
            MONITOR_DEFAULTTONEAREST,
        ),
    };
    let dpi = crate::ui::monitor_dpi(monitor);
    DPI.store(dpi, Ordering::Relaxed);
    let min_w = px_at(
        if style == Style::Badge {
            BADGE_MIN_W
        } else {
            W
        },
        dpi,
    );
    let wa = crate::ui::work_area(monitor);
    let max_w = px_at(MAX_W, dpi).min((wa.right - wa.left - 2 * px_at(MARGIN, dpi)).max(min_w));
    let one_line =
        crate::ui::text_width_at(text, FONT_SIZE, FONT_WEIGHT, dpi) + px_at(2 * PAD_X, dpi);
    // One line when it fits: the full pill. Longer text wraps inside a
    // narrower, taller rounded rectangle instead of running out of a pill.
    let rich = (style == Style::Pill).then(|| {
        let n = notice::compose(text);
        notice::record(&n);
        n
    });
    let (w, h, corner, wrapped) = if let Some(n) = &rich {
        let (w, h, corner, layout) = rich_layout(n, min_w, max_w, dpi);
        RICH.with(|r| *r.borrow_mut() = Some(layout));
        (w, h, corner, false)
    } else {
        RICH.with(|r| *r.borrow_mut() = None);
        if one_line <= max_w {
            let h = px_at(H, dpi);
            (one_line.max(min_w), h, h, false)
        } else {
            let inner = max_w - px_at(2 * PAD_X, dpi);
            let (text_h, line_h) = wrapped_height(text, inner, FONT_SIZE, FONT_WEIGHT, dpi);
            let text_h = text_h.min(line_h * MAX_LINES);
            let h = (text_h + px_at(2 * PAD_Y, dpi)).max(px_at(H, dpi));
            (max_w, h, px_at(WRAP_CORNER, dpi).min(h), true)
        }
    };
    WRAPPED.store(wrapped, Ordering::Relaxed);
    let (x, y) = place(monitor, anchor, w, h, dpi);
    let hold = match style {
        Style::Pill => rich.as_ref().map_or(SHOW_MS_BASE, notice::hold_ms),
        Style::Badge => BADGE_MS,
        Style::Offer => OFFER_MS,
    };
    STYLE.store(style == Style::Badge, Ordering::Relaxed);
    // Already on screen (and not leaving): move and hold again, without
    // fading in a second time — a flicker on every keystroke otherwise.
    let showing = ANIM
        .with(|c| c.get())
        .is_some_and(|a| a.phase != motion::Phase::Exit);
    let anim = Anim {
        phase: if showing {
            motion::Phase::Hold
        } else {
            motion::Phase::Enter
        },
        since: std::time::Instant::now(),
        hold_ms: hold,
        x,
        y,
        dpi,
        slide: matches!(anchor, Anchor::Corner),
        calm: !client_animation_on(),
    };
    let first = motion::frame_for(anim.phase, 0, anim.calm);
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), first.alpha, LWA_ALPHA);
    let drop = (first.drop_px * dpi as f32 / 96.0).round() as i32;
    let slide = if anim.slide {
        (first.slide_px * dpi as f32 / 96.0).round() as i32
    } else {
        0
    };
    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x + slide,
        y + drop,
        w,
        h,
        SWP_NOACTIVATE | SWP_SHOWWINDOW,
    );
    let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, corner, corner);
    SetWindowRgn(hwnd, rgn, true);
    ANIM.with(|c| c.set(Some(anim)));
    SetTimer(hwnd, ANIM_TIMER_ID, FRAME_MS, None);
    let _ = InvalidateRect(hwnd, None, true);
}

/// Where everything sits in a message pill (device pixels, relative to the
/// pill's top-left), and what to draw there.
struct Rich {
    kind: Kind,
    title: String,
    detail: Option<String>,
    icon: RECT,
    title_rc: RECT,
    detail_rc: RECT,
}

impl Rich {
    fn wipe(&mut self) {
        self.title.zeroize();
        if let Some(d) = self.detail.as_mut() {
            d.zeroize();
        }
    }
}

/// Lay out a message: an icon disc, a title, and under it the detail. One
/// short line is a full pill; anything taller is a rounded rectangle. Returns
/// width, height, corner diameter and the layout.
unsafe fn rich_layout(
    n: &notice::Notice,
    min_w: i32,
    max_w: i32,
    dpi: u32,
) -> (i32, i32, i32, Rich) {
    let pad_l = px_at(8, dpi);
    let icon_d = px_at(24, dpi);
    let gap = px_at(10, dpi);
    let pad_r = px_at(16, dpi);
    let pad_y = px_at(7, dpi);
    let line_gap = px_at(2, dpi);
    let min_h = px_at(H, dpi);
    let text_max = (max_w - pad_l - icon_d - gap - pad_r).max(px_at(80, dpi));
    let title_w = crate::ui::text_width_at(&n.title, FONT_SIZE, FONT_WEIGHT, dpi);
    let detail_w = n
        .detail
        .as_deref()
        .map_or(0, |d| crate::ui::text_width_at(d, DETAIL_SIZE, 400, dpi));
    let text_w = title_w.max(detail_w).min(text_max);
    let w = (pad_l + icon_d + gap + text_w + pad_r).max(min_w);

    let (title_all, title_line) = wrapped_height(&n.title, text_w, FONT_SIZE, FONT_WEIGHT, dpi);
    let title_h = title_all.min(title_line * 2);
    let detail_h = n.detail.as_deref().map_or(0, |d| {
        let (all, line) = wrapped_height(d, text_w, DETAIL_SIZE, 400, dpi);
        all.min(line * 2)
    });
    let single = detail_h == 0 && title_h <= title_line;
    let block = title_h + if detail_h > 0 { line_gap + detail_h } else { 0 };
    let h = (block + 2 * pad_y).max(min_h);
    let corner = if single {
        h
    } else {
        px_at(WRAP_CORNER, dpi).min(h)
    };
    let text_left = pad_l + icon_d + gap;
    let (title_top, icon_top) = if single {
        ((h - title_h) / 2, (h - icon_d) / 2)
    } else {
        (
            pad_y,
            (pad_y + (title_line - icon_d) / 2).max(px_at(5, dpi)),
        )
    };
    let rect = |l: i32, t: i32, r: i32, b: i32| RECT {
        left: l,
        top: t,
        right: r,
        bottom: b,
    };
    let rich = Rich {
        kind: n.kind,
        title: n.title.clone(),
        detail: n.detail.clone(),
        icon: rect(pad_l, icon_top, pad_l + icon_d, icon_top + icon_d),
        title_rc: rect(
            text_left,
            title_top,
            text_left + text_w,
            title_top + title_h,
        ),
        detail_rc: rect(
            text_left,
            title_top + title_h + line_gap,
            text_left + text_w,
            title_top + title_h + line_gap + detail_h,
        ),
    };
    (w, h, corner, rich)
}

/// Height `text` takes wrapped to `inner` device pixels, and one line's height.
unsafe fn wrapped_height(text: &str, inner: i32, size: i32, weight: i32, dpi: u32) -> (i32, i32) {
    let font = crate::ui::make_font_at(size, weight, dpi);
    let hdc = GetDC(None);
    let all = crate::ui::measure(hdc, text, inner, font);
    let one = crate::ui::measure(hdc, "A", inner, font).max(1);
    ReleaseDC(None, hdc);
    let _ = DeleteObject(HGDIOBJ(font.0));
    (all, one)
}

/// Top-left of a `w`×`h` pill for `anchor`, inside `monitor`'s work area (so
/// it never sits under the taskbar, wherever that is).
fn place(monitor: HMONITOR, anchor: Anchor, w: i32, h: i32, dpi: u32) -> (i32, i32) {
    let wa = crate::ui::work_area(monitor);
    let margin = px_at(MARGIN, dpi);
    let (x, y) = match anchor {
        Anchor::Corner | Anchor::Caret => (wa.right - w - margin, wa.bottom - h - margin),
        Anchor::Near(rc) => {
            let gap = px_at(GAP, dpi);
            let below = rc.bottom + gap;
            let y = if below + h <= wa.bottom {
                below
            } else {
                rc.top - gap - h
            };
            (rc.left, y)
        }
    };
    (
        x.clamp(wa.left, (wa.right - w).max(wa.left)),
        y.clamp(wa.top, (wa.bottom - h).max(wa.top)),
    )
}

unsafe fn paint(hwnd: HWND) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);

    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);

    // Dark pill; the TH / EN / CAPS tags each in a colour of their own, so
    // the language reads at a glance (COLORREF is 0x00BBGGRR).
    let (fill, edge, ink) = if crate::ui::high_contrast() {
        contrast_colours(STYLE.load(Ordering::Relaxed))
    } else {
        let (fill, edge) = TEXT.with(|t| tag_colours(&t.borrow(), STYLE.load(Ordering::Relaxed)));
        (fill, edge, 0x00FF_FFFF)
    };
    let brush = CreateSolidBrush(COLORREF(fill));
    FillRect(hdc, &rc, brush);
    let _ = DeleteObject(HGDIOBJ(brush.0));
    let border = CreateSolidBrush(COLORREF(edge));
    FrameRect(hdc, &rc, border);
    let _ = DeleteObject(HGDIOBJ(border.0));

    // White, centred text in the interface typeface.
    SetBkMode(hdc, TRANSPARENT);
    SetTextColor(hdc, COLORREF(ink));
    let font = crate::ui::make_font_at(FONT_SIZE, FONT_WEIGHT, DPI.load(Ordering::Relaxed));
    let old = SelectObject(hdc, HGDIOBJ(font.0));
    let drew_rich = RICH.with(|r| match r.borrow().as_ref() {
        Some(rich) => {
            draw_rich(hdc, rich, ink, crate::ui::high_contrast());
            true
        }
        None => false,
    });
    if !drew_rich && WRAPPED.load(Ordering::Relaxed) {
        let dpi = DPI.load(Ordering::Relaxed);
        rc.left += px_at(PAD_X, dpi);
        rc.right -= px_at(PAD_X, dpi);
        rc.top += px_at(PAD_Y, dpi);
        rc.bottom -= px_at(PAD_Y, dpi);
    }
    if !drew_rich {
        TEXT.with(|t| draw_centered(hdc, &t.borrow(), &mut rc));
    }
    SelectObject(hdc, old);
    let _ = DeleteObject(HGDIOBJ(font.0));

    let _ = EndPaint(hwnd, &ps);
}

/// The icon disc and glyph colours of a kind, as COLORREFs: the disc fill and
/// the glyph on it.
fn kind_colours(kind: Kind) -> (u32, u32) {
    match kind {
        Kind::Done => (0x003E_6B2F, 0x00E0_F5D9),
        Kind::Undo => (0x0014_5A7A, 0x00B7_E9FB),
        Kind::Info => (0x0080_4D25, 0x00FB_E6D6),
        Kind::Attention => (0x0016_3A8A, 0x00CB_DCFD),
        Kind::Problem => (0x0024_248A, 0x00D3_D3FB),
    }
}

fn kind_glyph(kind: Kind) -> char {
    match kind {
        Kind::Done => icons::CHECK,
        Kind::Undo => icons::UNDO,
        Kind::Info => icons::INFO,
        Kind::Attention => icons::WARNING,
        Kind::Problem => icons::ERROR,
    }
}

/// Draw a message pill's icon, title and detail on the dark pill.
unsafe fn draw_rich(hdc: HDC, rich: &Rich, ink: u32, high_contrast: bool) {
    let dpi = DPI.load(Ordering::Relaxed);
    let (disc, glyph_ink) = if high_contrast {
        (None, ink)
    } else {
        let (d, g) = kind_colours(rich.kind);
        (Some(d), g)
    };
    let i = rich.icon;
    if let Some(disc) = disc {
        let rgn = CreateEllipticRgn(i.left, i.top, i.right + 1, i.bottom + 1);
        let brush = CreateSolidBrush(COLORREF(disc));
        let _ = FillRgn(hdc, rgn, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(rgn.0));
    }
    if let Some(font) = crate::ui::make_icon_font(13) {
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        SetTextColor(hdc, COLORREF(glyph_ink));
        let mut glyph: Vec<u16> = kind_glyph(rich.kind).to_string().encode_utf16().collect();
        let mut rc = i;
        DrawTextW(
            hdc,
            &mut glyph,
            &mut rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
    let flags = DT_LEFT | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX;
    let draw = |text: &str, size: i32, weight: i32, colour: u32, mut rc: RECT| {
        let font = crate::ui::make_font_at(size, weight, dpi);
        let old = SelectObject(hdc, HGDIOBJ(font.0));
        SetTextColor(hdc, COLORREF(colour));
        let mut units: Vec<u16> = text.encode_utf16().collect();
        DrawTextW(hdc, &mut units, &mut rc, flags);
        units.zeroize();
        SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    };
    if !rich.title.is_empty() {
        draw(&rich.title, FONT_SIZE, FONT_WEIGHT, ink, rich.title_rc);
    }
    if let Some(detail) = rich.detail.as_deref() {
        let dim = if high_contrast { ink } else { 0x00C0_B9B9 };
        draw(detail, DETAIL_SIZE, 400, dim, rich.detail_rc);
    }
}

/// Draw `text` centred in `rc`.
///
/// Nothing for empty text: DrawTextW reads the first character even when told
/// the text is 0 long, and an empty `Vec`'s pointer is a dangling 0x2. That
/// crashed RightType (0xC000041D) when a paint arrived after [`hide`] had
/// wiped the text, which Edge's timing made happen.
unsafe fn draw_centered(hdc: HDC, text: &str, rc: &mut RECT) {
    if text.is_empty() {
        return;
    }
    let mut units: Vec<u16> = text.encode_utf16().collect();
    let flags = if WRAPPED.load(Ordering::Relaxed) {
        DT_CENTER | DT_WORDBREAK | DT_END_ELLIPSIS | DT_NOPREFIX
    } else {
        DT_CENTER | DT_VCENTER | DT_SINGLELINE
    };
    DrawTextW(hdc, &mut units, rc, flags);
    units.zeroize();
}

/// Fill and border of the pill: a tag's own colour, or dark for messages.
fn tag_colours(text: &str, badge: bool) -> (u32, u32) {
    match (badge, text) {
        (true, "TH") | (true, "ไทย") => (0x005C_6F1F, 0x007A_8F33), // teal
        (true, "EN") | (true, "ENG") => (0x0097_572B, 0x00B3_7040), // blue
        (true, "CAPS") => (0x0000_5A8A, 0x0010_74AA),               // amber
        // A spelling fix: its own colour, so it is not mistaken for a
        // keyboard fix.
        (false, t) if t.starts_with("Spelling:") || t.starts_with("แก้คำสะกด:") => {
            (0x0078_3C6A, 0x0092_5487)
        } // purple
        _ => (0x002A_2A2A, 0x0045_4545),
    }
}

/// Fill, border and text of the pill under Windows' High Contrast: the
/// theme's own colours (selection colours for a tag), as COLORREFs.
fn contrast_colours(badge: bool) -> (u32, u32, u32) {
    use windows::Win32::Graphics::Gdi::{
        GetSysColor, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    };
    unsafe {
        let edge = GetSysColor(COLOR_WINDOWTEXT);
        if badge {
            (
                GetSysColor(COLOR_HIGHLIGHT),
                edge,
                GetSysColor(COLOR_HIGHLIGHTTEXT),
            )
        } else {
            (GetSysColor(COLOR_WINDOW), edge, edge)
        }
    }
}

unsafe fn hide(hwnd: HWND) {
    let _ = KillTimer(hwnd, ANIM_TIMER_ID);
    ANIM.with(|c| c.set(None));
    let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 255_u8, LWA_ALPHA);
    let _ = ShowWindow(hwnd, SW_HIDE);
    // A suggestion preview is typed content: do not keep it past its display.
    TEXT.with(|t| t.borrow_mut().zeroize());
    RICH.with(|r| {
        if let Some(rich) = r.borrow_mut().as_mut() {
            rich.wipe();
        }
        *r.borrow_mut() = None;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Graphics::Gdi::{CreateCompatibleDC, DeleteDC};

    /// The pill can be painted after its text was wiped (`hide` clears it
    /// for privacy while a paint is still queued). Painting it then must not
    /// crash: in Edge it did, with an access violation in DrawTextW reading
    /// address 0x2 (0xC000041D).
    #[test]
    fn painting_after_the_text_was_wiped_does_not_crash() {
        unsafe {
            let hdc = CreateCompatibleDC(None);
            let mut rc = RECT {
                left: 0,
                top: 0,
                right: 100,
                bottom: 30,
            };
            draw_centered(hdc, "", &mut rc);
            draw_centered(hdc, "TH", &mut rc);
            let _ = DeleteDC(hdc);
        }
    }
}
