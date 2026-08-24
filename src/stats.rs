//! Usage counters + themed statistics window — Windows only.
//! **In-memory only, never written to disk.**
//!
//! RightType's no-persistence guarantee covers *content* (nothing typed is ever
//! saved), not counts — but to keep that guarantee unambiguous, these counters
//! reset every restart rather than accumulating in a file.
//!
//! The window follows the app-wide **refresh-on-reopen** rule: opening it while
//! one is already visible closes the old one and opens a fresh copy, announced
//! with a toast (mirroring mode changes).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, AtomicU64, Ordering};

use native_windows_gui as nwg;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::{learn, theme, toast};

static AUTO: AtomicU64 = AtomicU64::new(0);
static MANUAL: AtomicU64 = AtomicU64::new(0);
static OPEN_STATS: AtomicIsize = AtomicIsize::new(0);

const WM_CLOSE: u32 = 0x0010;

/// Record one correction made automatically (Auto mode: boundary or live).
pub fn record_auto() {
    AUTO.fetch_add(1, Ordering::Relaxed);
}

/// Record one correction made via a manual hotkey (fix-word or fix-selection).
pub fn record_manual() {
    MANUAL.fetch_add(1, Ordering::Relaxed);
}

/// `(auto corrections, manual corrections)` since this run started.
pub fn snapshot() -> (u64, u64) {
    (AUTO.load(Ordering::Relaxed), MANUAL.load(Ordering::Relaxed))
}

struct StatsWindow {
    window: nwg::Window,
    _font: nwg::Font,
    _big: nwg::Font,
    _labels: Vec<nwg::Label>,
    close: nwg::Button,
    _theme: Option<nwg::RawEventHandler>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open (or refresh) the statistics window.
pub fn open() {
    let prev = OPEN_STATS.load(Ordering::Acquire);
    if prev != 0 {
        unsafe {
            let _ = PostMessageW(
                HWND(prev as *mut core::ffi::c_void),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        toast::show("Statistics refreshed");
    } else {
        toast::show("Statistics");
    }

    let mut font = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(15)
        .build(&mut font);
    let mut big = nwg::Font::default();
    let _ = nwg::Font::builder()
        .family("Segoe UI")
        .size(22)
        .build(&mut big);

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((420, 340))
        .position((240, 160))
        .title("RightType — Statistics")
        .topmost(true)
        .build(&mut window);
    let hwnd = window
        .handle
        .hwnd()
        .map(|h| HWND(h as _))
        .unwrap_or(HWND(std::ptr::null_mut()));
    theme::apply_frame(hwnd);
    let themed = theme::subclass_colors(hwnd, 0x5254_0012);

    let mut labels: Vec<nwg::Label> = Vec::new();
    macro_rules! label {
        ($text:expr, $x:expr, $y:expr, $w:expr, $h:expr, $f:expr) => {{
            let mut l = nwg::Label::default();
            let _ = nwg::Label::builder()
                .text($text)
                .font(Some(&$f))
                .position(($x, $y))
                .size(($w, $h))
                .parent(&window)
                .build(&mut l);
            labels.push(l);
        }};
    }

    label!("Statistics — this session", 24, 18, 360, 34, big);
    label!("Corrected automatically", 28, 74, 220, 24, font);
    label!("Corrected via hotkey", 28, 106, 220, 24, font);
    label!("Total corrections", 28, 138, 220, 24, font);
    label!("Learned words (saved locally)", 28, 170, 260, 24, font);

    let (a, m) = snapshot();
    let sa = a.to_string();
    let sm = m.to_string();
    let st = (a + m).to_string();
    let sl = learn::count().to_string();
    label!(sa.as_str(), 330, 68, 64, 30, big);
    label!(sm.as_str(), 330, 100, 64, 30, big);
    label!(st.as_str(), 330, 132, 64, 30, big);
    label!(sl.as_str(), 330, 164, 64, 30, big);

    label!(
        "Counts reset on restart. Nothing you type is ever saved.",
        24,
        216,
        380,
        40,
        font
    );

    let mut close = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Close")
        .font(Some(&font))
        .position((300, 280))
        .size((96, 30))
        .parent(&window)
        .build(&mut close);

    window.set_visible(true);
    OPEN_STATS.store(
        window.handle.hwnd().map(|h| h as isize).unwrap_or(0),
        Ordering::Release,
    );

    let ui = Rc::new(StatsWindow {
        window,
        _font: font,
        _big: big,
        _labels: labels,
        close,
        _theme: themed,
        handler: RefCell::new(None),
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            E::OnButtonClick if handle == ui_h.close.handle => finish(&ui_h),
            E::OnWindowClose if handle == ui_h.window.handle => finish(&ui_h),
            _ => {}
        }
    });
    *ui.handler.borrow_mut() = Some(handler);

    fn finish(ui: &Rc<StatsWindow>) {
        let my = ui.window.handle.hwnd().map(|h| h as isize).unwrap_or(0);
        let _ = OPEN_STATS.compare_exchange(my, 0, Ordering::AcqRel, Ordering::Acquire);
        if let Some(h) = ui.handler.borrow_mut().take() {
            nwg::unbind_event_handler(&h);
        }
        ui.window.close();
    }
}
