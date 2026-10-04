//! App shortcuts for the "this app's shortcuts" list: a bundled table
//! (`assets/shortcuts.txt`) for apps whose shortcuts are not in a menu, and
//! the key names those shortcuts are written with. Plain data: no OS calls.

/// One shortcut: its keys as written (`Ctrl+Shift+T`) and what it does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub keys: String,
    pub en: String,
    pub th: String,
}

const TABLE: &str = include_str!("../assets/shortcuts.txt");

/// The bundled shortcuts of `exe` (lower case), then Windows' own.
pub fn for_app(exe: &str) -> Vec<Shortcut> {
    let mut app = Vec::new();
    let mut windows = Vec::new();
    let mut into: Option<bool> = None; // Some(true): this app; Some(false): Windows
    for line in TABLE.lines().map(str::trim) {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(names) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            into = if names == "windows" {
                Some(false)
            } else if names.split_whitespace().any(|n| n == exe) {
                Some(true)
            } else {
                None
            };
            continue;
        }
        let Some(target) = into else {
            continue;
        };
        let mut parts = line.split('|').map(str::trim);
        if let (Some(keys), Some(en), Some(th)) = (parts.next(), parts.next(), parts.next()) {
            let s = Shortcut {
                keys: keys.to_string(),
                en: en.to_string(),
                th: th.to_string(),
            };
            if target {
                app.push(s);
            } else {
                windows.push(s);
            }
        }
    }
    app.extend(windows);
    app
}

/// The virtual keys to press for `keys` (`Ctrl+Shift+T`): modifiers first,
/// then the key. `None` for a name not known.
pub fn virtual_keys(keys: &str) -> Option<Vec<u16>> {
    let parts: Vec<&str> = keys.split('+').collect();
    // `Ctrl+Plus` is written out; a lone `+` would split.
    let mut out = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        let vk = match *part {
            "Ctrl" if !last => 0x11,
            "Shift" if !last => 0x10,
            "Alt" if !last => 0x12,
            "Win" if !last => 0x5B,
            name => key_vk(name)?,
        };
        out.push(vk);
    }
    Some(out)
}

fn key_vk(name: &str) -> Option<u16> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.clone().next()) {
        return Some(match c {
            'A'..='Z' | '0'..='9' => c as u16,
            ';' => 0xBA,
            '=' => 0xBB,
            ',' => 0xBC,
            '-' => 0xBD,
            '.' => 0xBE,
            '/' => 0xBF,
            '`' => 0xC0,
            '[' => 0xDB,
            '\\' => 0xDC,
            ']' => 0xDD,
            '\'' => 0xDE,
            _ => return None,
        });
    }
    Some(match name {
        "Plus" => 0xBB,
        "Minus" => 0xBD,
        "Space" => 0x20,
        "Enter" => 0x0D,
        "Backspace" => 0x08,
        "Tab" => 0x09,
        "Esc" => 0x1B,
        "Delete" => 0x2E,
        "Insert" => 0x2D,
        "Home" => 0x24,
        "End" => 0x23,
        "PageUp" => 0x21,
        "PageDown" => 0x22,
        "Left" => 0x25,
        "Up" => 0x26,
        "Right" => 0x27,
        "Down" => 0x28,
        f if f.starts_with('F') => {
            let n: u16 = f[1..].parse().ok()?;
            if (1..=24).contains(&n) {
                0x70 + n - 1
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_shortcut_has_real_keys_and_both_languages() {
        let mut sections = 0;
        let mut seen = std::collections::HashSet::new();
        for line in TABLE.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                sections += 1;
                seen.clear();
                continue;
            }
            let parts: Vec<&str> = line.split('|').map(str::trim).collect();
            assert_eq!(parts.len(), 3, "{line}");
            assert!(virtual_keys(parts[0]).is_some(), "keys: {line}");
            assert!(!parts[1].is_empty() && !parts[2].is_empty(), "{line}");
            assert!(
                parts[2]
                    .chars()
                    .any(|c| ('\u{0E00}'..='\u{0E7F}').contains(&c)),
                "Thai: {line}"
            );
            assert!(seen.insert(parts[0]), "listed twice: {line}");
        }
        assert!(sections >= 10);
    }

    #[test]
    fn an_apps_list_comes_with_windows_own() {
        let chrome = for_app("chrome.exe");
        assert_eq!(chrome[0].keys, "Ctrl+T");
        assert!(chrome.iter().any(|s| s.keys == "Win+D"));
        let unknown = for_app("unknown.exe");
        assert!(!unknown.is_empty() && unknown.iter().all(|s| s.keys != "Ctrl+T"));
        assert_eq!(
            virtual_keys("Ctrl+Shift+T"),
            Some(vec![0x11, 0x10, b'T' as u16])
        );
        assert_eq!(virtual_keys("F5"), Some(vec![0x74]));
        assert_eq!(virtual_keys("Ctrl+Plus"), Some(vec![0x11, 0xBB]));
        assert_eq!(virtual_keys("Ctrl+Nope"), None);
    }
}
