//! Auto-learn user dictionary — Windows only. **Off by default.**
//!
//! The bundled English list is conversational (OpenSubtitles) and misses domain
//! vocabulary — a developer's `frontend`, `backend`, `kubernetes`, … don't ignite.
//! When enabled, this learns English words the user actually types and remembers
//! them in `%APPDATA%\RightType\learned.txt`, so they ignite next time.
//!
//! Guards (privacy first): only **pure-ASCII English** words (so wrong-layout Thai
//! gibberish is never learned), length 3–20, not already known, never
//! secret-shaped, and only after being seen [`REPEATS`] times (so one-off typos
//! and gibberish don't stick). It runs only after the per-context guards in
//! `safety`/`focus` have already excluded password fields, wallets, and terminals,
//! so secrets typed there never even reach here.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use righttype::{dict, secret};

const MIN_LEN: usize = 3;
const MAX_LEN: usize = 20;
const REPEATS: u8 = 3;

static ENABLED: AtomicBool = AtomicBool::new(false);
static LEARNED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static PENDING: Mutex<BTreeMap<String, u8>> = Mutex::new(BTreeMap::new());

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

fn learned_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(std::env::var_os("APPDATA")?);
    p.push("RightType");
    p.push("learned.txt");
    Some(p)
}

/// Load the persisted learned words. Call once at startup.
pub fn load() {
    let set: HashSet<String> = learned_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    *LEARNED.lock().unwrap() = Some(set);
}

/// Has this English word been learned? (Cheap; called on the hot path.)
pub fn contains(word: &str) -> bool {
    let key = word.to_ascii_lowercase();
    LEARNED
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.contains(&key))
}

/// Observe a completed word. After [`REPEATS`] sightings of a qualifying English
/// word, learn it (persist + add to the live set). No-op unless enabled.
pub fn observe(word: &str) {
    if !is_enabled() {
        return;
    }
    let n = word.chars().count();
    if !(MIN_LEN..=MAX_LEN).contains(&n) || !word.bytes().all(|b| b.is_ascii_alphabetic()) {
        return;
    }
    if secret::is_secret_token(word) {
        return;
    }
    let key = word.to_ascii_lowercase();
    if dict::english().contains(&key) || contains(&key) {
        return; // already known
    }

    let ready = {
        let mut pending = PENDING.lock().unwrap();
        let c = pending.entry(key.clone()).or_insert(0);
        *c = c.saturating_add(1);
        if *c >= REPEATS {
            pending.remove(&key);
            true
        } else {
            false
        }
    };
    if ready {
        commit(&key);
    }
}

fn commit(word: &str) {
    if let Some(set) = LEARNED.lock().unwrap().as_mut() {
        if !set.insert(word.to_string()) {
            return;
        }
    }
    // Append to the file (best-effort).
    let Some(p) = learned_path() else {
        return;
    };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(p) {
        let _ = writeln!(f, "{word}");
    }
}
