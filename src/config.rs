//! Persisted settings — Windows only.
//!
//! Stores the master enable flag + mode in `%APPDATA%\RightType\config.toml` so
//! they survive restarts. **Only app settings live here** — nothing typed is ever
//! written, in keeping with the no-on-disk-keystrokes guarantee.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{hook, learn};

#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(default)]
pub struct Config {
    /// Master on/off.
    pub enabled: bool,
    /// `true` = Auto mode, `false` = Manual.
    pub auto: bool,
    /// Auto-learn new words (off by default — privacy).
    pub learn: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            auto: false,
            learn: false,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(std::env::var_os("APPDATA")?);
    p.push("RightType");
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
    hook::set_auto(cfg.auto);
    learn::set_enabled(cfg.learn);
}

/// Snapshot the current runtime state and write it to disk. Best-effort.
pub fn persist() {
    let cfg = Config {
        enabled: hook::is_enabled(),
        auto: hook::is_auto(),
        learn: learn::is_enabled(),
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
