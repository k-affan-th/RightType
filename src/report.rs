//! Tray → "Save a problem report…": what RightType did lately, with no typed
//! text, written to a file the typist chooses (see [`righttype::diag`]).

use native_windows_gui as nwg;
use righttype::diag;
use righttype::i18n::{tr, T};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};

/// Ask where to save, then write the report there.
pub fn save() {
    let mut dialog = nwg::FileDialog::default();
    let filters = format!("{}|All files (*.*)", tr(T::FileFilter));
    if nwg::FileDialog::builder()
        .action(nwg::FileDialogAction::Save)
        .filters(filters.as_str())
        .build(&mut dialog)
        .is_err()
        || !dialog.run(None::<&nwg::Window>)
    {
        return;
    }
    let Ok(path) = dialog.get_selected_item() else {
        return;
    };
    let mut path = std::path::PathBuf::from(path);
    if path.extension().is_none() {
        path.set_extension("txt");
    }
    let ok = std::fs::write(&path, diag::report(&about())).is_ok();
    crate::overlay::show(tr(if ok { T::ToastSaved } else { T::ErrFile }));
}

/// Debug e2e builds: keep the report in the file `RIGHTTYPE_E2E_REPORT`
/// names, so the sweep can check what it holds.
#[cfg(debug_assertions)]
pub fn e2e_write() {
    if let Some(path) = std::env::var_os("RIGHTTYPE_E2E_REPORT") {
        let _ = std::fs::write(path, diag::report(&about()));
    }
}

/// Settings and system facts at the top of the report. No words, no paths.
fn about() -> Vec<(&'static str, String)> {
    let yes = |b: bool| if b { "yes" } else { "no" }.to_string();
    vec![
        ("version", env!("CARGO_PKG_VERSION").to_string()),
        ("windows", windows_version()),
        ("keyboards", keyboards()),
        ("on", yes(crate::hook::is_enabled())),
        ("mode", format!("{:?}", crate::hook::mode())),
        ("keyboard hook working", yes(crate::session::is_healthy())),
    ]
}

/// Installed keyboards by layout id (0409 US, 041E Thai, 0809 UK, …).
fn keyboards() -> String {
    unsafe {
        let n = GetKeyboardLayoutList(None).max(0) as usize;
        let mut list = vec![HKL::default(); n];
        let got = GetKeyboardLayoutList(Some(&mut list)).max(0) as usize;
        list.iter()
            .take(got)
            .map(|h| format!("{:08X}", h.0 as usize as u32))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// "10.0.26100" from the registry's CurrentVersion key.
fn windows_version() -> String {
    use windows::core::w;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    let mut buf = [0u16; 32];
    let mut len = (buf.len() * 2) as u32;
    let build = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if build.is_err() {
        return "?".into();
    }
    let n = (len as usize / 2).saturating_sub(1).min(buf.len());
    let build = String::from_utf16_lossy(&buf[..n]);
    diag::app_name(&build).map_or("?".into(), |b| format!("build {b}"))
}
