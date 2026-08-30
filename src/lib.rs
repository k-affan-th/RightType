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
//! - [`detect`] — layout-mismatch detection over completed words.
//! - [`buffer`] — current-word input buffer (zeroized on every word boundary).
//! - [`segment`] — Thai word segmentation (maximal matching) for space-less Thai.
//! - [`render`] — reconciling on-screen text with the run's current best reading.

pub mod buffer;
pub mod detect;
pub mod dict;
pub mod layout;
pub mod policy;
pub mod render;
pub mod secret;
pub mod segment;
