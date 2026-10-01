//! Switching to a field's usual language as it gets focus — Windows only.
//!
//! Off unless the user turns it on (Settings → Apps). When on, each finished
//! word adds one to a Thai or English count for the field it was typed in —
//! the program's file name plus the focused control's class name — and when
//! focus moves into a field with a clear habit ([`righttype::predict`]), the
//! keyboard is switched before the first key.
//!
//! Stored in `contexts.toml` next to the config: field keys and two numbers
//! each, never a word or a window title. Nothing is recorded or switched in a
//! blocked app, an app switched off in its per-app mode, or a password field.

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use righttype::policy::InputLayout;
use righttype::predict::{field_key, Counts, Habits};
use serde::{Deserialize, Serialize};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, GUITHREADINFO,
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicBool = AtomicBool::new(false);
/// Timer ticks since the last save (the session timer runs every 1.5 s).
static TICKS: AtomicU32 = AtomicU32::new(0);
/// Save at most about once a minute while typing.
const SAVE_EVERY_TICKS: u32 = 40;

thread_local! {
    static HABITS: RefCell<Habits> = RefCell::new(Habits::new());
}

#[derive(Serialize, Deserialize, Default)]
struct File {
    /// Field key → [Thai words, English words].
    #[serde(default)]
    fields: std::collections::BTreeMap<String, [u32; 2]>,
}

fn path() -> Option<PathBuf> {
    let mut p = crate::data_dir::righttype_dir()?;
    p.push("contexts.toml");
    Some(p)
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Turn the feature on or off. Turning it on loads the saved counts.
pub fn set_enabled(on: bool) {
    let was = ENABLED.swap(on, Ordering::Relaxed);
    if on && !was {
        load();
    }
}

fn load() {
    let file: File = path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default();
    let habits = Habits::from_entries(
        file.fields
            .into_iter()
            .map(|(k, [thai, english])| (k, Counts { thai, english })),
    );
    HABITS.with(|h| *h.borrow_mut() = habits);
}

/// Write the counts if they changed.
pub fn save() {
    if !DIRTY.swap(false, Ordering::Relaxed) {
        return;
    }
    let file = HABITS.with(|h| File {
        fields: h
            .borrow()
            .entries()
            .map(|(k, c)| (k.to_string(), [c.thai, c.english]))
            .collect(),
    });
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = toml::to_string(&file) {
        let _ = std::fs::write(p, s);
    }
}

/// Session timer tick: save now and then while counts change.
pub fn tick() {
    if TICKS.fetch_add(1, Ordering::Relaxed) + 1 >= SAVE_EVERY_TICKS {
        TICKS.store(0, Ordering::Relaxed);
        save();
    }
}

/// Forget every count, in memory and on disk.
pub fn clear() {
    HABITS.with(|h| h.borrow_mut().clear());
    DIRTY.store(false, Ordering::Relaxed);
    if let Some(p) = path() {
        let _ = std::fs::remove_file(p);
    }
}

/// The focused control's class name in the foreground window.
pub(crate) fn focused_class() -> Option<String> {
    unsafe {
        let thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
        let mut gui = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut gui).is_err() || gui.hwndFocus.0.is_null() {
            return None;
        }
        let mut buf = [0u16; 128];
        let n = GetClassNameW(gui.hwndFocus, &mut buf);
        (n > 0).then(|| String::from_utf16_lossy(&buf[..n as usize]))
    }
}

/// A word just finished in `exe` (the hook's cached foreground app). Called
/// only on the path where the hook may act, so blocked apps and password
/// fields never get here.
pub fn record_word(exe: &str, word: &str) {
    if !is_enabled() {
        return;
    }
    let layout = if word.chars().any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)) {
        InputLayout::ThaiKedmanee
    } else if word.chars().any(|c| c.is_ascii_alphabetic()) {
        InputLayout::UsQwerty
    } else {
        return;
    };
    let Some(class) = focused_class() else {
        return;
    };
    HABITS.with(|h| h.borrow_mut().record(&field_key(exe, &class), layout));
    DIRTY.store(true, Ordering::Relaxed);
}

/// A finished word in `exe` changed language after it was counted (a flip,
/// an accepted suggestion, an Undo): move it to the language kept, so the
/// habit follows what the user meant, not what they first typed.
pub fn correct_word(exe: &str, was_thai: bool, now_thai: bool) {
    if !is_enabled() || was_thai == now_thai {
        return;
    }
    let Some(class) = focused_class() else {
        return;
    };
    let layout = |thai| {
        if thai {
            InputLayout::ThaiKedmanee
        } else {
            InputLayout::UsQwerty
        }
    };
    HABITS.with(|h| {
        h.borrow_mut()
            .correct(&field_key(exe, &class), layout(was_thai), layout(now_thai))
    });
    DIRTY.store(true, Ordering::Relaxed);
}

/// Focus moved: if the new field has a clear habit, switch to its language.
pub fn on_focus() {
    if !is_enabled() || !crate::hook::is_enabled() || crate::focus::is_password_field() {
        return;
    }
    let Some(exe) = (unsafe { crate::safety::foreground_exe(GetForegroundWindow()) }) else {
        return;
    };
    if crate::safety::is_blacklisted_name(&exe)
        || crate::apps::lookup(&exe) == Some(righttype::per_app::AppMode::Off)
    {
        return;
    }
    let Some(class) = focused_class() else {
        return;
    };
    if let Some(layout) = HABITS.with(|h| h.borrow().preferred(&field_key(&exe, &class))) {
        unsafe { crate::hook::switch_layout(layout) };
    }
}
