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

use crate::{config, focus, hook, learn, session, settings, startup, stats, toast};

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
    m_suggest: nwg::MenuItem,
    m_learn: nwg::MenuItem,
    m_startup: nwg::MenuItem,
    m_settings: nwg::MenuItem,
    m_stats: nwg::MenuItem,
    _sep: nwg::MenuSeparator,
    m_quit: nwg::MenuItem,
}

/// Build the tray UI, install the hook, and run the event loop until Quit.
pub fn run() {
    eprintln!("[rt-boot] init");
    nwg::init().expect("Failed to init Native Windows GUI");
    eprintln!("[rt-boot] init ok");

    // The small status toast (shown on layout switch / mode change).
    eprintln!("[rt-boot] toast");
    toast::init();

    // Load the learned-words dictionary, then restore saved settings (enabled +
    // mode + learn) before building the menu so its checkmarks reflect them.
    eprintln!("[rt-boot] config");
    learn::load();
    config::apply(&config::load());

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

    let mut m_suggest = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Suggest mode")
        .parent(&menu)
        .build(&mut m_suggest)
        .expect("suggest item");

    let mut m_learn = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Learn new words")
        .parent(&menu)
        .build(&mut m_learn)
        .expect("learn item");

    let mut m_startup = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Start with Windows")
        .parent(&menu)
        .build(&mut m_startup)
        .expect("startup item");

    let mut m_settings = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Settings...")
        .parent(&menu)
        .build(&mut m_settings)
        .expect("settings item");

    let mut m_stats = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text("Stats...")
        .parent(&menu)
        .build(&mut m_stats)
        .expect("stats item");

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
    m_auto.set_checked(hook::mode() == hook::Mode::Auto);
    m_manual.set_checked(hook::mode() == hook::Mode::Manual);
    m_suggest.set_checked(hook::mode() == hook::Mode::Suggest);
    m_learn.set_checked(learn::is_enabled());
    m_startup.set_checked(startup::is_enabled());

    let ui = Rc::new(Tray {
        window,
        _icon: icon,
        _tray: tray,
        menu,
        m_enabled,
        m_auto,
        m_manual,
        m_suggest,
        m_learn,
        m_startup,
        m_settings,
        m_stats,
        _sep: sep,
        m_quit,
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            // Right-click on the tray icon opens the menu at the cursor. Refresh
            // every checkmark first — hotkeys (mode toggle, panic switch) change
            // this state without going through the menu, so it can be stale.
            E::OnContextMenu => {
                ui_h.m_enabled.set_checked(hook::is_enabled());
                ui_h.m_auto.set_checked(hook::mode() == hook::Mode::Auto);
                ui_h.m_manual
                    .set_checked(hook::mode() == hook::Mode::Manual);
                ui_h.m_suggest
                    .set_checked(hook::mode() == hook::Mode::Suggest);
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
                    toast::show(if on {
                        "RightType: ON"
                    } else {
                        "RightType: OFF"
                    });
                    config::persist();
                } else if handle == ui_h.m_auto.handle {
                    hook::set_mode(hook::Mode::Auto);
                    ui_h.m_auto.set_checked(true);
                    ui_h.m_manual.set_checked(false);
                    ui_h.m_suggest.set_checked(false);
                    toast::show("Auto mode");
                    config::persist();
                } else if handle == ui_h.m_manual.handle {
                    hook::set_mode(hook::Mode::Manual);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(true);
                    ui_h.m_suggest.set_checked(false);
                    toast::show("Manual mode");
                    config::persist();
                } else if handle == ui_h.m_suggest.handle {
                    hook::set_mode(hook::Mode::Suggest);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(false);
                    ui_h.m_suggest.set_checked(true);
                    toast::show("Suggest mode");
                    config::persist();
                } else if handle == ui_h.m_learn.handle {
                    let on = !learn::is_enabled();
                    learn::set_enabled(on);
                    ui_h.m_learn.set_checked(on);
                    toast::show(if on { "Learning: ON" } else { "Learning: OFF" });
                    config::persist();
                } else if handle == ui_h.m_startup.handle {
                    let on = !startup::is_enabled();
                    startup::set_enabled(on);
                    ui_h.m_startup.set_checked(on);
                } else if handle == ui_h.m_settings.handle {
                    settings::open();
                } else if handle == ui_h.m_stats.handle {
<<<<<<< Updated upstream
                    let (auto, manual) = stats::snapshot();
                    nwg::modal_info_message(
                        &ui_h.window.handle,
                        "RightType — Stats",
                        &format!(
                            "This session:\n\n\
                             Corrected automatically: {auto}\n\
                             Corrected via hotkey: {manual}\n\
                             Total: {}\n\n\
                             (Counts reset when RightType restarts — nothing typed is ever saved.)",
                            auto + manual
                        ),
                    );
=======
                    stats::open();
>>>>>>> Stashed changes
                }
            }
            _ => {}
        }
    });

    // Session resilience: reinstall the hook across sleep/resume + lock/unlock by
    // watching raw power/session messages on this window (Bug 1).
    let hwnd = ui.window.handle.hwnd().map(|h| HWND(h as _));
<<<<<<< Updated upstream
=======
    eprintln!("[rt-boot] sync");
    sync_state(&ui);
    eprintln!("[rt-boot] onboard-check");
    if !config::onboarded() {
        crate::onboard::show(true);
    }
    #[cfg(debug_assertions)]
    if let Ok(what) = std::env::var("RIGHTTYPE_SHOW") {
        match what.as_str() {
            "settings" => settings::open(),
            "stats" => stats::open(),
            _ => {}
        }
    }
>>>>>>> Stashed changes
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
    // UIA focus hook for password-field detection (incl. browsers).
    unsafe { focus::arm() };

    nwg::dispatch_thread_events();

    unsafe { focus::disarm() };
    if let Some(hwnd) = hwnd {
        unsafe { session::disarm(hwnd) };
    }
    unsafe { hook::uninstall() };
    if let Some(h) = raw {
        let _ = nwg::unbind_raw_event_handler(&h);
    }
    nwg::unbind_event_handler(&handler);
}
<<<<<<< Updated upstream
=======

/// Refresh every state-bearing surface: the disabled status header, the tray
/// tooltip, and the mode/enable checkmarks. Called before the menu opens and
/// after any mutating action.
fn sync_state(ui: &Rc<Tray>) {
    let enabled = hook::is_enabled();
    let state = if enabled {
        hook::mode().label().to_string()
    } else {
        "OFF".to_string()
    };
    ui._tray.set_tip(&format!("RightType — {state}"));
    ui.m_enabled.set_checked(enabled);
    ui.m_auto.set_checked(hook::mode() == hook::Mode::Auto);
    ui.m_manual.set_checked(hook::mode() == hook::Mode::Manual);
    ui.m_suggest
        .set_checked(hook::mode() == hook::Mode::Suggest);
}


>>>>>>> Stashed changes
