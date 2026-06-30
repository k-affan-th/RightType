//! US English QWERTY layout — the canonical reference layout.
//!
//! Because we identify physical keys *by* the US QWERTY character they produce,
//! this layout is the identity map for every printable ASCII character.

use super::{Layout, LayoutId};

/// US English QWERTY.
pub struct QwertyEn;

impl QwertyEn {
    pub fn new() -> Self {
        QwertyEn
    }
}

impl Default for QwertyEn {
    fn default() -> Self {
        Self::new()
    }
}

impl Layout for QwertyEn {
    fn id(&self) -> LayoutId {
        LayoutId::QwertyEn
    }

    fn key_of(&self, ch: char) -> Option<char> {
        // Any printable ASCII character *is* its own canonical key.
        if ch.is_ascii_graphic() {
            Some(ch)
        } else {
            None
        }
    }

    fn char_of(&self, key: char) -> Option<char> {
        if key.is_ascii_graphic() {
            Some(key)
        } else {
            None
        }
    }
}
