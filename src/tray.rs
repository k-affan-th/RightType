//! System-tray application shell — Windows only.
//!
//! Hosts the keyboard hook on a thread that pumps a Win32 message loop (a
//! low-level hook requires one) and gives the user a tray icon + menu to enable/
//! disable RightType, switch Auto/Manual, and quit. The whole app is windowless
//! (`windows_subsystem = "windows"` in `main.rs`) so there's no console — it lives
//! quietly in the tray.

use std::rc::Rc;

use native_windows_gui as nwg;
use windows::Win32::Foundation::HWND;

use crate::{hook, session, toast};

/// The tray icon, embedded so the binary stays portable (no external file).
static ICON_BYTES: &[u8] = include_bytes!("../assets/icon.ico");

struct Tray {
    window: nwg::MessageWindow,
    _icon: nwg::Icon,
    _tray: nwg::TrayNotification,
    menu: nwg::Menu,
    m_enabled: nwg::MenuItem,
    m_auto: nwg::MenuItem,
    m_manual: nwg::MenuItem,
    _sep: nwg::MenuSeparator,
    m_quit: nwg::MenuItem,
}

/// Build the tray UI, install the hook, and run the event loop until Quit.
pub fn run() {
    nwg::init().expect("Failed to init Native Windows GUI");

    // The small status toast (shown on layout switch / mode change).
    toast::init();

    let mut window = nwg::MessageWindow::default();
    nwg::MessageWindow::builder()
        .build(&mut window)
        .expect("window");

    let mut icon = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_BYTES))
        .build(&mut icon)
        .expect("icon");

    let mut tray = nwg::TrayNotification::default();
    nwg::TrayNotification::builder()
        .parent(&window)
        .icon(Some(&icon))
        .tip(Some("RightType"))
        .build(&mut tray)
        .expect("tray");

    let mut menu = nwg::Menu::default();
    nwg::Menu::builder()
        .popup(true)
        .parent(&window)
        .build(&mut menu)
        .expect("menu");

    let mut m_enabled = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Enabled")
        .parent(&menu)
        .build(&mut m_enabled)
        .expect("enabled item");

    let mut m_auto = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Auto mode")
        .parent(&menu)
        .build(&mut m_auto)
        .expect("auto item");

    let mut m_manual = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Manual mode")
        .parent(&menu)
        .build(&mut m_manual)
        .expect("manual item");

    let mut sep = nwg::MenuSeparator::default();
    nwg::MenuSeparator::builder()
        .parent(&menu)
        .build(&mut sep)
        .expect("sep");

    let mut m_quit = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Quit")
        .parent(&menu)
        .build(&mut m_quit)
        .expect("quit item");

    // Reflect current state in the menu checkmarks.
    m_enabled.set_checked(hook::is_enabled());
    m_auto.set_checked(hook::is_auto());
    m_manual.set_checked(!hook::is_auto());

    let ui = Rc::new(Tray {
        window,
        _icon: icon,
        _tray: tray,
        menu,
        m_enabled,
        m_auto,
        m_manual,
        _sep: sep,
        m_quit,
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            // Right-click on the tray icon opens the menu at the cursor.
            E::OnContextMenu => {
                let (x, y) = nwg::GlobalCursor::position();
                ui_h.menu.popup(x, y);
            }
            E::OnMenuItemSelected => {
                if handle == ui_h.m_quit.handle {
                    nwg::stop_thread_dispatch();
                } else if handle == ui_h.m_enabled.handle {
                    let on = !hook::is_enabled();
                    hook::set_enabled(on);
                    ui_h.m_enabled.set_checked(on);
                    toast::show(if on { "RightType: ON" } else { "RightType: OFF" });
                } else if handle == ui_h.m_auto.handle {
                    hook::set_auto(true);
                    ui_h.m_auto.set_checked(true);
                    ui_h.m_manual.set_checked(false);
                    toast::show("Auto mode");
                } else if handle == ui_h.m_manual.handle {
                    hook::set_auto(false);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(true);
                    toast::show("Manual mode");
                }
            }
            _ => {}
        }
    });

    // Session resilience: reinstall the hook across sleep/resume + lock/unlock by
    // watching raw power/session messages on this window (Bug 1).
    let hwnd = ui.window.handle.hwnd().map(|h| HWND(h as _));
    let raw = nwg::bind_raw_event_handler(&ui.window.handle, 0x5254_0001, move |_h, msg, w, _l| {
        unsafe { session::on_message(msg, w) };
        None
    })
    .ok();

    // The hook lives on this (message-pumping) thread.
    if let Err(e) = unsafe { hook::install() } {
        nwg::modal_error_message(
            &ui.window.handle,
            "RightType",
            &format!("Failed to install the keyboard hook:\n{e}"),
        );
        return;
    }
    if let Some(hwnd) = hwnd {
        unsafe { session::arm(hwnd) };
    }

    nwg::dispatch_thread_events();

    if let Some(hwnd) = hwnd {
        unsafe { session::disarm(hwnd) };
    }
    unsafe { hook::uninstall() };
    if let Some(h) = raw {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    nwg::unbind_event_handler(&handler);
}
