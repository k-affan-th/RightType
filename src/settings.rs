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
use righttype::hotkeys::{Action, Hotkeys, Refusal};
use righttype::per_app::AppMode;
use zeroize::Zeroize;

/// The open settings window, if any.
static OPEN: AtomicIsize = AtomicIsize::new(0);

/// Client size and layout, in 96-DPI units.
const W: i32 = 760;
const H: i32 = 700;
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
/// The per-field language card on the Apps page.
const PREDICT_Y: i32 = 548;
/// The restart-after-a-crash card on the Privacy & about page.
const RESTART_Y: i32 = 486;
/// The Thai keyboard picker on the Hotkeys page.
const KEYBOARD_Y: i32 = 516;

/// The label of each hotkey action.
pub fn action_label(action: Action) -> T {
    match action {
        Action::Flip => T::HkFlip,
        Action::Selection => T::HkSelection,
        Action::Cycle => T::HkCycle,
        Action::Undo => T::HkUndo,
        Action::Accept => T::HkAccept,
        Action::Panic => T::HkPanic,
        Action::Palette => T::HkPalette,
    }
}

/// Every hotkey as `(label, keys)`, shared with the Welcome window.
pub fn hotkey_rows() -> Vec<(T, String)> {
    let keys = hook::hotkeys();
    Action::ALL
        .into_iter()
        .map(|a| (action_label(a), keys.chord(a).format()))
        .collect()
}

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
    caret_hints: u16,
    spelling: u16,
    learned: u16,
    edit_learned: u16,
    learned_list: u16,
    learned_status: u16,
    /// Per action (in `Action::ALL` order): its keys label and Change button.
    key_labels: Vec<u16>,
    key_buttons: Vec<u16>,
    keys_status: u16,
    keys_reset: u16,
    kedmanee: u16,
    pattachote: u16,
    save_learned: u16,
    clear_learned: u16,
    apps_table: u16,
    /// Mode buttons for the selected app, in [`APP_MODE_CHOICES`] order.
    app_seg: [u16; 5],
    apps_add: u16,
    apps_keep: u16,
    apps_remove: u16,
    apps_status: u16,
    restart: u16,
    import_learned: u16,
    export_learned: u16,
    check_updates: u16,
    predict: u16,
    clear_habits: u16,
    folder_label: u16,
    sync_folder: u16,
    this_pc: u16,
}

/// The modes offered for an app on the Apps page, in button order.
const APP_MODE_CHOICES: [AppMode; 5] = [
    AppMode::Auto,
    AppMode::Suggest,
    AppMode::Manual,
    AppMode::Code,
    AppMode::Off,
];

/// Where an Apps-page row's mode comes from.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum RowSource {
    ForNow,
    Chosen,
    Blocked,
    Default,
    Safety,
}

/// One row of the Apps table.
#[derive(Clone)]
struct AppRow {
    exe: String,
    /// `None` for a blocked app (off, and hotkeys too).
    mode: Option<AppMode>,
    source: RowSource,
}

struct SettingsWindow {
    window: nwg::Window,
    surface: Rc<Surface>,
    ids: Ids,
    /// The Apps table's rows, as shown.
    apps: RefCell<Vec<AppRow>>,
    /// Where each running program lives, read when the window opened.
    paths: RefCell<std::collections::HashMap<String, String>>,
    handler: RefCell<Option<nwg::EventHandler>>,
    raw: RefCell<Option<nwg::RawEventHandler>>,
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
    let started = std::time::Instant::now();
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
    let caret_hints = s.toggle(tr(T::RowCaret), tr(T::SubCaret), row(3), p.surface, g);
    let spelling = s.toggle(tr(T::RowSpelling), tr(T::SubSpelling), row(4), p.surface, g);
    let learned_y = CARD_B_Y + 5 * ROW_H + 14;
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
    let mut key_labels = Vec::new();
    let mut key_buttons = Vec::new();
    for (i, (label, keys)) in hotkey_rows().into_iter().enumerate() {
        let y = 74 + i as i32 * 52;
        key_labels.push(s.label(
            &format!("{}\t{keys}", tr(label)),
            TextStyle::Keys,
            (X0 + 20, y + 12, CW - 40 - 96, 30),
            p.surface,
            h,
        ));
        key_buttons.push(s.button(
            tr(T::BtnChange),
            false,
            (X0 + CW - 20 - 88, y + 10, 88, 32),
            p.surface,
            h,
        ));
    }
    let note_y = 74 + Action::ALL.len() as i32 * 52 + 16;
    let keys_status = s.label(
        tr(T::NoteHotkeys),
        TextStyle::Small,
        (X0, note_y, CW - 170, 44),
        p.bg,
        h,
    );
    let keys_reset = s.button(
        tr(T::BtnResetKeys),
        false,
        (X0 + CW - 150, note_y, 150, 34),
        p.bg,
        h,
    );
    s.label(
        tr(T::HeadThaiKeyboard),
        TextStyle::BodyStrong,
        (X0, KEYBOARD_Y + 6, 200, 22),
        p.bg,
        h,
    );
    let kedmanee = s.segment(
        "Kedmanee",
        true,
        (X0 + CW - 20 - 2 * 130, KEYBOARD_Y + 4, 130, 32),
        p.inset,
        h,
    );
    let pattachote = s.segment(
        "Pattachote",
        false,
        (X0 + CW - 20 - 130, KEYBOARD_Y + 4, 130, 32),
        p.inset,
        h,
    );
    s.label(
        tr(T::NoteKeyboards),
        TextStyle::Small,
        (X0, KEYBOARD_Y + 46, CW, 22),
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
    let learned_status = s.label("", TextStyle::Small, (X0, 470, CW, 36), p.bg, l);
    let folder_label = s.label("", TextStyle::Small, (X0, 526, CW - 290, 40), p.bg, l);
    let sync_folder = s.button(
        tr(T::BtnSyncFolder),
        false,
        (X0 + CW - 284, 520, 150, 34),
        p.bg,
        l,
    );
    let this_pc = s.button(
        tr(T::BtnThisPc),
        false,
        (X0 + CW - 126, 520, 126, 34),
        p.bg,
        l,
    );
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

    // --- Apps: a table of every app with a mode of its own ------------------
    let b = PAGE_BLOCKED;
    s.label(
        tr(T::NavBlocked),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        b,
    );
    s.label(tr(T::AppsIntro), TextStyle::Dim, (X0, 58, CW, 40), p.bg, b);
    let apps_table = s.table(
        &[
            (tr(T::ColApp), 150),
            (tr(T::ColMode), 104),
            (tr(T::ColSetBy), 120),
            (tr(T::ColWhere), 300),
        ],
        (X0, 102, CW, 232),
        b,
    );
    let seg_w = CW / 5;
    let mut app_seg = [0u16; 5];
    for (i, mode) in APP_MODE_CHOICES.iter().enumerate() {
        app_seg[i] = s.segment(
            tr(crate::tray::app_mode_name(*mode)),
            i == 0,
            (X0 + 4 + i as i32 * seg_w, 346, seg_w - 2, 32),
            p.inset,
            b,
        );
    }
    let apps_add = s.button(tr(T::BtnAddApp), false, (X0, 392, 210, 34), p.bg, b);
    let apps_keep = s.button(tr(T::BtnKeepMode), false, (X0 + 216, 392, 150, 34), p.bg, b);
    let apps_remove = s.button(
        tr(T::BtnRemoveApp),
        false,
        (X0 + CW - 120, 392, 120, 34),
        p.bg,
        b,
    );
    let apps_status = s.label("", TextStyle::Small, (X0, 432, CW, 22), p.bg, b);
    s.label(
        tr(T::BlockedAlways),
        TextStyle::Small,
        (X0, 458, CW, 40),
        p.bg,
        b,
    );
    let predict = s.toggle(
        tr(T::RowPredict),
        tr(T::SubPredict),
        (X0 + 4, PREDICT_Y + 4, CW - 136, 68),
        p.surface,
        b,
    );
    let clear_habits = s.button(
        tr(T::BtnClearHabits),
        false,
        (X0 + CW - 116, PREDICT_Y + 21, 100, 34),
        p.surface,
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
    let restart = s.toggle(
        tr(T::RowRestart),
        tr(T::SubRestart),
        (X0 + 4, RESTART_Y + 4, CW - 8, 64),
        p.surface,
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
        caret_hints,
        spelling,
        learned,
        edit_learned,
        learned_list,
        learned_status,
        key_labels,
        key_buttons,
        keys_status,
        keys_reset,
        kedmanee,
        pattachote,
        save_learned,
        clear_learned,
        apps_table,
        app_seg,
        apps_add,
        apps_keep,
        apps_remove,
        apps_status,
        restart,
        import_learned,
        export_learned,
        check_updates,
        predict,
        clear_habits,
        folder_label,
        sync_folder,
        this_pc,
    };

    ui::size_and_center(surface.hwnd, W, H);
    let hwnd_isize = surface.hwnd.0 as isize;
    let win = Rc::new(SettingsWindow {
        window,
        surface,
        ids,
        apps: RefCell::new(Vec::new()),
        paths: RefCell::new(crate::apps::running().into_iter().collect()),
        handler: RefCell::new(None),
        raw: RefCell::new(None),
    });
    fill_apps(&win, None);
    sync(&win);
    win.surface
        .set_checked(win.ids.nav[(page - 1) as usize], true);
    win.surface.show_page(page);
    if let Some(h) = win.window.handle.hwnd() {
        crate::ui::present(HWND(h as _), "settings", started);
    } else {
        win.window.set_visible(true);
    }
    OPEN.store(hwnd_isize, Ordering::Release);

    let weak = Rc::downgrade(&win);
    win.surface.on_click(move |id| {
        if let Some(win) = weak.upgrade() {
            clicked(&win, id);
        }
    });
    let weak = Rc::downgrade(&win);
    win.surface.on_table(move |_id, event| {
        if let Some(win) = weak.upgrade() {
            app_table_event(&win, event);
        }
    });
    let win_h = win.clone();
    let handler = nwg::full_bind_event_handler(&win.window.handle, move |evt, _data, handle| {
        if matches!(evt, nwg::Event::OnWindowClose) && handle == win_h.window.handle {
            finish(&win_h);
        }
    });
    *win.handler.borrow_mut() = Some(handler);
    let weak = Rc::downgrade(&win);
    let raw =
        nwg::bind_raw_event_handler(&win.window.handle, 0x5254_0016, move |_h, msg, _w, _l| {
            if msg == hook::WM_HOTKEY_CAPTURED {
                if let Some(win) = weak.upgrade() {
                    captured(&win);
                }
                return Some(0);
            }
            None
        })
        .ok();
    *win.raw.borrow_mut() = raw;
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
            hook::Mode::Manual | hook::Mode::Code => T::DescManual,
        }),
    );
    s.set_checked(ids.enabled, hook::is_enabled());
    s.set_checked(ids.startup, startup::is_enabled());
    s.set_checked(ids.learn, learn::is_enabled());
    s.set_checked(ids.caret_hints, crate::caret::is_enabled());
    s.set_checked(ids.spelling, hook::fixes_spelling());
    s.set_checked(ids.predict, crate::habits::is_enabled());
    s.set_checked(
        ids.restart,
        config::RESTART_AFTER_CRASH.load(Ordering::Relaxed),
    );
    sync_app_choice(win);
    let pattachote =
        righttype::layout::thai_variant() == righttype::layout::ThaiVariant::Pattachote;
    s.set_checked(ids.kedmanee, !pattachote);
    s.set_checked(ids.pattachote, pattachote);
    s.set_text(
        ids.folder_label,
        &match learn::folder() {
            Some(folder) => trf(T::LearnedInFolder, &[("v", &folder.to_string_lossy())]),
            None => tr(T::LearnedHere).to_string(),
        },
    );
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
    } else if id == ids.predict {
        crate::habits::set_enabled(s.checked(ids.predict));
        config::persist();
    } else if id == ids.clear_habits {
        crate::habits::clear();
        overlay::show(tr(T::ToastHabitsCleared));
    } else if id == ids.spelling {
        hook::set_fixes_spelling(s.checked(ids.spelling));
        config::persist();
    } else if id == ids.caret_hints {
        crate::caret::set_enabled(s.checked(ids.caret_hints));
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
    } else if let Some(i) = ids.app_seg.iter().position(|&b| b == id) {
        set_app_mode(win, APP_MODE_CHOICES[i]);
    } else if id == ids.apps_add {
        add_app(win);
    } else if id == ids.apps_keep {
        keep_app(win);
    } else if id == ids.apps_remove {
        remove_app(win);
    } else if id == ids.restart {
        config::RESTART_AFTER_CRASH.store(s.checked(ids.restart), Ordering::Relaxed);
        config::persist();
    } else if id == ids.sync_folder || id == ids.this_pc {
        let folder = if id == ids.sync_folder {
            match pick_folder(win) {
                Some(folder) => Some(folder),
                None => return,
            }
        } else {
            None
        };
        match learn::set_folder(folder) {
            Some(n) => {
                config::persist();
                s.set_text(ids.learned_list, &learn::list().join("\r\n"));
                s.set_text(
                    ids.learned_status,
                    &trf(T::LearnedMoved, &[("n", &n.to_string())]),
                );
            }
            // The folder could not be written: nothing changed.
            None => s.set_text(ids.learned_status, tr(T::ErrFile)),
        }
    } else if let Some(i) = ids.key_buttons.iter().position(|b| *b == id) {
        let action = Action::ALL[i];
        hook::begin_capture(action, s.hwnd.0 as isize);
        s.set_text(
            ids.keys_status,
            &trf(T::HkPress, &[("v", tr(action_label(action)))]),
        );
    } else if id == ids.kedmanee || id == ids.pattachote {
        righttype::layout::set_thai_variant(if id == ids.pattachote {
            righttype::layout::ThaiVariant::Pattachote
        } else {
            righttype::layout::ThaiVariant::Kedmanee
        });
        config::persist();
    } else if id == ids.keys_reset {
        hook::cancel_capture();
        hook::set_hotkeys(Hotkeys::default());
        config::persist();
        show_hotkeys(win);
        s.set_text(ids.keys_status, tr(T::HkReset));
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

// ------------------------------------------------------------- Apps page

/// Every app with a mode of its own: for now, chosen, blocked, code editors
/// by default, then the built-in safety list.
fn app_rows() -> Vec<AppRow> {
    use crate::apps::Source;
    let mut rows: Vec<AppRow> = crate::apps::rows()
        .into_iter()
        .map(|(exe, mode, source)| AppRow {
            exe,
            mode: Some(mode),
            source: match source {
                Source::ForNow => RowSource::ForNow,
                Source::Chosen => RowSource::Chosen,
                Source::Default => RowSource::Default,
            },
        })
        .collect();
    for exe in safety::custom_list() {
        rows.retain(|r| r.exe != exe);
        rows.push(AppRow {
            exe,
            mode: None,
            source: RowSource::Blocked,
        });
    }
    for exe in safety::BLACKLIST {
        rows.retain(|r| r.exe != *exe);
        rows.push(AppRow {
            exe: exe.to_string(),
            mode: None,
            source: RowSource::Safety,
        });
    }
    rows.sort_by(|a, b| (a.source, &a.exe).cmp(&(b.source, &b.exe)));
    rows
}

/// Rebuild the Apps table, selecting `exe` when given (else keeping the
/// selected row's place).
fn fill_apps(win: &SettingsWindow, exe: Option<&str>) {
    let s = &win.surface;
    let keep = s.selected_row(win.ids.apps_table);
    let rows = app_rows();
    let paths = win.paths.borrow();
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let mode = match (r.mode, r.source) {
                (_, RowSource::Safety) => tr(T::ModeAlwaysOff).to_string(),
                (None, _) => tr(T::ModeBlocked).to_string(),
                (Some(m), _) => tr(crate::tray::app_mode_name(m)).to_string(),
            };
            let by = tr(match r.source {
                RowSource::ForNow => T::SetByForNow,
                RowSource::Chosen => T::SetByYou,
                RowSource::Blocked => T::SetByYouBlocked,
                RowSource::Default => T::SetByDefault,
                RowSource::Safety => T::SetBySafety,
            });
            let place = paths.get(&r.exe).cloned().unwrap_or_else(|| "—".into());
            vec![r.exe.clone(), mode, by.to_string(), place]
        })
        .collect();
    drop(paths);
    let select = exe
        .and_then(|e| rows.iter().position(|r| r.exe == e))
        .or(keep);
    *win.apps.borrow_mut() = rows;
    s.set_rows(win.ids.apps_table, &cells, select);
    sync_app_choice(win);
}

/// The selected row of the Apps table.
fn selected_app(win: &SettingsWindow) -> Option<AppRow> {
    let i = win.surface.selected_row(win.ids.apps_table)?;
    win.apps.borrow().get(i).cloned()
}

/// Show the selected app's mode on the mode buttons.
fn sync_app_choice(win: &SettingsWindow) {
    let row = selected_app(win);
    for (i, id) in win.ids.app_seg.iter().enumerate() {
        let on = row.as_ref().is_some_and(|r| match r.mode {
            Some(m) => m == APP_MODE_CHOICES[i],
            None => APP_MODE_CHOICES[i] == AppMode::Off,
        });
        win.surface.set_checked(*id, on);
    }
}

fn app_table_event(win: &Rc<SettingsWindow>, event: ui::TableEvent) {
    match event {
        ui::TableEvent::Selected => sync_app_choice(win),
        ui::TableEvent::Delete => remove_app(win),
        ui::TableEvent::Menu { x, y } => app_menu(win, x, y),
    }
}

/// Give the selected app `mode` (chosen, saved).
fn set_app_mode(win: &SettingsWindow, mode: AppMode) {
    let Some(row) = selected_app(win) else {
        win.surface
            .set_text(win.ids.apps_status, tr(T::AppsPickFirst));
        sync_app_choice(win);
        return;
    };
    if row.source == RowSource::Safety {
        win.surface
            .set_text(win.ids.apps_status, tr(T::AppsBuiltIn));
        sync_app_choice(win);
        return;
    }
    if row.source == RowSource::Blocked {
        let rest: Vec<String> = safety::custom_list()
            .into_iter()
            .filter(|e| *e != row.exe)
            .collect();
        safety::set_custom_list(rest);
    }
    crate::apps::set(&row.exe, Some(mode));
    config::persist();
    fill_apps(win, Some(&row.exe));
    win.surface.set_text(
        win.ids.apps_status,
        &trf(
            T::ToastAppMode,
            &[
                ("mode", tr(crate::tray::app_mode_name(mode))),
                ("app", &row.exe),
            ],
        ),
    );
}

/// The selected app goes back to the general mode (or its default).
fn remove_app(win: &SettingsWindow) {
    let Some(row) = selected_app(win) else {
        win.surface
            .set_text(win.ids.apps_status, tr(T::AppsPickFirst));
        return;
    };
    match row.source {
        RowSource::Safety | RowSource::Default => {
            win.surface
                .set_text(win.ids.apps_status, tr(T::AppsBuiltIn));
            return;
        }
        RowSource::ForNow => crate::apps::clear_for_now(&row.exe),
        RowSource::Chosen => crate::apps::set(&row.exe, None),
        RowSource::Blocked => {
            let rest: Vec<String> = safety::custom_list()
                .into_iter()
                .filter(|e| *e != row.exe)
                .collect();
            safety::set_custom_list(rest);
        }
    }
    config::persist();
    fill_apps(win, None);
    win.surface.set_text(
        win.ids.apps_status,
        &trf(T::AppsRemoved, &[("app", &row.exe)]),
    );
}

/// Keep the selected app's mode for now for good.
fn keep_app(win: &SettingsWindow) {
    let Some(row) = selected_app(win).filter(|r| r.source == RowSource::ForNow) else {
        win.surface
            .set_text(win.ids.apps_status, tr(T::AppsKeepWhat));
        return;
    };
    crate::apps::keep(&row.exe);
    config::persist();
    fill_apps(win, Some(&row.exe));
    win.surface.set_text(win.ids.apps_status, tr(T::AppsSaved));
}

/// Pick a program that has a window open now and add it to the table (with
/// the general mode, to change from there).
fn add_app(win: &SettingsWindow) {
    let have: Vec<String> = win.apps.borrow().iter().map(|r| r.exe.clone()).collect();
    let running: Vec<(String, String)> = crate::apps::running()
        .into_iter()
        .filter(|(exe, _)| !have.contains(exe))
        .collect();
    {
        let mut paths = win.paths.borrow_mut();
        for (exe, path) in &running {
            paths.insert(exe.clone(), path.clone());
        }
    }
    if running.is_empty() {
        win.surface
            .set_text(win.ids.apps_status, tr(T::AppsNoneRunning));
        return;
    }
    let labels: Vec<String> = running.iter().map(|(exe, _)| exe.clone()).collect();
    let rc = unsafe {
        let mut rc = windows::Win32::Foundation::RECT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(
            win.surface.hwnd_of(win.ids.apps_add),
            &mut rc,
        );
        rc
    };
    let Some(i) = popup(win, &labels, rc.left, rc.bottom) else {
        return;
    };
    let exe = &running[i].0;
    crate::apps::set(exe, Some(hook::mode().into()));
    config::persist();
    fill_apps(win, Some(exe));
    win.surface
        .set_text(win.ids.apps_status, &trf(T::AppsAdded, &[("app", exe)]));
}

/// Right-click on a row: its actions.
fn app_menu(win: &Rc<SettingsWindow>, x: i32, y: i32) {
    let Some(row) = selected_app(win) else {
        return;
    };
    if row.source == RowSource::Safety {
        win.surface
            .set_text(win.ids.apps_status, tr(T::AppsBuiltIn));
        return;
    }
    let mut labels: Vec<String> = APP_MODE_CHOICES
        .iter()
        .map(|m| {
            let mark = if row.mode == Some(*m) { "✓  " } else { "" };
            format!("{mark}{}", tr(crate::tray::app_mode_name(*m)))
        })
        .collect();
    let keep = row.source == RowSource::ForNow;
    if keep {
        labels.push(tr(T::BtnKeepMode).to_string());
    }
    let removable = row.source != RowSource::Default;
    if removable {
        labels.push(tr(T::BtnRemoveApp).to_string());
    }
    let path = win.paths.borrow().get(&row.exe).cloned();
    if path.is_some() {
        labels.push(tr(T::BtnShowFile).to_string());
    }
    let Some(i) = popup(win, &labels, x, y) else {
        return;
    };
    let n = APP_MODE_CHOICES.len();
    if i < n {
        set_app_mode(win, APP_MODE_CHOICES[i]);
        return;
    }
    let mut rest = i - n;
    if keep {
        if rest == 0 {
            keep_app(win);
            return;
        }
        rest -= 1;
    }
    if removable {
        if rest == 0 {
            remove_app(win);
            return;
        }
        rest -= 1;
    }
    if rest == 0 {
        if let Some(path) = path {
            show_in_folder(&path);
        }
    }
}

/// A popup menu of `labels` at screen `(x, y)`; the index picked.
fn popup(win: &SettingsWindow, labels: &[String], x: i32, y: i32) -> Option<usize> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, TrackPopupMenu, MF_STRING, TPM_RETURNCMD,
        TPM_RIGHTBUTTON,
    };
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        for (i, label) in labels.iter().enumerate() {
            let text: Vec<u16> = format!("{label}\0").encode_utf16().collect();
            let _ = AppendMenuW(menu, MF_STRING, i + 1, PCWSTR(text.as_ptr()));
        }
        let picked = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            x,
            y,
            0,
            win.surface.hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        usize::try_from(picked.0).ok().filter(|&p| p > 0).map(|p| p - 1)
    }
}

/// Open Explorer with `path` selected.
fn show_in_folder(path: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let exe: Vec<u16> = "explorer.exe\0".encode_utf16().collect();
    let args: Vec<u16> = format!("/select,\"{path}\"\0").encode_utf16().collect();
    unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(exe.as_ptr()),
            PCWSTR(args.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
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

/// Ask for a folder (the sync folder for learned words).
fn pick_folder(win: &SettingsWindow) -> Option<std::path::PathBuf> {
    let mut dialog = nwg::FileDialog::default();
    nwg::FileDialog::builder()
        .action(nwg::FileDialogAction::OpenDirectory)
        .build(&mut dialog)
        .ok()?;
    if !dialog.run(Some(&win.window)) {
        return None;
    }
    Some(std::path::PathBuf::from(dialog.get_selected_item().ok()?))
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

/// Put the current chords in the Hotkeys page's labels.
fn show_hotkeys(win: &SettingsWindow) {
    for ((action, keys), label) in hotkey_rows().iter().zip(&win.ids.key_labels) {
        win.surface
            .set_text(*label, &format!("{}\t{keys}", tr(*action)));
    }
}

/// The keyboard hook captured a chord for the action being changed.
fn captured(win: &SettingsWindow) {
    let Some((action, chord)) = hook::take_captured() else {
        return;
    };
    let status = win.ids.keys_status;
    let Some(chord) = chord else {
        win.surface.set_text(status, tr(T::NoteHotkeys));
        return;
    };
    let mut keys = hook::hotkeys();
    match keys.set(action, chord) {
        Ok(()) => {
            hook::set_hotkeys(keys);
            config::persist();
            show_hotkeys(win);
            win.surface.set_text(status, tr(T::ToastSaved));
        }
        Err(Refusal::Unusable) => win.surface.set_text(status, tr(T::HkUnusable)),
        Err(Refusal::Taken(other)) => win
            .surface
            .set_text(status, &trf(T::HkTaken, &[("v", tr(action_label(other)))])),
    }
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
    hook::cancel_capture();
    win.surface.detach();
    if let Some(h) = win.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    if let Some(h) = win.raw.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
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
            card(g, rect(X0, CARD_B_Y, CW, 5 * ROW_H + 64));
            for i in 1..=5 {
                divider(hdc, X0 + 16, CARD_B_Y + i * ROW_H + 2, CW - 32);
            }
        }
        PAGE_HOTKEYS => {
            track(g, rect(X0 + CW - 24 - 2 * 130, KEYBOARD_Y, 2 * 130 + 8, 40));
            card(g, rect(X0, 68, CW, Action::ALL.len() as i32 * 52 + 8));
            for i in 1..Action::ALL.len() as i32 {
                divider(hdc, X0 + 16, 74 + i * 52 - 1, CW - 32);
            }
        }
        PAGE_LEARNED => {
            field(g, rect(X0, 142, CW, 268));
        }
        PAGE_BLOCKED => {
            track(g, rect(X0, 342, CW, 40));
            card(g, rect(X0, PREDICT_Y, CW, 76));
        }
        PAGE_ABOUT => {
            card(g, rect(X0, RESTART_Y, CW, 72));
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
