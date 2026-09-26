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
use righttype::i18n::{tr, T};

thread_local! {
    /// Last tooltip pushed to the shell, so the timer refresh is a no-op
    /// unless something actually changed.
    static TIP: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
    /// Whether the tray shows the "off" icon, for the same reason.
    static SHOWING_OFF: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// Language the menu was last labelled in.
    static MENU_LANG: std::cell::Cell<Option<righttype::i18n::Lang>> = const { std::cell::Cell::new(None) };
}

/// The tray icons, embedded so the binary stays portable (no external file):
/// blue while RightType is on, grey while it is off.
static ICON_BYTES: &[u8] = include_bytes!("../assets/icon.ico");
static ICON_OFF_BYTES: &[u8] = include_bytes!("../assets/icon_off.ico");

struct Tray {
    window: nwg::MessageWindow,
    icon: nwg::Icon,
    icon_off: nwg::Icon,
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
    m_help: nwg::MenuItem,
    _sep: nwg::MenuSeparator,
    m_quit: nwg::MenuItem,
}

/// Build the tray UI, install the hook, and run the event loop until Quit.
pub fn run() {
    nwg::init().expect("Failed to init Native Windows GUI");
    crate::ui::load_fonts();
    crate::ui::refresh();

    // The small status toast (shown on layout switch / mode change).

    // Load the learned-words dictionary, then restore saved settings (enabled +
    // mode + learn) before building the menu so its checkmarks reflect them.
    learn::load();
    config::apply(&config::load());
    // Build the dictionaries and compound tables now: built lazily they cost
    // tens of milliseconds on the first keystroke, inside the keyboard hook.
    righttype::english::warm();

    let mut window = nwg::MessageWindow::default();
    nwg::MessageWindow::builder()
        .build(&mut window)
        .expect("window");

    let mut icon = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_BYTES))
        .build(&mut icon)
        .expect("icon");
    let mut icon_off = nwg::Icon::default();
    nwg::Icon::builder()
        .source_bin(Some(ICON_OFF_BYTES))
        .build(&mut icon_off)
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
        .text(tr(T::TrayEnabled))
        .parent(&menu)
        .build(&mut m_enabled)
        .expect("enabled item");

    let mut m_auto = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayAuto))
        .parent(&menu)
        .build(&mut m_auto)
        .expect("auto item");

    let mut m_manual = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayManual))
        .parent(&menu)
        .build(&mut m_manual)
        .expect("manual item");

    let mut m_suggest = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TraySuggest))
        .parent(&menu)
        .build(&mut m_suggest)
        .expect("suggest item");

    let mut m_learn = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayLearn))
        .parent(&menu)
        .build(&mut m_learn)
        .expect("learn item");

    let mut m_startup = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayStartup))
        .parent(&menu)
        .build(&mut m_startup)
        .expect("startup item");

    let mut m_settings = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TraySettings))
        .parent(&menu)
        .build(&mut m_settings)
        .expect("settings item");

    let mut m_stats = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayStats))
        .parent(&menu)
        .build(&mut m_stats)
        .expect("stats item");

    let mut m_help = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayHelp))
        .parent(&menu)
        .build(&mut m_help)
        .expect("help item");

    let mut sep = nwg::MenuSeparator::default();
    nwg::MenuSeparator::builder()
        .parent(&menu)
        .build(&mut sep)
        .expect("sep");

    let mut m_quit = nwg::MenuItem::default();
    nwg::MenuItem::builder()
        .text(tr(T::TrayQuit))
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
        icon,
        icon_off,
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
        m_help,
        _sep: sep,
        m_quit,
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            // Either click on the tray icon opens the menu at the cursor — a
            // left click used to do nothing, which read as "the app is not
            // responding". Refresh every checkmark first: hotkeys (mode
            // toggle, panic switch) change state without going through the
            // menu, so it can be stale.
            E::OnContextMenu | E::OnMousePress(nwg::MousePressEvent::MousePressLeftUp) => {
                sync_state(&ui_h);
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
                    toast::show(tr(if on { T::ToastOn } else { T::ToastOff }));
                    config::persist();
                } else if handle == ui_h.m_auto.handle {
                    hook::set_mode(hook::Mode::Auto);
                    ui_h.m_auto.set_checked(true);
                    ui_h.m_manual.set_checked(false);
                    ui_h.m_suggest.set_checked(false);
                    toast::show(tr(T::ToastModeAuto));
                    config::persist();
                } else if handle == ui_h.m_manual.handle {
                    hook::set_mode(hook::Mode::Manual);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(true);
                    ui_h.m_suggest.set_checked(false);
                    toast::show(tr(T::ToastModeManual));
                    config::persist();
                } else if handle == ui_h.m_suggest.handle {
                    hook::set_mode(hook::Mode::Suggest);
                    ui_h.m_auto.set_checked(false);
                    ui_h.m_manual.set_checked(false);
                    ui_h.m_suggest.set_checked(true);
                    toast::show(tr(T::ToastModeSuggest));
                    config::persist();
                } else if handle == ui_h.m_learn.handle {
                    let on = !learn::is_enabled();
                    learn::set_enabled(on);
                    ui_h.m_learn.set_checked(on);
                    toast::show(tr(if on {
                        T::ToastLearnOn
                    } else {
                        T::ToastLearnOff
                    }));
                    config::persist();
                } else if handle == ui_h.m_startup.handle {
                    let on = !startup::is_enabled();
                    startup::set_enabled(on);
                    ui_h.m_startup.set_checked(on);
                } else if handle == ui_h.m_settings.handle {
                    settings::open();
                } else if handle == ui_h.m_stats.handle {
                    stats::open();
                } else if handle == ui_h.m_help.handle {
                    crate::onboard::show(false);
                }
                sync_state(&ui_h);
            }
            _ => {}
        }
    });

    // Session resilience: reinstall the hook across sleep/resume + lock/unlock by
    // watching raw power/session messages on this window (Bug 1).
    let hwnd = ui.window.handle.hwnd().map(|h| HWND(h as _));
    sync_state(&ui);
    if !config::onboarded() {
        crate::onboard::show(true);
    }
    #[cfg(debug_assertions)]
    if let Ok(what) = std::env::var("RIGHTTYPE_SHOW") {
        match what.as_str() {
            "settings" => settings::open(),
            "settings-hotkeys" => settings::open_page(2),
            "settings-learned" => settings::open_page(3),
            "settings-blocked" => settings::open_page(4),
            "settings-about" => settings::open_page(5),
            "stats" => stats::open(),
            "welcome" => crate::onboard::show(true),
            "help" => crate::onboard::show(false),
            _ => {}
        }
    }
    let ui_t = ui.clone();
    let raw = nwg::bind_raw_event_handler(&ui.window.handle, 0x5254_0001, move |_h, msg, w, _l| {
        unsafe { session::on_message(msg, w) };
        // The session retry timer doubles as a cheap refresh, so the tooltip
        // follows hotkey and Settings changes without waiting for the menu.
        if msg == WM_TIMER {
            sync_state(&ui_t);
        }
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

const WM_TIMER: u32 = 0x0113;

/// Refresh every state-bearing surface: the tray tooltip and the mode/enable/
/// learn checkmarks. Called before the menu opens, after any menu action, and
/// on the session timer.
fn sync_state(ui: &Rc<Tray>) {
    let enabled = hook::is_enabled();
    let state = if enabled {
        tr(match hook::mode() {
            hook::Mode::Auto => T::ModeAuto,
            hook::Mode::Suggest => T::ModeSuggest,
            hook::Mode::Manual => T::ModeManual,
        })
    } else {
        tr(T::StateOff)
    };
    let tip = format!("RightType — {state}");
    if TIP.with(|t| t.replace(tip.clone())) != tip {
        ui._tray.set_tip(&tip);
    }
    if SHOWING_OFF.with(|c| c.replace(Some(!enabled))) != Some(!enabled) {
        ui._tray
            .set_icon(if enabled { &ui.icon } else { &ui.icon_off });
    }
    let lang = righttype::i18n::lang();
    if MENU_LANG.with(|c| c.replace(Some(lang))) != Some(lang) {
        for (item, key) in [
            (&ui.m_enabled, T::TrayEnabled),
            (&ui.m_auto, T::TrayAuto),
            (&ui.m_manual, T::TrayManual),
            (&ui.m_suggest, T::TraySuggest),
            (&ui.m_learn, T::TrayLearn),
            (&ui.m_startup, T::TrayStartup),
            (&ui.m_settings, T::TraySettings),
            (&ui.m_stats, T::TrayStats),
            (&ui.m_help, T::TrayHelp),
            (&ui.m_quit, T::TrayQuit),
        ] {
            set_menu_text(item, tr(key));
        }
    }
    ui.m_learn.set_checked(learn::is_enabled());
    ui.m_enabled.set_checked(enabled);
    ui.m_auto.set_checked(hook::mode() == hook::Mode::Auto);
    ui.m_manual.set_checked(hook::mode() == hook::Mode::Manual);
    ui.m_suggest
        .set_checked(hook::mode() == hook::Mode::Suggest);
}

/// Relabel a menu item (nwg has no setter): used when the language changes.
fn set_menu_text(item: &nwg::MenuItem, text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetMenuItemInfoW, HMENU, MENUITEMINFOW, MIIM_STRING,
    };
    let Some((hmenu, id)) = item.handle.hmenu_item() else {
        return;
    };
    let mut wide: Vec<u16> = format!("{text}\0").encode_utf16().collect();
    let info = MENUITEMINFOW {
        cbSize: std::mem::size_of::<MENUITEMINFOW>() as u32,
        fMask: MIIM_STRING,
        dwTypeData: windows::core::PWSTR(wide.as_mut_ptr()),
        ..Default::default()
    };
    unsafe {
        let _ = SetMenuItemInfoW(HMENU(hmenu as _), id, false, &info);
    }
}
