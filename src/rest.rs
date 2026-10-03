//! Rest reminders (2.3 C9) — Windows only; the timing is in
//! [`righttype::breaks`]. Off unless turned on. Counts and times only, in
//! memory: nothing about the keys is kept.

use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use righttype::breaks::Streak;
use righttype::i18n::{trf, T};

static ON: AtomicBool = AtomicBool::new(false);

thread_local! {
    static STREAK: RefCell<Streak> = RefCell::new(Streak::new());
}

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

pub fn set_enabled(on: bool) {
    ON.store(on, Ordering::Relaxed);
    if !on {
        STREAK.with(|s| *s.borrow_mut() = Streak::new());
    }
}

/// A key pressed (the keyboard hook).
pub fn key() {
    if enabled() {
        STREAK.with(|s| s.borrow_mut().key(now_ms()));
    }
}

/// Every few seconds (the session timer): remind when due.
pub fn tick() {
    if !enabled() {
        return;
    }
    if let Some(due) = STREAK.with(|s| s.borrow_mut().due(now_ms())) {
        crate::overlay::show(&trf(
            T::RestDue,
            &[
                ("minutes", &due.minutes.to_string()),
                ("keys", &due.keys.to_string()),
            ],
        ));
    }
}
