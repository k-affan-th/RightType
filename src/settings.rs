//! Settings window — Windows only.
//!
//! A sidebar with five pages — General, Hotkeys, Learned words, Apps (per-app
//! modes and blocked apps), Privacy & about — in the shared [`ui`] look. Every
//! change applies and is saved the moment it is made (no Apply/OK), except the
//! lists (learned words, per-app modes, blocked apps), which are text boxes
//! with a Save button.
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
use crate::{config, hook, learn, overlay, safety, startup};
use righttype::per_app;
use zeroize::Zeroize;

/// The open settings window, if any.
static OPEN: AtomicIsize = AtomicIsize::new(0);

/// Client size and layout, in 96-DPI units.
const W: i32 = 760;
const H: i32 = 572;
const X0: i32 = 232;
const CW: i32 = 504;

const PAGE_GENERAL: u8 = 1;
const PAGE_HOTKEYS: u8 = 2;
const PAGE_LEARNED: u8 = 3;
const PAGE_BLOCKED: u8 = 4;
const PAGE_ABOUT: u8 = 5;
/// Sidebar entries, in page order.
const NAV: [T; 5] = [
    T::NavGeneral,
    T::NavHotkeys,
    T::NavLearned,
    T::NavBlocked,
    T::NavAbout,
];

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
    nav: [u16; 5],
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
    edit_learned: u16,
    learned_list: u16,
    learned_status: u16,
    save_learned: u16,
    clear_learned: u16,
    list: u16,
    save_list: u16,
    app_modes: u16,
    apps_status: u16,
    import_learned: u16,
    export_learned: u16,
    check_updates: u16,
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

/// Open on a given page (1 = General … 5 = Privacy & about); debug harness use.
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
    let mut nav = [0u16; 5];
    for (i, label) in NAV.iter().enumerate() {
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
    let edit_learned = s.button(
        tr(T::BtnEditLearned),
        false,
        (X0 + CW - 136, learned_y, 116, 32),
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

    // --- Learned words ----------------------------------------------------
    let l = PAGE_LEARNED;
    s.label(
        tr(T::NavLearned),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        l,
    );
    s.label(
        tr(T::LearnedIntro),
        TextStyle::Dim,
        (X0, 70, CW, 60),
        p.bg,
        l,
    );
    let learned_list = s.edit(&learn::list().join("\r\n"), (X0 + 10, 150, CW - 20, 252), l);
    let import_learned = s.button(tr(T::BtnImport), false, (X0, 428, 100, 34), p.bg, l);
    let export_learned = s.button(tr(T::BtnExport), false, (X0 + 106, 428, 100, 34), p.bg, l);
    let learned_status = s.label("", TextStyle::Small, (X0, 474, CW, 40), p.bg, l);
    let clear_learned = s.button(
        tr(T::BtnClearAll),
        false,
        (X0 + CW - 284, 428, 136, 34),
        p.bg,
        l,
    );
    let save_learned = s.button(
        tr(T::BtnSaveLearned),
        true,
        (X0 + CW - 140, 428, 140, 34),
        p.bg,
        l,
    );

    // --- Apps: per-app modes and blocked apps -------------------------------
    let b = PAGE_BLOCKED;
    s.label(
        tr(T::NavBlocked),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        b,
    );
    s.label(
        tr(T::HeadAppModes),
        TextStyle::BodyStrong,
        (X0, 66, CW, 20),
        p.bg,
        b,
    );
    s.label(
        tr(T::AppModesIntro),
        TextStyle::Dim,
        (X0, 90, CW, 44),
        p.bg,
        b,
    );
    let app_modes = s.edit(
        &per_app::format_list(&crate::apps::all()),
        (X0 + 10, 146, CW - 20, 96),
        b,
    );
    s.label(
        tr(T::HeadBlocked),
        TextStyle::BodyStrong,
        (X0, 270, CW, 20),
        p.bg,
        b,
    );
    s.label(
        tr(T::BlockedAdd),
        TextStyle::Dim,
        (X0, 294, CW, 44),
        p.bg,
        b,
    );
    let list = s.edit(
        &safety::custom_list().join("\r\n"),
        (X0 + 10, 350, CW - 20, 72),
        b,
    );
    let apps_status = s.label("", TextStyle::Small, (X0, 452, CW - 160, 36), p.bg, b);
    let save_list = s.button(
        tr(T::BtnSaveList),
        true,
        (X0 + CW - 140, 448, 140, 34),
        p.bg,
        b,
    );
    s.label(
        tr(T::BlockedAlways),
        TextStyle::Small,
        (X0, 496, CW, 40),
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
    let check_updates = s.button(
        tr(T::BtnCheckUpdates),
        false,
        (X0, about_y + 88, 190, 34),
        p.bg,
        a,
    );
    s.label(
        tr(T::AboutUpdates),
        TextStyle::Small,
        (X0 + 204, about_y + 86, CW - 204, 40),
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
        edit_learned,
        learned_list,
        learned_status,
        save_learned,
        clear_learned,
        list,
        save_list,
        app_modes,
        apps_status,
        import_learned,
        export_learned,
        check_updates,
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
    if id == ids.edit_learned {
        go_to(win, PAGE_LEARNED);
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
    } else if id == ids.clear_learned {
        learn::clear();
        s.set_text(ids.learned_list, "");
        s.set_text(ids.learned_status, "");
        overlay::show(tr(T::ToastLearnedCleared));
    } else if id == ids.save_learned {
        let lines: Vec<String> = s
            .text_of(ids.learned_list)
            .lines()
            .map(str::to_string)
            .collect();
        let result = learn::replace(&lines);
        // Show the list as it was kept: sorted, deduplicated, invalid lines gone.
        s.set_text(ids.learned_list, &learn::list().join("\r\n"));
        let n = result.kept.to_string();
        let status = if result.skipped == 0 {
            trf(T::LearnedSaved, &[("n", &n)])
        } else {
            trf(
                T::LearnedSkipped,
                &[("n", &n), ("k", &result.skipped.to_string())],
            )
        };
        s.set_text(ids.learned_status, &status);
        overlay::show(tr(T::ToastSaved));
    } else if id == ids.save_list {
        let entries: Vec<String> = s
            .text_of(ids.list)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_lowercase)
            .collect();
        safety::set_custom_list(entries);
        let (modes, skipped) = per_app::parse_list(&s.text_of(ids.app_modes));
        crate::apps::set_all(modes);
        // Show the list as it was kept: normalised, sorted, bad lines gone.
        s.set_text(ids.app_modes, &per_app::format_list(&crate::apps::all()));
        s.set_text(
            ids.apps_status,
            &if skipped == 0 {
                tr(T::AppsSaved).to_string()
            } else {
                trf(T::AppsSkipped, &[("k", &skipped.to_string())])
            },
        );
        config::persist();
        overlay::show(tr(T::ToastSaved));
    } else if id == ids.import_learned {
        import_learned(win);
    } else if id == ids.export_learned {
        export_learned(win);
    } else if id == ids.check_updates {
        open_releases_page();
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

/// Where new versions are published. Opened in the browser: RightType itself
/// never connects to a network.
const RELEASES_URL: &str = "https://github.com/k-affan-th/RightType/releases/latest";

fn open_releases_page() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let url: Vec<u16> = format!("{RELEASES_URL}\0").encode_utf16().collect();
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(url.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Learned-word files larger than this are refused (a word list is small).
const MAX_IMPORT_BYTES: u64 = 1 << 20;

/// Ask for a `.txt` file to open (`save` = false) or write.
fn pick_file(win: &SettingsWindow, save: bool) -> Option<std::path::PathBuf> {
    let mut dialog = nwg::FileDialog::default();
    let filters = format!("{}|All files (*.*)", tr(T::FileFilter));
    nwg::FileDialog::builder()
        .action(if save {
            nwg::FileDialogAction::Save
        } else {
            nwg::FileDialogAction::Open
        })
        .filters(filters.as_str())
        .build(&mut dialog)
        .ok()?;
    if !dialog.run(Some(&win.window)) {
        return None;
    }
    let mut path = std::path::PathBuf::from(dialog.get_selected_item().ok()?);
    if save && path.extension().is_none() {
        path.set_extension("txt");
    }
    Some(path)
}

/// Add the words in a file to the learned list (nothing already there is
/// removed). The words pass the same checks as ones typed in the editor.
fn import_learned(win: &SettingsWindow) {
    let Some(path) = pick_file(win, false) else {
        return;
    };
    let text = std::fs::metadata(&path)
        .ok()
        .filter(|m| m.len() <= MAX_IMPORT_BYTES)
        .and_then(|_| std::fs::read_to_string(&path).ok());
    let s = &win.surface;
    let Some(mut text) = text else {
        s.set_text(win.ids.learned_status, tr(T::ErrFile));
        return;
    };
    let before = learn::count();
    let mut lines = learn::list();
    lines.extend(text.lines().map(str::to_string));
    text.zeroize();
    let result = learn::replace(&lines);
    let added = result.kept.saturating_sub(before);
    s.set_text(win.ids.learned_list, &learn::list().join("\r\n"));
    s.set_text(
        win.ids.learned_status,
        &trf(T::LearnedImported, &[("n", &added.to_string())]),
    );
    overlay::show(tr(T::ToastSaved));
}

/// Write the learned list to a file of the user's choosing, one word per line.
fn export_learned(win: &SettingsWindow) {
    let Some(path) = pick_file(win, true) else {
        return;
    };
    let words = learn::list();
    let mut text = words.join("\r\n");
    text.push_str("\r\n");
    let ok = std::fs::write(&path, &text).is_ok();
    text.zeroize();
    win.surface.set_text(
        win.ids.learned_status,
        &if ok {
            trf(T::LearnedExported, &[("n", &words.len().to_string())])
        } else {
            tr(T::ErrFile).to_string()
        },
    );
}

/// Switch page as if its sidebar entry had been clicked.
fn go_to(win: &SettingsWindow, page: u8) {
    for (i, nav) in win.ids.nav.iter().enumerate() {
        win.surface.set_checked(*nav, i as u8 + 1 == page);
    }
    win.surface.show_page(page);
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
        PAGE_LEARNED => {
            field(g, rect(X0, 142, CW, 268));
        }
        PAGE_BLOCKED => {
            field(g, rect(X0, 138, CW, 112));
            field(g, rect(X0, 342, CW, 88));
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
