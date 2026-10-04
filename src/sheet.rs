//! "This app's shortcuts" — Windows only.
//!
//! A list of the shortcuts of the app in front: those its own menu shows
//! (read from the classic menu bar, as the app writes them), then RightType's
//! table for apps whose shortcuts are not in a menu
//! ([`righttype::shortcuts`]), then Windows' own. Typing filters it; Enter
//! or a double-click presses the shortcut in the app. Opened from the
//! palette, or by holding Ctrl (opt-in). Reads the app's menu titles only.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use native_windows_gui as nwg;
use righttype::i18n::{lang, tr, trf, Lang, T};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::HDC;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetMenu, GetMenuItemCount, GetMenuStringW, GetSubMenu, KillTimer,
    SetForegroundWindow, SetTimer, SetWindowPos, HMENU, HWND_TOPMOST, MF_BYPOSITION, SWP_NOMOVE,
    SWP_NOSIZE,
};

use crate::ui::{self, pal, Gfx, Surface, TextStyle};

const W: i32 = 580;
const H: i32 = 600;
const PAD: i32 = 24;
const FILTER_MS: u32 = 150;

/// One row: the keys and what they do (in the interface language).
#[derive(Clone)]
struct Entry {
    keys: String,
    what: String,
}

struct Sheet {
    window: nwg::Window,
    surface: Rc<Surface>,
    search: u16,
    table: u16,
    /// The app the list is for, to give the focus back to.
    target: isize,
    all: Vec<Entry>,
    shown: RefCell<Vec<Entry>>,
    query: RefCell<String>,
    timer: Cell<usize>,
    handler: RefCell<Option<nwg::RawEventHandler>>,
}

thread_local! {
    static CURRENT: RefCell<Option<Rc<Sheet>>> = const { RefCell::new(None) };
}

/// The open list's window (0: none), for the keyboard hook.
static OPEN: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
/// A key the hook hands the list (wParam: the key).
const WM_SHEET_KEY: u32 = 0x8000 + 0x5A1;

/// The app the open list is for (0: none).
static TARGET: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// The keyboard hook hands the open list its keys, as it does for the
/// palette: typing and Backspace edit the search, Enter presses the
/// shortcut, Esc closes, Up/Down move. So the list works whether or not
/// Windows let it come to the front (CI: it did not, the search went to
/// Notepad behind it, and Enter pressed the first shortcut, Ctrl+N).
/// Returns whether the list took the key (the hook then swallows it).
pub fn key(vk: u16, ch: Option<char>) -> bool {
    use std::sync::atomic::Ordering;
    let open = OPEN.load(Ordering::Acquire);
    if open == 0 {
        return false;
    }
    let printable = ch.is_some_and(|c| !c.is_control());
    if !printable && !matches!(vk, 0x08 | 0x0D | 0x1B | 0x26 | 0x28) {
        return false;
    }
    let fg = unsafe { GetForegroundWindow() }.0 as isize;
    if fg != open && fg != TARGET.load(Ordering::Acquire) {
        return false;
    }
    unsafe {
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            HWND(open as *mut _),
            WM_SHEET_KEY,
            windows::Win32::Foundation::WPARAM(vk as usize),
            windows::Win32::Foundation::LPARAM(if printable {
                ch.map_or(0, |c| c as isize)
            } else {
                0
            }),
        );
    }
    true
}

thread_local! {
    /// The app the list is asked for (0: the one in front when it opens).
    static FOR: Cell<isize> = const { Cell::new(0) };
}

/// Open the list for the app in front (from the message loop).
pub fn request_open() {
    request_open_for(0);
}

/// Open the list for the app of window `hwnd` (the palette passes the
/// window it was opened over: by the time the list opens, the palette may
/// still be in front).
pub fn request_open_for(hwnd: isize) {
    FOR.with(|f| f.set(hwnd));
    unsafe extern "system" fn fire(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        open();
    }
    unsafe {
        SetTimer(None, 0, 1, Some(fire));
    }
}

pub fn is_open() -> bool {
    CURRENT.with(|c| c.borrow().is_some())
}

/// The shortcuts in the app's classic menu bar: "&Save\tCtrl+S".
fn menu_shortcuts(hwnd: HWND) -> Vec<Entry> {
    unsafe fn walk(menu: HMENU, depth: u32, out: &mut Vec<Entry>) {
        if depth > 3 {
            return;
        }
        let n = GetMenuItemCount(menu);
        for i in 0..n.max(0) {
            let mut buf = [0u16; 256];
            let len = GetMenuStringW(menu, i as u32, Some(&mut buf), MF_BYPOSITION);
            if len > 0 {
                let text = String::from_utf16_lossy(&buf[..len as usize]);
                if let Some((name, keys)) = text.split_once('\t') {
                    let name = name
                        .replace('&', "")
                        .trim_end_matches("...")
                        .trim()
                        .to_string();
                    let keys = keys.trim().to_string();
                    if !name.is_empty() && !keys.is_empty() {
                        out.push(Entry { keys, what: name });
                    }
                }
            }
            let sub = GetSubMenu(menu, i);
            if !sub.is_invalid() {
                walk(sub, depth + 1, out);
            }
        }
    }
    let mut out = Vec::new();
    unsafe {
        let menu = GetMenu(hwnd);
        if !menu.is_invalid() {
            walk(menu, 0, &mut out);
        }
    }
    out
}

/// The commands of the app in `target` with their keys, in the interface
/// language: its menus', then the bundled table's (keys the menu already
/// named skipped), then Windows'. For the list at the cursor (2.4 E).
pub fn app_commands(target: HWND) -> Vec<righttype::atcaret::Command> {
    let exe = unsafe { crate::safety::foreground_exe(target) }.unwrap_or_default();
    let thai = lang() == Lang::Th;
    let mut all: Vec<righttype::atcaret::Command> = menu_shortcuts(target)
        .into_iter()
        .map(|e| righttype::atcaret::Command {
            name: e.what,
            keys: e.keys,
            other: String::new(),
        })
        .collect();
    for s in righttype::shortcuts::for_app(&exe) {
        if !all.iter().any(|c| c.keys == s.keys) {
            let (name, other) = if thai { (s.th, s.en) } else { (s.en, s.th) };
            all.push(righttype::atcaret::Command {
                name,
                keys: s.keys,
                other,
            });
        }
    }
    all
}

/// Bring `target` back to the front and press `keys` (`Ctrl+H`) in it, a
/// moment later. Whether the keys were known.
pub fn press_in(target: isize, keys: &str) -> bool {
    let Some(keys) = righttype::shortcuts::virtual_keys(keys) else {
        return false;
    };
    thread_local! {
        static PENDING: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    }
    PENDING.with(|p| *p.borrow_mut() = keys);
    unsafe extern "system" fn press(_: HWND, _: u32, id: usize, _: u32) {
        let _ = KillTimer(None, id);
        let keys = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
        crate::hook::trace_note("shortcut list: shortcut pressed");
        crate::inject::press_chord(&keys);
    }
    unsafe {
        let _ = SetForegroundWindow(HWND(target as *mut _));
        // A moment for the app to take the focus back first.
        SetTimer(None, 0, 150, Some(press));
    }
    true
}

fn close(sheet: &Rc<Sheet>) {
    OPEN.store(0, std::sync::atomic::Ordering::Release);
    TARGET.store(0, std::sync::atomic::Ordering::Release);
    unsafe {
        let _ = KillTimer(sheet.surface.hwnd, sheet.timer.get());
    }
    sheet.surface.detach();
    if let Some(h) = sheet.handler.borrow_mut().take() {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    sheet.window.close();
}

fn open() {
    if let Some(sheet) = CURRENT.with(|c| c.borrow_mut().take()) {
        close(&sheet);
        return;
    }
    let target = match FOR.with(|f| f.replace(0)) {
        0 => unsafe { GetForegroundWindow() },
        h => HWND(h as *mut _),
    };
    let exe = unsafe { crate::safety::foreground_exe(target) }.unwrap_or_default();
    let app = exe.trim_end_matches(".exe").to_string();
    let all: Vec<Entry> = app_commands(target)
        .into_iter()
        .map(|c| Entry {
            keys: c.keys,
            what: c.name,
        })
        .collect();
    ui::refresh();
    let mut window = nwg::Window::default();
    let title = trf(T::SheetTitle, &[("app", &app)]);
    if nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW | nwg::WindowFlags::VISIBLE)
        .size((W, H))
        .title(&title)
        .build(&mut window)
        .is_err()
    {
        return;
    }
    let surface = Surface::attach(&window, 0x5254_001A, Box::new(paint));
    let s = &surface;
    let p = pal();
    s.label(
        &title,
        TextStyle::Title,
        (PAD, 16, W - 2 * PAD, 36),
        p.bg,
        0,
    );
    s.label(
        tr(T::SheetSearch),
        TextStyle::Small,
        (PAD, 60, 200, 18),
        p.bg,
        0,
    );
    let search = s.line_edit("", (PAD, 80, W - 2 * PAD, 32), 0);
    let table = s.table(
        &[
            (tr(T::SheetKeys), 180),
            (tr(T::SheetWhat), W - 2 * PAD - 180 - 28),
        ],
        (PAD, 124, W - 2 * PAD, H - 124 - 64),
        0,
    );
    s.label(
        tr(T::SheetNote),
        TextStyle::Small,
        (PAD, H - 52, W - 2 * PAD, 36),
        p.bg,
        0,
    );
    ui::size_and_center(surface.hwnd, W, H);
    let sheet = Rc::new(Sheet {
        window,
        surface,
        search,
        table,
        target: target.0 as isize,
        all: all.clone(),
        shown: RefCell::new(all),
        query: RefCell::new(String::new()),
        timer: Cell::new(0),
        handler: RefCell::new(None),
    });
    fill(&sheet);
    let hwnd = sheet.surface.hwnd;
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        let _ = SetForegroundWindow(hwnd);
        let _ =
            windows::Win32::UI::Input::KeyboardAndMouse::SetFocus(sheet.surface.hwnd_of(search));
        sheet.timer.set(SetTimer(hwnd, 1, FILTER_MS, None));
    }
    let weak = Rc::downgrade(&sheet);
    sheet.surface.on_table(move |_, event| {
        if let (Some(sheet), ui::TableEvent::Activated) = (weak.upgrade(), event) {
            let row = sheet.surface.selected_row(sheet.table).unwrap_or(0);
            run(&sheet, row);
        }
    });
    let weak = Rc::downgrade(&sheet);
    let raw =
        nwg::bind_raw_event_handler(&sheet.window.handle, 0x5254_001B, move |_h, msg, w, l| {
            const WM_TIMER: u32 = 0x0113;
            const WM_CLOSE: u32 = 0x0010;
            const WM_COMMAND: u32 = 0x0111;
            let sheet = weak.upgrade()?;
            match msg {
                WM_TIMER => {
                    refilter(&sheet);
                    Some(0)
                }
                WM_SHEET_KEY => {
                    on_key(
                        &sheet,
                        w as u16,
                        char::from_u32(l as u32).filter(|c| *c != '\0'),
                    );
                    Some(0)
                }
                // Enter in the search box (IDOK): the first row.
                WM_COMMAND if w & 0xFFFF == 1 => {
                    run(&sheet, sheet.surface.selected_row(sheet.table).unwrap_or(0));
                    Some(0)
                }
                // Esc (IDCANCEL), or the window's close box.
                WM_COMMAND if w & 0xFFFF == 2 => {
                    CURRENT.with(|c| c.borrow_mut().take());
                    close(&sheet);
                    Some(0)
                }
                WM_CLOSE => {
                    CURRENT.with(|c| c.borrow_mut().take());
                    close(&sheet);
                    Some(0)
                }
                _ => None,
            }
        })
        .ok();
    *sheet.handler.borrow_mut() = raw;
    crate::hook::trace_note("shortcut list: open");
    TARGET.store(target.0 as isize, std::sync::atomic::Ordering::Release);
    OPEN.store(hwnd.0 as isize, std::sync::atomic::Ordering::Release);
    CURRENT.with(|c| *c.borrow_mut() = Some(sheet));
}

/// A key handed over by the hook (see [`key`]).
fn on_key(sheet: &Rc<Sheet>, vk: u16, ch: Option<char>) {
    let row = sheet.surface.selected_row(sheet.table).unwrap_or(0);
    match (vk, ch) {
        (_, Some(c)) => edit_search(sheet, |q| q.push(c)),
        (0x08, _) => edit_search(sheet, |q| {
            q.pop();
        }),
        (0x0D, _) => {
            refilter(sheet);
            let row = sheet.surface.selected_row(sheet.table).unwrap_or(0);
            run(sheet, row);
        }
        (0x1B, _) => {
            CURRENT.with(|c| c.borrow_mut().take());
            close(sheet);
        }
        _ => {
            let n = sheet.shown.borrow().len();
            if n > 0 {
                let to = if vk == 0x26 {
                    row.saturating_sub(1)
                } else {
                    (row + 1).min(n - 1)
                };
                sheet.surface.select_row(sheet.table, to);
            }
        }
    }
}

/// Change the search text (shown in the box, caret at the end) and filter.
fn edit_search(sheet: &Sheet, f: impl FnOnce(&mut String)) {
    let mut q = sheet.surface.text_of(sheet.search);
    f(&mut q);
    sheet.surface.set_text(sheet.search, &q);
    unsafe {
        const EM_SETSEL: u32 = 0x00B1;
        let n = q.encode_utf16().count();
        let _ = windows::Win32::UI::WindowsAndMessaging::SendMessageW(
            sheet.surface.hwnd_of(sheet.search),
            EM_SETSEL,
            windows::Win32::Foundation::WPARAM(n),
            windows::Win32::Foundation::LPARAM(n as isize),
        );
    }
    refilter(sheet);
}

fn fill(sheet: &Sheet) {
    let rows: Vec<Vec<String>> = sheet
        .shown
        .borrow()
        .iter()
        .map(|e| vec![e.keys.clone(), e.what.clone()])
        .collect();
    sheet
        .surface
        .set_rows(sheet.table, &rows, (!rows.is_empty()).then_some(0));
}

/// The search box changed: show the rows whose keys or words match.
fn refilter(sheet: &Sheet) {
    let query = sheet.surface.text_of(sheet.search).to_lowercase();
    if *sheet.query.borrow() == query {
        return;
    }
    *sheet.shown.borrow_mut() = sheet
        .all
        .iter()
        .filter(|e| {
            query.is_empty()
                || e.what.to_lowercase().contains(&query)
                || e.keys.to_lowercase().contains(&query)
        })
        .cloned()
        .collect();
    *sheet.query.borrow_mut() = query;
    fill(sheet);
}

/// Press the shortcut of row `row` in the app the list is for.
fn run(sheet: &Rc<Sheet>, row: usize) {
    let Some(entry) = sheet.shown.borrow().get(row).cloned() else {
        return;
    };
    let target = sheet.target;
    CURRENT.with(|c| c.borrow_mut().take());
    close(sheet);
    press_in(target, &entry.keys);
}

fn paint(g: &Gfx, _hdc: HDC, _rc: RECT, _page: u8) {
    // The search box's field (text boxes are drawn on one).
    ui::field(g, ui::rect(PAD, 80, W - 2 * PAD, 32));
}
