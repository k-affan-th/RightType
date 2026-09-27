//! Usage counters + themed statistics window — Windows only.
//!
//! The session counters live in memory only and reset every restart. For the
//! 7-day view the user can opt in to **daily counts** (`righttype::usage`):
//! two numbers per calendar day in `stats.toml` — never what was typed, never
//! when or where. Turning it off deletes the file.
//!
//! Opening the window while one is already visible replaces it with a fresh
//! copy, so the numbers are always current.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use righttype::usage::{self, Daily, Day};
use serde::{Deserialize, Serialize};

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::Gdi::{DeleteObject, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, HGDIOBJ};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::learn;
use crate::ui::{self, card, pal, rect, Gfx, Surface, TextStyle};

static AUTO: AtomicU64 = AtomicU64::new(0);
/// Daily counts are kept (opt-in).
static KEEP: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicBool = AtomicBool::new(false);
static TICKS: AtomicU32 = AtomicU32::new(0);
static DAILY: Mutex<Option<Daily>> = Mutex::new(None);
static MANUAL: AtomicU64 = AtomicU64::new(0);
static OPEN_STATS: AtomicIsize = AtomicIsize::new(0);

const WM_CLOSE: u32 = 0x0010;

/// Record one correction made automatically (Auto mode: boundary or live).
pub fn record_auto() {
    AUTO.fetch_add(1, Ordering::Relaxed);
    with_daily(|d, today| d.record_auto(today));
}

/// Record one correction made via a manual hotkey (fix-word or fix-selection).
pub fn record_manual() {
    record_manual_n(1);
}

/// Record `n` words fixed by hand at once (the Fix text window).
pub fn record_manual_n(n: u64) {
    MANUAL.fetch_add(n, Ordering::Relaxed);
    with_daily(|d, today| d.record_manual(today, n));
}

/// Today as a day number, in local time.
fn today() -> u32 {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    usage::day_number(t.wYear as i32, t.wMonth as u32, t.wDay as u32)
}

fn with_daily(f: impl FnOnce(&mut Daily, u32)) {
    if !KEEP.load(Ordering::Relaxed) {
        return;
    }
    if let Some(daily) = DAILY.lock().unwrap().as_mut() {
        f(daily, today());
        DIRTY.store(true, Ordering::Relaxed);
    }
}

#[derive(Serialize, Deserialize, Default)]
struct StatsFile {
    /// Day number (days since 1970-01-01) → [automatic, hotkey].
    #[serde(default)]
    days: std::collections::BTreeMap<String, [u64; 2]>,
}

fn stats_path() -> Option<PathBuf> {
    let mut p = crate::data_dir::righttype_dir()?;
    p.push("stats.toml");
    Some(p)
}

pub fn keeps_daily() -> bool {
    KEEP.load(Ordering::Relaxed)
}

/// Turn daily counts on (loading any saved ones) or off (deleting them).
pub fn set_keep_daily(on: bool) {
    KEEP.store(on, Ordering::Relaxed);
    if on {
        let file: StatsFile = stats_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| toml::from_str(&s).ok())
            .unwrap_or_default();
        let entries = file
            .days
            .into_iter()
            .filter_map(|(k, [auto, manual])| Some((k.parse::<u32>().ok()?, Day { auto, manual })));
        let entries: Vec<(u32, Day)> = entries.collect();
        let loaded = entries.len();
        let daily = Daily::from_entries(entries, today());
        // Expired days were dropped on load: write that back, so the file
        // never holds more than the promised 56 days.
        if daily.entries().count() != loaded {
            DIRTY.store(true, Ordering::Relaxed);
        }
        *DAILY.lock().unwrap() = Some(daily);
    } else {
        *DAILY.lock().unwrap() = None;
        DIRTY.store(false, Ordering::Relaxed);
        if let Some(p) = stats_path() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Write the daily counts if they changed.
pub fn save() {
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let file = match DAILY.lock().unwrap().as_ref() {
        Some(daily) => StatsFile {
            days: daily
                .entries()
                .map(|(d, c)| (d.to_string(), [c.auto, c.manual]))
                .collect(),
        },
        None => return,
    };
    let Some(p) = stats_path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(text) = toml::to_string(&file) {
        let _ = std::fs::write(p, text);
    }
}

/// Session timer tick (1.5 s): save about once a minute while counts change.
pub fn tick() {
    if TICKS.fetch_add(1, Ordering::Relaxed) + 1 >= 40 {
        TICKS.store(0, Ordering::Relaxed);
        // A PC left running for weeks still ages out old days.
        if let Some(daily) = DAILY.lock().unwrap().as_mut() {
            if daily.prune(today()) {
                DIRTY.store(true, Ordering::Relaxed);
            }
        }
        save();
    }
}

/// The last 7 days, oldest first, when daily counts are kept.
fn last_week() -> Option<[(u32, Day); 7]> {
    DAILY.lock().unwrap().as_ref().map(|d| d.last_week(today()))
}

/// Rough time a correction saves, in seconds, using the typing benchmark's
/// human timings (`examples/typing_benchmark.rs`: 0.30 s a keystroke, 0.60 s
/// to switch layout, 0.80 s to notice a wrong word, 0.40 s for
/// Shift+Backspace) and a typical 6-key word. An automatic fix saves noticing,
/// deleting and retyping the word and switching layout; a hotkey fix saves the
/// same minus the noticing, plus the hotkey itself.
const SECONDS_PER_AUTO: f64 = 0.80 + 6.0 * 0.30 + 0.60 + 6.0 * 0.30;
const SECONDS_PER_MANUAL: f64 = 6.0 * 0.30 + 0.60 + 6.0 * 0.30 - 0.40;

/// Estimated seconds saved by `auto` and `manual` corrections.
pub fn seconds_saved(auto: u64, manual: u64) -> u64 {
    (auto as f64 * SECONDS_PER_AUTO + manual as f64 * SECONDS_PER_MANUAL).round() as u64
}

/// "42 s" under a minute, whole minutes after that.
fn format_saved(seconds: u64) -> String {
    use righttype::i18n::trf;
    if seconds < 60 {
        trf(T::StatsSavedSeconds, &[("n", &seconds.to_string())])
    } else {
        trf(T::StatsSavedValue, &[("n", &(seconds / 60).to_string())])
    }
}

/// `(auto corrections, manual corrections)` since this run started.
pub fn snapshot() -> (u64, u64) {
    (AUTO.load(Ordering::Relaxed), MANUAL.load(Ordering::Relaxed))
}

const W: i32 = 680;
const H: i32 = 628;
const WEEK_Y: i32 = 262;
const WEEK_H: i32 = 196;
const KEEP_Y: i32 = WEEK_Y + WEEK_H + 14;
const X: i32 = 28;
const TILE_W: i32 = 144;
const TILE_GAP: i32 = 16;
const TILE_Y: i32 = 76;
const TILE_H: i32 = 118;

struct StatsWindow {
    window: nwg::Window,
    surface: Rc<Surface>,
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
    }
    ui::refresh();

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, H))
        .title(tr(T::StatsTitle))
        .topmost(true)
        .build(&mut window);
    let week = last_week();
    let surface = Surface::attach(
        &window,
        0x5254_0012,
        Box::new(move |g, hdc, rc, page| paint(g, hdc, rc, page, week.as_ref())),
    );
    let s = &surface;
    let p = pal();

    s.label(
        tr(T::StatsHead),
        TextStyle::Title,
        (X, 22, W - 2 * X, 36),
        p.bg,
        0,
    );
    let (auto, manual) = snapshot();
    let tiles = [
        (auto.to_string(), T::StatsAuto),
        (manual.to_string(), T::StatsManual),
        (learn::count().to_string(), T::StatsLearned),
        (format_saved(seconds_saved(auto, manual)), T::StatsSaved),
    ];
    for (i, (value, caption)) in tiles.iter().enumerate() {
        let x = X + i as i32 * (TILE_W + TILE_GAP);
        s.label(
            value,
            TextStyle::Display,
            (x + 18, TILE_Y + 14, TILE_W - 36, 48),
            p.surface,
            0,
        );
        s.label(
            tr(*caption),
            TextStyle::Small,
            (x + 18, TILE_Y + 66, TILE_W - 36, 40),
            p.surface,
            0,
        );
    }
    s.label(
        tr(T::StatsNote),
        TextStyle::Small,
        (X, TILE_Y + TILE_H + 18, W - 2 * X, 36),
        p.bg,
        0,
    );
    s.label(
        tr(T::StatsWeekHead),
        TextStyle::BodyStrong,
        (X + 18, WEEK_Y + 14, W - 2 * X - 36, 22),
        p.surface,
        0,
    );
    let week_line = match week {
        Some(days) => {
            let auto: u64 = days.iter().map(|(_, d)| d.auto).sum();
            let manual: u64 = days.iter().map(|(_, d)| d.manual).sum();
            trf(
                T::StatsWeekTotal,
                &[
                    ("n", &(auto + manual).to_string()),
                    ("m", &(seconds_saved(auto, manual) / 60).to_string()),
                ],
            )
        }
        None => tr(T::StatsWeekOff).to_string(),
    };
    s.label(
        &week_line,
        TextStyle::Dim,
        (X + 18, WEEK_Y + 38, W - 2 * X - 36, 22),
        p.surface,
        0,
    );
    let keep = s.toggle(
        tr(T::RowKeepStats),
        tr(T::SubKeepStats),
        (X + 4, KEEP_Y + 4, W - 2 * X - 8, 64),
        p.surface,
        0,
    );
    s.set_checked(keep, keeps_daily());
    let close = s.button(
        tr(T::BtnClose),
        false,
        (W - X - 110, H - 28 - 34, 110, 34),
        p.bg,
        0,
    );

    ui::size_and_center(surface.hwnd, W, H);
    let win = Rc::new(StatsWindow {
        window,
        surface,
        handler: RefCell::new(None),
    });
    win.window.set_visible(true);
    OPEN_STATS.store(win.surface.hwnd.0 as isize, Ordering::Release);

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if id == keep {
            if let Some(win) = weak.upgrade() {
                set_keep_daily(win.surface.checked(keep));
                crate::config::persist();
                // Rebuild so the 7-day view reflects the choice.
                finish(&win);
                open();
            }
        } else if id == close {
            if let Some(win) = weak.upgrade() {
                finish(&win);
            }
        }
    });
    let win_h = win.clone();
    let handler = nwg::full_bind_event_handler(&win.window.handle, move |evt, _data, handle| {
        if matches!(evt, nwg::Event::OnWindowClose) && handle == win_h.window.handle {
            finish(&win_h);
        }
    });
    *win.handler.borrow_mut() = Some(handler);
}

fn finish(win: &Rc<StatsWindow>) {
    let my = win.surface.hwnd.0 as isize;
    let _ = OPEN_STATS.compare_exchange(my, 0, Ordering::AcqRel, Ordering::Acquire);
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    win.window.close();
}

fn paint(g: &Gfx, hdc: HDC, _rc: RECT, _page: u8, week: Option<&[(u32, Day); 7]>) {
    for i in 0..4 {
        let x = X + i * (TILE_W + TILE_GAP);
        card(g, rect(x, TILE_Y, TILE_W, TILE_H));
    }
    card(g, rect(X, WEEK_Y, W - 2 * X, WEEK_H));
    card(g, rect(X, KEEP_Y, W - 2 * X, 72));
    let Some(days) = week else {
        return;
    };
    // Seven bars, automatic fixes in the accent colour stacked under hotkey
    // fixes in the dim text colour; the day of the month under each.
    let p = pal();
    let max = days
        .iter()
        .map(|(_, d)| d.auto + d.manual)
        .max()
        .unwrap_or(0)
        .max(1);
    let chart_x = X + 18;
    let chart_w = W - 2 * X - 36;
    let slot = chart_w / 7;
    let bar_w = 28;
    let base = WEEK_Y + WEEK_H - 34;
    let full = 96;
    let font = ui::make_font(12, 400);
    for (i, (day, counts)) in days.iter().enumerate() {
        let x = chart_x + i as i32 * slot + (slot - bar_w) / 2;
        let total = counts.auto + counts.manual;
        let h = (total * full as u64 / max) as i32;
        let h_auto = (counts.auto * full as u64 / max) as i32;
        g.fill_round(rect(x, base - full, bar_w, full), ui::px(4) as f32, p.inset);
        if h > 0 {
            g.fill_round(rect(x, base - h, bar_w, h), ui::px(4) as f32, p.text_dim);
        }
        if h_auto > 0 {
            g.fill_round(
                rect(x, base - h_auto, bar_w, h_auto),
                ui::px(4) as f32,
                p.accent,
            );
        }
        let (month, dom) = usage::month_day(*day);
        ui::text(
            hdc,
            &format!("{dom}/{month}"),
            rect(x - 14, base + 6, bar_w + 28, 18),
            font,
            p.text_dim,
            DT_CENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    unsafe {
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_saved_is_a_plain_estimate() {
        assert_eq!(seconds_saved(0, 0), 0);
        assert_eq!(seconds_saved(1, 0), 5);
        assert_eq!(seconds_saved(0, 10), 38);
    }
}
