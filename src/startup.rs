//! Run-at-startup via the per-user `Run` registry key — Windows only.
//!
//! Writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\RightType` = the
//! quoted path to this executable, so Windows launches it at login. Per-user (no
//! admin needed), matching the non-elevated, least-privilege design.
//!
//! Inside an MSIX package (Microsoft Store build) that key is virtualised and
//! Windows would never read it, so the package declares a `windows.startupTask`
//! (id [`TASK_ID`]) and this module drives that instead.

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

/// Must match `TaskId` of the `windows.startupTask` extension in packaging/msix/AppxManifest.xml.
const TASK_ID: &str = "RightTypeStartup";

/// True when running with package identity (installed from the MSIX / Store).
fn is_packaged() -> bool {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::APPMODEL_ERROR_NO_PACKAGE;
    use windows::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut len = 0u32;
    // SAFETY: a null buffer with length 0 only asks for the required length.
    let rc = unsafe { GetCurrentPackageFullName(&mut len, PWSTR::null()) };
    rc != APPMODEL_ERROR_NO_PACKAGE
}

/// Run `f` on a short-lived MTA thread: the WinRT calls block, and the callers
/// here are on the UI (STA) thread.
fn on_mta<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};
    std::thread::spawn(move || unsafe {
        let init = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let out = f();
        if init {
            CoUninitialize();
        }
        out
    })
    .join()
    .ok()
}

fn packaged_is_enabled() -> bool {
    use windows::core::HSTRING;
    use windows::ApplicationModel::{StartupTask, StartupTaskState};
    on_mta(|| {
        let task = StartupTask::GetAsync(&HSTRING::from(TASK_ID))?.get()?;
        let state = task.State()?;
        windows::core::Result::Ok(
            state == StartupTaskState::Enabled || state == StartupTaskState::EnabledByPolicy,
        )
    })
    .and_then(|r| r.ok())
    .unwrap_or(false)
}

fn packaged_set_enabled(on: bool) {
    use windows::core::HSTRING;
    use windows::ApplicationModel::StartupTask;
    // If the user turned it off in Settings > Apps > Startup, Windows refuses
    // RequestEnable; that is their choice, so ignore the result.
    let _ = on_mta(move || {
        let task = StartupTask::GetAsync(&HSTRING::from(TASK_ID))?.get()?;
        if on {
            task.RequestEnableAsync()?.get()?;
        } else {
            task.Disable()?;
        }
        windows::core::Result::Ok(())
    });
}

/// Is RightType registered to launch at login?
pub fn is_enabled() -> bool {
    if is_packaged() {
        return packaged_is_enabled();
    }
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
    if is_packaged() {
        packaged_set_enabled(on);
        return;
    }
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
