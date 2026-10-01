//! Persisted settings — Windows only.
//!
//! Stores the master enable flag + mode in `%APPDATA%\RightType\config.toml` so
//! they survive restarts. **Only app settings live here** — nothing typed is ever
//! written, in keeping with the no-on-disk-keystrokes guarantee.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{hook, learn, safety};
use righttype::i18n::Lang;
use righttype::per_app::{normalize_exe, AppMode};

static PERSIST_TX: OnceLock<SyncSender<()>> = OnceLock::new();

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Config {
    /// Master on/off.
    pub enabled: bool,
    /// Current three-state correction mode. `None` only for legacy migration.
    pub mode: Option<ConfigMode>,
    /// Legacy v0.1 field. When present it takes precedence once, then the next
    /// persistence writes `mode` and omits this field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto: Option<bool>,
    /// Auto-learn new words (off by default — privacy).
    pub learn: bool,
    /// User-added app names to block (layered on top of the fixed defaults).
    pub custom_blacklist: Vec<String>,
    /// First-run onboarding shown? UX: the welcome/hotkeys window appears once.
    #[serde(default)]
    pub onboarded: bool,
    /// Interface language, `"en"` or `"th"`; absent means follow Windows.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Per-app modes: executable name → `auto` / `suggest` / `manual` / `off`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub app_modes: BTreeMap<String, String>,
    /// TH/EN tag and Suggest hints next to the text cursor.
    pub caret_hints: bool,
    /// Switch to each field's usual language on focus (opt-in).
    pub predict_layout: bool,
    /// Keep two counts per day for the 7-day view (opt-in).
    pub keep_stats: bool,
    /// A folder to keep the learned words in (e.g. a OneDrive folder).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub learned_folder: Option<String>,
    /// Hotkeys changed from the defaults: action → `Ctrl + Alt + Space`.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub hotkeys: BTreeMap<String, String>,
    /// `kedmanee` (default) or `pattachote`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thai_layout: Option<String>,
    /// Convert a selection by copying it (Ctrl+C) in apps that do not share
    /// it through UI Automation. Off: the copied text would enter Windows
    /// clipboard history, cloud sync to other devices and every clipboard
    /// monitor. Only settable by editing this file.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub selection_via_clipboard: bool,
    /// CapsLock tapped switches Thai/English; held, it is CapsLock.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub capslock_switches_language: bool,
    /// The tray icon shows TH / EN.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tray_shows_language: bool,
    /// Keep the settings and snippets in the sync folder too (with the
    /// learned words), so every PC using that folder shares them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sync_settings: bool,
    /// The typist's snippets (trigger → text, and which keyboard).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub snippets: Vec<SnippetConfig>,
    /// Put right common Thai misspellings (opt-in).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fix_spelling: bool,
    /// Start RightType again if it crashes (see instance.rs).
    pub restart_after_crash: bool,
}

/// One snippet as saved.
#[derive(Serialize, Deserialize, Clone)]
pub struct SnippetConfig {
    pub trigger: String,
    pub text: String,
    /// `thai`, `english` or `either`.
    pub scope: String,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum ConfigMode {
    Manual,
    Auto,
    Suggest,
}

impl From<ConfigMode> for hook::Mode {
    fn from(value: ConfigMode) -> Self {
        match value {
            ConfigMode::Manual => hook::Mode::Manual,
            ConfigMode::Auto => hook::Mode::Auto,
            ConfigMode::Suggest => hook::Mode::Suggest,
        }
    }
}

impl From<hook::Mode> for ConfigMode {
    fn from(value: hook::Mode) -> Self {
        match value {
            hook::Mode::Manual => ConfigMode::Manual,
            // Code is a per-app mode only.
            hook::Mode::Auto | hook::Mode::Code => ConfigMode::Auto,
            hook::Mode::Suggest => ConfigMode::Suggest,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            mode: Some(ConfigMode::Manual),
            auto: None,
            learn: false,
            custom_blacklist: Vec::new(),
            onboarded: false,
            language: None,
            app_modes: BTreeMap::new(),
            caret_hints: true,
            predict_layout: false,
            keep_stats: false,
            learned_folder: None,
            hotkeys: BTreeMap::new(),
            thai_layout: None,
            selection_via_clipboard: false,
            capslock_switches_language: false,
            tray_shows_language: false,
            restart_after_crash: true,
            fix_spelling: false,
            snippets: Vec::new(),
            sync_settings: false,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let mut p = crate::data_dir::righttype_dir()?;
    p.push("config.toml");
    Some(p)
}

/// Load saved settings, or defaults if the file is missing or unreadable.
pub fn load() -> Config {
    config_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}

/// Apply loaded settings to the running hook. Call once at startup.
pub fn apply(cfg: &Config) {
    hook::set_enabled(cfg.enabled);
    let mode = cfg
        .auto
        .map(|auto| {
            if auto {
                hook::Mode::Auto
            } else {
                hook::Mode::Manual
            }
        })
        .or_else(|| cfg.mode.map(Into::into))
        .unwrap_or(hook::Mode::Manual);
    hook::set_mode(mode);
    learn::set_enabled(cfg.learn);
    safety::set_custom_list(cfg.custom_blacklist.clone());
    set_language(cfg.language.as_deref().and_then(Lang::from_code));
    crate::caret::set_enabled(cfg.caret_hints);
    crate::manual::set_clipboard_fallback(cfg.selection_via_clipboard);
    hook::set_caps_switches_language(cfg.capslock_switches_language);
    crate::tray::set_shows_language(cfg.tray_shows_language);
    SYNC_SETTINGS.store(cfg.sync_settings, std::sync::atomic::Ordering::Relaxed);
    hook::set_fixes_spelling(cfg.fix_spelling);
    hook::set_snippets(
        cfg.snippets
            .iter()
            .filter_map(|s| {
                let scope = righttype::snippets::Scope::parse(&s.scope)?;
                righttype::snippets::check(&s.trigger, &s.text, scope).ok()
            })
            .take(righttype::snippets::MAX_SNIPPETS)
            .collect(),
    );
    RESTART_AFTER_CRASH.store(
        cfg.restart_after_crash,
        std::sync::atomic::Ordering::Relaxed,
    );
    crate::habits::set_enabled(cfg.predict_layout);
    righttype::layout::set_thai_variant(match cfg.thai_layout.as_deref() {
        Some("pattachote") => righttype::layout::ThaiVariant::Pattachote,
        _ => righttype::layout::ThaiVariant::Kedmanee,
    });
    hook::set_hotkeys(righttype::hotkeys::Hotkeys::from_config(
        cfg.hotkeys.iter().map(|(k, v)| (k.as_str(), v.as_str())),
    ));
    if let Some(folder) = cfg.learned_folder.as_deref() {
        learn::start_with_folder(Some(folder.into()));
    }
    if cfg.keep_stats != crate::stats::keeps_daily() {
        crate::stats::set_keep_daily(cfg.keep_stats);
    }
    crate::apps::set_all(
        cfg.app_modes
            .iter()
            .filter_map(|(exe, mode)| Some((normalize_exe(exe)?, AppMode::parse(mode)?)))
            .collect(),
    );
}

// ------------------------------------------------------------ sync folder

/// Settings → Learned words → "Sync settings and snippets too".
pub static SYNC_SETTINGS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The shared file's time when we last wrote or read it.
static SHARED_SEEN: std::sync::Mutex<Option<std::time::SystemTime>> = std::sync::Mutex::new(None);
static SHARED_TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
const SHARED_FILE: &str = "RightType settings.toml";

/// What PCs sharing a sync folder share: how RightType behaves, never where
/// this PC keeps things (the folder itself, first-run state, statistics).
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
struct Shared {
    mode: Option<ConfigMode>,
    custom_blacklist: Vec<String>,
    app_modes: BTreeMap<String, String>,
    hotkeys: BTreeMap<String, String>,
    snippets: Vec<SnippetConfig>,
    caret_hints: bool,
    fix_spelling: bool,
    capslock_switches_language: bool,
    language: Option<String>,
    thai_layout: Option<String>,
}

impl Shared {
    fn of(cfg: &Config) -> Shared {
        Shared {
            mode: cfg.mode,
            custom_blacklist: cfg.custom_blacklist.clone(),
            app_modes: cfg.app_modes.clone(),
            hotkeys: cfg.hotkeys.clone(),
            snippets: cfg.snippets.clone(),
            caret_hints: cfg.caret_hints,
            fix_spelling: cfg.fix_spelling,
            capslock_switches_language: cfg.capslock_switches_language,
            language: cfg.language.clone(),
            thai_layout: cfg.thai_layout.clone(),
        }
    }

    fn put_into(self, cfg: &mut Config) {
        cfg.mode = self.mode.or(cfg.mode);
        cfg.custom_blacklist = self.custom_blacklist;
        cfg.app_modes = self.app_modes;
        cfg.hotkeys = self.hotkeys;
        cfg.snippets = self.snippets;
        cfg.caret_hints = self.caret_hints;
        cfg.fix_spelling = self.fix_spelling;
        cfg.capslock_switches_language = self.capslock_switches_language;
        cfg.language = self.language;
        cfg.thai_layout = self.thai_layout;
    }
}

fn shared_path() -> Option<PathBuf> {
    Some(learn::folder()?.join(SHARED_FILE))
}

fn shared_modified() -> Option<std::time::SystemTime> {
    shared_path()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// Write the shared settings to the sync folder, unless they are already
/// what is there (so two PCs never bounce the file back and forth).
fn write_shared(cfg: &Config) {
    let Some(p) = shared_path() else {
        return;
    };
    let Ok(text) = toml::to_string_pretty(&Shared::of(cfg)) else {
        return;
    };
    if std::fs::read_to_string(&p).ok().as_deref() == Some(text.as_str()) {
        return;
    }
    if std::fs::write(&p, text).is_ok() {
        *SHARED_SEEN.lock().unwrap() = shared_modified();
    }
}

/// Session timer tick (1.5 s): every few seconds, take on settings another
/// PC wrote to the sync folder. Returns whether anything was taken on.
pub fn tick_shared() -> bool {
    if SHARED_TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 4 != 0
        || !SYNC_SETTINGS.load(std::sync::atomic::Ordering::Relaxed)
    {
        return false;
    }
    let modified = shared_modified();
    if modified.is_none() || modified == *SHARED_SEEN.lock().unwrap() {
        return false;
    }
    *SHARED_SEEN.lock().unwrap() = modified;
    let Some(shared) = shared_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| toml::from_str::<Shared>(&t).ok())
    else {
        return false;
    };
    let mut cfg = snapshot();
    shared.put_into(&mut cfg);
    apply(&cfg);
    write_local(&cfg);
    true
}

/// Sync settings too (or stop): writes them to the folder now when on.
pub fn set_sync_settings(on: bool) {
    SYNC_SETTINGS.store(on, std::sync::atomic::Ordering::Relaxed);
    if on {
        // What the folder already has wins (another PC set it up first).
        *SHARED_SEEN.lock().unwrap() = None;
        if shared_modified().is_none() || !tick_now() {
            persist();
        }
    } else {
        persist();
    }
}

/// [`tick_shared`] without waiting for the timer.
fn tick_now() -> bool {
    SHARED_TICKS.store(0, std::sync::atomic::Ordering::Relaxed);
    tick_shared()
}

/// Settings → restart after a crash. Takes effect at the next start.
pub static RESTART_AFTER_CRASH: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(true);

/// The language the user picked, or `None` to follow Windows.
static LANGUAGE: std::sync::Mutex<Option<Lang>> = std::sync::Mutex::new(None);

/// The user's language choice (`None` = follow Windows).
pub fn language_choice() -> Option<Lang> {
    *LANGUAGE.lock().unwrap()
}

/// Record the language choice and switch the interface to it.
pub fn set_language(choice: Option<Lang>) {
    *LANGUAGE.lock().unwrap() = choice;
    let lang = choice.unwrap_or_else(|| {
        let langid = unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() };
        Lang::from_windows_langid(langid)
    });
    righttype::i18n::set_lang(lang);
}

/// Has the first-run onboarding been shown/completed?
pub fn onboarded() -> bool {
    load().onboarded
}

/// Mark onboarding done and persist (best-effort, async-safe).
pub fn mark_onboarded() {
    let mut cfg = load();
    cfg.onboarded = true;
    let Some(p) = config_path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = toml::to_string_pretty(&cfg) {
        let _ = std::fs::write(p, s);
    }
}

/// Snapshot the current runtime state and write it to disk (and to the sync
/// folder, when settings are synced). Best-effort.
pub fn persist() {
    let cfg = snapshot();
    write_local(&cfg);
    if cfg.sync_settings {
        write_shared(&cfg);
    }
}

/// The running state as a config.
fn snapshot() -> Config {
    Config {
        // A pause is temporary: it must not be saved as "off".
        enabled: hook::is_enabled() || crate::session::is_paused(),
        mode: Some(hook::mode().into()),
        auto: None,
        learn: learn::is_enabled(),
        custom_blacklist: safety::custom_list(),
        onboarded: onboarded(),
        language: language_choice().map(|lang| lang.code().to_string()),
        app_modes: crate::apps::all()
            .into_iter()
            .map(|(exe, mode)| (exe, mode.name().to_string()))
            .collect(),
        caret_hints: crate::caret::is_enabled(),
        selection_via_clipboard: crate::manual::clipboard_fallback(),
        capslock_switches_language: hook::caps_switches_language(),
        tray_shows_language: crate::tray::shows_language(),
        fix_spelling: hook::fixes_spelling(),
        sync_settings: SYNC_SETTINGS.load(std::sync::atomic::Ordering::Relaxed),
        snippets: hook::snippets()
            .into_iter()
            .map(|s| SnippetConfig {
                trigger: s.trigger,
                text: s.text,
                scope: s.scope.name().to_string(),
            })
            .collect(),
        restart_after_crash: RESTART_AFTER_CRASH.load(std::sync::atomic::Ordering::Relaxed),
        predict_layout: crate::habits::is_enabled(),
        keep_stats: crate::stats::keeps_daily(),
        learned_folder: learn::folder().map(|p| p.to_string_lossy().into_owned()),
        hotkeys: hook::hotkeys()
            .to_config()
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        thai_layout: (righttype::layout::thai_variant()
            == righttype::layout::ThaiVariant::Pattachote)
            .then(|| "pattachote".to_string()),
    }
}

fn write_local(cfg: &Config) {
    let Some(p) = config_path() else {
        return;
    };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(s) = toml::to_string_pretty(&cfg) {
        let _ = std::fs::write(p, s);
    }
}

/// Coalesce a persistence request onto a worker so the low-level keyboard hook
/// never performs directory creation or file I/O.
pub fn persist_async() {
    let tx = PERSIST_TX.get_or_init(|| {
        let (tx, rx) = sync_channel(1);
        let _ = std::thread::Builder::new()
            .name("righttype-config".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    persist();
                }
            });
        tx
    });
    let _ = tx.try_send(());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_auto_field_migrates_without_changing_behavior() {
        let cfg: Config = toml::from_str("enabled = true\nauto = true\n").unwrap();
        assert_eq!(cfg.auto, Some(true));
        let resolved = cfg
            .auto
            .map(|auto| {
                if auto {
                    hook::Mode::Auto
                } else {
                    hook::Mode::Manual
                }
            })
            .or_else(|| cfg.mode.map(Into::into))
            .unwrap();
        assert_eq!(resolved, hook::Mode::Auto);
    }

    #[test]
    fn suggest_round_trips_without_legacy_field() {
        let cfg = Config {
            mode: Some(ConfigMode::Suggest),
            ..Config::default()
        };
        let encoded = toml::to_string(&cfg).unwrap();
        assert!(encoded.contains("mode = \"suggest\""));
        assert!(!encoded.contains("auto ="));
        let decoded: Config = toml::from_str(&encoded).unwrap();
        assert!(matches!(decoded.mode, Some(ConfigMode::Suggest)));
    }
}
