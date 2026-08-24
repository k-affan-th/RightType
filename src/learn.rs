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
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};

use righttype::{dict, secret};
use zeroize::Zeroize;

const MIN_LEN: usize = 3;
const MAX_LEN: usize = 20;
const REPEATS: u8 = 3;

static ENABLED: AtomicBool = AtomicBool::new(false);
static LEARNED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static PENDING: Mutex<BTreeMap<String, u8>> = Mutex::new(BTreeMap::new());
static PERSIST_TX: OnceLock<SyncSender<String>> = OnceLock::new();

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        let pending = std::mem::take(&mut *PENDING.lock().unwrap());
        for (mut word, _) in pending {
            word.zeroize();
        }
    }
}

fn learned_path() -> Option<PathBuf> {
    let mut p = crate::data_dir::righttype_dir()?;
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

/// Number of learned words (for the stats dialog).
pub fn count() -> usize {
    LEARNED
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.len())
        .unwrap_or(0)
}

/// Forget every learned word (settings button). Also clears the file.
pub fn clear() {
    if let Some(p) = learned_path() {
        let _ = std::fs::write(p, "");
    }
    if let Some(s) = LEARNED.lock().unwrap().as_mut() {
        s.clear();
    }
}

/// Observe a completed word. After [`REPEATS`] sightings of a qualifying English
/// word, learn it (persist + add to the live set). No-op unless enabled.
pub fn observe(word: &str) {
    if !is_enabled() {
        return;
    }
    if !eligible_shape(word) {
        return;
    }
    let mut key = word.to_ascii_lowercase();
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
    key.zeroize();
}

fn eligible_shape(word: &str) -> bool {
    let len = word.chars().count();
    (MIN_LEN..=MAX_LEN).contains(&len)
        && word.bytes().all(|byte| byte.is_ascii_alphabetic())
        && !secret::is_secret_token(word)
}

fn commit(word: &str) {
    if let Some(set) = LEARNED.lock().unwrap().as_mut() {
        if !set.insert(word.to_string()) {
            return;
        }
    }
    queue_persist(word.to_string());
}

fn queue_persist(word: String) {
    let tx = PERSIST_TX.get_or_init(|| {
        let (tx, rx) = sync_channel::<String>(64);
        let _ = std::thread::Builder::new()
            .name("righttype-learn".into())
            .spawn(move || {
                while let Ok(mut word) = rx.recv() {
                    persist_word(&word);
                    word.zeroize();
                }
            });
        tx
    });
    if let Err(err) = tx.try_send(word) {
        let mut word = match err {
            TrySendError::Full(word) | TrySendError::Disconnected(word) => word,
        };
        word.zeroize();
    }
}

fn persist_word(word: &str) {
    // Append to the file (best-effort, off the keyboard-hook thread).
    let Some(p) = learned_path() else {
        return;
    };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(p)
    {
        let _ = writeln!(f, "{word}");
    }
}

#[cfg(test)]
mod tests {
    use super::eligible_shape;

    #[test]
    fn learning_shape_rejects_secrets_non_ascii_and_non_words() {
        assert!(eligible_shape("kubernetes"));
        for denied in [
            "ab",
            "P@ssw0rd123",
            "abc123",
            "l;ylfu",
            "สวัสดี",
            "thiswordisfarbeyondthelearningcap",
        ] {
            assert!(
                !eligible_shape(denied),
                "unexpected learning candidate: {denied}"
            );
        }
    }
}
