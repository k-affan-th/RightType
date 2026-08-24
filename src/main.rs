//! RightType — Windows tray application entry point.
//!
//! The OS integration layer (low-level keyboard hook, Unicode injection, session
//! resilience, tray UI) is built on top of the OS-free core in `lib.rs` and is
//! gated behind the `winos` feature so the core stays buildable/testable anywhere.
//!
//! Build the Windows app with `cargo run --features winos`.

// The real Windows app is windowless — it lives in the tray, no console. The
// non-winos build stays a console stub for quick core smoke-checks.
#![cfg_attr(feature = "winos", windows_subsystem = "windows")]

#[cfg(feature = "winos")]
mod clipboard;
#[cfg(feature = "winos")]
mod config;
#[cfg(feature = "winos")]
mod data_dir;
#[cfg(feature = "winos")]
mod focus;
#[cfg(feature = "winos")]
mod hook;
#[cfg(feature = "winos")]
mod inject;
#[cfg(feature = "winos")]
mod learn;
#[cfg(feature = "winos")]
mod manual;
#[cfg(feature = "winos")]
mod onboard;
#[cfg(feature = "winos")]
mod ram;
#[cfg(feature = "winos")]
mod safety;
#[cfg(feature = "winos")]
mod session;
#[cfg(feature = "winos")]
mod settings;
#[cfg(feature = "winos")]
mod startup;
#[cfg(feature = "winos")]
mod stats;
#[cfg(feature = "winos")]
mod theme;
#[cfg(feature = "winos")]
mod toast;
#[cfg(feature = "winos")]
mod tray;

#[cfg(not(feature = "winos"))]
fn main() {
    println!(
        "RightType {} — keyboard-layout corrector (build with --features winos for the Windows app)",
        env!("CARGO_PKG_VERSION")
    );

    // Smoke-check the pure-logic core is wired up.
    let demo = righttype::layout::en_to_th("correct");
    println!("demo: \"correct\" typed on Thai layout = \"{demo}\"");
}

#[cfg(feature = "winos")]
fn main() {
    // RAM hardening: exclude our heap from crash dumps, suppress the fault
    // dialog. Best-effort, before anything else touches secret-adjacent memory.
    unsafe { ram::harden_process() };
    // The clipboard "convert selection" worker runs off the hook thread.
    let _manual = manual::spawn();
    // Build the tray, install the hook, and run the message loop until Quit.
    tray::run();
}
