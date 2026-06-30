//! Run-at-startup via the per-user `Run` registry key — Windows only.
//!
//! Writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RightType` = the
//! quoted path to this executable, so Windows launches it at login. Per-user (no
//! admin needed), matching the non-elevated, least-privilege design.

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SAM_FLAGS, REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE: PCWSTR = w!("RightType");

unsafe fn open(sam: REG_SAM_FLAGS) -> Option<HKEY> {
    let mut hkey = HKEY::default();
    let rc = RegCreateKeyExW(
        HKEY_CURRENT_USER,
        RUN_KEY,
        0,
        PCWSTR::null(),
        REG_OPTION_NON_VOLATILE,
        sam,
        None,
        &mut hkey,
        None,
    );
    (rc == ERROR_SUCCESS).then_some(hkey)
}

/// Is RightType registered to launch at login?
pub fn is_enabled() -> bool {
    unsafe {
        let Some(hkey) = open(KEY_READ) else {
            return false;
        };
        let present = RegQueryValueExW(hkey, VALUE, None, None, None, None) == ERROR_SUCCESS;
        let _ = RegCloseKey(hkey);
        present
    }
}

/// Register or unregister run-at-login (best-effort).
pub fn set_enabled(on: bool) {
    unsafe {
        let Some(hkey) = open(KEY_WRITE) else {
            return;
        };
        if on {
            if let Ok(exe) = std::env::current_exe() {
                // Quote the path so spaces in it survive.
                let mut data: Vec<u16> = format!("\"{}\"", exe.display()).encode_utf16().collect();
                data.push(0);
                let bytes = std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 2);
                let _ = RegSetValueExW(hkey, VALUE, 0, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(hkey, VALUE);
        }
        let _ = RegCloseKey(hkey);
    }
}
