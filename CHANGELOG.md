# Changelog

All notable changes to RightType. Versions follow [Semantic Versioning](https://semver.org/).

## [1.1.0] — unreleased

### Fixed

- **Whole-word decisions.** A word converted to Thai mid-word is re-judged as a
  whole at the space: if all of its keystrokes spell English, the entire word
  goes back to English and the layout returns to US. Previously only the part
  typed after the switch was judged, leaving half-Thai, half-English words that
  `Shift`+`Backspace` could not flip back whole.
- Keystrokes typed right after RightType switches the layout are read with the
  new layout, and RightType's own switch no longer drops the word in progress.
- English typed right after a Thai word (layout still Thai) is brought back.
- `Ctrl`+`CapsLock` (mode cycle) works in every app, including Electron apps
  such as the Claude desktop app, where it used to toggle CapsLock instead.
- `Alt`+`CapsLock` (accept suggestion) no longer triggers the app's menu bar in
  Chrome/Electron, which swallowed the correction.
- Selection Undo waits for the hotkey's modifiers to be released, so it can no
  longer type a literal `z`.
- Undo is discarded once more text has been typed, so it can no longer delete
  what you typed after the correction; after Undo the layout follows the
  restored text.

### Changed

- Compounds of everyday English words (`middleware`, `workflow`, `frontend`,
  `codebase`) are treated as English.
- Learned words take effect immediately in every decision (they were stored
  but never used). Flipping back an automatic conversion (`Shift`+`Backspace`
  or Undo) with learning on remembers that word at once, in either language.
- Tray: left-click opens the menu, new **Hotkeys & help** item, and the tooltip
  shows the current mode. The Suggest hint shows the suggested text.
- Dictionaries are loaded at startup instead of on the first keystroke.

### Build and release

- Cargo.toml is the single source of the version; `build_release.ps1` passes it
  to the installer, and the portable zip now carries the licenses and this
  changelog.
- Release artifacts are no longer committed to the repository; they are
  published as GitHub Release assets.
- CI now builds, lints and tests the Windows integration layer and runs
  `cargo audit`.

## [1.0.0] — 2026-08-31

First release: Thai Kedmanee ↔ US English QWERTY, Manual/Auto/Suggest modes,
sensitive-context guards, per-user installer and portable zip (unsigned).
