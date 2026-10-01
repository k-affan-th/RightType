//! Per-app modes and "the app you were typing in" — Windows only.
//!
//! Holds the per-app modes (see [`righttype::per_app`]) from three places,
//! first match wins:
//!
//! 1. **For now** — chosen "only this time" from an offer; gone when
//!    RightType exits, never saved.
//! 2. **Chosen** by the typist (Settings, tray, palette); saved.
//! 3. **Default** — Code mode in code editors ([`righttype::code`]).
//!
//! Remembers the executable name of the last app the hook saw a keystroke
//! in, so the tray menu's **In this app** can apply to it (clicking the tray
//! icon makes the taskbar the foreground window, so "the foreground app"
//! would be wrong). Only executable names are kept — never a window title or
//! anything typed.
//!
//! **Offering a calmer mode**: when fixes keep being taken back in one app
//! (three in ten minutes), RightType offers Suggest (or Manual) there, for now
//! or for good, through the palette. Only the times of the take-backs are
//! counted, in memory.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use righttype::per_app::{self, AppMode};

static MODES: Mutex<BTreeMap<String, AppMode>> = Mutex::new(BTreeMap::new());
static TEMP: Mutex<BTreeMap<String, AppMode>> = Mutex::new(BTreeMap::new());
static LAST_APP: Mutex<Option<String>> = Mutex::new(None);
static REJECTIONS: Mutex<Option<HashMap<String, Vec<Instant>>>> = Mutex::new(None);
/// The calmer mode on offer: app, mode, when offered.
static OFFER: Mutex<Option<(String, AppMode, Instant)>> = Mutex::new(None);
/// Apps an offer was turned down or taken in, this run: not asked again.
static ASKED: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// How long an offer stays open for the palette.
const OFFER_OPEN: Duration = Duration::from_secs(90);

/// Where an app's mode comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    ForNow,
    Chosen,
    Default,
}

/// Replace every chosen per-app mode (from the config or Settings).
pub fn set_all(modes: BTreeMap<String, AppMode>) {
    *MODES.lock().unwrap() = modes;
}

/// The chosen (saved) modes.
pub fn all() -> BTreeMap<String, AppMode> {
    MODES.lock().unwrap().clone()
}

/// Every app with a mode of any kind, with where it comes from.
pub fn rows() -> Vec<(String, AppMode, Source)> {
    let mut rows: BTreeMap<String, (AppMode, Source)> = righttype::code::CODE_EDITORS
        .iter()
        .map(|e| (e.to_string(), (AppMode::Code, Source::Default)))
        .collect();
    for (exe, mode) in MODES.lock().unwrap().iter() {
        rows.insert(exe.clone(), (*mode, Source::Chosen));
    }
    for (exe, mode) in TEMP.lock().unwrap().iter() {
        rows.insert(exe.clone(), (*mode, Source::ForNow));
    }
    rows.into_iter().map(|(e, (m, s))| (e, m, s)).collect()
}

/// The mode for `exe` (lower case), if it has one of its own.
pub fn lookup(exe: &str) -> Option<AppMode> {
    if let Some(mode) = TEMP.lock().unwrap().get(exe) {
        return Some(*mode);
    }
    if let Some(mode) = MODES.lock().unwrap().get(exe) {
        return Some(*mode);
    }
    righttype::code::is_code_editor(exe).then_some(AppMode::Code)
}

/// Set (or with `None`, remove) the chosen mode of `exe`. A mode for now in
/// the same app gives way to it.
pub fn set(exe: &str, mode: Option<AppMode>) {
    TEMP.lock().unwrap().remove(exe);
    let mut modes = MODES.lock().unwrap();
    match mode {
        Some(mode) => {
            modes.insert(exe.to_string(), mode);
        }
        None => {
            modes.remove(exe);
        }
    }
}

/// Use `mode` in `exe` until RightType exits (not saved).
pub fn set_for_now(exe: &str, mode: AppMode) {
    TEMP.lock().unwrap().insert(exe.to_string(), mode);
}

/// Keep the mode for now in `exe` for good.
pub fn keep(exe: &str) {
    let mode = TEMP.lock().unwrap().remove(exe);
    if let Some(mode) = mode {
        MODES.lock().unwrap().insert(exe.to_string(), mode);
    }
}

/// The hook saw typing in `exe`.
pub fn note_typing_in(exe: &str) {
    // RightType's own windows (Settings) are not "an app you type in".
    if exe == "righttype.exe" {
        return;
    }
    let mut last = LAST_APP.lock().unwrap();
    if last.as_deref() != Some(exe) {
        *last = Some(exe.to_string());
    }
}

/// The last app typed in, for the tray menu.
pub fn last_app() -> Option<String> {
    LAST_APP.lock().unwrap().clone()
}

/// A fix RightType made in `exe` was taken back, while `mode` applied there.
/// Returns the calmer mode to offer, once enough have been.
pub fn note_rejection(exe: &str, mode: AppMode) -> Option<AppMode> {
    let calmer = per_app::calmer(mode)?;
    if ASKED.lock().unwrap().iter().any(|a| a == exe) {
        return None;
    }
    let mut all = REJECTIONS.lock().unwrap();
    let times = all.get_or_insert_with(HashMap::new).entry(exe.to_string()).or_default();
    if !per_app::rejection_offers(times, Instant::now()) {
        return None;
    }
    ASKED.lock().unwrap().push(exe.to_string());
    *OFFER.lock().unwrap() = Some((exe.to_string(), calmer, Instant::now()));
    Some(calmer)
}

/// The calmer mode on offer, if it is still open.
pub fn offer() -> Option<(String, AppMode)> {
    let offer = OFFER.lock().unwrap();
    offer
        .as_ref()
        .filter(|(_, _, at)| at.elapsed() < OFFER_OPEN)
        .map(|(exe, mode, _)| (exe.clone(), *mode))
}

/// The offer was answered.
pub fn close_offer() {
    OFFER.lock().unwrap().take();
}

/// Forget the mode for now in `exe` (the chosen or default one applies).
pub fn clear_for_now(exe: &str) {
    TEMP.lock().unwrap().remove(exe);
}

/// Programs with a window open now: `(name.exe, full path)`, by name. For
/// Settings → Apps (adding one, and where each program lives); read when
/// asked, not kept.
pub fn running() -> Vec<(String, String)> {
    use windows::Win32::Foundation::{BOOL, CloseHandle, HWND, LPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
    };
    unsafe extern "system" fn each(hwnd: HWND, l: LPARAM) -> BOOL {
        let pids = &mut *(l.0 as *mut Vec<u32>);
        if IsWindowVisible(hwnd).as_bool() && GetWindowTextLengthW(hwnd) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid != 0 && !pids.contains(&pid) {
                pids.push(pid);
            }
        }
        BOOL(1)
    }
    let mut pids: Vec<u32> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(each), LPARAM(&mut pids as *mut Vec<u32> as isize));
    }
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for pid in pids {
        unsafe {
            let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                continue;
            };
            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(
                h,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            )
            .is_ok();
            let _ = CloseHandle(h);
            if !ok {
                continue;
            }
            let path = String::from_utf16_lossy(&buf[..len as usize]);
            let Some(name) = std::path::Path::new(&path)
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
            else {
                continue;
            };
            if name != "righttype.exe" {
                out.entry(name).or_insert(path);
            }
        }
    }
    out.into_iter().collect()
}
