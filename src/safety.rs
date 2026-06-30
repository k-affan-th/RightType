//! Sensitive-context guards — Windows only.
//!
//! RightType must never touch keystrokes where secrets are typed. This disables
//! the whole pipeline when the focused control is a **password field**, or when
//! the foreground app is a known **wallet, password manager, or terminal**. The
//! secret-shaped bail-out in `secret` is the per-token backstop; this is the
//! per-context one (see `docs/PLAN.md`, the security section).

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
const BLACKLIST: &[&str] = &[
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
];

/// Is the foreground app one we must not run in? Heavier (opens the process), so
/// the caller caches this per foreground window rather than per keystroke.
///
/// # Safety
/// `hwnd` must be a valid window handle (the current foreground window).
pub unsafe fn is_blacklisted_app(hwnd: HWND) -> bool {
    foreground_exe(hwnd).is_some_and(|exe| BLACKLIST.contains(&exe.as_str()))
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
        return false;
    }
    (GetWindowLongPtrW(gui.hwndFocus, GWL_STYLE) & ES_PASSWORD) != 0
}

unsafe fn foreground_exe(hwnd: HWND) -> Option<String> {
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid == 0 {
        return None;
    }
    let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;

    let mut buf = [0u16; 260];
    let mut len = buf.len() as u32;
    let ok =
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
            .is_ok();
    let _ = CloseHandle(handle);
    if !ok {
        return None;
    }

    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_lowercase)
}
