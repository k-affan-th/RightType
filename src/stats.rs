//! Usage counters — Windows only. **In-memory only, never written to disk.**
//!
//! RightType's no-persistence guarantee covers *content* (nothing typed is ever
//! saved), not counts — but to keep that guarantee unambiguous, these counters
//! reset every restart rather than accumulating in a file. They exist purely so
//! the user can see RightType is doing something ("Stats" in the tray menu),
//! mirroring RightLang's tray stats.

use std::sync::atomic::{AtomicU64, Ordering};

static AUTO: AtomicU64 = AtomicU64::new(0);
static MANUAL: AtomicU64 = AtomicU64::new(0);

/// Record one correction made automatically (Auto mode: boundary or eager run-on).
pub fn record_auto() {
    AUTO.fetch_add(1, Ordering::Relaxed);
}

/// Record one correction made via a manual hotkey (fix-word or fix-selection).
pub fn record_manual() {
    MANUAL.fetch_add(1, Ordering::Relaxed);
}

/// `(auto corrections, manual corrections)` since this run started.
pub fn snapshot() -> (u64, u64) {
    (AUTO.load(Ordering::Relaxed), MANUAL.load(Ordering::Relaxed))
}
