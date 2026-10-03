//! The physical keyboard: where each key sits on a full-size PC keyboard
//! and the scan code it sends, for the cleaning lock and the key tester to
//! draw and light up. Plain data: no OS calls.
//!
//! Positions are in key units (one letter key is 1 × 1). A key is told
//! apart by its scan code plus Windows' "extended" flag: the separate arrow
//! and editing keys share scan codes with the keypad and differ only by it.
//! Fn and the laptop keys built on it (brightness, Wi-Fi, airplane mode)
//! never reach Windows as keys, so they are not here.

/// One key on the drawing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Key {
    pub scan: u16,
    pub extended: bool,
    /// What its cap says (the US character for character keys).
    pub label: &'static str,
    /// Its US character, when it types one (`None` for Shift, F1, …).
    pub us: Option<char>,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

const fn key(scan: u16, extended: bool, label: &'static str, x: f32, y: f32, w: f32) -> Key {
    Key {
        scan,
        extended,
        label,
        us: None,
        x,
        y,
        w,
        h: 1.0,
    }
}

const fn ch(scan: u16, c: char, label: &'static str, x: f32, y: f32) -> Key {
    Key {
        scan,
        extended: false,
        label,
        us: Some(c),
        x,
        y,
        w: 1.0,
        h: 1.0,
    }
}

const fn tall(mut k: Key) -> Key {
    k.h = 2.0;
    k
}

/// The whole drawing is this wide and tall (key units).
pub const WIDTH: f32 = 22.5;
pub const HEIGHT: f32 = 6.5;

/// A full-size keyboard (function row, main block, editing keys and
/// arrows, keypad).
pub const KEYS: &[Key] = &[
    // Function row.
    key(0x01, false, "Esc", 0.0, 0.0, 1.0),
    key(0x3B, false, "F1", 2.0, 0.0, 1.0),
    key(0x3C, false, "F2", 3.0, 0.0, 1.0),
    key(0x3D, false, "F3", 4.0, 0.0, 1.0),
    key(0x3E, false, "F4", 5.0, 0.0, 1.0),
    key(0x3F, false, "F5", 6.5, 0.0, 1.0),
    key(0x40, false, "F6", 7.5, 0.0, 1.0),
    key(0x41, false, "F7", 8.5, 0.0, 1.0),
    key(0x42, false, "F8", 9.5, 0.0, 1.0),
    key(0x43, false, "F9", 11.0, 0.0, 1.0),
    key(0x44, false, "F10", 12.0, 0.0, 1.0),
    key(0x57, false, "F11", 13.0, 0.0, 1.0),
    key(0x58, false, "F12", 14.0, 0.0, 1.0),
    key(0x37, true, "PrtSc", 15.25, 0.0, 1.0),
    key(0x46, false, "ScrLk", 16.25, 0.0, 1.0),
    key(0x45, false, "Pause", 17.25, 0.0, 1.0),
    // Number row.
    ch(0x29, '`', "`", 0.0, 1.5),
    ch(0x02, '1', "1", 1.0, 1.5),
    ch(0x03, '2', "2", 2.0, 1.5),
    ch(0x04, '3', "3", 3.0, 1.5),
    ch(0x05, '4', "4", 4.0, 1.5),
    ch(0x06, '5', "5", 5.0, 1.5),
    ch(0x07, '6', "6", 6.0, 1.5),
    ch(0x08, '7', "7", 7.0, 1.5),
    ch(0x09, '8', "8", 8.0, 1.5),
    ch(0x0A, '9', "9", 9.0, 1.5),
    ch(0x0B, '0', "0", 10.0, 1.5),
    ch(0x0C, '-', "-", 11.0, 1.5),
    ch(0x0D, '=', "=", 12.0, 1.5),
    key(0x0E, false, "Backspace", 13.0, 1.5, 2.0),
    key(0x52, true, "Ins", 15.25, 1.5, 1.0),
    key(0x47, true, "Home", 16.25, 1.5, 1.0),
    key(0x49, true, "PgUp", 17.25, 1.5, 1.0),
    key(0x45, true, "Num", 18.5, 1.5, 1.0),
    key(0x35, true, "/", 19.5, 1.5, 1.0),
    key(0x37, false, "*", 20.5, 1.5, 1.0),
    key(0x4A, false, "-", 21.5, 1.5, 1.0),
    // Top letter row.
    key(0x0F, false, "Tab", 0.0, 2.5, 1.5),
    ch(0x10, 'q', "Q", 1.5, 2.5),
    ch(0x11, 'w', "W", 2.5, 2.5),
    ch(0x12, 'e', "E", 3.5, 2.5),
    ch(0x13, 'r', "R", 4.5, 2.5),
    ch(0x14, 't', "T", 5.5, 2.5),
    ch(0x15, 'y', "Y", 6.5, 2.5),
    ch(0x16, 'u', "U", 7.5, 2.5),
    ch(0x17, 'i', "I", 8.5, 2.5),
    ch(0x18, 'o', "O", 9.5, 2.5),
    ch(0x19, 'p', "P", 10.5, 2.5),
    ch(0x1A, '[', "[", 11.5, 2.5),
    ch(0x1B, ']', "]", 12.5, 2.5),
    Key {
        w: 1.5,
        ..ch(0x2B, '\\', "\\", 13.5, 2.5)
    },
    key(0x53, true, "Del", 15.25, 2.5, 1.0),
    key(0x4F, true, "End", 16.25, 2.5, 1.0),
    key(0x51, true, "PgDn", 17.25, 2.5, 1.0),
    key(0x47, false, "7", 18.5, 2.5, 1.0),
    key(0x48, false, "8", 19.5, 2.5, 1.0),
    key(0x49, false, "9", 20.5, 2.5, 1.0),
    tall(key(0x4E, false, "+", 21.5, 2.5, 1.0)),
    // Home row.
    key(0x3A, false, "Caps", 0.0, 3.5, 1.75),
    ch(0x1E, 'a', "A", 1.75, 3.5),
    ch(0x1F, 's', "S", 2.75, 3.5),
    ch(0x20, 'd', "D", 3.75, 3.5),
    ch(0x21, 'f', "F", 4.75, 3.5),
    ch(0x22, 'g', "G", 5.75, 3.5),
    ch(0x23, 'h', "H", 6.75, 3.5),
    ch(0x24, 'j', "J", 7.75, 3.5),
    ch(0x25, 'k', "K", 8.75, 3.5),
    ch(0x26, 'l', "L", 9.75, 3.5),
    ch(0x27, ';', ";", 10.75, 3.5),
    ch(0x28, '\'', "'", 11.75, 3.5),
    key(0x1C, false, "Enter", 12.75, 3.5, 2.25),
    key(0x4B, false, "4", 18.5, 3.5, 1.0),
    key(0x4C, false, "5", 19.5, 3.5, 1.0),
    key(0x4D, false, "6", 20.5, 3.5, 1.0),
    // Bottom letter row.
    key(0x2A, false, "Shift", 0.0, 4.5, 2.25),
    ch(0x2C, 'z', "Z", 2.25, 4.5),
    ch(0x2D, 'x', "X", 3.25, 4.5),
    ch(0x2E, 'c', "C", 4.25, 4.5),
    ch(0x2F, 'v', "V", 5.25, 4.5),
    ch(0x30, 'b', "B", 6.25, 4.5),
    ch(0x31, 'n', "N", 7.25, 4.5),
    ch(0x32, 'm', "M", 8.25, 4.5),
    ch(0x33, ',', ",", 9.25, 4.5),
    ch(0x34, '.', ".", 10.25, 4.5),
    ch(0x35, '/', "/", 11.25, 4.5),
    key(0x36, false, "Shift", 12.25, 4.5, 2.75),
    key(0x48, true, "↑", 16.25, 4.5, 1.0),
    key(0x4F, false, "1", 18.5, 4.5, 1.0),
    key(0x50, false, "2", 19.5, 4.5, 1.0),
    key(0x51, false, "3", 20.5, 4.5, 1.0),
    tall(key(0x1C, true, "Enter", 21.5, 4.5, 1.0)),
    // Bottom row.
    key(0x1D, false, "Ctrl", 0.0, 5.5, 1.25),
    key(0x5B, true, "Win", 1.25, 5.5, 1.25),
    key(0x38, false, "Alt", 2.5, 5.5, 1.25),
    key(0x39, false, "", 3.75, 5.5, 6.25),
    key(0x38, true, "Alt", 10.0, 5.5, 1.25),
    key(0x5C, true, "Win", 11.25, 5.5, 1.25),
    key(0x5D, true, "Menu", 12.5, 5.5, 1.25),
    key(0x1D, true, "Ctrl", 13.75, 5.5, 1.25),
    key(0x4B, true, "←", 15.25, 5.5, 1.0),
    key(0x50, true, "↓", 16.25, 5.5, 1.0),
    key(0x4D, true, "→", 17.25, 5.5, 1.0),
    Key {
        w: 2.0,
        ..key(0x52, false, "0", 18.5, 5.5, 1.0)
    },
    key(0x53, false, ".", 20.5, 5.5, 1.0),
];

/// The key with this scan code and extended flag, as Windows reports it.
pub fn index_of(scan: u16, extended: bool) -> Option<usize> {
    // Windows reports a few keys differently from the scan code tables:
    // Pause comes as 0x45 without the flag and NumLock as 0x45 with it
    // (both as drawn here), and Right Shift sometimes carries the flag.
    let extended = extended && !(scan == 0x36);
    KEYS.iter()
        .position(|k| k.scan == scan && k.extended == extended)
}

/// Which keys have been pressed: one bit per key of [`KEYS`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Pressed {
    bits: u128,
}

impl Pressed {
    pub fn press(&mut self, i: usize) {
        if i < 128 {
            self.bits |= 1 << i;
        }
    }

    pub fn is_pressed(&self, i: usize) -> bool {
        i < 128 && self.bits & (1 << i) != 0
    }

    pub fn count(&self) -> usize {
        self.bits.count_ones() as usize
    }

    pub fn clear(&mut self) {
        self.bits = 0;
    }

    pub fn bits(&self) -> u128 {
        self.bits
    }

    pub fn from_bits(bits: u128) -> Self {
        Pressed { bits }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_fit_and_do_not_overlap() {
        assert!(KEYS.len() <= 128, "Pressed holds 128 keys");
        for (i, a) in KEYS.iter().enumerate() {
            assert!(a.x >= 0.0 && a.x + a.w <= WIDTH, "{}", a.label);
            assert!(a.y >= 0.0 && a.y + a.h <= HEIGHT, "{}", a.label);
            for b in &KEYS[i + 1..] {
                let apart =
                    a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y;
                assert!(apart, "{} and {} overlap", a.label, b.label);
                assert!(
                    (a.scan, a.extended) != (b.scan, b.extended),
                    "{} and {} share a scan code",
                    a.label,
                    b.label
                );
            }
        }
    }

    #[test]
    fn keys_are_found_by_what_windows_reports() {
        let label = |scan, ext| index_of(scan, ext).map(|i| KEYS[i].label);
        assert_eq!(label(0x1E, false), Some("A"));
        assert_eq!(label(0x48, true), Some("↑"));
        assert_eq!(label(0x48, false), Some("8"));
        assert_eq!(label(0x45, true), Some("Num"));
        assert_eq!(label(0x45, false), Some("Pause"));
        assert_eq!(label(0x36, true), Some("Shift"));
        assert_eq!(label(0x7F, false), None);
        // Every character key of the US layout is drawn once.
        let us: String = KEYS.iter().filter_map(|k| k.us).collect();
        assert_eq!(us.len(), 47);
    }

    #[test]
    fn pressed_keys_are_counted() {
        let mut p = Pressed::default();
        p.press(0);
        p.press(5);
        p.press(5);
        assert_eq!(p.count(), 2);
        assert!(p.is_pressed(5) && !p.is_pressed(4));
        assert_eq!(Pressed::from_bits(p.bits()), p);
        p.clear();
        assert_eq!(p.count(), 0);
    }
}
