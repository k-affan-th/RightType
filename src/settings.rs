//! Settings window — Windows only. Currently: the custom app blacklist.
//!
//! The built-in defaults in [`crate::safety::BLACKLIST`] (wallets, password
//! managers, terminals) always apply and are **not editable here** — keeping
//! that baseline non-optional means this dialog can never accidentally weaken
//! it, only extend it.

use std::cell::RefCell;
use std::rc::Rc;

use native_windows_gui as nwg;

use crate::{config, safety, toast};

struct SettingsWindow {
    window: nwg::Window,
    editor: nwg::TextBox,
    save: nwg::Button,
    close: nwg::Button,
    _info: nwg::Label,
    _label: nwg::Label,
    handler: RefCell<Option<nwg::EventHandler>>,
}

/// Open the settings window. A new one each call — it's opened rarely enough
/// that de-duplicating instances isn't worth the extra bookkeeping.
pub fn open() {
    let mut window = nwg::Window::default();
    if nwg::Window::builder()
        .size((420, 340))
        .position((300, 300))
        .title("RightType — Blocked apps")
        .build(&mut window)
        .is_err()
    {
        return;
    }

    let mut info = nwg::Label::default();
    let _ = nwg::Label::builder()
        .text(
            "Always blocked (built-in, not editable): terminals, password\n\
             managers, and crypto wallets — see the README for the full list.",
        )
        .parent(&window)
        .position((10, 10))
        .size((400, 40))
        .build(&mut info);

    let mut label = nwg::Label::default();
    let _ = nwg::Label::builder()
        .text("Also block these (one .exe name per line):")
        .parent(&window)
        .position((10, 55))
        .size((400, 20))
        .build(&mut label);

    let mut editor = nwg::TextBox::default();
    let current = safety::custom_list().join("\r\n");
    let _ = nwg::TextBox::builder()
        .text(&current)
        .parent(&window)
        .position((10, 80))
        .size((400, 195))
        .build(&mut editor);

    let mut save = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Save")
        .parent(&window)
        .position((245, 285))
        .size((80, 28))
        .build(&mut save);

    let mut close = nwg::Button::default();
    let _ = nwg::Button::builder()
        .text("Close")
        .parent(&window)
        .position((330, 285))
        .size((80, 28))
        .build(&mut close);

    window.set_visible(true);

    let ui = Rc::new(SettingsWindow {
        window,
        editor,
        save,
        close,
        _info: info,
        _label: label,
        handler: RefCell::new(None),
    });

    let ui_h = ui.clone();
    let handler = nwg::full_bind_event_handler(&ui.window.handle, move |evt, _data, handle| {
        use nwg::Event as E;
        match evt {
            E::OnButtonClick => {
                if handle == ui_h.save.handle {
                    let entries: Vec<String> = ui_h
                        .editor
                        .text()
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty())
                        .map(str::to_lowercase)
                        .collect();
                    safety::set_custom_list(entries);
                    config::persist();
                    toast::show("Saved");
                    cleanup(&ui_h);
                } else if handle == ui_h.close.handle {
                    cleanup(&ui_h);
                }
            }
            E::OnWindowClose if handle == ui_h.window.handle => cleanup(&ui_h),
            _ => {}
        }
    });
    *ui.handler.borrow_mut() = Some(handler);
}

/// Unbind the event handler and close the window. Safe to call more than once
/// (e.g. Close-button + the resulting OnWindowClose): the second call finds
/// nothing left to do.
fn cleanup(ui: &Rc<SettingsWindow>) {
    if let Some(h) = ui.handler.borrow_mut().take() {
        nwg::unbind_event_handler(&h);
    }
    ui.window.close();
}
