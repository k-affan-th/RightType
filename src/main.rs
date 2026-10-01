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
mod announce;
#[cfg(feature = "winos")]
mod apps;
#[cfg(feature = "winos")]
mod caret;
#[cfg(feature = "winos")]
mod clipboard;
#[cfg(feature = "winos")]
mod config;
#[cfg(feature = "winos")]
mod data_dir;
#[cfg(feature = "winos")]
mod fixer;
#[cfg(feature = "winos")]
mod focus;
#[cfg(feature = "winos")]
mod habits;
#[cfg(feature = "winos")]
mod hook;
#[cfg(feature = "winos")]
mod inject;
#[cfg(feature = "winos")]
mod instance;
#[cfg(feature = "winos")]
mod learn;
#[cfg(feature = "winos")]
mod manual;
#[cfg(feature = "winos")]
mod marks;
#[cfg(feature = "winos")]
mod onboard;
#[cfg(feature = "winos")]
mod overlay;
#[cfg(feature = "winos")]
mod palette;
#[cfg(feature = "winos")]
mod ram;
#[cfg(feature = "winos")]
mod report;
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
mod tray;
#[cfg(feature = "winos")]
mod ui;
#[cfg(feature = "winos")]
mod verify;

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
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--watchdog") {
        instance::run_watchdog(&args[1..]);
    }
    let restarts: Option<u32> = args
        .iter()
        .position(|a| a == "--restarted")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse().ok());
    // One RightType at a time: settle with any already running (no dialog).
    let replaced = match instance::claim() {
        instance::Start::Leave => return,
        instance::Start::Run { replaced } => replaced,
    };
    instance::set_notice(replaced, restarts);
    if config::load().restart_after_crash && instance::watchdog_allowed() {
        instance::start_watchdog(restarts.unwrap_or(0));
    }
    // Crisp text at 125–200 % display scaling on every monitor: per-monitor
    // aware (v2), so a window moved to a screen with another scale re-lays
    // itself out (ui.rs) instead of being bitmap-stretched. The manifest asks
    // for the same; this covers a binary run without it. Windows too old for
    // v2 fall back to system-aware.
    unsafe {
        use windows::Win32::UI::HiDpi::{
            SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            DPI_AWARENESS_CONTEXT_SYSTEM_AWARE,
        };
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2).is_err() {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_SYSTEM_AWARE);
        }
    }
    // RAM hardening: exclude our heap from crash dumps, suppress the fault
    // dialog. Best-effort, before anything else touches secret-adjacent memory.
    unsafe { ram::harden_process() };
    #[cfg(debug_assertions)]
    hook::report_fatal_exceptions();
    // The clipboard "convert selection" worker runs off the hook thread.
    let _manual = manual::spawn();
    safety::watch_full_screen();
    // Build the tray, install the hook, and run the message loop until Quit.
    tray::run();
}
