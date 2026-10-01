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

const W: i32 = 320;
const ROW: i32 = 36;
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
    /// Rewrite the selection (digits, letter case).
    Transform(TransformKind),
    /// Off (or back on) in the field that had focus.
    FieldOff,
    FieldOn,
    /// Keep a word the typist reversed lately as typed (by its place in
    /// `learn::reversed_words`).
    KeepAsTyped(usize),
}

struct Palette {
    window: nwg::Window,
    surface: Rc<Surface>,
    items: Vec<(u16, Command)>,
    /// Each item's text, for filtering and numbering.
    labels: Vec<String>,
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

/// Show the items that match the filter, numbered, packed from the top, and
/// select the first.
fn refilter(p: &Palette) {
    let filter = p.filter.borrow().to_lowercase();
    // Typed on the wrong keyboard still finds it (`fxw` and `ซ่อม` alike).
    let other = righttype::layout::auto_convert(&filter).to_lowercase();
    let visible: Vec<usize> = (0..p.items.len())
        .filter(|&i| {
            let label = p.labels[i].to_lowercase();
            filter.is_empty() || label.contains(&filter) || label.contains(&other)
        })
        .collect();
    for (i, (id, _)) in p.items.iter().enumerate() {
        let hwnd = p.surface.hwnd_of(*id);
        match visible.iter().position(|&v| v == i) {
            Some(k) => {
                let label = if k < 9 {
                    format!("{}   {}", k + 1, p.labels[i])
                } else {
                    format!("    {}", p.labels[i])
                };
                p.surface.set_text(*id, &label);
                unsafe {
                    let _ = SetWindowPos(
                        hwnd,
                        None,
                        ui::px(PAD),
                        ui::px(PAD + 28 + k as i32 * ROW),
                        0,
                        0,
                        SWP_NOSIZE | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
                    );
                    let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                        hwnd,
                        windows::Win32::UI::WindowsAndMessaging::SW_SHOW,
                    );
                }
            }
            None => unsafe {
                let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                    hwnd,
                    windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                );
            },
        }
    }
    let rows = visible.len().max(1) as i32;
    unsafe {
        let _ = SetWindowPos(
            p.surface.hwnd,
            None,
            0,
            0,
            ui::px(W),
            ui::px(PAD * 2 + 28 + rows * ROW),
            windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
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
    select(p, 0);
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
        _ => match ch {
            // 1–9 run that line, unless a filter is being typed.
            Some(d @ '1'..='9') if p.filter.borrow().is_empty() => {
                run_shown(p, d as usize - '1' as usize)
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
    close(p, true);
    run(command, p.app.as_deref());
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

/// The commands, in order, for the current state.
fn commands(app: Option<&str>) -> Vec<(String, Command)> {
    let mut list = vec![(tr(T::TrayFix).to_string(), Command::FixText)];
    if app.is_some() {
        list.push((tr(T::PaletteFixField).to_string(), Command::FixField));
        list.push((
            tr(T::PaletteSwapDigits).to_string(),
            Command::Transform(TransformKind::Digits),
        ));
        for (key, kind) in [
            (T::PaletteUpper, TransformKind::Upper),
            (T::PaletteLower, TransformKind::Lower),
            (T::PaletteTitle, TransformKind::Title),
            (T::PaletteSwapCase, TransformKind::SwapCase),
        ] {
            list.push((tr(key).to_string(), Command::Transform(kind)));
        }
    }
    if session::is_paused() {
        list.push((tr(T::TrayResume).to_string(), Command::Resume));
    } else if hook::is_enabled() {
        list.push((tr(T::PalettePause).to_string(), Command::Pause));
    }
    let field = FIELD.load(Ordering::Acquire);
    if app.is_some() && field != 0 {
        if crate::focus::is_off(field) {
            list.push((tr(T::PaletteFieldOn).to_string(), Command::FieldOn));
        } else {
            list.push((tr(T::PaletteFieldOff).to_string(), Command::FieldOff));
        }
    }
    if let Some(app) = app {
        if apps::lookup(app) == Some(AppMode::Off) {
            list.push((trf(T::PaletteAppOn, &[("app", app)]), Command::AppDefault));
        } else {
            list.push((trf(T::PaletteAppOff, &[("app", app)]), Command::AppOff));
        }
    }
    let now = hook::mode();
    for (mode, key) in [
        (hook::Mode::Auto, T::TrayAuto),
        (hook::Mode::Suggest, T::TraySuggest),
        (hook::Mode::Manual, T::TrayManual),
    ] {
        let mark = if mode == now { "✓  " } else { "" };
        list.push((format!("{mark}{}", tr(key)), Command::Mode(mode)));
    }
    for (i, word) in crate::learn::reversed_words().iter().enumerate().take(3) {
        list.push((
            trf(T::PaletteKeepAsTyped, &[("word", word)]),
            Command::KeepAsTyped(i),
        ));
    }
    let mark = if crate::tray::shows_language() {
        "✓  "
    } else {
        ""
    };
    list.push((
        format!("{mark}{}", tr(T::PaletteTrayLanguage)),
        Command::TrayLanguage,
    ));
    let mark = if hook::caps_switches_language() {
        "✓  "
    } else {
        ""
    };
    list.push((
        format!("{mark}{}", tr(T::PaletteCapsSwitch)),
        Command::CapsSwitch,
    ));
    list.push((tr(T::TraySettings).to_string(), Command::Settings));
    list
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

    let list = commands(app.as_deref());
    let h = PAD * 2 + 28 + list.len() as i32 * ROW;
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
    let items: Vec<(u16, Command)> = list
        .iter()
        .enumerate()
        .map(|(i, (label, command))| {
            let id = surface.list_item(
                label,
                i == 0,
                (PAD, PAD + 28 + i as i32 * ROW, W - 2 * PAD, ROW - 4),
                p.surface,
            );
            (id, *command)
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
        labels: list.iter().map(|(label, _)| label.clone()).collect(),
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
            if let Some((_, command)) = p.items.iter().find(|(i, _)| *i == id) {
                let command = *command;
                close(&p, true);
                run(command, p.app.as_deref());
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
