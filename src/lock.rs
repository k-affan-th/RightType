//! The keyboard lock behind cleaning and the key tester — Windows only.
//!
//! While it is on, the keyboard hook swallows every key before anything
//! else sees it (Windows keys, Alt+Tab, media and app keys included), and
//! the mouse hook every mouse event when the mouse is locked too. What is
//! kept: which keys were pressed (one bit per key of
//! [`righttype::keyboard::KEYS`]) and which are down now, to draw them;
//! never the order.
//!
//! It always ends: at its deadline (checked on every key and by the
//! window's timer), by the window's button, or with RightType itself, since
//! the hooks go with it. Ctrl+Alt+Del is never swallowed: Windows keeps it
//! from every program.

use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use righttype::keyboard::{self, Pressed};

/// The longest a lock may last.
pub const MAX: Duration = Duration::from_secs(5 * 60);
/// Holding Esc this long ends the key tester (it has no deadline).
pub const ESC_HOLD: Duration = Duration::from_secs(2);

static ON: AtomicBool = AtomicBool::new(false);
static MOUSE: AtomicBool = AtomicBool::new(false);
/// Esc held long ends it (the key tester).
static ESC_ENDS: AtomicBool = AtomicBool::new(false);
/// Pressed since the lock began, and down now: two halves of 128 bits each.
static PRESSED: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
static DOWN: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
/// The last key pressed (index into KEYS + 1; 0 = none).
static LAST: AtomicU64 = AtomicU64::new(0);
/// The window to repaint when a key comes.
static WINDOW: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    /// When the lock ends (`None`: when the window says so).
    static UNTIL: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static ESC_SINCE: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
}

/// Lock the keyboard (and the mouse when `mouse`) until `until`, or until
/// [`stop`] when `None`. `window` is repainted on every key.
pub fn start(until: Option<Instant>, mouse: bool, esc_ends: bool, window: isize) {
    let until = until.map(|u| u.min(Instant::now() + MAX));
    UNTIL.with(|u| u.set(until));
    ESC_SINCE.with(|e| e.set(None));
    for half in PRESSED.iter().chain(DOWN.iter()) {
        half.store(0, Ordering::Relaxed);
    }
    LAST.store(0, Ordering::Relaxed);
    WINDOW.store(window, Ordering::Release);
    MOUSE.store(mouse, Ordering::Relaxed);
    ESC_ENDS.store(esc_ends, Ordering::Relaxed);
    ON.store(true, Ordering::Release);
    crate::hook::trace_note("keyboard lock: on");
}

pub fn stop() {
    if ON.swap(false, Ordering::AcqRel) {
        MOUSE.store(false, Ordering::Relaxed);
        crate::hook::trace_note("keyboard lock: off");
        repaint();
    }
}

pub fn is_on() -> bool {
    ON.load(Ordering::Acquire)
}

/// Time left, if the lock has a deadline (UI thread).
pub fn left() -> Option<Duration> {
    UNTIL
        .with(|u| u.get())
        .map(|u| u.saturating_duration_since(Instant::now()))
}

/// Ends the lock when its deadline has passed (UI thread); whether it is
/// still on.
pub fn check_deadline() -> bool {
    if is_on() && left().is_some_and(|l| l.is_zero()) {
        stop();
    }
    is_on()
}

pub fn pressed() -> Pressed {
    Pressed::from_bits(bits(&PRESSED))
}

pub fn down() -> Pressed {
    Pressed::from_bits(bits(&DOWN))
}

/// The last key pressed, as an index into [`keyboard::KEYS`].
pub fn last() -> Option<usize> {
    (LAST.load(Ordering::Relaxed) as usize).checked_sub(1)
}

fn bits(halves: &[AtomicU64; 2]) -> u128 {
    (halves[0].load(Ordering::Relaxed) as u128)
        | ((halves[1].load(Ordering::Relaxed) as u128) << 64)
}

fn set_bit(halves: &[AtomicU64; 2], i: usize, on: bool) {
    if i >= 128 {
        return;
    }
    let bit = 1u64 << (i % 64);
    if on {
        halves[i / 64].fetch_or(bit, Ordering::Relaxed);
    } else {
        halves[i / 64].fetch_and(!bit, Ordering::Relaxed);
    }
}

fn repaint() {
    let hwnd = WINDOW.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(
                windows::Win32::Foundation::HWND(hwnd as *mut _),
                None,
                true,
            );
        }
    }
}

/// From the keyboard hook, before anything else: whether the key is
/// swallowed (the lock is on).
pub fn key_event(scan: u16, extended: bool, down: bool) -> bool {
    if !is_on() {
        return false;
    }
    if !check_deadline() {
        // It ended just now: this key is the typist's again.
        return false;
    }
    if let Some(i) = keyboard::index_of(scan, extended) {
        set_bit(&DOWN, i, down);
        if down {
            set_bit(&PRESSED, i, true);
            LAST.store(i as u64 + 1, Ordering::Relaxed);
        }
    }
    if ESC_ENDS.load(Ordering::Relaxed) && scan == 0x01 {
        let since = ESC_SINCE.with(|e| {
            if !down {
                e.set(None);
                return None;
            }
            let since = e.get().unwrap_or_else(Instant::now);
            e.set(Some(since));
            Some(since)
        });
        if since.is_some_and(|s| s.elapsed() >= ESC_HOLD) {
            stop();
            crate::clean::request_close();
            return true;
        }
    }
    repaint();
    true
}

/// From the mouse hook: whether the mouse event is swallowed.
pub fn mouse_event() -> bool {
    is_on() && MOUSE.load(Ordering::Relaxed) && check_deadline()
}
