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
    /// Keep a word the typist reversed lately as typed (by its place in
    /// `learn::reversed_words`).
    KeepAsTyped(usize),
}

struct Palette {
    window: nwg::Window,
    surface: Rc<Surface>,
    items: Vec<(u16, Command)>,
    /// The window that had focus, to return to.
    previous: isize,
    app: Option<String>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Palette>>> = const { RefCell::new(None) };
}

/// Open the palette soon, from the message loop (the keyboard hook calls
/// this and must not create windows itself).
pub fn request_open() {
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
    }
    if session::is_paused() {
        list.push((tr(T::TrayResume).to_string(), Command::Resume));
    } else if hook::is_enabled() {
        list.push((tr(T::PalettePause).to_string(), Command::Pause));
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
    list.push((tr(T::TraySettings).to_string(), Command::Settings));
    list
}

fn open() {
    if let Some(existing) = CURRENT.with(|c| c.borrow().clone()) {
        close(&existing, false);
        return;
    }
    let previous = unsafe { GetForegroundWindow() };
    PREVIOUS.store(previous.0 as isize, Ordering::Release);
    let app = unsafe { crate::safety::foreground_exe(previous) }.filter(|e| e != "righttype.exe");
    let caret = crate::caret::find_caret();
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
    surface.label(
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
    let first = items.first().map(|(id, _)| *id);
    let palette = Rc::new(Palette {
        window,
        surface,
        items,
        previous: previous.0 as isize,
        app,
        handler: RefCell::new(None),
    });
    palette.window.set_visible(true);
    unsafe {
        let _ = SetForegroundWindow(palette.surface.hwnd);
        if let Some(first) = first {
            let _ = windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(
                palette.surface.hwnd_of(first),
            );
        }
    }
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
    CURRENT.with(|c| *c.borrow_mut() = Some(palette));
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
