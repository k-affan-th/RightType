//! Keys shown on screen (for teaching, recording or presenting): what may
//! be shown for one key press. Shortcuts (with Ctrl, Alt or the Windows
//! key) and keys that type nothing (Enter, Esc, arrows, F-keys…) are shown;
//! letters, digits and symbols never are, so the text typed stays off the
//! screen. The Windows side shows nothing at all in password fields and
//! apps on the safety list.

/// The modifiers held with a key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

/// A key's name by its virtual-key code (`None`: a modifier, or a key
/// not worth naming).
pub fn key_name(vk: u16) -> Option<String> {
    let named = match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Esc",
        0x20 => "Space",
        0x21 => "PgUp",
        0x22 => "PgDn",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "←",
        0x26 => "↑",
        0x27 => "→",
        0x28 => "↓",
        0x2C => "PrtSc",
        0x2D => "Ins",
        0x2E => "Del",
        0x5D => "Menu",
        0x90 => "NumLock",
        0x91 => "ScrLk",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => "",
    };
    if !named.is_empty() {
        return Some(named.to_string());
    }
    match vk {
        0x30..=0x39 | 0x41..=0x5A => Some(char::from(vk as u8).to_string()),
        0x60..=0x69 => Some(format!("Num {}", vk - 0x60)),
        0x70..=0x87 => Some(format!("F{}", vk - 0x6F)),
        _ => None,
    }
}

fn is_modifier(vk: u16) -> bool {
    matches!(vk, 0x10..=0x12 | 0x5B | 0x5C | 0xA0..=0xA5)
}

/// Keys that type a character (or a space) on their own.
fn types_text(vk: u16) -> bool {
    matches!(
        vk,
        0x20 | 0x30..=0x39 | 0x41..=0x5A | 0x60..=0x6F | 0xBA..=0xC0 | 0xDB..=0xDF | 0xE2
    )
}

/// What to show for `vk` pressed with `mods`: `Ctrl+Shift+S`, `Alt+Tab`,
/// `Enter`. `None` for modifiers alone and for anything that types text
/// (with Shift alone too: `Shift+A` is just a capital letter).
pub fn label(vk: u16, mods: Mods) -> Option<String> {
    if is_modifier(vk) {
        return None;
    }
    let shortcut = mods.ctrl || mods.alt || mods.win;
    if !shortcut && types_text(vk) {
        return None;
    }
    let name = key_name(vk)?;
    let mut parts = Vec::new();
    if mods.win {
        parts.push("Win");
    }
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.alt {
        parts.push("Alt");
    }
    if mods.shift {
        parts.push("Shift");
    }
    parts.push(&name);
    Some(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CTRL: Mods = Mods {
        ctrl: true,
        alt: false,
        shift: false,
        win: false,
    };

    #[test]
    fn shows_shortcuts_and_keys_that_type_nothing() {
        assert_eq!(label(0x53, CTRL).as_deref(), Some("Ctrl+S"));
        let ctrl_shift = Mods {
            shift: true,
            ..CTRL
        };
        assert_eq!(label(0x54, ctrl_shift).as_deref(), Some("Ctrl+Shift+T"));
        let alt = Mods {
            alt: true,
            ..Mods::default()
        };
        assert_eq!(label(0x09, alt).as_deref(), Some("Alt+Tab"));
        let win = Mods {
            win: true,
            ..Mods::default()
        };
        assert_eq!(label(0x45, win).as_deref(), Some("Win+E"));
        assert_eq!(label(0x0D, Mods::default()).as_deref(), Some("Enter"));
        assert_eq!(label(0x74, Mods::default()).as_deref(), Some("F5"));
        assert_eq!(label(0xBF, CTRL).as_deref(), Some("Ctrl+/"));
    }

    #[test]
    fn never_shows_what_is_typed() {
        let shift = Mods {
            shift: true,
            ..Mods::default()
        };
        for vk in [0x41u16, 0x5A, 0x30, 0x39, 0x20, 0xBA, 0xDE, 0x61] {
            assert_eq!(label(vk, Mods::default()), None, "{vk:#x}");
            assert_eq!(label(vk, shift), None, "Shift+{vk:#x}");
        }
        // Modifiers alone.
        assert_eq!(label(0xA2, CTRL), None);
        assert_eq!(label(0x10, shift), None);
    }
}
