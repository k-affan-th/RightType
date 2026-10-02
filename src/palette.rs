//! The command palette — Windows only.
//!
//! One hotkey (Ctrl+Alt+Space by default) opens a short list of commands next
//! to the text cursor: pause, switch mode, turn RightType off in this app,
//! fix a piece of text, settings. Arrow keys and Enter pick one, Esc closes;
//! focus goes back to where you were typing.
//!
//! The palette never touches text: every command is a setting or opens a
//! window. The app it acts on ("this app") is the one that had focus when
//! the palette was opened.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicIsize, Ordering};
use zeroize::Zeroize;

use native_windows_gui as nwg;
use righttype::i18n::{tr, trf, T};
use righttype::per_app::AppMode;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MONITOR_DEFAULTTONEAREST};
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, KillTimer, SetForegroundWindow, SetTimer, SetWindowPos, HWND_TOPMOST,
    SWP_NOSIZE,
};

use crate::ui::{self, pal, Gfx, Surface, TextStyle};
use crate::{apps, config, hook, overlay, session};

static OPEN: AtomicIsize = AtomicIsize::new(0);
/// The window that had focus when the palette last opened.
static PREVIOUS: AtomicIsize = AtomicIsize::new(0);

const W: i32 = 400;
const ROW: i32 = 34;
/// A section heading's height.
const HEAD: i32 = 24;
const PAD: i32 = 10;
const WM_COMMAND: u32 = 0x0111;
const WM_ACTIVATE: u32 = 0x0006;
const IDCANCEL: usize = 2;

/// What a "selection" command does to the text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum TransformKind {
    Digits,
    Upper,
    Lower,
    Title,
    SwapCase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    FixText,
    Pause,
    Resume,
    AppOff,
    AppDefault,
    Mode(hook::Mode),
    Settings,
    /// Fix every wrong-layout word in the field that had focus.
    FixField,
    /// The tray icon shows TH / EN, on or off.
    TrayLanguage,
    /// CapsLock as a language key, on or off.
    CapsSwitch,
    /// Common Thai misspellings put right, on or off.
    Spelling,
    /// Rewrite the selection (digits, letter case).
    Transform(TransformKind),
    /// A recent word (by its place in the hook's list, oldest first): flip
    /// it, or every ticked one.
    History(usize),
    /// The calmer mode on offer (see `apps::note_rejection`): for now, or
    /// for good.
    OfferForNow(AppMode),
    OfferKeep(AppMode),
    /// Off (or back on) in the field that had focus.
    FieldOff,
    FieldOn,
    /// Keep a word the typist reversed lately as typed (by its place in
    /// `learn::reversed_words`).
    KeepAsTyped(usize),
    /// Show the options section (folded away at first).
    MoreOptions,
    /// English prefix words written with their hyphen, on or off.
    Hyphens,
}

/// The palette's sections, top to bottom. Things done to text come first
/// (numbered 1–9); settings come last, folded away.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    /// The words just typed.
    Words,
    /// Fixing text.
    Fix,
    /// The selected text.
    Selection,
    /// The app (and field) that had focus.
    Here,
    Mode,
    /// Switches and Settings (folded until asked for or searched).
    Options,
}

impl Section {
    const ALL: [Section; 6] = [
        Section::Words,
        Section::Fix,
        Section::Selection,
        Section::Here,
        Section::Mode,
        Section::Options,
    ];

    /// Rows here get the numbers 1–9.
    fn numbered(self) -> bool {
        !matches!(self, Section::Mode | Section::Options)
    }

    /// The heading's icon (a glyph of Windows' icon font).
    fn icon(self) -> char {
        match self {
            Section::Words => '\u{E81C}',     // History
            Section::Fix => '\u{E90F}',       // Repair
            Section::Selection => '\u{E8B3}', // SelectAll
            Section::Here => '\u{E7F4}',      // TVMonitor
            Section::Mode => '\u{E9E9}',      // Equalizer
            Section::Options => '\u{E713}',   // Settings
        }
    }
}

impl Command {
    /// The row's icon (a glyph of Windows' icon font), so the eye finds a
    /// row by its shape before reading it.
    fn icon(self) -> char {
        match self {
            Command::History(_) => '\u{E8AB}',     // Switch
            Command::KeepAsTyped(_) => '\u{E7A7}', // Undo
            Command::FixText => '\u{E8A5}',        // Document
            Command::FixField => '\u{E8AC}',       // Rename
            Command::Transform(kind) => match kind {
                TransformKind::Digits => '\u{E8EF}',   // Calculator
                TransformKind::Upper => '\u{E8E8}',    // FontIncrease
                TransformKind::Lower => '\u{E8E7}',    // FontDecrease
                TransformKind::Title => '\u{E8D2}',    // Font
                TransformKind::SwapCase => '\u{E895}', // Sync
            },
            Command::OfferForNow(_) => '\u{E916}', // Stopwatch
            Command::OfferKeep(_) => '\u{E74E}',   // Save
            Command::Pause => '\u{E769}',          // Pause
            Command::Resume => '\u{E768}',         // Play
            Command::FieldOff => '\u{E733}',       // Blocked
            Command::FieldOn => '\u{E73E}',        // CheckMark
            Command::AppOff | Command::AppDefault => '\u{E7E8}', // PowerButton
            Command::Mode(mode) => match mode {
                hook::Mode::Auto => '\u{E945}',    // LightningBolt
                hook::Mode::Suggest => '\u{EA80}', // Lightbulb
                _ => '\u{E765}',                   // KeyboardClassic
            },
            Command::MoreOptions => '\u{E712}',  // More
            Command::Spelling => '\u{E82D}',     // Dictionary
            Command::Hyphens => '\u{E738}',      // Remove (a dash)
            Command::CapsSwitch => '\u{E72E}',   // Lock
            Command::TrayLanguage => '\u{E774}', // Globe
            Command::Settings => '\u{E713}',     // Settings
        }
    }
}

/// One row: its section, text, hint at the right, and command.
struct Entry {
    section: Section,
    label: String,
    hint: String,
    command: Command,
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.label.zeroize();
    }
}

struct Palette {
    window: nwg::Window,
    surface: Rc<Surface>,
    items: Vec<(u16, Command)>,
    /// Each item's section.
    item_sections: Vec<Section>,
    /// Each section's heading (a label).
    headings: Vec<(Section, u16)>,
    /// The options section is open.
    expanded: std::cell::Cell<bool>,
    /// Which shown item each number key (1–9) runs.
    numbers: RefCell<Vec<usize>>,
    /// Each item's text, for filtering and numbering.
    labels: RefCell<Vec<String>>,
    /// The recent words listed (oldest first), wiped on close.
    words: RefCell<Vec<String>>,
    /// Where each recent word is on screen, when the app says.
    boxes: Vec<Option<RECT>>,
    /// Recent words ticked with Space (their places in `words`).
    checked: RefCell<Vec<usize>>,
    /// The line above the list: how to use it, or what has been typed.
    header: u16,
    /// Items shown (indices into `items`), in order, after filtering.
    visible: RefCell<Vec<usize>>,
    /// Which shown item Enter runs.
    selected: std::cell::Cell<usize>,
    /// What has been typed to filter the list.
    filter: RefCell<String>,
    /// The window that had focus, to return to.
    previous: isize,
    app: Option<String>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

/// The field that had focus when the palette was asked for (its UI
/// Automation identity; 0 = unknown). Taken before the palette takes focus.
static FIELD: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The palette was asked for and is not open yet: its keys are kept for it.
static OPENING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static EARLY_KEYS: std::sync::Mutex<Vec<(u16, Option<char>)>> = std::sync::Mutex::new(Vec::new());

/// A key for the open palette, posted by the keyboard hook (wParam: virtual
/// key, lParam: the character it types, 0 for none).
const WM_PALETTE_KEY: u32 = 0x8000 + 0x560;

/// Is the palette open, with the typist still where they opened it (so
/// its keys are its own)?
///
/// Windows often refuses the palette the keyboard focus — it is opened from
/// a hotkey that RightType's keyboard hook saw, and that does not count as
/// RightType being "in front" — so the app keeps the focus and the keys went
/// to it (CI: `upper` typed into the page). The hook hands the palette its
/// keys either way, while the window in front is the palette or the one it
/// was opened from.
pub fn is_open() -> bool {
    if OPENING.load(Ordering::Acquire) {
        return true;
    }
    let open = OPEN.load(Ordering::Acquire);
    if open == 0 {
        return false;
    }
    let fg = unsafe { GetForegroundWindow() }.0 as isize;
    fg == open || fg == PREVIOUS.load(Ordering::Acquire)
}

/// The UI thread's 1.5 s timer: close a palette the typist has left (it may
/// never have had the focus, so leaving it does not deactivate it).
pub fn close_if_left() {
    if OPEN.load(Ordering::Acquire) != 0 && !is_open() {
        if let Some(p) = CURRENT.with(|c| c.borrow().clone()) {
            close(&p, false);
        }
    }
}

/// The keyboard hook hands the open palette its keys: arrows move, Enter
/// runs, 1–9 run that line, typing filters, Backspace un-types, Esc closes.
/// Returns whether the palette takes `vk` (the hook then swallows it).
pub fn key(vk: u16, ch: Option<char>) -> bool {
    const NAV: [u16; 8] = [0x26, 0x28, 0x0D, 0x1B, 0x08, 0x24, 0x23, 0x09];
    let printable = ch.is_some_and(|c| !c.is_control());
    if !NAV.contains(&vk) && !printable {
        return false;
    }
    let open = OPEN.load(Ordering::Acquire);
    if open == 0 {
        // Still opening: keep it for when it is.
        if let Ok(mut keys) = EARLY_KEYS.lock() {
            keys.push((vk, ch));
        }
        return true;
    }
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            HWND(open as *mut _),
            WM_PALETTE_KEY,
            windows::Win32::Foundation::WPARAM(vk as usize),
            windows::Win32::Foundation::LPARAM(ch.map_or(0, |c| c as isize)),
        );
    }
    true
}

/// Show the items that match the filter under their headings, numbered,
/// packed from the top, and select the first.
fn refilter(p: &Palette) {
    use windows::Win32::UI::WindowsAndMessaging::{
        ShowWindow, SWP_NOMOVE, SWP_NOZORDER, SW_HIDE, SW_SHOW,
    };
    let filter = p.filter.borrow().to_lowercase();
    // Typed on the wrong keyboard still finds it (`fxw` and `ซ่อม` alike).
    let other = righttype::layout::auto_convert(&filter).to_lowercase();
    let searching = !filter.is_empty();
    let shows = |i: usize| {
        let command = p.items[i].1;
        let section = p.item_sections[i];
        if command == Command::MoreOptions {
            return !searching && !p.expanded.get();
        }
        if section == Section::Options && !searching && !p.expanded.get() {
            return false;
        }
        let label = p.labels.borrow()[i].to_lowercase();
        !searching || label.contains(&filter) || label.contains(&other)
    };
    let mut visible = Vec::new();
    let mut numbers = Vec::new();
    let mut y = PAD + 28;
    for section in Section::ALL {
        let rows: Vec<usize> = (0..p.items.len())
            .filter(|&i| p.item_sections[i] == section && shows(i))
            .collect();
        let heading = p.headings.iter().find(|(s, _)| *s == section).map(|h| h.1);
        if let Some(id) = heading {
            let hwnd = p.surface.hwnd_of(id);
            unsafe {
                if rows.is_empty() {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                } else {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        ui::px(PAD + 8),
                        ui::px(y + 4),
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER,
                    );
                    let _ = ShowWindow(hwnd, SW_SHOW);
                    y += HEAD;
                }
            }
        }
        for i in rows {
            let k = visible.len();
            let number = (section.numbered() && numbers.len() < 9).then(|| {
                numbers.push(k);
                numbers.len()
            });
            let label = &p.labels.borrow()[i];
            let text = match number {
                Some(n) => format!("{n}\t{label}"),
                None => format!("\t{label}"),
            };
            p.surface.set_text(p.items[i].0, &text);
            unsafe {
                let _ = SetWindowPos(
                    p.surface.hwnd_of(p.items[i].0),
                    None,
                    ui::px(PAD),
                    ui::px(y),
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER,
                );
                let _ = ShowWindow(p.surface.hwnd_of(p.items[i].0), SW_SHOW);
            }
            y += ROW;
            visible.push(i);
        }
    }
    for (i, (id, _)) in p.items.iter().enumerate() {
        if !visible.contains(&i) {
            unsafe {
                let _ = ShowWindow(p.surface.hwnd_of(*id), SW_HIDE);
            }
        }
    }
    unsafe {
        let _ = SetWindowPos(
            p.surface.hwnd,
            None,
            0,
            0,
            ui::px(W),
            ui::px(y.max(PAD + 28 + ROW) + PAD),
            SWP_NOMOVE | SWP_NOZORDER,
        );
    }
    let typed = p.filter.borrow();
    p.surface.set_text(
        p.header,
        &if typed.is_empty() {
            tr(T::PaletteHead).to_string()
        } else {
            trf(T::PaletteFiltering, &[("text", &typed)])
        },
    );
    drop(typed);
    *p.visible.borrow_mut() = visible;
    *p.numbers.borrow_mut() = numbers;
    select(p, 0);
}

/// The number shown on the `k`-th shown item, if it has one.
fn number_of(p: &Palette, k: usize) -> Option<usize> {
    p.numbers
        .borrow()
        .iter()
        .position(|&v| v == k)
        .map(|n| n + 1)
}

/// Select the `k`-th shown item (keyboard focus on it).
fn select(p: &Palette, k: usize) {
    let visible = p.visible.borrow();
    if visible.is_empty() {
        return;
    }
    let k = k.min(visible.len() - 1);
    p.selected.set(k);
    let id = p.items[visible[k]].0;
    unsafe {
        let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(p.surface.hwnd_of(id));
    }
    drop(visible);
    if p.filter.borrow().is_empty() {
        let head = if matches!(selected_command(p), Some(Command::History(_))) {
            T::PaletteHistoryHint
        } else {
            T::PaletteHead
        };
        p.surface.set_text(p.header, tr(head));
    }
    show_marks(p);
}

/// The command of the selected item.
fn selected_command(p: &Palette) -> Option<Command> {
    let visible = p.visible.borrow();
    visible.get(p.selected.get()).map(|&i| p.items[i].1)
}

/// The recent words a press of Enter would flip now: the ticked ones, or
/// the selected one.
fn picked(p: &Palette) -> Vec<usize> {
    let checked = p.checked.borrow();
    if !checked.is_empty() {
        let mut v = checked.clone();
        v.sort_unstable();
        return v;
    }
    match selected_command(p) {
        Some(Command::History(i)) => vec![i],
        _ => Vec::new(),
    }
}

/// Tint, in the app, the words a press of Enter would flip.
fn show_marks(p: &Palette) {
    let boxes: Vec<RECT> = picked(p)
        .iter()
        .filter_map(|&i| p.boxes.get(i).copied().flatten())
        .collect();
    crate::marks::show(&boxes);
}

/// Tick or untick the selected recent word (Space).
fn toggle_checked(p: &Palette) -> bool {
    let Some(Command::History(word)) = selected_command(p) else {
        return false;
    };
    {
        let mut checked = p.checked.borrow_mut();
        match checked.iter().position(|&c| c == word) {
            Some(at) => {
                checked.remove(at);
            }
            None => checked.push(word),
        }
    }
    let on = p.checked.borrow().contains(&word);
    let k = p.selected.get();
    let i = p.visible.borrow()[k];
    let label = {
        let mut labels = p.labels.borrow_mut();
        let rest = labels[i].chars().skip(1).collect::<String>();
        labels[i] = format!("{}{rest}", if on { '☑' } else { '☐' });
        labels[i].clone()
    };
    let text = match number_of(p, k) {
        Some(n) => format!("{n}\t{label}"),
        None => format!("\t{label}"),
    };
    p.surface.set_text(p.items[i].0, &text);
    show_marks(p);
    true
}

/// One key from the hook (see [`key`]).
fn on_key(p: &Rc<Palette>, vk: u16, ch: Option<char>) {
    let n = p.visible.borrow().len();
    let at = p.selected.get();
    match vk {
        // Tab moves like Down (Shift+Tab like Up), so Enter runs what is
        // highlighted whichever way the typist moved.
        0x09 if n > 0 && unsafe { hook_shift_down() } => select(p, (at + n - 1) % n),
        0x09 if n > 0 => select(p, (at + 1) % n),
        0x26 if n > 0 => select(p, (at + n - 1) % n), // Up
        0x28 if n > 0 => select(p, (at + 1) % n),     // Down
        0x24 => select(p, 0),                         // Home
        0x23 if n > 0 => select(p, n - 1),            // End
        0x1B => close(p, true),                       // Esc
        0x08 => {
            // Backspace: un-type, or close when nothing is typed.
            if p.filter.borrow_mut().pop().is_some() {
                refilter(p);
            }
        }
        0x0D => run_shown(p, at),
        // Space ticks a recent word (several can be flipped at once).
        0x20 if p.filter.borrow().is_empty() && toggle_checked(p) => {}
        _ => match ch {
            // 1–9 run that line, unless a filter is being typed.
            Some(d @ '1'..='9') if p.filter.borrow().is_empty() => {
                let shown = p.numbers.borrow().get(d as usize - '1' as usize).copied();
                if let Some(k) = shown {
                    run_shown(p, k);
                }
            }
            Some(c) => {
                p.filter.borrow_mut().push(c);
                refilter(p);
            }
            None => {}
        },
    }
}

unsafe fn hook_shift_down() -> bool {
    windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState(0x10) < 0
}

fn run_shown(p: &Rc<Palette>, k: usize) {
    let Some(&i) = p.visible.borrow().get(k) else {
        return;
    };
    let command = p.items[i].1;
    if command == Command::MoreOptions {
        // Open the options in place; the first of them is selected.
        p.expanded.set(true);
        refilter(p);
        let first = p
            .visible
            .borrow()
            .iter()
            .position(|&v| p.item_sections[v] == Section::Options);
        if let Some(k) = first {
            select(p, k);
        }
        return;
    }
    let words = if let Command::History(_) = command {
        // Enter on a word flips the ticked ones (or this one if none is).
        p.selected.set(k);
        picked(p)
    } else {
        Vec::new()
    };
    close(p, true);
    if words.is_empty() {
        run(command, p.app.as_deref());
    } else {
        flip_later(words);
    }
}

thread_local! {
    static TO_FLIP: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

/// Flip the recent words `picked` once the app has the focus back.
fn flip_later(picked: Vec<usize>) {
    TO_FLIP.with(|t| *t.borrow_mut() = picked);
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        let picked = TO_FLIP.with(|t| std::mem::take(&mut *t.borrow_mut()));
        hook::flip_picked(&picked);
    }
    unsafe {
        SetTimer(None, 0, 150, Some(fire));
    }
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Palette>>> = const { RefCell::new(None) };
}

/// Open the palette soon, from the message loop (the keyboard hook calls
/// this and must not create windows itself).
pub fn request_open() {
    // Keys typed right after the hotkey belong to the palette, though it is
    // not open yet (CI: 'up' of 'upper' went into the page).
    OPENING.store(true, Ordering::Release);
    FIELD.store(crate::focus::field_key(), Ordering::Release);
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        open();
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

/// The rows, in order, for the current state. `words` are the recent words
/// (oldest first).
fn commands(app: Option<&str>, words: &[String]) -> Vec<Entry> {
    let mut list = Vec::new();
    let mut add = |section, label: String, hint: &str, command| {
        list.push(Entry {
            section,
            label,
            hint: hint.to_string(),
            command,
        })
    };
    // The words just typed, newest first (at most five): flip one, or tick
    // several with Space.
    for (i, word) in words.iter().enumerate().rev().take(5) {
        let mut flipped = hook::flipped(word);
        add(
            Section::Words,
            format!("☐  {word}  →  {flipped}"),
            "",
            Command::History(i),
        );
        flipped.zeroize();
    }
    for (i, word) in crate::learn::reversed_words().iter().enumerate().take(3) {
        add(
            Section::Words,
            trf(T::PaletteKeepAsTyped, &[("word", word)]),
            "",
            Command::KeepAsTyped(i),
        );
    }
    add(
        Section::Fix,
        tr(T::TrayFix).to_string(),
        "",
        Command::FixText,
    );
    if app.is_some() {
        add(
            Section::Fix,
            tr(T::PaletteFixField).to_string(),
            "",
            Command::FixField,
        );
        for (key, kind) in [
            (T::PaletteSwapDigits, TransformKind::Digits),
            (T::PaletteUpper, TransformKind::Upper),
            (T::PaletteLower, TransformKind::Lower),
            (T::PaletteTitle, TransformKind::Title),
            (T::PaletteSwapCase, TransformKind::SwapCase),
        ] {
            add(
                Section::Selection,
                tr(key).to_string(),
                "",
                Command::Transform(kind),
            );
        }
    }
    // A calmer mode on offer for this app.
    if let (Some(app), Some((exe, mode))) = (app, apps::offer()) {
        if app == exe {
            let name = tr(crate::tray::app_mode_name(mode));
            add(
                Section::Here,
                trf(T::PaletteOfferForNow, &[("mode", name), ("app", app)]),
                "",
                Command::OfferForNow(mode),
            );
            add(
                Section::Here,
                trf(T::PaletteOfferKeep, &[("mode", name), ("app", app)]),
                "",
                Command::OfferKeep(mode),
            );
        }
    }
    if session::is_paused() {
        add(
            Section::Here,
            tr(T::TrayResume).to_string(),
            "",
            Command::Resume,
        );
    } else if hook::is_enabled() {
        add(
            Section::Here,
            tr(T::PalettePause).to_string(),
            "",
            Command::Pause,
        );
    }
    let field = FIELD.load(Ordering::Acquire);
    if app.is_some() && field != 0 {
        if crate::focus::is_off(field) {
            add(
                Section::Here,
                tr(T::PaletteFieldOn).to_string(),
                "",
                Command::FieldOn,
            );
        } else {
            add(
                Section::Here,
                tr(T::PaletteFieldOff).to_string(),
                "",
                Command::FieldOff,
            );
        }
    }
    if let Some(app) = app {
        let (key, command) = if apps::lookup(app) == Some(AppMode::Off) {
            (T::PaletteAppOnShort, Command::AppDefault)
        } else {
            (T::PaletteAppOffShort, Command::AppOff)
        };
        add(Section::Here, tr(key).to_string(), "", command);
    }
    let now = hook::mode();
    for (mode, key) in [
        (hook::Mode::Auto, T::TrayAuto),
        (hook::Mode::Suggest, T::TraySuggest),
        (hook::Mode::Manual, T::TrayManual),
    ] {
        let hint = if mode == now { tr(T::HintInUse) } else { "" };
        add(
            Section::Mode,
            tr(key).to_string(),
            hint,
            Command::Mode(mode),
        );
    }
    // Stands in for the options until they are opened.
    add(
        Section::Mode,
        tr(T::PaletteMoreOptions).to_string(),
        "▸",
        Command::MoreOptions,
    );
    let state = |on: bool| tr(if on { T::HintOn } else { T::HintOff });
    for (key, on, command) in [
        (
            T::PaletteSpelling,
            hook::fixes_spelling(),
            Command::Spelling,
        ),
        (T::PaletteHyphens, hook::fixes_hyphens(), Command::Hyphens),
        (
            T::PaletteCapsSwitch,
            hook::caps_switches_language(),
            Command::CapsSwitch,
        ),
        (
            T::PaletteTrayLanguage,
            crate::tray::shows_language(),
            Command::TrayLanguage,
        ),
    ] {
        add(Section::Options, tr(key).to_string(), state(on), command);
    }
    add(
        Section::Options,
        tr(T::TraySettings).to_string(),
        "",
        Command::Settings,
    );
    list
}

/// A section's heading.
fn heading(section: Section, app: Option<&str>) -> String {
    match section {
        Section::Words => tr(T::PaletteSecWords).to_string(),
        Section::Fix => tr(T::PaletteSecFix).to_string(),
        Section::Selection => tr(T::PaletteSecSelection).to_string(),
        Section::Here => match app {
            Some(app) => trf(T::PaletteSecHereApp, &[("app", app)]),
            None => tr(T::PaletteSecHere).to_string(),
        },
        Section::Mode => tr(T::PaletteSecMode).to_string(),
        Section::Options => tr(T::PaletteSecOptions).to_string(),
    }
}

fn open() {
    // However this ends (opened, closed instead, or failed), it is no longer
    // opening: never leave the hook holding keys for a palette that is not
    // coming.
    struct Opened;
    impl Drop for Opened {
        fn drop(&mut self) {
            OPENING.store(false, Ordering::Release);
            if let Ok(mut keys) = EARLY_KEYS.lock() {
                keys.clear();
            }
        }
    }
    let _opened = Opened;
    if let Some(existing) = CURRENT.with(|c| c.borrow().clone()) {
        close(&existing, false);
        return;
    }
    let previous = unsafe { GetForegroundWindow() };
    PREVIOUS.store(previous.0 as isize, Ordering::Release);
    let app = unsafe { crate::safety::foreground_exe(previous) }.filter(|e| e != "righttype.exe");
    // The system caret only: asking the app (UI Automation) can be slow,
    // and keys typed meanwhile wait for the palette.
    let caret = crate::caret::caret_rect();
    ui::refresh();

    // The recent words, and where they are, asked while the app still has
    // the focus (before the palette exists).
    let (words, spans) = hook::recent_words();
    let boxes = if words.is_empty() {
        Vec::new()
    } else {
        crate::focus::boxes_before_caret_within(spans, std::time::Duration::from_millis(150))
    };
    let list = commands(app.as_deref(), &words);
    let h = PAD * 2 + 28 + list.len() as i32 * ROW + Section::ALL.len() as i32 * HEAD;
    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::POPUP)
        .ex_flags(0x0000_0008 | 0x0000_0080) // topmost, tool window
        .size((W, h))
        .title("RightType")
        .build(&mut window)
        .is_err()
    {
        return;
    }
    let surface = Surface::attach(&window, 0x5254_0014, Box::new(paint));
    let p = pal();
    let header = surface.label(
        tr(T::PaletteHead),
        TextStyle::Small,
        (PAD + 6, PAD, W - 2 * PAD, 22),
        p.surface,
        0,
    );
    let headings: Vec<(Section, u16)> = Section::ALL
        .iter()
        .map(|&section| {
            let id = surface.label(
                &heading(section, app.as_deref()),
                TextStyle::Heading(section.icon()),
                (PAD + 8, PAD + 28, W - 2 * PAD - 16, HEAD - 4),
                p.surface,
                0,
            );
            (section, id)
        })
        .collect();
    let items: Vec<(u16, Command)> = list
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let id = surface.row(
                &entry.label,
                &entry.hint,
                entry.command.icon(),
                i == 0,
                (PAD, PAD + 28 + i as i32 * ROW, W - 2 * PAD, ROW - 2),
                p.surface,
            );
            (id, entry.command)
        })
        .collect();
    ui::size_and_center(surface.hwnd, W, h);
    // Next to the caret when there is one: below it, or above it when there
    // is no room below, and always inside that monitor's work area.
    if let Some(caret) = caret {
        unsafe {
            let (w, h) = (ui::px(W), ui::px(h));
            let monitor = MonitorFromPoint(
                POINT {
                    x: caret.left,
                    y: caret.bottom,
                },
                MONITOR_DEFAULTTONEAREST,
            );
            let wa = ui::work_area(monitor);
            let gap = ui::px(8);
            let below = caret.bottom + gap;
            let y = if below + h <= wa.bottom {
                below
            } else {
                caret.top - gap - h
            };
            let x = caret.left.clamp(wa.left, (wa.right - w).max(wa.left));
            let y = y.clamp(wa.top, (wa.bottom - h).max(wa.top));
            let _ = SetWindowPos(surface.hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE);
        }
    }
    let palette = Rc::new(Palette {
        window,
        surface,
        items,
        item_sections: list.iter().map(|e| e.section).collect(),
        headings,
        expanded: std::cell::Cell::new(false),
        numbers: RefCell::new(Vec::new()),
        labels: RefCell::new(list.iter().map(|e| e.label.clone()).collect()),
        words: RefCell::new(words),
        boxes,
        checked: RefCell::new(Vec::new()),
        header,
        visible: RefCell::new(Vec::new()),
        selected: std::cell::Cell::new(0),
        filter: RefCell::new(String::new()),
        previous: previous.0 as isize,
        app,
        handler: RefCell::new(None),
    });
    palette.window.set_visible(true);
    unsafe {
        let _ = SetForegroundWindow(palette.surface.hwnd);
    }
    refilter(&palette);
    OPEN.store(palette.surface.hwnd.0 as isize, Ordering::Release);

    let weak = Rc::downgrade(&palette);
    palette.surface.on_click(move |id| {
        if let Some(p) = weak.upgrade() {
            let shown = p.visible.borrow().iter().position(|&i| p.items[i].0 == id);
            if let Some(k) = shown {
                run_shown(&p, k);
            }
        }
    });
    // Esc (IDCANCEL from the dialog manager) and clicking elsewhere close it.
    let weak = Rc::downgrade(&palette);
    let raw = nwg::bind_raw_event_handler(
        &palette.window.handle,
        0x5254_0015,
        move |_h, msg, w, _l| {
            let p = weak.upgrade()?;
            if msg == WM_PALETTE_KEY {
                on_key(
                    &p,
                    w as u16,
                    char::from_u32(_l as u32).filter(|c| *c != '\0'),
                );
                return Some(0);
            }
            let cancel = msg == WM_COMMAND && w & 0xFFFF == IDCANCEL;
            let deactivated = msg == WM_ACTIVATE && w & 0xFFFF == 0;
            if cancel || deactivated {
                close(&p, cancel);
                return Some(0);
            }
            None
        },
    )
    .ok();
    *palette.handler.borrow_mut() = raw;
    CURRENT.with(|c| *c.borrow_mut() = Some(palette.clone()));
    // Keys typed while it was opening, in order.
    let early: Vec<(u16, Option<char>)> = EARLY_KEYS
        .lock()
        .map(|mut k| std::mem::take(&mut *k))
        .unwrap_or_default();
    for (vk, ch) in early {
        if CURRENT.with(|c| c.borrow().is_none()) {
            break;
        }
        on_key(&palette, vk, ch);
    }
}

/// Close the palette; `refocus` returns focus to where the user was typing.
fn close(p: &Rc<Palette>, refocus: bool) {
    if CURRENT.with(|c| c.borrow_mut().take()).is_none() {
        return;
    }
    let _ = OPEN.compare_exchange(
        p.surface.hwnd.0 as isize,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
    crate::marks::clear();
    for word in p.words.borrow_mut().iter_mut() {
        word.zeroize();
    }
    for label in p.labels.borrow_mut().iter_mut() {
        label.zeroize();
    }
    p.surface.detach();
    if let Some(h) = p.handler.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    p.window.close();
    if refocus {
        unsafe {
            let _ = SetForegroundWindow(HWND(p.previous as *mut _));
        }
    }
}

fn run(command: Command, app: Option<&str>) {
    match command {
        // Handled in `run_shown`.
        Command::History(_) => {}
        Command::FixText => crate::fixer::open(),
        Command::Settings => crate::settings::open(),
        Command::FixField => crate::manual::request_fix_field(PREVIOUS.load(Ordering::Acquire)),
        Command::Transform(kind) => {
            use righttype::layout as l;
            let f: fn(&str) -> String = match kind {
                TransformKind::Digits => l::swap_digits,
                TransformKind::Upper => l::upper_case,
                TransformKind::Lower => l::lower_case,
                TransformKind::Title => l::title_case,
                TransformKind::SwapCase => l::swap_case,
            };
            crate::manual::request_transform(PREVIOUS.load(Ordering::Acquire), f)
        }
        Command::KeepAsTyped(i) => {
            if let Some(mut word) = crate::learn::reversed_words().into_iter().nth(i) {
                crate::learn::keep_as_typed(&word);
                overlay::show(tr(T::ToastSaved));
                word.zeroize();
            }
        }
        Command::Pause => {
            session::pause(30);
            overlay::show(&trf(T::ToastPaused, &[("n", "30")]));
        }
        Command::Resume => {
            session::resume();
            overlay::show(tr(T::ToastOn));
        }
        Command::AppOff | Command::AppDefault => {
            if let Some(app) = app {
                let mode = (command == Command::AppOff).then_some(AppMode::Off);
                apps::set(app, mode);
                config::persist();
                let label = match mode {
                    Some(_) => tr(T::ModeOff),
                    None => tr(T::TrayAppDefault),
                };
                overlay::show(&trf(T::ToastAppMode, &[("mode", label), ("app", app)]));
            }
        }
        Command::OfferForNow(mode) | Command::OfferKeep(mode) => {
            if let Some(app) = app {
                apps::close_offer();
                let name = tr(crate::tray::app_mode_name(mode));
                if matches!(command, Command::OfferKeep(_)) {
                    apps::set(app, Some(mode));
                    config::persist();
                    overlay::show(&trf(T::ToastAppMode, &[("mode", name), ("app", app)]));
                } else {
                    apps::set_for_now(app, mode);
                    overlay::show(&trf(T::ToastModeForNow, &[("mode", name), ("app", app)]));
                }
            }
        }
        Command::FieldOff | Command::FieldOn => {
            let off = command == Command::FieldOff;
            crate::focus::set_field_off(FIELD.load(Ordering::Acquire), off);
            overlay::show(tr(if off {
                T::ToastFieldOff
            } else {
                T::ToastFieldOn
            }));
        }
        Command::TrayLanguage => {
            crate::tray::set_shows_language(!crate::tray::shows_language());
            config::persist();
        }
        Command::CapsSwitch => {
            let on = !hook::caps_switches_language();
            hook::set_caps_switches_language(on);
            config::persist();
            overlay::show(tr(if on {
                T::ToastCapsSwitchOn
            } else {
                T::ToastCapsSwitchOff
            }));
        }
        Command::Hyphens => {
            hook::set_fixes_hyphens(!hook::fixes_hyphens());
            config::persist();
        }
        // Handled in `run_shown`.
        Command::MoreOptions => {}
        Command::Spelling => {
            hook::set_fixes_spelling(!hook::fixes_spelling());
            config::persist();
        }
        Command::Mode(mode) => {
            hook::set_mode(mode);
            config::persist();
            overlay::show(mode.label());
        }
    }
}

fn paint(g: &Gfx, _hdc: HDC, rc: RECT, _page: u8) {
    let p = pal();
    g.fill_round(rc, ui::px(8) as f32, p.border);
    g.fill_round(ui::inset(rc, ui::px(1)), ui::px(7) as f32, p.surface);
}
