//! RightType — a modern, safe Rust successor to RightLang.
//!
//! Fixes text typed in the wrong keyboard layout (e.g. Thai Kedmanee typed while
//! the English QWERTY layout is active, or vice-versa) without retyping.
//!
//! The crate is split into an OS-free **pure-logic core** (this library) and a
//! Windows **integration layer** (the binary). Keeping the core OS-free makes it
//! exhaustively unit-testable and lets a future macOS backend reuse it unchanged.
//!
//! Pure-logic modules:
//! - [`layout`] — N-layout-generic conversion engine (Kedmanee ↔ QWERTY in v1).
//! - [`secret`] — hard-deny key/address shapes plus password/BIP39 stream guards.
//! - [`dict`] — word dictionaries for membership checks.
//! - [`english`] — English beyond the dictionary: compounds and continuations.
//! - [`hotkeys`] — the hotkeys, how they are written, and which are safe.
//! - [`i18n`] — interface strings in English and Thai.
//! - [`predict`] — per-field language habits (opt-in) for switching before typing.
//! - [`per_app`] — per-app correction modes and their text form.
//! - [`detect`] — layout-mismatch detection over completed words.
//! - [`diag`] — recent decisions, with no typed text, for a problem report.
//! - [`code`] — Code mode: what to fix in a code editor.
//! - [`buffer`] — current-word input buffer (zeroized on every word boundary).
//! - [`segment`] — Thai word segmentation (maximal matching) for space-less Thai.
//! - [`motion`] — how the overlay pill fades and rises in and out.
//! - [`render`] — reconciling on-screen text with the run's current best reading.
//! - [`repair`] — fixing a whole piece of finished text (the Fix text window).
//! - [`recent`] — the last few completed words, for flipping back several at once.
//! - [`usage`] — daily correction counts for the opt-in weekly view.
//! - [`sim`] — the hook's Auto pipeline replayed on a string, for benchmarks and tests.

pub mod buffer;
pub mod code;
pub mod detect;
pub mod diag;
pub mod dict;
pub mod english;
pub mod hotkeys;
pub mod i18n;
pub mod layout;
pub mod motion;
pub mod per_app;
pub mod policy;
pub mod predict;
pub mod recent;
pub mod render;
pub mod repair;
pub mod secret;
pub mod segment;
pub mod sim;
pub mod usage;
