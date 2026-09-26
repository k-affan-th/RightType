//! First-run welcome + anytime hotkeys help — Windows only.
//!
//! One small window, and the one place RightType asks for attention: what it
//! does (with a live example), the hotkeys, the current mode, and a single
//! button. Shown automatically on first launch (config `onboarded`) and
//! reopenable from the tray menu, where it lists every hotkey.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::HDC;

use crate::settings::HOTKEYS;
use crate::ui::{self, card, divider, pal, rect, Gfx, Surface, TextStyle};
use crate::{config, hook};

const W: i32 = 560;
const X: i32 = 32;
const CW: i32 = W - 2 * X;
const ROW: i32 = 46;

struct Welcome {
    window: nwg::Window,
    surface: Rc<Surface>,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Vertical layout shared by the controls and the background painter.
struct Layout {
    example_y: Option<i32>,
    keys_y: i32,
    rows: usize,
    height: i32,
}

fn layout(first_run: bool) -> Layout {
    if first_run {
        let example_y = 150;
        let keys_y = example_y + 72 + 44;
        let rows = 4;
        Layout {
            example_y: Some(example_y),
            keys_y,
            rows,
            height: keys_y + rows as i32 * ROW + 8 + 112,
        }
    } else {
        let keys_y = 84;
        let rows = HOTKEYS.len();
        Layout {
            example_y: None,
            keys_y,
            rows,
            height: keys_y + rows as i32 * ROW + 8 + 112,
        }
    }
}

/// Show the welcome/help window. `first_run` picks the introduction; closing
/// it then records that onboarding is done.
pub fn show(first_run: bool) {
    ui::refresh();
    let lay = layout(first_run);
    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, lay.height))
        .title(tr(if first_run {
            T::WelcomeTitle
        } else {
            T::HelpTitle
        }))
        .topmost(true)
        .build(&mut window);

    let paint_layout = layout(first_run);
    let surface = Surface::attach(
        &window,
        0x5254_0011,
        Box::new(move |g, hdc, rc, _| paint(g, hdc, rc, &paint_layout)),
    );
    let s = &surface;
    let p = pal();

    if let Some(example_y) = lay.example_y {
        s.label(
            tr(T::WelcomeHeadline),
            TextStyle::Title,
            (X, 26, CW, 44),
            p.bg,
            0,
        );
        s.label(tr(T::WelcomeSub), TextStyle::Dim, (X, 80, CW, 60), p.bg, 0);
        s.label(
            tr(T::WelcomeExample),
            TextStyle::Subtitle,
            (X + 24, example_y + 20, CW - 48, 32),
            p.surface,
            0,
        );
        s.label(
            tr(T::HeadHotkeys),
            TextStyle::BodyStrong,
            (X, lay.keys_y - 30, CW, 20),
            p.bg,
            0,
        );
    } else {
        s.label(
            tr(T::HelpHeadline),
            TextStyle::Title,
            (X, 26, CW, 40),
            p.bg,
            0,
        );
    }
    for (i, hk) in HOTKEYS.iter().take(lay.rows).enumerate() {
        let y = lay.keys_y + 4 + i as i32 * ROW;
        s.label(
            tr(hk.action),
            TextStyle::Body,
            (X + 20, y + 12, 230, 22),
            p.surface,
            0,
        );
        s.label(
            hk.keys,
            TextStyle::Keys,
            (X + CW - 20 - 240, y + 8, 240, 30),
            p.surface,
            0,
        );
    }
    let foot_y = lay.keys_y + lay.rows as i32 * ROW + 8 + 20;
    let mode = tr(match hook::mode() {
        hook::Mode::Auto => T::ModeAuto,
        hook::Mode::Suggest => T::ModeSuggest,
        hook::Mode::Manual => T::ModeManual,
    });
    s.label(
        &trf(T::WelcomeMode, &[("mode", mode)]),
        TextStyle::Dim,
        (X, foot_y, CW - 170, 50),
        p.bg,
        0,
    );
    let start = s.button(
        tr(if first_run {
            T::BtnGetStarted
        } else {
            T::BtnClose
        }),
        true,
        (X + CW - 150, foot_y + 4, 150, 36),
        p.bg,
        0,
    );

    ui::size_and_center(surface.hwnd, W, lay.height);
    let win = Rc::new(Welcome {
        window,
        surface,
        handler: RefCell::new(None),
    });
    win.window.set_visible(true);

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if id == start {
            if let Some(win) = weak.upgrade() {
                finish(&win, first_run);
            }
        }
    });
    let win_h = win.clone();
    let handler = nwg::full_bind_event_handler(&win.window.handle, move |evt, _data, handle| {
        if matches!(evt, nwg::Event::OnWindowClose) && handle == win_h.window.handle {
            finish(&win_h, first_run);
        }
    });
    *win.handler.borrow_mut() = Some(handler);
}

fn finish(win: &Rc<Welcome>, first_run: bool) {
    if first_run {
        config::mark_onboarded();
    }
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    win.window.close();
}

fn paint(g: &Gfx, hdc: HDC, _rc: RECT, lay: &Layout) {
    if let Some(example_y) = lay.example_y {
        card(g, rect(X, example_y, CW, 72));
        // Accent edge: this card is the product in one line.
        g.fill_round(rect(X, example_y, 4, 72), ui::px(2) as f32, pal().accent);
    }
    card(g, rect(X, lay.keys_y, CW, lay.rows as i32 * ROW + 8));
    for i in 1..lay.rows as i32 {
        divider(hdc, X + 16, lay.keys_y + 4 + i * ROW - 1, CW - 32);
    }
}
