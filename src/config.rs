//! Persisted settings — Windows only.
//!
//! Stores the master enable flag + mode in `%APPDATA%\RightType\config.toml` so
//! they survive restarts. **Only app settings live here** — nothing typed is ever
//! written, in keeping with the no-on-disk-keystrokes guarantee.

use std::path::PathBuf;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::{hook, learn, safety};

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
            hook::Mode::Auto => ConfigMode::Auto,
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

/// Snapshot the current runtime state and write it to disk. Best-effort.
pub fn persist() {
    let cfg = Config {
        enabled: hook::is_enabled(),
        mode: Some(hook::mode().into()),
        auto: None,
        learn: learn::is_enabled(),
        custom_blacklist: safety::custom_list(),
        onboarded: onboarded(),
    };
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
