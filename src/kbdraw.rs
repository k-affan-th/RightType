//! Drawing a full keyboard ([`righttype::keyboard::KEYS`]) — Windows only.
//! Shared by cleaning, the key tester and typing practice: each says how
//! each key looks.

use righttype::keyboard::{self, KEYS};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, DT_CENTER, DT_SINGLELINE, DT_VCENTER, HDC, HGDIOBJ,
};

use crate::ui::{self, pal, Gfx, Rgb};

/// How one key is drawn.
pub struct Look {
    pub fill: Rgb,
    pub ink: Rgb,
    /// The big label (`None`: the key's own, `Space` for the space bar).
    pub label: Option<String>,
    /// A small character in the bottom-right corner (what it types).
    pub corner: Option<String>,
}

impl Look {
    /// A plain key cap.
    pub fn plain() -> Look {
        Look {
            fill: pal().keycap,
            ink: pal().text,
            label: None,
            corner: None,
        }
    }

    pub fn lit(fill: Rgb) -> Look {
        Look {
            fill,
            ink: pal().on_accent,
            label: None,
            corner: None,
        }
    }
}

/// The drawing's size for a key `unit` (96-DPI units).
pub fn size(unit: i32) -> (i32, i32) {
    (
        (keyboard::WIDTH * unit as f32) as i32,
        (keyboard::HEIGHT * unit as f32) as i32,
    )
}

/// Draw the keyboard with its top-left at (`x0`, `y0`) (96-DPI units), each
/// key looking as `look` says.
pub fn draw(g: &Gfx, hdc: HDC, x0: i32, y0: i32, unit: i32, look: impl Fn(usize) -> Look) {
    let p = pal();
    let cap = ui::make_font(12, 600);
    let small = ui::make_font(10, 400);
    for (i, k) in KEYS.iter().enumerate() {
        let r = ui::rect(
            x0 + (k.x * unit as f32) as i32 + 2,
            y0 + (k.y * unit as f32) as i32 + 2,
            (k.w * unit as f32) as i32 - 4,
            (k.h * unit as f32) as i32 - 4,
        );
        let l = look(i);
        let edge = if l.fill == p.keycap || l.fill == p.inset {
            p.keycap_border
        } else {
            l.fill
        };
        g.fill_round(r, ui::px(5) as f32, edge);
        g.fill_round(ui::inset(r, ui::px(1)), ui::px(4) as f32, l.fill);
        let label = l
            .label
            .unwrap_or_else(|| if k.label.is_empty() { "Space" } else { k.label }.to_string());
        let font = if label.chars().count() > 3 {
            small
        } else {
            cap
        };
        ui::text(
            hdc,
            &label,
            r,
            font,
            l.ink,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        if let Some(corner) = l.corner {
            let rc = RECT {
                left: r.right - ui::px(16),
                top: r.bottom - ui::px(16),
                right: r.right - ui::px(2),
                bottom: r.bottom - ui::px(1),
            };
            let dim = if l.ink == p.text { p.text_dim } else { l.ink };
            ui::text(hdc, &corner, rc, small, dim, DT_CENTER | DT_SINGLELINE);
        }
    }
    unsafe {
        for f in [cap, small] {
            let _ = DeleteObject(HGDIOBJ(f.0));
        }
    }
}

/// The Thai character a key types on the keyboard in use, as drawn in a
/// corner (`None` when it types its own character).
pub fn thai_corner(us: Option<char>) -> Option<String> {
    let c = us?;
    let (typed, shown) = crate::keymap::thai_of(c);
    (typed != c.to_string()).then_some(shown)
}
