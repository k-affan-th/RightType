//! Per-app correction modes: the model and its text form.
//!
//! Each app (by executable name, lower case) may have its own mode that
//! replaces the global one there — say Manual in a code editor and Auto in a
//! chat app — or be switched off. The built-in safety list (terminals, password
//! managers, wallets) is checked before this and cannot be overridden by it.
//!
//! The Settings window edits the list as text, one app per line:
//!
//! ```text
//! code.exe = manual
//! line.exe = auto
//! ```
//!
//! Mode names are accepted in English or Thai.

use std::collections::BTreeMap;

/// What RightType does in one app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Auto,
    Suggest,
    Manual,
    /// Stay out of this app entirely (like the blocked-apps list).
    Off,
}

impl AppMode {
    /// The name written to the config and the Settings list.
    pub fn name(self) -> &'static str {
        match self {
            AppMode::Auto => "auto",
            AppMode::Suggest => "suggest",
            AppMode::Manual => "manual",
            AppMode::Off => "off",
        }
    }

    /// Parse a mode name (English or Thai, any case).
    pub fn parse(s: &str) -> Option<AppMode> {
        match s.trim().to_lowercase().as_str() {
            "auto" | "อัตโนมัติ" => Some(AppMode::Auto),
            "suggest" | "แนะนำ" => Some(AppMode::Suggest),
            "manual" | "กดแก้เอง" => Some(AppMode::Manual),
            "off" | "ปิด" => Some(AppMode::Off),
            _ => None,
        }
    }
}

/// Normalise an executable name: trimmed, lower case, and `.exe` added when
/// missing. `None` when it cannot be a program name.
pub fn normalize_exe(name: &str) -> Option<String> {
    let name = name.trim().to_lowercase();
    if name.is_empty() || name.contains(['/', '\\', '=', ':']) || name.chars().count() > 128 {
        return None;
    }
    Some(if name.ends_with(".exe") {
        name
    } else {
        format!("{name}.exe")
    })
}

/// Parse the Settings list. Blank lines are ignored; returns the modes and how
/// many lines were not understood.
pub fn parse_list(text: &str) -> (BTreeMap<String, AppMode>, usize) {
    let mut modes = BTreeMap::new();
    let mut skipped = 0;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let parsed = line
            .split_once('=')
            .and_then(|(exe, mode)| Some((normalize_exe(exe)?, AppMode::parse(mode)?)));
        match parsed {
            Some((exe, mode)) => {
                modes.insert(exe, mode);
            }
            None => skipped += 1,
        }
    }
    (modes, skipped)
}

/// The Settings list for `modes`, one `name.exe = mode` per line.
pub fn format_list(modes: &BTreeMap<String, AppMode>) -> String {
    modes
        .iter()
        .map(|(exe, mode)| format!("{exe} = {}", mode.name()))
        .collect::<Vec<_>>()
        .join("\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_round_trips() {
        let (modes, skipped) =
            parse_list("Code.exe = Manual\r\n\r\nline = auto\nnotepad.exe=off\n");
        assert_eq!(skipped, 0);
        assert_eq!(modes.get("code.exe"), Some(&AppMode::Manual));
        assert_eq!(modes.get("line.exe"), Some(&AppMode::Auto));
        assert_eq!(modes.get("notepad.exe"), Some(&AppMode::Off));
        let text = format_list(&modes);
        assert_eq!(parse_list(&text).0, modes);
    }

    #[test]
    fn thai_mode_names_are_understood() {
        let (modes, _) = parse_list("winword.exe = แนะนำ\nchrome.exe = ปิด");
        assert_eq!(modes.get("winword.exe"), Some(&AppMode::Suggest));
        assert_eq!(modes.get("chrome.exe"), Some(&AppMode::Off));
    }

    #[test]
    fn bad_lines_are_counted_not_guessed() {
        let (modes, skipped) =
            parse_list("code.exe\nC:\\\\x.exe = auto\nfoo.exe = fast\nok.exe = auto");
        assert_eq!(skipped, 3);
        assert_eq!(modes.len(), 1);
    }
}
