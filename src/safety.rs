//! Sensitive-context guards — Windows only.
//!
//! RightType must never touch keystrokes where secrets are typed. This disables
//! the whole pipeline when the focused control is a **password field**, or when
//! the foreground app is a known **wallet, password manager, or terminal**. The
//! secret-shaped bail-out in `secret` is the per-token backstop; this is the
//! per-context one (see `docs/PLAN.md`, the security section).

use std::sync::Mutex;

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetGUIThreadInfo, GetWindowLongPtrW, GetWindowThreadProcessId, GUITHREADINFO, GWL_STYLE,
};

/// `ES_PASSWORD` edit-control style — set on concealed text fields.
const ES_PASSWORD: isize = 0x0020;

/// Foreground apps we stay completely out of (lowercased executable names).
/// Fixed and **not user-editable** — [`CUSTOM_BLACKLIST`] is the extension
/// point, so this baseline can only ever be added to, never weakened.
pub const BLACKLIST: &[&str] = &[
    // terminals
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "windowsterminal.exe",
    "conhost.exe",
    "mintty.exe",
    "alacritty.exe",
    "wezterm-gui.exe",
    // password managers
    "keepass.exe",
    "keepassxc.exe",
    "1password.exe",
    "bitwarden.exe",
    "lastpass.exe",
    // crypto wallets
    "electrum.exe",
    "sparrow.exe",
    "exodus.exe",
    "bitcoin-qt.exe",
    "ledger live.exe",
    "trezor suite.exe",
    // remote desktops and virtual machines: the keys belong to another
    // computer, which may have a different layout (and RightType of its own)
    "mstsc.exe",
    "msrdc.exe",
    "windows365.exe",
    "vmconnect.exe",
    "virtualboxvm.exe",
    "vmware.exe",
    "vmplayer.exe",
    "vmware-remotemks.exe",
    "anydesk.exe",
    "teamviewer.exe",
    "rustdesk.exe",
    "parsecd.exe",
    "vncviewer.exe",
    "tvnviewer.exe",
];

/// A full-screen game (or a slide show) is in front: RightType stays out, as
/// in a blocked app. Movement keys spell Thai (`wasd` is ไฟหก), and a
/// correction would send Backspaces into the game. Kept up to date by
/// [`watch_full_screen`] on a thread of its own: the shell call may wait on
/// the window in front, and neither the keyboard hook nor the UI thread may.
static FULL_SCREEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn is_full_screen() -> bool {
    FULL_SCREEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// Start the thread that keeps [`is_full_screen`] current (every 0.5 s).
pub fn watch_full_screen() {
    let _ = std::thread::Builder::new()
        .name("full-screen".into())
        .spawn(|| loop {
            unsafe { refresh_full_screen() };
            std::thread::sleep(std::time::Duration::from_millis(500));
        });
}

/// Ask Windows whether a full-screen program without a text cursor is in
/// front: an exclusive-mode game or a presentation, or any other full-screen
/// window whose thread shows no caret (a borderless game) — unless the focus
/// is in a text field (a full-screen browser or editor).
unsafe fn refresh_full_screen() -> bool {
    use windows::Win32::UI::Shell::{
        SHQueryUserNotificationState, QUNS_BUSY, QUNS_PRESENTATION_MODE,
        QUNS_RUNNING_D3D_FULL_SCREEN,
    };
    let state = SHQueryUserNotificationState().ok();
    // Typing into a text field: a full-screen browser or editor (on a real
    // PC, Chrome after F11 shows Windows no text cursor at all, and was
    // taken for a game). The focus worker's answer, read from memory.
    let typing_text = crate::focus::is_text_field();
    let now = match state {
        Some(s) if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE => !typing_text,
        Some(s) if s == QUNS_BUSY => {
            let mut gui = GUITHREADINFO {
                cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            // The foreground thread's caret (idThread 0), read from the
            // system's own record: it does not wait on the app.
            let no_caret = GetGUIThreadInfo(0, &mut gui).is_ok() && gui.hwndCaret.0.is_null();
            no_caret && !typing_text
        }
        _ => false,
    };
    let was = FULL_SCREEN.swap(now, std::sync::atomic::Ordering::Relaxed);
    if was != now {
        righttype::diag::note(
            "full-screen program without a text cursor",
            &[("now", now.into())],
        );
    }
    now
}

/// User-added blacklist entries (lowercased exe names), layered on top of the
/// fixed [`BLACKLIST`]. Edited via the settings window, persisted in config.
static CUSTOM_BLACKLIST: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The user's custom blacklist entries, for the settings UI and config save.
pub fn custom_list() -> Vec<String> {
    CUSTOM_BLACKLIST.lock().unwrap().clone()
}

/// Replace the custom blacklist (e.g. from the settings window or loaded config).
pub fn set_custom_list(entries: Vec<String>) {
    *CUSTOM_BLACKLIST.lock().unwrap() = entries.into_iter().map(|s| s.to_lowercase()).collect();
}

/// Is `exe` (an executable name) on the built-in or the user's blocked list?
pub fn is_blacklisted_name(exe: &str) -> bool {
    let exe = exe.to_lowercase();
    BLACKLIST.contains(&exe.as_str()) || CUSTOM_BLACKLIST.lock().unwrap().contains(&exe)
}

/// Is the currently focused control a password / concealed-text field? Cheap
/// enough to check every keystroke (focus can move between fields inside one
/// window without the foreground window changing).
pub unsafe fn is_password_field() -> bool {
    let mut gui = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // idThread = 0 → the foreground thread.
    if GetGUIThreadInfo(0, &mut gui).is_err() || gui.hwndFocus.0.is_null() {
        // Fail closed when Windows cannot identify the focused control.
        return true;
    }
    (GetWindowLongPtrW(gui.hwndFocus, GWL_STYLE) & ES_PASSWORD) != 0
}

/// The executable name (lower case) of the process owning `hwnd`.
///
/// # Safety
/// `hwnd` must be a window handle (it may be stale; that yields `None`).
pub unsafe fn foreground_exe(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return None;
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

    let mut buf = [0u16; 260];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(
        handle,
        PROCESS_NAME_WIN32,
        PWSTR(buf.as_mut_ptr()),
        &mut len,
    )
    .is_ok();
    let _ = CloseHandle(handle);
    if !ok {
        return None;
    }

    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_lowercase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_blacklist_covers_each_sensitive_app_class() {
        for exe in ["powershell.exe", "KeePassXC.EXE", "electrum.exe"] {
            assert!(is_blacklisted_name(exe));
        }
        assert!(!is_blacklisted_name("notepad.exe"));
    }

    #[test]
    fn custom_blacklist_adds_but_cannot_remove_fixed_entries() {
        set_custom_list(vec!["ExampleSensitive.exe".to_string()]);
        assert!(is_blacklisted_name("examplesensitive.exe"));
        assert!(is_blacklisted_name("cmd.exe"));
        set_custom_list(Vec::new());
        assert!(is_blacklisted_name("cmd.exe"));
    }
}
