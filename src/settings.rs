//! Settings window — Windows only.
//!
//! A sidebar with four pages — General, Hotkeys, Blocked apps, Privacy &
//! about — in the shared [`ui`] look. Every change applies and is saved the
//! moment it is made (no Apply/OK), except the blocked-apps list, which is a
//! text box and has its own Save button.
//!
//! Only one settings window exists at a time: opening it again brings the
//! existing one forward. The built-in app blacklist is described but not
//! editable — only the user's *additional* list is, so a settings bug can never
//! weaken the safety baseline.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};

use native_windows_gui as nwg;
use righttype::i18n::{self, tr, trf, Lang, T};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};

use crate::ui::{self, card, divider, field, pal, rect, track, Gfx, Surface, TextStyle};
use crate::{config, hook, learn, safety, startup, toast};

/// The open settings window, if any.
static OPEN: AtomicIsize = AtomicIsize::new(0);

/// Client size and layout, in 96-DPI units.
const W: i32 = 760;
const H: i32 = 572;
const X0: i32 = 232;
const CW: i32 = 504;

const PAGE_GENERAL: u8 = 1;
const PAGE_HOTKEYS: u8 = 2;
const PAGE_BLOCKED: u8 = 3;
const PAGE_ABOUT: u8 = 4;

/// Toggle rows on the General page: y of each row inside the behaviour card.
const ROW_H: i32 = 64;
const CARD_B_Y: i32 = 282;

pub struct Hotkey {
    pub action: T,
    pub keys: &'static str,
}

/// The fixed v1 hotkeys, shared with the Welcome window.
pub const HOTKEYS: &[Hotkey] = &[
    Hotkey {
        action: T::HkFlip,
        keys: "Shift + Backspace",
    },
    Hotkey {
        action: T::HkSelection,
        keys: "Shift + CapsLock",
    },
    Hotkey {
        action: T::HkCycle,
        keys: "Ctrl + CapsLock",
    },
    Hotkey {
        action: T::HkUndo,
        keys: "Ctrl + Shift + CapsLock",
    },
    Hotkey {
        action: T::HkAccept,
        keys: "Alt + CapsLock",
    },
    Hotkey {
        action: T::HkPanic,
        keys: "Ctrl + Alt + CapsLock",
    },
];

struct Ids {
    nav: [u16; 4],
    lang_en: u16,
    lang_th: u16,
    mode_auto: u16,
    mode_suggest: u16,
    mode_manual: u16,
    mode_desc: u16,
    enabled: u16,
    startup: u16,
    learn: u16,
    learned: u16,
    clear: u16,
    list: u16,
    save_list: u16,
}

struct SettingsWindow {
    window: nwg::Window,
    surface: Rc<Surface>,
    ids: Ids,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open the settings window, or bring the open one forward.
pub fn open() {
    open_on(PAGE_GENERAL);
}

/// Open on a given page (1 = General … 4 = Privacy & about); debug harness use.
#[cfg(debug_assertions)]
pub fn open_page(page: u8) {
    open_on(page.clamp(PAGE_GENERAL, PAGE_ABOUT));
}

fn open_on(page: u8) {
    let existing = OPEN.load(Ordering::Acquire);
    if existing != 0 {
        unsafe {
            let hwnd = HWND(existing as *mut _);
            let _ = ShowWindow(hwnd, SW_RESTORE);
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    ui::refresh();

    let mut window = nwg::Window::default();
    let _ = nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((W, H))
        .title(tr(T::SettingsTitle))
        .build(&mut window);

    let surface = Surface::attach(&window, 0x5254_0010, Box::new(paint));
    let s = &surface;
    let p = pal();

    // --- sidebar (every page) ---------------------------------------------
    s.label("RightType", TextStyle::Subtitle, (24, 22, 180, 26), p.bg, 0);
    s.label(
        &format!("v{}", env!("CARGO_PKG_VERSION")),
        TextStyle::Small,
        (24, 48, 180, 18),
        p.bg,
        0,
    );
    let nav_labels = [T::NavGeneral, T::NavHotkeys, T::NavBlocked, T::NavAbout];
    let mut nav = [0u16; 4];
    for (i, label) in nav_labels.iter().enumerate() {
        nav[i] = s.nav(tr(*label), i == 0, (12, 84 + i as i32 * 40, 196, 36));
    }
    s.label(
        tr(T::HeadLanguage),
        TextStyle::Small,
        (24, H - 92, 180, 18),
        p.bg,
        0,
    );
    let lang_en = s.segment("English", true, (20, H - 66, 90, 32), p.inset, 0);
    let lang_th = s.segment("ไทย", false, (110, H - 66, 90, 32), p.inset, 0);

    // --- General ----------------------------------------------------------
    let g = PAGE_GENERAL;
    s.label(
        tr(T::NavGeneral),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        g,
    );
    s.label(
        tr(T::HeadMode),
        TextStyle::BodyStrong,
        (X0, 70, CW, 20),
        p.bg,
        g,
    );
    let seg_w = (CW - 32 - 8) / 3;
    let mode_auto = s.segment(tr(T::ModeAuto), true, (X0 + 20, 114, seg_w, 40), p.inset, g);
    let mode_suggest = s.segment(
        tr(T::ModeSuggest),
        false,
        (X0 + 20 + seg_w, 114, seg_w, 40),
        p.inset,
        g,
    );
    let mode_manual = s.segment(
        tr(T::ModeManual),
        false,
        (X0 + 20 + seg_w * 2, 114, seg_w, 40),
        p.inset,
        g,
    );
    let mode_desc = s.label(
        "",
        TextStyle::Dim,
        (X0 + 20, 164, CW - 40, 54),
        p.surface,
        g,
    );

    s.label(
        tr(T::HeadBehaviour),
        TextStyle::BodyStrong,
        (X0, CARD_B_Y - 26, CW, 20),
        p.bg,
        g,
    );
    let row = |i: i32| (X0 + 4, CARD_B_Y + 4 + i * ROW_H, CW - 8, ROW_H - 4);
    let enabled = s.toggle(tr(T::RowEnabled), tr(T::SubEnabled), row(0), p.surface, g);
    let startup = s.toggle(tr(T::RowStartup), tr(T::SubStartup), row(1), p.surface, g);
    let learn = s.toggle(tr(T::RowLearn), tr(T::SubLearn), row(2), p.surface, g);
    let learned_y = CARD_B_Y + 3 * ROW_H + 14;
    let learned = s.label(
        "",
        TextStyle::Dim,
        (X0 + 20, learned_y + 6, CW - 160, 20),
        p.surface,
        g,
    );
    let clear = s.button(
        tr(T::BtnClearLearned),
        false,
        (X0 + CW - 116, learned_y, 96, 32),
        p.surface,
        g,
    );

    // --- Hotkeys ----------------------------------------------------------
    let h = PAGE_HOTKEYS;
    s.label(
        tr(T::NavHotkeys),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        h,
    );
    for (i, hk) in HOTKEYS.iter().enumerate() {
        let y = 74 + i as i32 * 52;
        s.label(
            tr(hk.action),
            TextStyle::Body,
            (X0 + 20, y + 14, 240, 24),
            p.surface,
            h,
        );
        s.label(
            hk.keys,
            TextStyle::Keys,
            (X0 + CW - 20 - 250, y + 12, 250, 30),
            p.surface,
            h,
        );
    }
    let note_y = 74 + HOTKEYS.len() as i32 * 52 + 16;
    s.label(
        tr(T::NoteHotkeysFixed),
        TextStyle::Small,
        (X0, note_y, CW, 36),
        p.bg,
        h,
    );

    // --- Blocked apps -----------------------------------------------------
    let b = PAGE_BLOCKED;
    s.label(
        tr(T::NavBlocked),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        b,
    );
    s.label(
        tr(T::BlockedAlways),
        TextStyle::Dim,
        (X0, 70, CW, 56),
        p.bg,
        b,
    );
    s.label(
        tr(T::BlockedAdd),
        TextStyle::Body,
        (X0, 134, CW, 56),
        p.bg,
        b,
    );
    let list = s.edit(
        &safety::custom_list().join("\r\n"),
        (X0 + 10, 206, CW - 20, 196),
        b,
    );
    let save_list = s.button(
        tr(T::BtnSaveList),
        true,
        (X0 + CW - 140, 428, 140, 34),
        p.bg,
        b,
    );

    // --- Privacy & about --------------------------------------------------
    let a = PAGE_ABOUT;
    s.label(tr(T::NavAbout), TextStyle::Title, (X0, 18, CW, 36), p.bg, a);
    for (i, line) in [T::Privacy1, T::Privacy2, T::Privacy3, T::Privacy4]
        .iter()
        .enumerate()
    {
        let y = 74 + i as i32 * 56;
        s.label(
            tr(*line),
            TextStyle::Body,
            (X0 + 52, y + 8, CW - 72, 42),
            p.surface,
            a,
        );
    }
    let about_y = 74 + 4 * 56 + 28;
    s.label(
        tr(T::HeadAbout),
        TextStyle::BodyStrong,
        (X0, about_y, CW, 20),
        p.bg,
        a,
    );
    s.label(
        &trf(T::AboutVersion, &[("v", env!("CARGO_PKG_VERSION"))]),
        TextStyle::Body,
        (X0, about_y + 28, CW, 22),
        p.bg,
        a,
    );
    s.label(
        tr(T::AboutLicense),
        TextStyle::Dim,
        (X0, about_y + 52, CW, 22),
        p.bg,
        a,
    );

    let ids = Ids {
        nav,
        lang_en,
        lang_th,
        mode_auto,
        mode_suggest,
        mode_manual,
        mode_desc,
        enabled,
        startup,
        learn,
        learned,
        clear,
        list,
        save_list,
    };

    ui::size_and_center(surface.hwnd, W, H);
    let hwnd_isize = surface.hwnd.0 as isize;
    let win = Rc::new(SettingsWindow {
        window,
        surface,
        ids,
        handler: RefCell::new(None),
    });
    sync(&win);
    win.surface
        .set_checked(win.ids.nav[(page - 1) as usize], true);
    win.surface.show_page(page);
    win.window.set_visible(true);
    OPEN.store(hwnd_isize, Ordering::Release);

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if let Some(win) = weak.upgrade() {
            clicked(&win, id);
        }
    });
    let win_h = win.clone();
    let handler = nwg::full_bind_event_handler(&win.window.handle, move |evt, _data, handle| {
        if matches!(evt, nwg::Event::OnWindowClose) && handle == win_h.window.handle {
            finish(&win_h);
        }
    });
    *win.handler.borrow_mut() = Some(handler);
}

/// Reflect the running state in every control.
fn sync(win: &SettingsWindow) {
    let s = &win.surface;
    let ids = &win.ids;
    let mode = hook::mode();
    s.set_checked(ids.mode_auto, mode == hook::Mode::Auto);
    s.set_checked(ids.mode_suggest, mode == hook::Mode::Suggest);
    s.set_checked(ids.mode_manual, mode == hook::Mode::Manual);
    s.set_text(
        ids.mode_desc,
        tr(match mode {
            hook::Mode::Auto => T::DescAuto,
            hook::Mode::Suggest => T::DescSuggest,
            hook::Mode::Manual => T::DescManual,
        }),
    );
    s.set_checked(ids.enabled, hook::is_enabled());
    s.set_checked(ids.startup, startup::is_enabled());
    s.set_checked(ids.learn, learn::is_enabled());
    s.set_text(
        ids.learned,
        &trf(T::LearnedCount, &[("n", &learn::count().to_string())]),
    );
    let lang = i18n::lang();
    s.set_checked(ids.lang_en, lang == Lang::En);
    s.set_checked(ids.lang_th, lang == Lang::Th);
}

fn clicked(win: &Rc<SettingsWindow>, id: u16) {
    let ids = &win.ids;
    let s = &win.surface;
    if let Some(i) = ids.nav.iter().position(|&n| n == id) {
        s.show_page(i as u8 + 1);
        return;
    }
    let mode = if id == ids.mode_auto {
        Some(hook::Mode::Auto)
    } else if id == ids.mode_suggest {
        Some(hook::Mode::Suggest)
    } else if id == ids.mode_manual {
        Some(hook::Mode::Manual)
    } else {
        None
    };
    if let Some(mode) = mode {
        hook::set_mode(mode);
        config::persist();
    } else if id == ids.enabled {
        hook::set_enabled(s.checked(ids.enabled));
        config::persist();
    } else if id == ids.startup {
        startup::set_enabled(s.checked(ids.startup));
    } else if id == ids.learn {
        learn::set_enabled(s.checked(ids.learn));
        config::persist();
    } else if id == ids.clear {
        learn::clear();
        toast::show(tr(T::ToastLearnedCleared));
    } else if id == ids.save_list {
        let entries: Vec<String> = s
            .text_of(ids.list)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_lowercase)
            .collect();
        safety::set_custom_list(entries);
        config::persist();
        toast::show(tr(T::ToastSaved));
    } else if id == ids.lang_en || id == ids.lang_th {
        let lang = if id == ids.lang_th {
            Lang::Th
        } else {
            Lang::En
        };
        if lang != i18n::lang() {
            config::set_language(Some(lang));
            config::persist();
            // Every string in the window changes: rebuild it on the same page.
            let page = s.page();
            finish(win);
            open_on(page);
            return;
        }
    }
    sync(win);
}

fn finish(win: &Rc<SettingsWindow>) {
    let my = win.surface.hwnd.0 as isize;
    let _ = OPEN.compare_exchange(my, 0, Ordering::AcqRel, Ordering::Acquire);
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    win.window.close();
}

/// Cards and decoration behind the controls of `page`.
fn paint(g: &Gfx, hdc: HDC, _client: windows::Win32::Foundation::RECT, page: u8) {
    // Sidebar language picker track.
    track(g, rect(16, H - 70, 188, 40));
    match page {
        PAGE_GENERAL => {
            card(g, rect(X0, 98, CW, 132));
            track(g, rect(X0 + 16, 110, CW - 32, 48));
            card(g, rect(X0, CARD_B_Y, CW, 3 * ROW_H + 64));
            for i in 1..=3 {
                divider(hdc, X0 + 16, CARD_B_Y + i * ROW_H + 2, CW - 32);
            }
        }
        PAGE_HOTKEYS => {
            card(g, rect(X0, 68, CW, HOTKEYS.len() as i32 * 52 + 8));
            for i in 1..HOTKEYS.len() as i32 {
                divider(hdc, X0 + 16, 74 + i * 52 - 1, CW - 32);
            }
        }
        PAGE_BLOCKED => {
            field(g, rect(X0, 198, CW, 212));
        }
        PAGE_ABOUT => {
            card(g, rect(X0, 68, CW, 4 * 56 + 8));
            for i in 0..4 {
                let y = 74 + i * 56;
                // A check mark in an accent circle, one per promise.
                let c = rect(X0 + 16, y + 10, 22, 22);
                g.fill_round(c, ui::px(11) as f32, pal().accent);
                ui::glyph(hdc, "✓", c, pal().on_accent);
                if i > 0 {
                    divider(hdc, X0 + 16, y - 2, CW - 32);
                }
            }
        }
        _ => {}
    }
}
