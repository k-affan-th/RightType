//! Per-app modes and "the app you were typing in" — Windows only.
//!
//! Holds the user's per-app modes (see [`righttype::per_app`]) and remembers
//! the executable name of the last app the hook saw a keystroke in, so the tray
//! menu's **In this app** can apply to it (clicking the tray icon makes the
//! taskbar the foreground window, so "the foreground app" would be wrong).
//! Only the executable name is kept — never a window title or anything typed.

use std::collections::BTreeMap;
use std::sync::Mutex;

use righttype::per_app::AppMode;

static MODES: Mutex<BTreeMap<String, AppMode>> = Mutex::new(BTreeMap::new());
static LAST_APP: Mutex<Option<String>> = Mutex::new(None);

/// Replace every per-app mode (from the config or Settings).
pub fn set_all(modes: BTreeMap<String, AppMode>) {
    *MODES.lock().unwrap() = modes;
}

pub fn all() -> BTreeMap<String, AppMode> {
    MODES.lock().unwrap().clone()
}

/// The mode set for `exe` (lower case), if any.
pub fn lookup(exe: &str) -> Option<AppMode> {
    MODES.lock().unwrap().get(exe).copied()
}

/// Set (or with `None`, remove) the mode of `exe`.
pub fn set(exe: &str, mode: Option<AppMode>) {
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
