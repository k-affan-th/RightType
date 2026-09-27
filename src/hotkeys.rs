//! RightType's hotkeys: what they are, how they are written, and which ones
//! are safe to use.
//!
//! A hotkey is a key plus an exact set of Ctrl / Shift / Alt. The defaults are
//! the 1.x chords; each can be changed in Settings → Hotkeys. A chord must not
//! be something people type (a letter with only Shift, a bare Space), and two
//! actions may not share one.

/// Windows virtual-key codes used here.
pub mod vk {
    pub const BACK: u16 = 0x08;
    pub const TAB: u16 = 0x09;
    pub const RETURN: u16 = 0x0D;
    pub const PAUSE: u16 = 0x13;
    pub const CAPITAL: u16 = 0x14;
    pub const ESCAPE: u16 = 0x1B;
    pub const SPACE: u16 = 0x20;
    pub const PRIOR: u16 = 0x21;
    pub const NEXT: u16 = 0x22;
    pub const END: u16 = 0x23;
    pub const HOME: u16 = 0x24;
    pub const LEFT: u16 = 0x25;
    pub const UP: u16 = 0x26;
    pub const RIGHT: u16 = 0x27;
    pub const DOWN: u16 = 0x28;
    pub const INSERT: u16 = 0x2D;
    pub const DELETE: u16 = 0x2E;
    pub const F1: u16 = 0x70;
    pub const F24: u16 = 0x87;
    pub const SCROLL: u16 = 0x91;
}

/// Something a hotkey does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Action {
    /// Fix or flip back the last word (again: the word before).
    Flip,
    /// Convert the selected text.
    Selection,
    /// Cycle Manual → Auto → Suggest.
    Cycle,
    /// Undo the last fix.
    Undo,
    /// Use the Suggest hint.
    Accept,
    /// Turn RightType off / on.
    Panic,
    /// Open the command palette.
    Palette,
}

impl Action {
    pub const ALL: [Action; 7] = [
        Action::Flip,
        Action::Selection,
        Action::Cycle,
        Action::Undo,
        Action::Accept,
        Action::Panic,
        Action::Palette,
    ];

    /// The config name.
    pub fn name(self) -> &'static str {
        match self {
            Action::Flip => "flip",
            Action::Selection => "selection",
            Action::Cycle => "cycle",
            Action::Undo => "undo",
            Action::Accept => "accept",
            Action::Panic => "panic",
            Action::Palette => "palette",
        }
    }

    pub fn from_name(name: &str) -> Option<Action> {
        Action::ALL.into_iter().find(|a| a.name() == name)
    }
}

/// A key with an exact set of modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub key: u16,
}

impl Chord {
    pub const fn new(ctrl: bool, shift: bool, alt: bool, key: u16) -> Self {
        Self {
            ctrl,
            shift,
            alt,
            key,
        }
    }

    pub fn matches(&self, key: u16, ctrl: bool, shift: bool, alt: bool) -> bool {
        self.key == key && self.ctrl == ctrl && self.shift == shift && self.alt == alt
    }

    /// Would this chord get in the way of typing (or of Windows)? Plain keys
    /// that type text or move the caret need Ctrl or Alt; Shift alone is only
    /// enough on keys that type nothing (CapsLock, Backspace, F-keys, …).
    pub fn is_usable(&self) -> bool {
        if key_name(self.key).is_none() {
            return false;
        }
        if self.ctrl || self.alt {
            return true;
        }
        let quiet = matches!(self.key, vk::CAPITAL | vk::PAUSE | vk::SCROLL | vk::INSERT)
            || (vk::F1..=vk::F24).contains(&self.key);
        quiet || (self.shift && self.key == vk::BACK)
    }

    /// `Ctrl + Shift + CapsLock`.
    pub fn format(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".into());
        }
        if self.alt {
            parts.push("Alt".into());
        }
        if self.shift {
            parts.push("Shift".into());
        }
        parts.push(key_name(self.key).unwrap_or_else(|| format!("0x{:02X}", self.key)));
        parts.join(" + ")
    }

    /// Parse `Ctrl + Shift + CapsLock` (any case, `+` with or without spaces).
    pub fn parse(text: &str) -> Option<Chord> {
        let mut chord = Chord::new(false, false, false, 0);
        let mut key = None;
        for part in text.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => chord.ctrl = true,
                "shift" => chord.shift = true,
                "alt" => chord.alt = true,
                other => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(key_from_name(other)?);
                }
            }
        }
        chord.key = key?;
        Some(chord)
    }
}

/// The name of a key RightType lets you bind, or `None`.
pub fn key_name(key: u16) -> Option<String> {
    Some(match key {
        vk::BACK => "Backspace".into(),
        vk::TAB => "Tab".into(),
        vk::RETURN => "Enter".into(),
        vk::PAUSE => "Pause".into(),
        vk::CAPITAL => "CapsLock".into(),
        vk::SPACE => "Space".into(),
        vk::PRIOR => "PageUp".into(),
        vk::NEXT => "PageDown".into(),
        vk::END => "End".into(),
        vk::HOME => "Home".into(),
        vk::LEFT => "Left".into(),
        vk::UP => "Up".into(),
        vk::RIGHT => "Right".into(),
        vk::DOWN => "Down".into(),
        vk::INSERT => "Insert".into(),
        vk::DELETE => "Delete".into(),
        vk::SCROLL => "ScrollLock".into(),
        0x30..=0x39 | 0x41..=0x5A => char::from(key as u8).to_string(),
        k if (vk::F1..=vk::F24).contains(&k) => format!("F{}", k - vk::F1 + 1),
        _ => return None,
    })
}

fn key_from_name(name: &str) -> Option<u16> {
    let key = match name {
        "backspace" => vk::BACK,
        "tab" => vk::TAB,
        "enter" | "return" => vk::RETURN,
        "pause" => vk::PAUSE,
        "capslock" | "caps" => vk::CAPITAL,
        "space" => vk::SPACE,
        "pageup" => vk::PRIOR,
        "pagedown" => vk::NEXT,
        "end" => vk::END,
        "home" => vk::HOME,
        "left" => vk::LEFT,
        "up" => vk::UP,
        "right" => vk::RIGHT,
        "down" => vk::DOWN,
        "insert" | "ins" => vk::INSERT,
        "delete" | "del" => vk::DELETE,
        "scrolllock" => vk::SCROLL,
        _ => {
            let bytes = name.as_bytes();
            if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
                bytes[0].to_ascii_uppercase() as u16
            } else if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
                if !(1..=24).contains(&n) {
                    return None;
                }
                vk::F1 + n - 1
            } else {
                return None;
            }
        }
    };
    Some(key)
}

/// Every action's chord.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkeys {
    chords: [Chord; 7],
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            chords: Action::ALL.map(default_chord),
        }
    }
}

/// The 1.x chords, plus Ctrl+Alt+Space for the palette.
pub fn default_chord(action: Action) -> Chord {
    match action {
        Action::Flip => Chord::new(false, true, false, vk::BACK),
        Action::Selection => Chord::new(false, true, false, vk::CAPITAL),
        Action::Cycle => Chord::new(true, false, false, vk::CAPITAL),
        Action::Undo => Chord::new(true, true, false, vk::CAPITAL),
        Action::Accept => Chord::new(false, false, true, vk::CAPITAL),
        Action::Panic => Chord::new(true, false, true, vk::CAPITAL),
        Action::Palette => Chord::new(true, false, true, vk::SPACE),
    }
}

fn index(action: Action) -> usize {
    Action::ALL.iter().position(|a| *a == action).unwrap_or(0)
}

/// Why a chord cannot be used for an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// It would get in the way of typing.
    Unusable,
    /// Another action already uses it.
    Taken(Action),
}

impl Hotkeys {
    pub fn chord(&self, action: Action) -> Chord {
        self.chords[index(action)]
    }

    /// Bind `chord` to `action`, unless it is unusable or taken.
    pub fn set(&mut self, action: Action, chord: Chord) -> Result<(), Refusal> {
        if !chord.is_usable() {
            return Err(Refusal::Unusable);
        }
        if let Some(other) = Action::ALL
            .into_iter()
            .find(|a| *a != action && self.chord(*a) == chord)
        {
            return Err(Refusal::Taken(other));
        }
        self.chords[index(action)] = chord;
        Ok(())
    }

    /// The action a key press triggers, if any.
    pub fn action_for(&self, key: u16, ctrl: bool, shift: bool, alt: bool) -> Option<Action> {
        Action::ALL
            .into_iter()
            .find(|a| self.chord(*a).matches(key, ctrl, shift, alt))
    }

    /// Is `key` the key of any hotkey (whatever the modifiers)?
    pub fn uses_key(&self, key: u16) -> bool {
        self.chords.iter().any(|c| c.key == key)
    }

    /// From the config (`action = "Ctrl + CapsLock"`): unknown actions, bad or
    /// unusable chords and duplicates are ignored, keeping the default.
    pub fn from_config<'a>(entries: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut hotkeys = Hotkeys::default();
        for (name, text) in entries {
            if let (Some(action), Some(chord)) = (Action::from_name(name), Chord::parse(text)) {
                let _ = hotkeys.set(action, chord);
            }
        }
        hotkeys
    }

    /// The entries that differ from the defaults, for the config.
    pub fn to_config(&self) -> Vec<(&'static str, String)> {
        Action::ALL
            .into_iter()
            .filter(|a| self.chord(*a) != default_chord(*a))
            .map(|a| (a.name(), self.chord(a).format()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_the_1x_chords() {
        let h = Hotkeys::default();
        assert_eq!(h.chord(Action::Flip).format(), "Shift + Backspace");
        assert_eq!(h.chord(Action::Undo).format(), "Ctrl + Shift + CapsLock");
        assert_eq!(h.chord(Action::Panic).format(), "Ctrl + Alt + CapsLock");
        assert_eq!(
            h.action_for(vk::CAPITAL, true, false, false),
            Some(Action::Cycle)
        );
        // Modifiers must match exactly.
        assert_eq!(h.action_for(vk::CAPITAL, true, true, true), None);
        assert!(h.to_config().is_empty());
    }

    #[test]
    fn chords_parse_and_format() {
        let c = Chord::parse("ctrl+alt + f9").unwrap();
        assert_eq!(c, Chord::new(true, false, true, vk::F1 + 8));
        assert_eq!(c.format(), "Ctrl + Alt + F9");
        assert_eq!(Chord::parse(&c.format()), Some(c));
        for bad in ["", "Ctrl", "Ctrl + A + B", "Ctrl + F25", "Ctrl + ~"] {
            assert_eq!(Chord::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn chords_that_would_break_typing_are_refused() {
        let mut h = Hotkeys::default();
        for text in [
            "A",
            "Shift + A",
            "Space",
            "Shift + Space",
            "Tab",
            "Backspace",
        ] {
            let chord = Chord::parse(text).unwrap();
            assert_eq!(
                h.set(Action::Palette, chord),
                Err(Refusal::Unusable),
                "{text}"
            );
        }
        for text in ["F8", "Ctrl + Shift + K", "Alt + Q", "Shift + CapsLock"] {
            assert!(Chord::parse(text).unwrap().is_usable(), "{text}");
        }
    }

    #[test]
    fn two_actions_cannot_share_a_chord() {
        let mut h = Hotkeys::default();
        let cycle = h.chord(Action::Cycle);
        assert_eq!(
            h.set(Action::Palette, cycle),
            Err(Refusal::Taken(Action::Cycle))
        );
        assert!(h.set(Action::Palette, Chord::parse("F8").unwrap()).is_ok());
        assert_eq!(h.to_config(), vec![("palette", "F8".to_string())]);
    }

    #[test]
    fn the_config_round_trips_and_ignores_junk() {
        let h = Hotkeys::from_config([
            ("palette", "F8"),
            ("undo", "nonsense"),
            ("nope", "F9"),
            ("flip", "A"),
        ]);
        assert_eq!(h.chord(Action::Palette).format(), "F8");
        assert_eq!(h.chord(Action::Undo), default_chord(Action::Undo));
        assert_eq!(h.chord(Action::Flip), default_chord(Action::Flip));
        let saved = h.to_config();
        let again = Hotkeys::from_config(saved.iter().map(|(k, v)| (*k, v.as_str())));
        assert_eq!(again, h);
    }
}
