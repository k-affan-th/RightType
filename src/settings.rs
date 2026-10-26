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
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};

use native_windows_gui as nwg;
use righttype::i18n::{self, tr, trf, Lang, T};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::WindowsAndMessaging::{SetForegroundWindow, ShowWindow, SW_RESTORE};

use crate::ui::{self, card, divider, field, pal, rect, track, Gfx, Surface, TextStyle};
use crate::{config, hook, icons, learn, overlay, safety, startup};
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
/// The snippet scope buttons ("English only" is the widest): together with
/// their frame they end before the Add / save button.
const SNIP_SCOPE_WIDTHS: [i32; 4] = [68, 84, 62, 68];

const PAGE_GENERAL: u8 = 1;
const PAGE_HOTKEYS: u8 = 2;
const PAGE_LEARNED: u8 = 3;
const PAGE_BLOCKED: u8 = 4;
const PAGE_ABOUT: u8 = 5;
const PAGE_SNIPPETS: u8 = 6;
const PAGE_KEYBOARD: u8 = 7;
const PAGE_TOOLS: u8 = 8;
const PAGE_SPONSOR: u8 = 9;
/// Sidebar entries, top to bottom, and their pages.
const NAV: [(T, u8, char); 9] = [
    (T::NavGeneral, PAGE_GENERAL, icons::GENERAL),
    (T::NavKeyboard, PAGE_KEYBOARD, icons::KEYBOARD),
    (T::NavTools, PAGE_TOOLS, icons::TOOLS),
    (T::NavHotkeys, PAGE_HOTKEYS, icons::HOTKEYS),
    (T::NavLearned, PAGE_LEARNED, icons::LEARNED),
    (T::NavSnippets, PAGE_SNIPPETS, icons::SNIPPETS),
    (T::NavBlocked, PAGE_BLOCKED, icons::APPS),
    (T::NavAbout, PAGE_ABOUT, icons::PRIVACY),
    (T::NavSponsor, PAGE_SPONSOR, icons::SUPPORT),
];

pub const BTC_LIGHTNING: &str = "yawninggrass92@walletofsatoshi.com";
pub const BTC_MAINNET: &str = "bc1qdac8h7hl57x3kv8x8xpz6zg0fgw6zvp7uld3a6";

pub const QR_LIGHTNING: [u64; 33] = [
    0x1fc9b207f,
    0x10452fe41,
    0x1748fa15d,
    0x17571495d,
    0x17504955d,
    0x10457d341,
    0x1fd55557f,
    0x00020c900,
    0x07cd6ec7d,
    0x184b9552f,
    0x0e4348e60,
    0x17961d61d,
    0x09b240041,
    0x144d3ffa5,
    0x0b62c616d,
    0x06cf953ad,
    0x19bce6af6,
    0x1649b3f9d,
    0x0bc43a9ff,
    0x1f30ed594,
    0x191f19afe,
    0x14484fc87,
    0x0f257dec9,
    0x077a4bdb9,
    0x1bf5098f5,
    0x111bd4700,
    0x0d5b5927f,
    0x071f90d41,
    0x0bf24935d,
    0x17871fb5d,
    0x0464c735d,
    0x06e7d2641,
    0x085cb6b7f,
];

pub const QR_BITCOIN: [u64; 33] = [
    0x1fcab727f,
    0x10572e241,
    0x17476b95d,
    0x17451515d,
    0x174a4b75d,
    0x104338b41,
    0x1fd55557f,
    0x000e09500,
    0x07d53ee7d,
    0x18e88599a,
    0x0fa6495e1,
    0x17730c49e,
    0x0b04c144c,
    0x107b3d129,
    0x0950c4be9,
    0x1657d4024,
    0x01a0f194d,
    0x1e4fb4918,
    0x04262afd6,
    0x17d26f0a9,
    0x11a092bec,
    0x166c4e21d,
    0x0b927d165,
    0x144b8a989,
    0x03f43d741,
    0x1d1bc5900,
    0x0b594887f,
    0x071791941,
    0x01f54fb5d,
    0x1da8b915d,
    0x045365b5d,
    0x074a15e41,
    0x09593337f,
];

static TIP_NETWORK: AtomicU8 = AtomicU8::new(0);

const AB_CARD1_Y: i32 = 68;
const AB_CARD1_H: i32 = 66;
const AB_CARD2_Y: i32 = AB_CARD1_Y + AB_CARD1_H + 12; // 146
const AB_CARD2_H: i32 = 186;
const AB_CARD3_Y: i32 = AB_CARD2_Y + AB_CARD2_H + 12; // 344
const AB_CARD3_H: i32 = 88;
const RESTART_Y: i32 = AB_CARD3_Y + AB_CARD3_H + 12; // 428

const SP_CARD1_Y: i32 = 68;
const SP_CARD1_H: i32 = 92;
const SP_CARD2_Y: i32 = SP_CARD1_Y + SP_CARD1_H + 14; // 166
const SP_CARD2_H: i32 = 216;

/// Toggle rows on the General page: y of each row inside the behaviour card.
const ROW_H: i32 = 64;
const CARD_B_Y: i32 = 282;
/// The per-field language card on the Apps page.
const PREDICT_Y: i32 = 548;
/// The Keyboard page: its three cards (y of each card's top) and how tall
/// the page is (it scrolls).
const KB_PICK_Y: i32 = 98;
const KB_TYPING_Y: i32 = 246;
/// Taller than the General page's rows: a two-line description needs the room.
const KB_ROW_H: i32 = 72;
const KB_KEYS_Y: i32 = KB_TYPING_Y + 7 * KB_ROW_H + 8 + 56;
const KB_DEV_Y: i32 = KB_KEYS_Y + 3 * KB_ROW_H + 8 + 56;
const KB_SHOW_Y: i32 = KB_DEV_Y + 2 * KB_ROW_H + 8 + 56;
const KB_CARET_Y: i32 = KB_SHOW_Y + 2 * KB_ROW_H + 8 + 56;
const KB_HEIGHT: i32 = KB_CARET_Y + 3 * KB_ROW_H + 8 + 24;
/// The Tools page: the tools' card, the health check's card, the page.
const TOOLS_Y: i32 = 98;
const HEALTH_Y: i32 = 294;
const TOOLS_HEIGHT: i32 = HEALTH_Y + crate::health::COUNT as i32 * ROW_H + 8 + 24;
/// The segment buttons at the right of a Keyboard-page row.
const KB_SEG_W: i32 = 66;

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
        Action::KeyMap => T::HkKeyMap,
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
    nav: [u16; 9],
    snip_table: u16,
    snip_trigger: u16,
    snip_text: u16,
    /// Puts a date or time field into the text.
    snip_date: u16,
    /// Thai, English, either.
    snip_scope: [u16; 4],
    snip_save: u16,
    snip_remove: u16,
    snip_status: u16,
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
    manoonchai: u16,
    kb_addresses: u16,
    kb_complete: u16,
    kb_grave: u16,
    kb_guard: u16,
    kb_accents: u16,
    kb_shortcuts: u16,
    kb_ctrl_hold: u16,
    kb_password_tag: u16,
    /// NumLock and Insert: off, warn, fix.
    kb_numlock: [u16; 3],
    kb_insert: [u16; 3],
    kb_scanner: u16,
    kb_fake: u16,
    kb_show_keys: u16,
    kb_rest: u16,
    kb_caret_list: u16,
    kb_ghosts: u16,
    kb_cycle: u16,
    tools_clean: u16,
    tools_test: u16,
    tools_map: u16,
    tools_practice: u16,
    health_again: u16,
    /// Per check: its status line and its fix button.
    health_status: [u16; crate::health::COUNT],
    health_fix: [u16; crate::health::COUNT],
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
    sync_settings: u16,
    link_github: u16,
    tip_ln: u16,
    tip_onchain: u16,
    tip_address: u16,
    tip_desc: u16,
    tip_copy: u16,
}

thread_local! {
    /// The health check's last results, for the fix buttons and the painter.
    static HEALTH: RefCell<Vec<crate::health::Check>> = const { RefCell::new(Vec::new()) };
}

/// Run the health check and show it.
fn fill_health(win: &SettingsWindow) {
    let s = &win.surface;
    let checks = crate::health::run();
    for (i, check) in checks.iter().enumerate().take(crate::health::COUNT) {
        s.set_text(win.ids.health_status[i], &check.status);
        let button = s.hwnd_of(win.ids.health_fix[i]);
        match &check.fix {
            Some((label, _)) => {
                s.set_text(win.ids.health_fix[i], tr(*label));
                unsafe {
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        button,
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
                    );
                }
            }
            None => unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    button,
                    windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                );
            },
        }
    }
    HEALTH.with(|h| *h.borrow_mut() = checks);
    unsafe {
        let _ = windows::Win32::Graphics::Gdi::InvalidateRect(s.hwnd, None, true);
    }
}

/// The choices for NumLock and Insert, in button order.
const KEY_GUARDS: [hook::KeyGuard; 3] = [
    hook::KeyGuard::Off,
    hook::KeyGuard::Warn,
    hook::KeyGuard::Fix,
];

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
    /// No mode of its own, only a keyboard.
    KeyboardOnly,
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
    open_on(page.clamp(PAGE_GENERAL, PAGE_SPONSOR));
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
    s.label("RightType", TextStyle::Subtitle, (24, 20, 180, 26), p.bg, 0);
    s.label(
        tr(T::BrandTagline),
        TextStyle::Small,
        (24, 46, 180, 18),
        p.bg,
        0,
    );
    let mut nav = [0u16; 9];
    for (i, (label, p_id, icon)) in NAV.iter().enumerate() {
        // The first entry starts the radio group, whichever page is shown;
        // the shown page is checked on its own.
        nav[i] = s.nav(tr(*label), *icon, i == 0, (12, 74 + i as i32 * 38, 196, 34));
        s.set_checked(nav[i], *p_id == page);
    }

    s.label(
        tr(T::HeadLanguage),
        TextStyle::Small,
        (20, H - 98, 180, 16),
        p.bg,
        0,
    );
    let lang_en = s.segment("English", true, (16, H - 78, 92, 30), p.inset, 0);
    let lang_th = s.segment("ไทย", false, (112, H - 78, 92, 30), p.inset, 0);

    let ver_str = trf(T::VersionLabel, &[("v", env!("CARGO_PKG_VERSION"))]);
    s.label(&ver_str, TextStyle::Dim, (20, H - 36, 188, 20), p.bg, 0);

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
        TextStyle::Section(icons::MODE_SECTION),
        (X0, 70, CW, 20),
        p.bg,
        g,
    );
    let seg_w = (CW - 32 - 8) / 3;
    let mode_auto = s.segment_icon(
        tr(T::ModeAuto),
        icons::MODE_AUTO,
        true,
        (X0 + 20, 114, seg_w, 40),
        p.inset,
        g,
    );
    let mode_suggest = s.segment_icon(
        tr(T::ModeSuggest),
        icons::MODE_SUGGEST,
        false,
        (X0 + 20 + seg_w, 114, seg_w, 40),
        p.inset,
        g,
    );
    let mode_manual = s.segment_icon(
        tr(T::ModeManual),
        icons::MODE_MANUAL,
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
        TextStyle::Section(icons::GENERAL),
        (X0, CARD_B_Y - 26, CW, 20),
        p.bg,
        g,
    );
    let row = |i: i32| (X0 + 4, CARD_B_Y + 4 + i * ROW_H, CW - 8, ROW_H - 4);
    let enabled = s.toggle_icon(
        tr(T::RowEnabled),
        tr(T::SubEnabled),
        icons::POWER,
        row(0),
        p.surface,
        g,
    );
    let startup = s.toggle_icon(
        tr(T::RowStartup),
        tr(T::SubStartup),
        icons::START_WITH_WINDOWS,
        row(1),
        p.surface,
        g,
    );
    let learn = s.toggle_icon(
        tr(T::RowLearn),
        tr(T::SubLearnShort),
        icons::LEARNED,
        row(2),
        p.surface,
        g,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubLearn),
        row(2),
        true,
        tr(T::RowLearn),
        p.surface,
        g,
    );
    let caret_hints = s.toggle_icon(
        tr(T::RowCaret),
        tr(T::SubCaretShort),
        icons::CURSOR_HINTS,
        row(3),
        p.surface,
        g,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubCaret),
        row(3),
        true,
        tr(T::RowCaret),
        p.surface,
        g,
    );
    let spelling = s.toggle_icon(
        tr(T::RowSpelling),
        tr(T::SubSpellingShort),
        icons::SPELLING,
        row(4),
        p.surface,
        g,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubSpelling),
        row(4),
        true,
        tr(T::RowSpelling),
        p.surface,
        g,
    );
    let learned_y = CARD_B_Y + 5 * ROW_H + 14;
    let learned = s.label(
        "",
        TextStyle::Dim,
        (X0 + 20, learned_y + 6, CW - 160, 20),
        p.surface,
        g,
    );
    let edit_learned = s.button_icon(
        tr(T::BtnEditLearned),
        icons::EDIT,
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
        key_buttons.push(s.button_icon(
            tr(T::BtnChange),
            icons::EDIT,
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
    let keys_reset = s.button_icon(
        tr(T::BtnResetKeys),
        icons::UNDO,
        false,
        (X0 + CW - 150, note_y, 150, 34),
        p.bg,
        h,
    );
    // --- Keyboard ---------------------------------------------------------
    let k = PAGE_KEYBOARD;
    surface.set_page_height(k, KB_HEIGHT);
    s.label(
        tr(T::NavKeyboard),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        k,
    );
    s.label(
        tr(T::HeadThaiKeyboard),
        TextStyle::Section(icons::KEYBOARD),
        (X0, KB_PICK_Y - 28, CW, 20),
        p.bg,
        k,
    );
    let kb_seg_w = (CW - 32 - 8) / 3;
    let kb_seg = |i: i32| (X0 + 20 + i * kb_seg_w, KB_PICK_Y + 16, kb_seg_w, 40);
    let kedmanee = s.segment("Kedmanee", true, kb_seg(0), p.inset, k);
    let pattachote = s.segment("Pattachote", false, kb_seg(1), p.inset, k);
    let manoonchai = s.segment("Manoonchai", false, kb_seg(2), p.inset, k);
    s.label(
        tr(T::NoteKeyboards),
        TextStyle::Dim,
        (X0 + 20, KB_PICK_Y + 66, CW - 40, 22),
        p.surface,
        k,
    );
    s.label(
        tr(T::HeadWhileTyping),
        TextStyle::Section(icons::EDIT),
        (X0, KB_TYPING_Y - 28, CW, 20),
        p.bg,
        k,
    );
    let kb_row = |top: i32, i: i32| (X0 + 4, top + 4 + i * KB_ROW_H, CW - 8, KB_ROW_H - 4);
    let kb_addresses = s.toggle_icon(
        tr(T::RowAddresses),
        tr(T::SubAddressesShort),
        icons::WEB_ADDRESSES,
        kb_row(KB_TYPING_Y, 0),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubAddresses),
        kb_row(KB_TYPING_Y, 0),
        true,
        tr(T::RowAddresses),
        p.surface,
        k,
    );
    let kb_complete = s.toggle_icon(
        tr(T::RowComplete),
        tr(T::SubCompleteShort),
        icons::TAB_COMPLETE,
        kb_row(KB_TYPING_Y, 1),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubComplete),
        kb_row(KB_TYPING_Y, 1),
        true,
        tr(T::RowComplete),
        p.surface,
        k,
    );
    let kb_grave = s.toggle_icon(
        tr(T::RowGrave),
        tr(T::SubGraveShort),
        icons::LANGUAGE_SWITCH,
        kb_row(KB_TYPING_Y, 2),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubGrave),
        kb_row(KB_TYPING_Y, 2),
        true,
        tr(T::RowGrave),
        p.surface,
        k,
    );
    let kb_guard = s.toggle_icon(
        tr(T::RowGuardSwitch),
        tr(T::SubGuardSwitchShort),
        icons::UNDO,
        kb_row(KB_TYPING_Y, 3),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubGuardSwitch),
        kb_row(KB_TYPING_Y, 3),
        true,
        tr(T::RowGuardSwitch),
        p.surface,
        k,
    );
    let kb_accents = s.toggle_icon(
        tr(T::RowHoldAccents),
        tr(T::SubHoldAccentsShort),
        icons::HOLD_KEY,
        kb_row(KB_TYPING_Y, 4),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubHoldAccents),
        kb_row(KB_TYPING_Y, 4),
        true,
        tr(T::RowHoldAccents),
        p.surface,
        k,
    );
    let kb_shortcuts = s.toggle_icon(
        tr(T::RowShortcutsEnglish),
        tr(T::SubShortcutsEnglishShort),
        icons::SHORTCUTS,
        kb_row(KB_TYPING_Y, 5),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubShortcutsEnglish),
        kb_row(KB_TYPING_Y, 5),
        true,
        tr(T::RowShortcutsEnglish),
        p.surface,
        k,
    );
    let kb_ctrl_hold = s.toggle_icon(
        tr(T::RowCtrlHold),
        tr(T::SubCtrlHoldShort),
        icons::KEYBOARD,
        kb_row(KB_TYPING_Y, 6),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubCtrlHold),
        kb_row(KB_TYPING_Y, 6),
        true,
        tr(T::RowCtrlHold),
        p.surface,
        k,
    );
    s.label(
        tr(T::HeadStateKeys),
        TextStyle::Section(icons::SHORTCUTS),
        (X0, KB_KEYS_Y - 28, CW, 20),
        p.bg,
        k,
    );
    let kb_password_tag = s.toggle_icon(
        tr(T::RowPasswordTag),
        tr(T::SubPasswordTagShort),
        icons::PRIVACY,
        kb_row(KB_KEYS_Y, 0),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubPasswordTag),
        kb_row(KB_KEYS_Y, 0),
        true,
        tr(T::RowPasswordTag),
        p.surface,
        k,
    );
    // A row with three choices: title and description left, the choices
    // right where a toggle's switch would be.
    let guard_row = |i: i32, title: T, sub: T, last: T| -> [u16; 3] {
        let top = KB_KEYS_Y + 4 + i * KB_ROW_H;
        let text_w = CW - 40 - 3 * KB_SEG_W - 16 - 44;
        s.label(
            tr(title),
            TextStyle::Body,
            (X0 + 20 + 44, top + 12, text_w, 22),
            p.surface,
            k,
        );
        s.label(
            tr(sub),
            TextStyle::Small,
            (X0 + 20 + 44, top + 34, text_w, 30),
            p.surface,
            k,
        );
        let x = X0 + CW - 20 - 3 * KB_SEG_W;
        [T::GuardOff, T::GuardWarn, last]
            .iter()
            .enumerate()
            .map(|(j, label)| {
                s.segment(
                    tr(*label),
                    j == 0,
                    (x + j as i32 * KB_SEG_W, top + 18, KB_SEG_W, 32),
                    p.inset,
                    k,
                )
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or([0; 3])
    };
    let kb_numlock = guard_row(1, T::RowNumLock, T::SubNumLock, T::GuardFix);
    let kb_insert = guard_row(2, T::RowInsertKey, T::SubInsertKey, T::GuardBlock);
    s.label(
        tr(T::HeadDevices),
        TextStyle::Section(icons::SCANNER),
        (X0, KB_DEV_Y - 28, CW, 20),
        p.bg,
        k,
    );
    let kb_scanner = s.toggle_icon(
        tr(T::RowScanner),
        tr(T::SubScannerShort),
        icons::SCANNER,
        kb_row(KB_DEV_Y, 0),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubScanner),
        kb_row(KB_DEV_Y, 0),
        true,
        tr(T::RowScanner),
        p.surface,
        k,
    );
    let kb_fake = s.toggle_icon(
        tr(T::RowFakeKeyboard),
        tr(T::SubFakeKeyboardShort),
        icons::SHIELD,
        kb_row(KB_DEV_Y, 1),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubFakeKeyboard),
        kb_row(KB_DEV_Y, 1),
        true,
        tr(T::RowFakeKeyboard),
        p.surface,
        k,
    );
    s.label(
        tr(T::HeadComfort),
        TextStyle::Section(icons::ON_SCREEN),
        (X0, KB_SHOW_Y - 28, CW, 20),
        p.bg,
        k,
    );
    let kb_show_keys = s.toggle_icon(
        tr(T::RowShowKeys),
        tr(T::SubShowKeysShort),
        icons::ON_SCREEN,
        kb_row(KB_SHOW_Y, 0),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubShowKeys),
        kb_row(KB_SHOW_Y, 0),
        true,
        tr(T::RowShowKeys),
        p.surface,
        k,
    );
    let kb_rest = s.toggle_icon(
        tr(T::RowRest),
        tr(T::SubRestShort),
        icons::REST,
        kb_row(KB_SHOW_Y, 1),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubRest),
        kb_row(KB_SHOW_Y, 1),
        true,
        tr(T::RowRest),
        p.surface,
        k,
    );
    s.label(
        tr(T::HeadCaret),
        TextStyle::Section(icons::CURSOR_HINTS),
        (X0, KB_CARET_Y - 28, CW, 20),
        p.bg,
        k,
    );
    let kb_caret_list = s.toggle_icon(
        tr(T::RowCaretList),
        tr(T::SubCaretListShort),
        icons::SEARCH,
        kb_row(KB_CARET_Y, 0),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubCaretList),
        kb_row(KB_CARET_Y, 0),
        true,
        tr(T::RowCaretList),
        p.surface,
        k,
    );
    let kb_ghosts = s.toggle_icon(
        tr(T::RowGhosts),
        tr(T::SubGhostsShort),
        icons::MATH,
        kb_row(KB_CARET_Y, 1),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubGhosts),
        kb_row(KB_CARET_Y, 1),
        true,
        tr(T::RowGhosts),
        p.surface,
        k,
    );
    let kb_cycle = s.toggle_icon(
        tr(T::RowCycleChars),
        tr(T::SubCycleCharsShort),
        icons::REFRESH,
        kb_row(KB_CARET_Y, 2),
        p.surface,
        k,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubCycleChars),
        kb_row(KB_CARET_Y, 2),
        true,
        tr(T::RowCycleChars),
        p.surface,
        k,
    );

    // --- Tools ------------------------------------------------------------
    let t = PAGE_TOOLS;
    surface.set_page_height(t, TOOLS_HEIGHT);
    s.label(tr(T::NavTools), TextStyle::Title, (X0, 18, CW, 36), p.bg, t);
    let tool_w = (CW - 32 - 8) / 2;
    let tool = |i: i32, label: T, icon: char| {
        s.button_icon(
            tr(label),
            icon,
            false,
            (
                X0 + 16 + (i % 2) * (tool_w + 8),
                TOOLS_Y + 16 + (i / 2) * 48,
                tool_w,
                40,
            ),
            p.surface,
            t,
        )
    };
    let tools_clean = tool(0, T::ToolClean, icons::TOOL_CLEAN);
    let tools_test = tool(1, T::ToolTest, icons::TOOL_TEST);
    let tools_map = tool(2, T::KeyMapTitle, icons::TOOL_MAP);
    let tools_practice = tool(3, T::PracticeTitle, icons::TOOL_PRACTICE);
    s.label(
        tr(T::HeadHealth),
        TextStyle::Section(icons::HEALTH),
        (X0, HEALTH_Y - 28, CW - 160, 20),
        p.bg,
        t,
    );
    let health_again = s.button_icon(
        tr(T::BtnCheckAgain),
        icons::REFRESH,
        false,
        (X0 + CW - 140, HEALTH_Y - 36, 140, 30),
        p.bg,
        t,
    );
    let checks = crate::health::run();
    let mut health_status = [0u16; crate::health::COUNT];
    let mut health_fix = [0u16; crate::health::COUNT];
    for (i, check) in checks.iter().enumerate().take(crate::health::COUNT) {
        let top = HEALTH_Y + 4 + i as i32 * ROW_H;
        let text_w = CW - 56 - 160;
        s.label(
            tr(check.title),
            TextStyle::Body,
            (X0 + 56, top + 8, text_w, 22),
            p.surface,
            t,
        );
        health_status[i] = s.label(
            "",
            TextStyle::Small,
            (X0 + 56, top + 30, text_w, 30),
            p.surface,
            t,
        );
        health_fix[i] = s.button(
            "",
            false,
            (X0 + CW - 16 - 140, top + 14, 140, 34),
            p.surface,
            t,
        );
    }

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
    let import_learned = s.button_icon(
        tr(T::BtnImport),
        icons::IMPORT,
        false,
        (X0, 428, 100, 34),
        p.bg,
        l,
    );
    let export_learned = s.button_icon(
        tr(T::BtnExport),
        icons::EXPORT,
        false,
        (X0 + 106, 428, 100, 34),
        p.bg,
        l,
    );
    let learned_status = s.label("", TextStyle::Small, (X0, 470, CW, 36), p.bg, l);
    let folder_label = s.label("", TextStyle::Small, (X0, 526, CW - 290, 40), p.bg, l);
    let sync_folder = s.button_icon(
        tr(T::BtnSyncFolder),
        icons::FOLDER,
        false,
        (X0 + CW - 284, 520, 150, 34),
        p.bg,
        l,
    );
    let this_pc = s.button_icon(
        tr(T::BtnThisPc),
        icons::APPS,
        false,
        (X0 + CW - 126, 520, 126, 34),
        p.bg,
        l,
    );
    let sync_settings = s.toggle_icon(
        tr(T::RowSyncSettings),
        tr(T::SubSyncSettingsShort),
        icons::FOLDER,
        (X0 + 4, 576, CW - 8, 64),
        p.surface,
        l,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubSyncSettings),
        (X0 + 4, 576, CW - 8, 64),
        true,
        tr(T::RowSyncSettings),
        p.surface,
        l,
    );
    let clear_learned = s.button_icon(
        tr(T::BtnClearAll),
        icons::REMOVE,
        false,
        (X0 + CW - 284, 428, 136, 34),
        p.bg,
        l,
    );
    let save_learned = s.button_icon(
        tr(T::BtnSaveLearned),
        icons::SAVE,
        true,
        (X0 + CW - 140, 428, 140, 34),
        p.bg,
        l,
    );

    // --- Snippets -----------------------------------------------------------
    let n = PAGE_SNIPPETS;
    s.label(
        tr(T::NavSnippets),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        n,
    );
    s.label(
        tr(T::SnippetsIntro),
        TextStyle::Dim,
        (X0, 58, CW, 40),
        p.bg,
        n,
    );
    let snip_table = s.table(
        &[
            (tr(T::ColTrigger), 110),
            (tr(T::ColText), 270),
            (tr(T::ColKeyboard), 110),
        ],
        (X0, 102, CW, 210),
        n,
    );
    s.label(
        tr(T::ColTrigger),
        TextStyle::Small,
        (X0, 324, 150, 18),
        p.bg,
        n,
    );
    let snip_trigger = s.line_edit("", (X0 + 4, 348, 142, 24), n);
    s.label(
        tr(T::ColText),
        TextStyle::Small,
        (X0 + 160, 324, CW - 160, 18),
        p.bg,
        n,
    );
    let snip_text = s.edit("", (X0 + 164, 348, CW - 168 - 126, 60), n);
    let snip_date = s.button_icon(
        tr(T::BtnInsertDate),
        icons::CALENDAR,
        false,
        (X0 + CW - 120, 348, 120, 32),
        p.bg,
        n,
    );
    let mut snip_scope = [0u16; 4];
    let mut left = X0 + 4;
    for (i, key) in [T::ScopeThai, T::ScopeEnglish, T::ScopeEither, T::ScopeTypo]
        .iter()
        .enumerate()
    {
        snip_scope[i] = s.segment(
            tr(*key),
            i == 0,
            (left, 426, SNIP_SCOPE_WIDTHS[i], 32),
            p.inset,
            n,
        );
        left += SNIP_SCOPE_WIDTHS[i];
    }
    let snip_save = s.button_icon(
        tr(T::BtnSaveSnippet),
        icons::SAVE,
        true,
        (X0 + CW - 206, 424, 106, 34),
        p.bg,
        n,
    );
    let snip_remove = s.button_icon(
        tr(T::BtnRemoveApp),
        icons::REMOVE,
        false,
        (X0 + CW - 94, 424, 94, 34),
        p.bg,
        n,
    );
    let snip_status = s.label("", TextStyle::Small, (X0, 466, CW, 22), p.bg, n);
    s.label(
        tr(T::SnippetsNote),
        TextStyle::Small,
        (X0, 492, CW, 110),
        p.bg,
        n,
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
            (tr(T::ColKeyboard), 110),
            (tr(T::ColSetBy), 120),
            (tr(T::ColWhere), 190),
        ],
        (X0, 102, CW, 232),
        b,
    );
    let seg_w = CW / 5;
    let mut app_seg = [0u16; 5];
    for (i, mode) in APP_MODE_CHOICES.iter().enumerate() {
        app_seg[i] = s.segment_icon(
            tr(crate::tray::app_mode_name(*mode)),
            [
                icons::MODE_AUTO,
                icons::MODE_SUGGEST,
                icons::MODE_MANUAL,
                icons::MODE_CODE,
                icons::MODE_OFF,
            ][i],
            i == 0,
            (X0 + 4 + i as i32 * seg_w, 346, seg_w - 2, 32),
            p.inset,
            b,
        );
    }
    let apps_add = s.button_icon(
        tr(T::BtnAddApp),
        icons::ADD,
        false,
        (X0, 392, 210, 34),
        p.bg,
        b,
    );
    let apps_keep = s.button_icon(
        tr(T::BtnKeepMode),
        icons::PIN,
        false,
        (X0 + 216, 392, 150, 34),
        p.bg,
        b,
    );
    let apps_remove = s.button_icon(
        tr(T::BtnRemoveApp),
        icons::REMOVE,
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
    let predict = s.toggle_icon(
        tr(T::RowPredict),
        tr(T::SubPredictShort),
        icons::GUESS,
        (X0 + 4, PREDICT_Y + 4, CW - 136, 68),
        p.surface,
        b,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubPredict),
        (X0 + 4, PREDICT_Y + 4, CW - 136, 68),
        true,
        tr(T::RowPredict),
        p.surface,
        b,
    );
    let clear_habits = s.button_icon(
        tr(T::BtnClearHabits),
        icons::REMOVE,
        false,
        (X0 + CW - 116, PREDICT_Y + 21, 100, 34),
        p.surface,
        b,
    );

    // --- Privacy ----------------------------------------------------------
    let a = PAGE_ABOUT;
    s.label(
        tr(T::HeadPrivacy),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        a,
    );

    // Card 1: Software info & Github link
    s.label(
        "RightType",
        TextStyle::BodyStrong,
        (X0 + 20, AB_CARD1_Y + 12, 280, 22),
        p.surface,
        a,
    );
    s.label(
        tr(T::PrivacyLicense),
        TextStyle::Dim,
        (X0 + 20, AB_CARD1_Y + 36, 320, 20),
        p.surface,
        a,
    );
    let link_github = s.button_icon(
        tr(T::BtnGitHub),
        icons::OPEN_LINK,
        false,
        (X0 + CW - 110, AB_CARD1_Y + 17, 94, 32),
        p.surface,
        a,
    );

    // Card 2: Privacy promises
    for (i, (head, sub)) in [
        (T::Privacy1, T::SubPrivacy1),
        (T::Privacy2, T::SubPrivacy2),
        (T::Privacy3, T::SubPrivacy3),
    ]
    .iter()
    .enumerate()
    {
        let y = AB_CARD2_Y + 8 + i as i32 * 58;
        s.label(
            tr(*head),
            TextStyle::BodyStrong,
            (X0 + 64, y + 6, CW - 84, 22),
            p.surface,
            a,
        );
        s.label(
            tr(*sub),
            TextStyle::Dim,
            (X0 + 64, y + 28, CW - 84, 20),
            p.surface,
            a,
        );
    }

    // Card 3: Updates
    s.label(
        tr(T::HeadUpdates),
        TextStyle::BodyStrong,
        (X0 + 20, AB_CARD3_Y + 14, 250, 22),
        p.surface,
        a,
    );
    s.label(
        tr(T::AboutUpdates),
        TextStyle::Dim,
        (X0 + 20, AB_CARD3_Y + 38, CW - 200, 40),
        p.surface,
        a,
    );
    let check_updates = s.button_icon(
        tr(T::BtnCheckUpdates),
        icons::REFRESH,
        false,
        (X0 + CW - 172, AB_CARD3_Y + 20, 156, 32),
        p.surface,
        a,
    );

    // Card 4: Crash recovery toggle
    let restart = s.toggle_icon(
        tr(T::RowRestart),
        tr(T::SubRestartShort),
        icons::RESTART,
        (X0 + 4, RESTART_Y + 4, CW - 8, 60),
        p.surface,
        a,
    );
    s.info_after(
        tr(T::InfoMore),
        tr(T::SubRestart),
        (X0 + 4, RESTART_Y + 4, CW - 8, 60),
        true,
        tr(T::RowRestart),
        p.surface,
        a,
    );
    let sp = PAGE_SPONSOR;

    if ui::windows_autocorrects() {
        s.label(
            tr(T::AboutWindowsAutocorrect),
            TextStyle::Small,
            (X0, RESTART_Y + 70, CW, 36),
            p.bg,
            a,
        );
    }

    // --- Sponsor / Support ------------------------------------------------

    s.label(
        tr(T::HeadSponsor),
        TextStyle::Title,
        (X0, 18, CW, 36),
        p.bg,
        sp,
    );

    // Card 1: Free and open source
    s.label(
        tr(T::SponsorFreeHead),
        TextStyle::BodyStrong,
        (X0 + 64, SP_CARD1_Y + 12, CW - 84, 22),
        p.surface,
        sp,
    );
    s.label(
        tr(T::SponsorFreeSub),
        TextStyle::Dim,
        (X0 + 64, SP_CARD1_Y + 36, CW - 84, 42),
        p.surface,
        sp,
    );

    // Card 2: Support options
    s.label(
        tr(T::SponsorChannelHead),
        TextStyle::BodyStrong,
        (X0 + 20, SP_CARD2_Y + 14, 200, 24),
        p.surface,
        sp,
    );
    let tip_seg_w = 88;
    let tip_ln = s.segment(
        "Lightning",
        true,
        (
            X0 + CW - 20 - 2 * tip_seg_w - 4,
            SP_CARD2_Y + 12,
            tip_seg_w,
            28,
        ),
        p.inset,
        sp,
    );
    let tip_onchain = s.segment(
        "Bitcoin",
        false,
        (X0 + CW - 20 - tip_seg_w, SP_CARD2_Y + 12, tip_seg_w, 28),
        p.inset,
        sp,
    );

    let tip_desc = s.label(
        tr(T::SponsorLnDesc),
        TextStyle::Dim,
        (X0 + 172, SP_CARD2_Y + 54, 312, 22),
        p.surface,
        sp,
    );
    let tip_address = s.label(
        &address_lines(BTC_LIGHTNING),
        TextStyle::SmallMiddle,
        (X0 + 180, SP_CARD2_Y + 84, 218, 34),
        p.inset,
        sp,
    );
    let tip_copy = s.button_icon(
        tr(T::BtnCopy),
        icons::COPY,
        false,
        (X0 + CW - 20 - 80, SP_CARD2_Y + 85, 76, 32),
        p.surface,
        sp,
    );
    s.label(
        tr(T::SponsorQrNote),
        TextStyle::Dim,
        (X0 + 172, SP_CARD2_Y + 128, 312, 44),
        p.surface,
        sp,
    );

    let ids = Ids {
        nav,
        snip_table,
        snip_trigger,
        snip_text,
        snip_date,
        snip_scope,
        snip_save,
        snip_remove,
        snip_status,
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
        manoonchai,
        kb_addresses,
        kb_complete,
        kb_grave,
        kb_guard,
        kb_accents,
        kb_shortcuts,
        kb_ctrl_hold,
        kb_password_tag,
        kb_numlock,
        kb_insert,
        kb_scanner,
        kb_fake,
        kb_show_keys,
        kb_rest,
        kb_caret_list,
        kb_ghosts,
        kb_cycle,
        tools_clean,
        tools_test,
        tools_map,
        tools_practice,
        health_again,
        health_status,
        health_fix,
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
        sync_settings,
        link_github,
        tip_ln,
        tip_onchain,
        tip_address,
        tip_desc,
        tip_copy,
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
    fill_snippets(&win, None);
    win.surface.set_checked(win.ids.snip_scope[2], true);
    sync(&win);
    if let Some(i) = NAV.iter().position(|(_, p, _)| *p == page) {
        win.surface.set_checked(win.ids.nav[i], true);
    }
    win.surface.show_page(page);
    if page == PAGE_TOOLS {
        fill_health(&win);
    }
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
    win.surface.on_table(move |id, event| {
        if let Some(win) = weak.upgrade() {
            if id == win.ids.snip_table {
                snippet_table_event(&win, event);
            } else {
                app_table_event(&win, event);
            }
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
    s.set_checked(
        ids.sync_settings,
        config::SYNC_SETTINGS.load(Ordering::Relaxed),
    );
    s.set_checked(ids.predict, crate::habits::is_enabled());
    s.set_checked(
        ids.restart,
        config::RESTART_AFTER_CRASH.load(Ordering::Relaxed),
    );
    sync_app_choice(win);
    let variant = righttype::layout::thai_variant();
    s.set_checked(
        ids.kedmanee,
        variant == righttype::layout::ThaiVariant::Kedmanee,
    );
    s.set_checked(
        ids.pattachote,
        variant == righttype::layout::ThaiVariant::Pattachote,
    );
    s.set_checked(
        ids.manoonchai,
        variant == righttype::layout::ThaiVariant::Manoonchai,
    );
    s.set_checked(ids.kb_addresses, righttype::policy::fixes_addresses());
    s.set_checked(ids.kb_complete, hook::completes_thai());
    s.set_checked(ids.kb_grave, hook::grave_types());
    s.set_checked(ids.kb_guard, hook::guards_switch());
    s.set_checked(ids.kb_accents, hook::holds_for_accents());
    s.set_checked(ids.kb_shortcuts, hook::shortcuts_in_english());
    s.set_checked(ids.kb_ctrl_hold, hook::ctrl_hold_opens_sheet());
    s.set_checked(ids.kb_password_tag, crate::pwhint::is_enabled());
    s.set_checked(ids.kb_scanner, hook::fixes_scanners());
    s.set_checked(ids.kb_fake, hook::guards_fake_keyboards());
    s.set_checked(ids.kb_show_keys, crate::onscreen::enabled());
    s.set_checked(ids.kb_rest, crate::rest::enabled());
    s.set_checked(ids.kb_caret_list, crate::caretlist::enabled());
    s.set_checked(ids.kb_ghosts, hook::ghosts());
    s.set_checked(ids.kb_cycle, hook::cycles_characters());
    for (j, id) in ids.kb_numlock.iter().enumerate() {
        s.set_checked(*id, KEY_GUARDS[j] == hook::numlock_mode());
    }
    for (j, id) in ids.kb_insert.iter().enumerate() {
        s.set_checked(*id, KEY_GUARDS[j] == hook::insert_mode());
    }
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
    let ln = TIP_NETWORK.load(Ordering::Relaxed) == 0;
    s.set_checked(ids.tip_ln, ln);
    s.set_checked(ids.tip_onchain, !ln);
    if ln {
        s.set_text(ids.tip_address, &address_lines(BTC_LIGHTNING));
        s.set_text(ids.tip_desc, tr(T::SponsorLnDesc));
    } else {
        s.set_text(ids.tip_address, &address_lines(BTC_MAINNET));
        s.set_text(ids.tip_desc, tr(T::SponsorBtcDesc));
    }
}

fn clicked(win: &Rc<SettingsWindow>, id: u16) {
    let ids = &win.ids;
    let s = &win.surface;
    if id == ids.link_github {
        open_repo_page();
        return;
    }
    if id == ids.tip_ln {
        TIP_NETWORK.store(0, Ordering::Relaxed);
        sync(win);
        ui::redraw_all(s.hwnd);
        return;
    }
    if id == ids.tip_onchain {
        TIP_NETWORK.store(1, Ordering::Relaxed);
        sync(win);
        ui::redraw_all(s.hwnd);
        return;
    }
    if id == ids.tip_copy || id == ids.tip_address {
        let addr = if TIP_NETWORK.load(Ordering::Relaxed) == 0 {
            BTC_LIGHTNING
        } else {
            BTC_MAINNET
        };
        unsafe {
            let _ = crate::clipboard::set_text(addr);
        }
        overlay::show(tr(T::ToastCopied));
        return;
    }
    if let Some(i) = ids.nav.iter().position(|&n| n == id) {
        for (idx, &btn) in ids.nav.iter().enumerate() {
            s.set_checked(btn, idx == i);
        }
        s.show_page(NAV[i].1);
        if NAV[i].1 == PAGE_TOOLS {
            fill_health(win);
        }
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
    } else if id == ids.sync_settings {
        let on = s.checked(ids.sync_settings);
        if on && learn::folder().is_none() {
            s.set_checked(ids.sync_settings, false);
            s.set_text(ids.learned_status, tr(T::SyncNeedsFolder));
            return;
        }
        config::set_sync_settings(on);
        // Settings from the folder may have changed what this window shows.
        fill_apps(win, None);
        fill_snippets(win, None);
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
    } else if id == ids.snip_date {
        insert_date_field(win);
    } else if id == ids.snip_save {
        save_snippet(win);
    } else if id == ids.snip_remove {
        remove_snippet(win);
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
    } else if id == ids.kedmanee || id == ids.pattachote || id == ids.manoonchai {
        righttype::layout::set_thai_variant(if id == ids.pattachote {
            righttype::layout::ThaiVariant::Pattachote
        } else if id == ids.manoonchai {
            righttype::layout::ThaiVariant::Manoonchai
        } else {
            righttype::layout::ThaiVariant::Kedmanee
        });
        config::persist();
    } else if id == ids.tools_clean {
        crate::clean::request_open(crate::clean::Mode::Clean);
    } else if id == ids.tools_test {
        crate::clean::request_open(crate::clean::Mode::Test);
    } else if id == ids.tools_map {
        crate::keymap::request_toggle();
    } else if id == ids.tools_practice {
        crate::practice::request_open();
    } else if id == ids.health_again {
        fill_health(win);
    } else if let Some(i) = ids.health_fix.iter().position(|&b| b == id) {
        let fix = HEALTH.with(|h| h.borrow().get(i).and_then(|c| c.fix.clone()));
        if let Some((_, fix)) = fix {
            crate::health::apply(&fix);
            fill_health(win);
        }
    } else if id == ids.kb_addresses {
        righttype::policy::set_fixes_addresses(s.checked(id));
        config::persist();
    } else if id == ids.kb_complete {
        hook::set_completes_thai(s.checked(id));
        config::persist();
    } else if id == ids.kb_grave {
        hook::set_grave_types(s.checked(id));
        config::persist();
    } else if id == ids.kb_ctrl_hold {
        hook::set_ctrl_hold_opens_sheet(s.checked(id));
        config::persist();
    } else if id == ids.kb_shortcuts {
        hook::set_shortcuts_in_english(s.checked(id));
        config::persist();
    } else if id == ids.kb_accents {
        hook::set_holds_for_accents(s.checked(id));
        config::persist();
    } else if id == ids.kb_guard {
        hook::set_guards_switch(s.checked(id));
        config::persist();
    } else if id == ids.kb_scanner {
        hook::set_fixes_scanners(s.checked(id));
        config::persist();
    } else if id == ids.kb_fake {
        hook::set_guards_fake_keyboards(s.checked(id));
        config::persist();
    } else if id == ids.kb_show_keys {
        crate::onscreen::set_enabled(s.checked(id));
        config::persist();
    } else if id == ids.kb_rest {
        crate::rest::set_enabled(s.checked(id));
        config::persist();
    } else if id == ids.kb_caret_list {
        crate::caretlist::set_enabled(s.checked(id));
        config::persist();
    } else if id == ids.kb_ghosts {
        hook::set_ghosts(s.checked(id));
        config::persist();
    } else if id == ids.kb_cycle {
        hook::set_cycles_characters(s.checked(id));
        config::persist();
    } else if id == ids.kb_password_tag {
        crate::pwhint::set_enabled(s.checked(id));
        config::persist();
    } else if let Some(j) = ids.kb_numlock.iter().position(|&b| b == id) {
        hook::set_numlock_mode(KEY_GUARDS[j]);
        config::persist();
    } else if let Some(j) = ids.kb_insert.iter().position(|&b| b == id) {
        hook::set_insert_mode(KEY_GUARDS[j]);
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

// --------------------------------------------------------- Snippets page

const SCOPES: [righttype::snippets::Scope; 4] = [
    righttype::snippets::Scope::Thai,
    righttype::snippets::Scope::English,
    righttype::snippets::Scope::Either,
    righttype::snippets::Scope::Typo,
];

/// Fill the Snippets table, selecting the snippet `trigger` when given.
fn fill_snippets(win: &SettingsWindow, trigger: Option<&str>) {
    let list = hook::snippets();
    let cells: Vec<Vec<String>> = list
        .iter()
        .map(|sn| {
            let text: String = sn.text.replace('\n', " \\n ");
            let scope = tr(match sn.scope {
                righttype::snippets::Scope::Thai => T::ScopeThai,
                righttype::snippets::Scope::English => T::ScopeEnglish,
                righttype::snippets::Scope::Either => T::ScopeEither,
                righttype::snippets::Scope::Typo => T::ScopeTypo,
            });
            vec![sn.trigger.clone(), text, scope.to_string()]
        })
        .collect();
    let select = trigger.and_then(|t| list.iter().position(|sn| sn.trigger == t));
    win.surface.set_rows(win.ids.snip_table, &cells, select);
}

fn snippet_table_event(win: &Rc<SettingsWindow>, event: ui::TableEvent) {
    match event {
        ui::TableEvent::Delete => remove_snippet(win),
        ui::TableEvent::Selected | ui::TableEvent::Activated | ui::TableEvent::Menu { .. } => {
            let Some(i) = win.surface.selected_row(win.ids.snip_table) else {
                return;
            };
            let Some(sn) = hook::snippets().into_iter().nth(i) else {
                return;
            };
            let s = &win.surface;
            s.set_text(win.ids.snip_trigger, &sn.trigger);
            s.set_text(win.ids.snip_text, &sn.text.replace('\n', "\r\n"));
            for (k, scope) in SCOPES.iter().enumerate() {
                s.set_checked(win.ids.snip_scope[k], *scope == sn.scope);
            }
        }
    }
}

/// Add the snippet in the boxes, or replace the one with its trigger.
fn save_snippet(win: &SettingsWindow) {
    use righttype::snippets::{check, Problem, MAX_SNIPPETS};
    let s = &win.surface;
    let scope = (0..SCOPES.len())
        .find(|&k| s.checked(win.ids.snip_scope[k]))
        .map_or(righttype::snippets::Scope::Either, |k| SCOPES[k]);
    let mut text = s.text_of(win.ids.snip_text);
    let checked = check(&s.text_of(win.ids.snip_trigger), &text, scope);
    text.zeroize();
    let snippet = match checked {
        Ok(sn) => sn,
        Err(problem) => {
            s.set_text(
                win.ids.snip_status,
                tr(match problem {
                    Problem::TriggerLength => T::SnipTriggerLength,
                    Problem::TriggerSpace => T::SnipTriggerSpace,
                    Problem::TextEmpty => T::SnipTextEmpty,
                    Problem::TextLong => T::SnipTextLong,
                    Problem::MacroStep => T::SnipMacroStep,
                }),
            );
            return;
        }
    };
    let mut list = hook::snippets();
    let trigger = snippet.trigger.clone();
    match list.iter().position(|sn| sn.trigger == trigger) {
        Some(i) => list[i] = snippet,
        None if list.len() >= MAX_SNIPPETS => {
            s.set_text(win.ids.snip_status, tr(T::SnipTooMany));
            return;
        }
        None => list.push(snippet),
    }
    hook::set_snippets(list);
    config::persist();
    fill_snippets(win, Some(&trigger));
    s.set_text(win.ids.snip_status, tr(T::AppsSaved));
}

fn remove_snippet(win: &SettingsWindow) {
    let s = &win.surface;
    let Some(i) = s.selected_row(win.ids.snip_table) else {
        s.set_text(win.ids.snip_status, tr(T::AppsPickFirst));
        return;
    };
    let mut list = hook::snippets();
    if i < list.len() {
        list.remove(i);
    }
    hook::set_snippets(list);
    config::persist();
    fill_snippets(win, None);
    s.set_text(win.ids.snip_trigger, "");
    s.set_text(win.ids.snip_text, "");
    s.set_text(win.ids.snip_status, tr(T::SnipRemoved));
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
    // Apps with only a keyboard of their own.
    for exe in crate::apps::all_keyboards().into_keys() {
        if !rows.iter().any(|r| r.exe == exe) {
            rows.push(AppRow {
                exe,
                mode: Some(hook::mode().into()),
                source: RowSource::KeyboardOnly,
            });
        }
    }
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
                (_, RowSource::KeyboardOnly) => "—".to_string(),
                (None, _) => tr(T::ModeBlocked).to_string(),
                (Some(m), _) => tr(crate::tray::app_mode_name(m)).to_string(),
            };
            let by = tr(match r.source {
                RowSource::ForNow => T::SetByForNow,
                RowSource::Chosen | RowSource::KeyboardOnly => T::SetByYou,
                RowSource::Blocked => T::SetByYouBlocked,
                RowSource::Default => T::SetByDefault,
                RowSource::Safety => T::SetBySafety,
            });
            let place = paths.get(&r.exe).cloned().unwrap_or_else(|| "—".into());
            let keyboard = match crate::apps::keyboard(&r.exe) {
                None => "—".to_string(),
                k => tr(crate::palette::keyboard_name(k)).to_string(),
            };
            vec![r.exe.clone(), mode, keyboard, by.to_string(), place]
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
        ui::TableEvent::Selected | ui::TableEvent::Activated => sync_app_choice(win),
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
        RowSource::KeyboardOnly => {}
        RowSource::Blocked => {
            let rest: Vec<String> = safety::custom_list()
                .into_iter()
                .filter(|e| *e != row.exe)
                .collect();
            safety::set_custom_list(rest);
        }
    }
    // Removing an app forgets its keyboard too.
    crate::apps::set_keyboard(&row.exe, None);
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
    // The keyboard the app starts with.
    let keyboards: Vec<Option<righttype::per_app::AppKeyboard>> = std::iter::once(None)
        .chain(righttype::per_app::AppKeyboard::ALL.map(Some))
        .collect();
    let current = crate::apps::keyboard(&row.exe);
    for k in &keyboards {
        let mark = if *k == current { "✓  " } else { "" };
        labels.push(format!(
            "{mark}{}: {}",
            tr(T::ColKeyboard),
            tr(crate::palette::keyboard_name(*k))
        ));
    }
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
    if i < n + keyboards.len() {
        crate::apps::set_keyboard(&row.exe, keyboards[i - n]);
        config::persist();
        fill_apps(win, Some(&row.exe));
        return;
    }
    let mut rest = i - n - keyboards.len();
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

/// The date and time fields, each with what it gives today, under the
/// button; the one picked goes into the snippet's text at the caret.
fn insert_date_field(win: &SettingsWindow) {
    use righttype::snippets::{fill, FIELDS};
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, SendMessageW};
    let now = crate::hook::snippet_now();
    let labels: Vec<String> = FIELDS
        .iter()
        .map(|(name, _, _)| format!("{name}\t{}", fill(name, &now)))
        .collect();
    let mut rc = windows::Win32::Foundation::RECT::default();
    unsafe {
        let _ = GetWindowRect(win.surface.hwnd_of(win.ids.snip_date), &mut rc);
    }
    let Some(i) = popup(win, &labels, rc.left, rc.bottom) else {
        return;
    };
    let field: Vec<u16> = format!("{}\0", FIELDS[i].0).encode_utf16().collect();
    let edit = win.surface.hwnd_of(win.ids.snip_text);
    unsafe {
        const EM_REPLACESEL: u32 = 0x00C2;
        let _ = SendMessageW(
            edit,
            EM_REPLACESEL,
            windows::Win32::Foundation::WPARAM(1),
            windows::Win32::Foundation::LPARAM(field.as_ptr() as isize),
        );
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(edit);
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
        usize::try_from(picked.0)
            .ok()
            .filter(|&p| p > 0)
            .map(|p| p - 1)
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
const REPO_URL: &str = "https://github.com/k-affan-th/RightType";
const RELEASES_URL: &str = "https://github.com/k-affan-th/RightType/releases/latest";

fn open_repo_page() {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let verb: Vec<u16> = "open\0".encode_utf16().collect();
    let url: Vec<u16> = format!("{REPO_URL}\0").encode_utf16().collect();
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

fn draw_qr(hdc: HDC, x0: i32, y0: i32, size: i32, matrix: &[u64; 33]) {
    let cell = ((size * 78) / 100) / 33;
    let cell = cell.max(1);
    let qr_w = cell * 33;
    let pad = (size - qr_w) / 2;
    unsafe {
        let brush = windows::Win32::Graphics::Gdi::CreateSolidBrush(
            windows::Win32::Foundation::COLORREF(0x00000000),
        );
        for (r, &row) in matrix.iter().enumerate() {
            for c in 0..33 {
                if (row & (1 << c)) != 0 {
                    let rx = x0 + pad + c * cell;
                    let ry = y0 + pad + r as i32 * cell;
                    let rect = windows::Win32::Foundation::RECT {
                        left: rx,
                        top: ry,
                        right: rx + cell,
                        bottom: ry + cell,
                    };
                    windows::Win32::Graphics::Gdi::FillRect(hdc, &rect, brush);
                }
            }
        }
        let _ = windows::Win32::Graphics::Gdi::DeleteObject(brush);
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

/// An address as shown: on two lines when it is longer than one line of the
/// box holds (a drawn word is never broken in the middle, so the line break
/// is put in). What Copy puts on the clipboard is the address itself.
fn address_lines(addr: &str) -> String {
    const ONE_LINE: usize = 34;
    let n = addr.chars().count();
    if n <= ONE_LINE {
        return addr.to_string();
    }
    let cut = n.div_ceil(2);
    let first: String = addr.chars().take(cut).collect();
    let rest: String = addr.chars().skip(cut).collect();
    format!(
        "{first}
{rest}"
    )
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
            // Saved either way; the typist is told when the keys were
            // already someone else's (RightType's hook sees them first).
            let note = if let Some(what) = righttype::hotkeys::common_use(chord) {
                trf(T::HkSavedCommon, &[("v", tr(what))])
            } else if registered_elsewhere(chord) {
                tr(T::HkSavedElsewhere).to_string()
            } else {
                tr(T::ToastSaved).to_string()
            };
            win.surface.set_text(status, &note);
        }
        Err(Refusal::Unusable) => win.surface.set_text(status, tr(T::HkUnusable)),
        Err(Refusal::Taken(other)) => win
            .surface
            .set_text(status, &trf(T::HkTaken, &[("v", tr(action_label(other)))])),
    }
}

/// Has another program registered `chord` as its own system-wide hotkey?
/// Trying to register it tells (and it is let go at once).
fn registered_elsewhere(chord: righttype::hotkeys::Chord) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
        MOD_SHIFT,
    };
    const PROBE: i32 = 0xBF00;
    let mut mods = MOD_NOREPEAT;
    for (on, m) in [
        (chord.ctrl, MOD_CONTROL),
        (chord.shift, MOD_SHIFT),
        (chord.alt, MOD_ALT),
    ] {
        if on {
            mods = HOT_KEY_MODIFIERS(mods.0 | m.0);
        }
    }
    unsafe {
        if RegisterHotKey(None, PROBE, mods, chord.key as u32).is_ok() {
            let _ = UnregisterHotKey(None, PROBE);
            false
        } else {
            true
        }
    }
}

/// Switch page as if its sidebar entry had been clicked.
fn go_to(win: &SettingsWindow, page: u8) {
    for (i, nav) in win.ids.nav.iter().enumerate() {
        win.surface.set_checked(*nav, NAV[i].1 == page);
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
    track(g, rect(16, H - 80, 188, 34));
    match page {
        PAGE_GENERAL => {
            card(g, rect(X0, 98, CW, 132));
            track(g, rect(X0 + 16, 110, CW - 32, 48));
            card(g, rect(X0, CARD_B_Y, CW, 5 * ROW_H + 64));
            for i in 1..=5 {
                divider(hdc, X0 + 16, CARD_B_Y + i * ROW_H + 2, CW - 32);
            }
        }
        PAGE_TOOLS => {
            let dy = ui::page_scroll();
            card(g, rect(X0, TOOLS_Y - dy, CW, 120));
            card(
                g,
                rect(
                    X0,
                    HEALTH_Y - dy,
                    CW,
                    crate::health::COUNT as i32 * ROW_H + 8,
                ),
            );
            let ok = HEALTH.with(|h| h.borrow().iter().map(|c| c.ok).collect::<Vec<_>>());
            for (i, ok) in ok.iter().enumerate() {
                let top = HEALTH_Y + 4 + i as i32 * ROW_H - dy;
                if i > 0 {
                    divider(hdc, X0 + 16, top - 2, CW - 32);
                }
                // A check mark when fine; an accent "!" where something is
                // off, so problems stand out in the list.
                let c = rect(X0 + 18, top + 18, 24, 24);
                let (fill, ink, mark) = if *ok {
                    (pal().inset, pal().text_dim, "✓")
                } else {
                    (pal().accent, pal().on_accent, "!")
                };
                g.fill_round(c, ui::px(12) as f32, fill);
                if *ok {
                    g.check_mark(c, ink);
                } else {
                    ui::glyph(hdc, mark, c, ink);
                }
            }
        }
        PAGE_KEYBOARD => {
            // The page scrolls: everything moves up by the scroll.
            let dy = ui::page_scroll();
            card(g, rect(X0, KB_PICK_Y - dy, CW, 100));
            track(g, rect(X0 + 16, KB_PICK_Y + 12 - dy, CW - 32, 48));
            card(g, rect(X0, KB_TYPING_Y - dy, CW, 7 * KB_ROW_H + 8));
            for i in 1..7 {
                divider(hdc, X0 + 16, KB_TYPING_Y + i * KB_ROW_H + 2 - dy, CW - 32);
            }
            card(g, rect(X0, KB_KEYS_Y - dy, CW, 3 * KB_ROW_H + 8));
            card(g, rect(X0, KB_DEV_Y - dy, CW, 2 * KB_ROW_H + 8));
            divider(hdc, X0 + 16, KB_DEV_Y + KB_ROW_H + 2 - dy, CW - 32);
            card(g, rect(X0, KB_SHOW_Y - dy, CW, 2 * KB_ROW_H + 8));
            divider(hdc, X0 + 16, KB_SHOW_Y + KB_ROW_H + 2 - dy, CW - 32);
            card(g, rect(X0, KB_CARET_Y - dy, CW, 3 * KB_ROW_H + 8));
            divider(hdc, X0 + 16, KB_CARET_Y + KB_ROW_H + 2 - dy, CW - 32);
            divider(hdc, X0 + 16, KB_CARET_Y + 2 * KB_ROW_H + 2 - dy, CW - 32);
            for i in 1..3 {
                divider(hdc, X0 + 16, KB_KEYS_Y + i * KB_ROW_H + 2 - dy, CW - 32);
                let icon = [icons::NUMPAD, icons::INSERT_KEY][i as usize - 1];
                ui::icon_tile(
                    g,
                    hdc,
                    rect(X0 + 16, KB_KEYS_Y + 4 + i * KB_ROW_H + 18 - dy, 32, 32),
                    icon,
                    false,
                );
                let x = X0 + CW - 24 - 3 * KB_SEG_W;
                track(
                    g,
                    rect(
                        x,
                        KB_KEYS_Y + 4 + i * KB_ROW_H + 14 - dy,
                        3 * KB_SEG_W + 8,
                        40,
                    ),
                );
            }
        }
        PAGE_HOTKEYS => {
            card(g, rect(X0, 68, CW, Action::ALL.len() as i32 * 52 + 8));
            for i in 1..Action::ALL.len() as i32 {
                divider(hdc, X0 + 16, 74 + i * 52 - 1, CW - 32);
            }
        }
        PAGE_LEARNED => {
            field(g, rect(X0, 142, CW, 268));
            card(g, rect(X0, 572, CW, 72));
        }
        PAGE_SNIPPETS => {
            field(g, rect(X0, 344, 150, 32));
            field(g, rect(X0 + 160, 344, CW - 160, 68));
            track(
                g,
                rect(X0, 422, SNIP_SCOPE_WIDTHS.iter().sum::<i32>() + 8, 40),
            );
        }
        PAGE_BLOCKED => {
            track(g, rect(X0, 342, CW, 40));
            card(g, rect(X0, PREDICT_Y, CW, 76));
        }
        PAGE_ABOUT => {
            card(g, rect(X0, AB_CARD1_Y, CW, AB_CARD1_H));
            card(g, rect(X0, AB_CARD2_Y, CW, AB_CARD2_H));
            for i in 1..3 {
                let y = AB_CARD2_Y + 8 + i * 58;
                divider(hdc, X0 + 20, y - 2, CW - 40);
            }
            for i in 0..3 {
                let y = AB_CARD2_Y + 8 + i * 58;
                ui::icon_tile(g, hdc, rect(X0 + 20, y + 11, 32, 32), icons::CHECK, true);
            }
            card(g, rect(X0, AB_CARD3_Y, CW, AB_CARD3_H));
            card(g, rect(X0, RESTART_Y, CW, 68));
        }
        PAGE_SPONSOR => {
            card(g, rect(X0, SP_CARD1_Y, CW, SP_CARD1_H));
            ui::icon_tile(
                g,
                hdc,
                rect(X0 + 20, SP_CARD1_Y + (SP_CARD1_H - 32) / 2, 32, 32),
                icons::SUPPORT,
                true,
            );
            card(g, rect(X0, SP_CARD2_Y, CW, SP_CARD2_H));
            track(
                g,
                rect(X0 + CW - 20 - 2 * 88 - 6, SP_CARD2_Y + 10, 2 * 88 + 6, 32),
            );

            let qr_box = rect(X0 + 20, SP_CARD2_Y + 54, 136, 136);
            g.fill_round(qr_box, ui::px(8) as f32, 0x00FFFFFF);
            let is_ln = TIP_NETWORK.load(Ordering::Relaxed) == 0;
            let qr_matrix = if is_ln { &QR_LIGHTNING } else { &QR_BITCOIN };
            draw_qr(
                hdc,
                ui::px(X0 + 20),
                ui::px(SP_CARD2_Y + 54),
                ui::px(136),
                qr_matrix,
            );

            let addr_box = rect(X0 + 172, SP_CARD2_Y + 82, 312, 38);
            field(g, addr_box);
        }
        _ => {}
    }
}
