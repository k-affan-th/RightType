//! Keyboard health check — Windows only.
//!
//! Settings that make a keyboard act up without a sign of why, each with a
//! fix where one is safe to make for the typist: keys Windows still counts
//! as held down (often after Remote Desktop), Sticky Keys and Filter Keys
//! and the shortcuts that switch them on by accident, the keyboards
//! installed, the language switch keys, Windows' own autocorrect, CapsLock.
//! Only settings are read; nothing typed.

use righttype::i18n::{tr, trf, T};
use windows::Win32::UI::Accessibility::{
    FILTERKEYS, SKF_HOTKEYACTIVE, SKF_STICKYKEYSON, STICKYKEYS,
};

/// FILTERKEYS flags (winuser.h).
const FKF_FILTERKEYSON: u32 = 0x0000_0001;
const FKF_HOTKEYACTIVE: u32 = 0x0000_0004;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SPI_GETFILTERKEYS,
    SPI_GETSTICKYKEYS, SPI_SETFILTERKEYS, SPI_SETLANGTOGGLE, SPI_SETSTICKYKEYS,
};

/// How many checks there are (one row each).
pub const COUNT: usize = 8;

/// What a fix does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fix {
    /// Send the key-up of these keys.
    Release(Vec<u16>),
    StickyOff,
    FilterOff,
    LanguageSettings,
    AltShift,
    TypingSettings,
    CapsOff,
    /// Drop these keys' bounces (and keep the ones already filtered).
    Debounce(Vec<(u16, bool)>),
    StopDebounce,
}

pub struct Check {
    pub title: T,
    pub ok: bool,
    pub status: String,
    /// The button's label and what it does.
    pub fix: Option<(T, Fix)>,
}

/// Run every check.
pub fn run() -> Vec<Check> {
    vec![
        stuck_keys(),
        sticky_keys(),
        filter_keys(),
        keyboards(),
        switch_keys(),
        autocorrect(),
        caps_lock(),
        chatter(),
    ]
}

fn ok(title: T, status: String) -> Check {
    Check {
        title,
        ok: true,
        status,
        fix: None,
    }
}

fn stuck_keys() -> Check {
    const KEYS: [(u16, &str); 8] = [
        (0xA0, "Left Shift"),
        (0xA1, "Right Shift"),
        (0xA2, "Left Ctrl"),
        (0xA3, "Right Ctrl"),
        (0xA4, "Left Alt"),
        (0xA5, "Right Alt"),
        (0x5B, "Windows"),
        (0x5C, "Windows"),
    ];
    let held: Vec<(u16, &str)> = KEYS
        .iter()
        .copied()
        .filter(|(vk, _)| unsafe { GetAsyncKeyState(*vk as i32) } as u16 & 0x8000 != 0)
        .collect();
    if held.is_empty() {
        return ok(T::HealthStuck, tr(T::HealthStuckOk).to_string());
    }
    let mut names: Vec<&str> = held.iter().map(|(_, n)| *n).collect();
    names.dedup();
    Check {
        title: T::HealthStuck,
        ok: false,
        status: trf(T::HealthStuckBad, &[("keys", &names.join(", "))]),
        fix: Some((
            T::HealthRelease,
            Fix::Release(held.iter().map(|(vk, _)| *vk).collect()),
        )),
    }
}

fn sticky() -> STICKYKEYS {
    let mut sk = STICKYKEYS {
        cbSize: std::mem::size_of::<STICKYKEYS>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETSTICKYKEYS,
            sk.cbSize,
            Some(&mut sk as *mut _ as *mut _),
            Default::default(),
        );
    }
    sk
}

fn filter() -> FILTERKEYS {
    let mut fk = FILTERKEYS {
        cbSize: std::mem::size_of::<FILTERKEYS>() as u32,
        ..Default::default()
    };
    unsafe {
        let _ = SystemParametersInfoW(
            SPI_GETFILTERKEYS,
            fk.cbSize,
            Some(&mut fk as *mut _ as *mut _),
            Default::default(),
        );
    }
    fk
}

fn sticky_keys() -> Check {
    let flags = sticky().dwFlags;
    let (on, hotkey) = (
        flags.0 & SKF_STICKYKEYSON.0 != 0,
        flags.0 & SKF_HOTKEYACTIVE.0 != 0,
    );
    if !on && !hotkey {
        return ok(T::HealthSticky, tr(T::HealthStickyOk).to_string());
    }
    Check {
        title: T::HealthSticky,
        ok: false,
        status: tr(if on {
            T::HealthStickyOn
        } else {
            T::HealthStickyHotkey
        })
        .to_string(),
        fix: Some((T::HealthTurnOff, Fix::StickyOff)),
    }
}

fn filter_keys() -> Check {
    let flags = filter().dwFlags;
    let (on, hotkey) = (flags & FKF_FILTERKEYSON != 0, flags & FKF_HOTKEYACTIVE != 0);
    if !on && !hotkey {
        return ok(T::HealthFilter, tr(T::HealthFilterOk).to_string());
    }
    Check {
        title: T::HealthFilter,
        ok: false,
        status: tr(if on {
            T::HealthFilterOn
        } else {
            T::HealthFilterHotkey
        })
        .to_string(),
        fix: Some((T::HealthTurnOff, Fix::FilterOff)),
    }
}

fn keyboards() -> Check {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyboardLayoutList, HKL};
    let list: Vec<u32> = unsafe {
        let n = GetKeyboardLayoutList(None).max(0) as usize;
        let mut v = vec![HKL::default(); n];
        let got = GetKeyboardLayoutList(Some(&mut v)).max(0) as usize;
        v.iter().take(got).map(|h| h.0 as usize as u32).collect()
    };
    let thai = list.iter().filter(|h| *h & 0xFFFF == 0x041E).count();
    let english = list.iter().filter(|h| *h & 0x3FF == 0x09).count();
    let names = format!(
        "{} {thai} · {} {english}",
        tr(T::HealthThai),
        tr(T::HealthEnglish)
    );
    let bad = match (thai, english) {
        (0, _) => Some(T::HealthNoThai),
        (2.., _) => Some(T::HealthManyThai),
        _ => None,
    };
    match bad {
        None => ok(T::HealthKeyboards, names),
        Some(why) => Check {
            title: T::HealthKeyboards,
            ok: false,
            status: format!("{names}: {}", tr(why)),
            fix: Some((T::HealthLanguageSettings, Fix::LanguageSettings)),
        },
    }
}

/// Windows' language switch keys: "1" Alt+Shift, "2" Ctrl+Shift, "3" none,
/// "4" the grave key.
fn switch_hotkey() -> String {
    for name in ["Language Hotkey", "Hotkey"] {
        if let Some(v) = reg_string(r"Keyboard Layout\Toggle", name) {
            return v;
        }
    }
    "1".into()
}

fn switch_keys() -> Check {
    match switch_hotkey().trim() {
        "2" => Check {
            title: T::HealthSwitch,
            ok: false,
            status: tr(T::HealthSwitchCtrlShift).to_string(),
            fix: Some((T::HealthUseAltShift, Fix::AltShift)),
        },
        "3" => ok(T::HealthSwitch, tr(T::HealthSwitchNone).to_string()),
        "4" => ok(T::HealthSwitch, tr(T::HealthSwitchGrave).to_string()),
        _ => ok(T::HealthSwitch, "Alt+Shift".to_string()),
    }
}

fn autocorrect() -> Check {
    if crate::ui::windows_autocorrects() {
        Check {
            title: T::HealthAutocorrect,
            ok: false,
            status: tr(T::HealthAutocorrectOn).to_string(),
            fix: Some((T::HealthTypingSettings, Fix::TypingSettings)),
        }
    } else {
        ok(T::HealthAutocorrect, tr(T::HealthOff).to_string())
    }
}

fn caps_lock() -> Check {
    if unsafe { GetKeyState(0x14) } & 1 != 0 {
        Check {
            title: T::HealthCaps,
            ok: false,
            status: tr(T::HealthCapsOn).to_string(),
            fix: Some((T::HealthTurnOff, Fix::CapsOff)),
        }
    } else {
        ok(T::HealthCaps, tr(T::HealthOff).to_string())
    }
}

fn key_names(keys: &[(u16, bool)]) -> String {
    keys.iter()
        .map(|(scan, ext)| {
            righttype::keyboard::index_of(*scan, *ext)
                .map(|i| righttype::keyboard::KEYS[i].label)
                .filter(|l| !l.is_empty())
                .unwrap_or("Space")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn chatter() -> Check {
    let filtered = crate::hook::debounce_keys();
    let new: Vec<((u16, bool), u32)> = crate::hook::chatter_suspects()
        .into_iter()
        .filter(|(k, _)| !filtered.contains(k))
        .collect();
    if !new.is_empty() {
        let keys: Vec<(u16, bool)> = new.iter().map(|(k, _)| *k).collect();
        let times: u32 = new.iter().map(|(_, n)| n).sum();
        return Check {
            title: T::HealthChatter,
            ok: false,
            status: trf(
                T::HealthChatterBad,
                &[("keys", &key_names(&keys)), ("n", &times.to_string())],
            ),
            fix: Some((T::HealthFilterThem, Fix::Debounce(keys))),
        };
    }
    if filtered.is_empty() {
        return ok(T::HealthChatter, tr(T::HealthChatterOk).to_string());
    }
    Check {
        title: T::HealthChatter,
        ok: true,
        status: trf(T::HealthChatterFiltered, &[("keys", &key_names(&filtered))]),
        fix: Some((T::HealthStopFilter, Fix::StopDebounce)),
    }
}

/// Make the fix (UI thread).
pub fn apply(fix: &Fix) {
    crate::hook::e2e_trace(format!("health: fix {fix:?}"));
    unsafe {
        match fix {
            Fix::Release(keys) => crate::inject::release_keys(keys),
            Fix::StickyOff => {
                let mut sk = sticky();
                sk.dwFlags.0 &= !(SKF_STICKYKEYSON.0 | SKF_HOTKEYACTIVE.0);
                let _ = SystemParametersInfoW(
                    SPI_SETSTICKYKEYS,
                    sk.cbSize,
                    Some(&mut sk as *mut _ as *mut _),
                    SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
                );
            }
            Fix::FilterOff => {
                let mut fk = filter();
                fk.dwFlags &= !(FKF_FILTERKEYSON | FKF_HOTKEYACTIVE);
                let _ = SystemParametersInfoW(
                    SPI_SETFILTERKEYS,
                    fk.cbSize,
                    Some(&mut fk as *mut _ as *mut _),
                    SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
                );
            }
            Fix::AltShift => {
                let key = r"Keyboard Layout\Toggle";
                set_reg_string(key, "Hotkey", "1");
                set_reg_string(key, "Language Hotkey", "1");
                // Switching keyboards within a language must not take the
                // same keys.
                if reg_string(key, "Layout Hotkey").as_deref() == Some("1") {
                    set_reg_string(key, "Layout Hotkey", "3");
                }
                let _ = SystemParametersInfoW(SPI_SETLANGTOGGLE, 0, None, Default::default());
            }
            Fix::LanguageSettings => open("ms-settings:regionlanguage"),
            Fix::TypingSettings => open("ms-settings:typing"),
            Fix::CapsOff => crate::inject::toggle_capslock(),
            Fix::Debounce(keys) => {
                let mut all = crate::hook::debounce_keys();
                all.extend(
                    keys.iter()
                        .filter(|k| !all.contains(k))
                        .copied()
                        .collect::<Vec<_>>(),
                );
                crate::hook::set_debounce_keys(all);
                crate::config::persist();
            }
            Fix::StopDebounce => {
                crate::hook::set_debounce_keys(Vec::new());
                crate::config::persist();
            }
        }
    }
}

fn open(uri: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let target: Vec<u16> = format!("{uri}\0").encode_utf16().collect();
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

fn reg_string(key: &str, name: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
    let key: Vec<u16> = format!("{key}\0").encode_utf16().collect();
    let name: Vec<u16> = format!("{name}\0").encode_utf16().collect();
    let mut buf = [0u16; 16];
    let mut len = (buf.len() * 2) as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut len),
        )
    };
    if r.is_err() {
        return None;
    }
    let n = (len as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..n]))
}

fn set_reg_string(key: &str, name: &str, value: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};
    let key: Vec<u16> = format!("{key}\0").encode_utf16().collect();
    let name: Vec<u16> = format!("{name}\0").encode_utf16().collect();
    let data: Vec<u16> = format!("{value}\0").encode_utf16().collect();
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            REG_SZ.0,
            Some(data.as_ptr().cast()),
            (data.len() * 2) as u32,
        );
    }
}
