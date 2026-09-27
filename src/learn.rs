//! Auto-learn user dictionary — Windows only. **Off by default.**
//!
//! The bundled English list is conversational (OpenSubtitles) and misses domain
//! vocabulary — a developer's `kubernetes`, `tokenizer`, product names. When
//! enabled, this learns words the user actually types and remembers them in
//! `%APPDATA%\RightType\learned.txt`.
//!
//! Learned words join the live dictionaries ([`Dictionary::learn`]), so every
//! decision sees them immediately: an English word is held instead of turned
//! into Thai, a Thai word is left alone instead of turned into English. (Before
//! D-009 they were written to disk and then never consulted.)
//!
//! Two ways in:
//!
//! - [`observe`] — ordinary English typed on the English layout, after
//!   [`REPEATS`] sightings, so one-off typos and gibberish don't stick.
//! - [`learn_now`] — the typist reversed one of RightType's own automatic
//!   conversions (Undo, or Shift+Backspace on it). That is an explicit "this was
//!   a real word", so it is learned at once, in either script.
//!
//! **Sync folder** (2.0): the list can live in a folder the user picks — a
//! OneDrive, Google Drive or team folder — instead of `%APPDATA%`. Choosing it
//! merges both lists into the folder; a change made there by another PC is
//! picked up within seconds ([`tick`]). RightType itself never goes online:
//! syncing is the folder's own business.
//!
//! Guards (privacy first): letters of one script only, bounded length, not
//! already known, never secret-shaped. It runs only after the per-context
//! guards in `safety`/`focus` have excluded password fields, wallets and
//! terminals, so secrets typed there never reach here.
//!
//! [`Dictionary::learn`]: righttype::dict::Dictionary::learn

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use righttype::{dict, secret};
use zeroize::Zeroize;

const MIN_LEN: usize = 3;
const MAX_LEN: usize = 20;
/// Thai words are written without spaces and run longer; still bounded.
const MIN_THAI_LEN: usize = 2;
const MAX_THAI_LEN: usize = 30;
const REPEATS: u8 = 3;

static ENABLED: AtomicBool = AtomicBool::new(false);
static LEARNED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
static PENDING: Mutex<BTreeMap<String, u8>> = Mutex::new(BTreeMap::new());
static PERSIST_TX: OnceLock<SyncSender<String>> = OnceLock::new();
/// The user's sync folder, if any.
static FOLDER: Mutex<Option<PathBuf>> = Mutex::new(None);
/// The file's modification time after RightType last wrote or read it, to
/// tell another PC's change from our own.
static SEEN: Mutex<Option<SystemTime>> = Mutex::new(None);
static TICKS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
/// File name inside a sync folder.
const SYNC_FILE: &str = "RightType learned words.txt";

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
    if let Some(folder) = FOLDER.lock().unwrap().as_ref() {
        return Some(folder.join(SYNC_FILE));
    }
    let mut p = crate::data_dir::righttype_dir()?;
    p.push("learned.txt");
    Some(p)
}

/// The sync folder in use, if any.
pub fn folder() -> Option<PathBuf> {
    FOLDER.lock().unwrap().clone()
}

/// Remember the file's current modification time as our own.
fn note_seen() {
    let modified = learned_path()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok());
    *SEEN.lock().unwrap() = modified;
}

/// At startup, from the config: use `folder` (no merge) and load from it.
pub fn start_with_folder(folder: Option<PathBuf>) {
    *FOLDER.lock().unwrap() = folder;
    reload();
}

/// Move the list to `folder` (or back to this PC with `None`): the words
/// here and the words already there are merged, written there, and used.
/// Returns how many words the list has afterwards.
pub fn set_folder(folder: Option<PathBuf>) -> usize {
    let mut words = list();
    *FOLDER.lock().unwrap() = folder;
    words.extend(read_file());
    replace(&words).kept
}

/// The words in the current file (unchecked lines; `replace` checks them).
fn read_file() -> Vec<String> {
    learned_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.lines().map(str::trim).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Make the file the list again: forget what was learned and load it.
fn reload() {
    dict::english().forget_learned();
    dict::thai().forget_learned();
    load();
    note_seen();
}

/// Session timer tick (1.5 s): every few seconds, pick up a change another
/// PC made to the list in the sync folder.
pub fn tick() {
    if TICKS.fetch_add(1, Ordering::Relaxed) % 4 != 0 || FOLDER.lock().unwrap().is_none() {
        return;
    }
    let modified = learned_path()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok());
    if modified.is_some() && modified != *SEEN.lock().unwrap() {
        reload();
    }
}

/// Load the persisted learned words into the live dictionaries. Call once at
/// startup. Lines that no longer pass the shape guards are ignored.
pub fn load() {
    let set: HashSet<String> = learned_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .filter(|l| teach(l))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    *LEARNED.lock().unwrap() = Some(set);
}

/// Add a word to whichever live dictionary its script belongs to.
fn teach(word: &str) -> bool {
    if eligible_shape(word) {
        dict::english().learn(word);
        true
    } else if eligible_thai_shape(word) {
        dict::thai().learn(word);
        true
    } else {
        false
    }
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
    note_seen();
    if let Some(s) = LEARNED.lock().unwrap().as_mut() {
        s.clear();
    }
    dict::english().forget_learned();
    dict::thai().forget_learned();
}

/// The typist reversed one of RightType's automatic conversions and this is
/// the word they kept. Learn it immediately — English or Thai. No-op unless
/// learning is enabled.
pub fn learn_now(word: &str) {
    if !is_enabled() {
        return;
    }
    // Edge punctuation travels with a token (it may be a Thai letter on the
    // other layout) but is not part of the word.
    let word = word
        .trim()
        .trim_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace());
    let mut key = if eligible_shape(word) {
        if dict::english().contains(word) {
            return;
        }
        word.to_ascii_lowercase()
    } else if eligible_thai_shape(word) {
        if dict::thai().contains(word) {
            return;
        }
        word.to_string()
    } else {
        return;
    };
    PENDING.lock().unwrap().remove(&key);
    commit(&key);
    key.zeroize();
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
    if dict::english().contains(&key) {
        return; // already known (bundled or learned)
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

fn eligible_thai_shape(word: &str) -> bool {
    let len = word.chars().count();
    (MIN_THAI_LEN..=MAX_THAI_LEN).contains(&len)
        && word.chars().all(|c| ('\u{0E01}'..='\u{0E5B}').contains(&c))
}

/// Every learned word, sorted, for the Settings editor.
pub fn list() -> Vec<String> {
    let mut words: Vec<String> = LEARNED
        .lock()
        .unwrap()
        .as_ref()
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default();
    words.sort();
    words
}

/// What [`replace`] did with the lines it was given.
pub struct Replaced {
    pub kept: usize,
    /// Lines that are not a single English or Thai word (or look like a
    /// secret) and were left out.
    pub skipped: usize,
}

/// Make `lines` the complete learned list (the Settings editor's Save): words
/// removed from it are forgotten at once, new ones take effect at once, and
/// `learned.txt` is rewritten. Each line must pass the same guards as a word
/// RightType learns by itself.
pub fn replace(lines: &[String]) -> Replaced {
    let mut kept: Vec<String> = Vec::new();
    let mut skipped = 0;
    for line in lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()) {
        let word = if eligible_shape(line) {
            line.to_ascii_lowercase()
        } else if eligible_thai_shape(line) {
            line.to_string()
        } else {
            skipped += 1;
            continue;
        };
        if !kept.contains(&word) {
            kept.push(word);
        }
    }
    kept.sort();

    dict::english().forget_learned();
    dict::thai().forget_learned();
    for word in &kept {
        teach(word);
    }
    if let Some(p) = learned_path() {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut text = kept.join("\n");
        if !text.is_empty() {
            text.push('\n');
        }
        let _ = std::fs::write(p, &text);
        text.zeroize();
    }
    note_seen();
    let count = kept.len();
    *LEARNED.lock().unwrap() = Some(kept.into_iter().collect());
    Replaced {
        kept: count,
        skipped,
    }
}

fn commit(word: &str) {
    if let Some(set) = LEARNED.lock().unwrap().as_mut() {
        if !set.insert(word.to_string()) {
            return;
        }
    }
    teach(word);
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
    note_seen();
}

#[cfg(test)]
mod tests {
    use super::{eligible_shape, eligible_thai_shape};

    #[test]
    fn thai_learning_shape_is_thai_letters_only() {
        assert!(eligible_thai_shape("ไลน์"));
        assert!(eligible_thai_shape("อัฟฟาน"));
        for denied in ["ก", "abc", "ไลน์x", "ไลน์ 1", "ไลน์123"] {
            assert!(!eligible_thai_shape(denied), "{denied}");
        }
    }

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
