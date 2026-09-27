# Changelog

All notable changes to RightType. Versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased] — 2.0 in progress

### Fixed

- **The status pill (mode, on/off, errors, Suggest hint) never appeared** in
  1.1.0: since it became lazily created, it was only ever allowed to be created
  on a thread it had not yet recorded. It shows again.
- Settings, Welcome and Statistics stay sharp on a second monitor with a
  different scale: RightType is now per-monitor DPI aware (v2), opens windows on
  the monitor under the mouse, and re-lays a window out when it is dragged to a
  monitor with another scale.

### Added

- **Pause** from the tray: 10 minutes, 30 minutes or an hour, then RightType
  switches itself back on (the tooltip shows the time left). A pause is never
  saved as "off".
- **A mode per app**: Auto, Suggest, Manual or Off for one program — say Manual
  in your code editor and Auto in chat. Set it from the tray (**In this app**,
  for the app you were just typing in) or in Settings → **Apps**. The built-in
  safety list (password managers, terminals, wallets) still always wins.
  `Ctrl`+`CapsLock` in such an app cycles that app's mode.
- **Flip back several words**: press `Shift`+`Backspace` again to flip the word
  before as well, up to 8 words, so a phrase typed in the wrong layout comes
  back without selecting it. A click, arrow key or Backspace in between starts
  over.
- **Import / Export** learned words as a text file (Settings → Learned words).
- **Time saved** estimate in Statistics.
- **Check for updates** in Settings → Privacy & about: opens the Releases page
  in your browser; RightType itself still never goes online.

### Changed

- A mouse click now counts as moving the caret: the word in progress, Undo and
  the words kept for `Shift`+`Backspace` are dropped, so a flip can never edit
  text somewhere else.
- The status pill appears on the monitor you are working on (the one holding
  the active window) instead of always the primary one, sized for that
  monitor. It can also be placed next to the text cursor (used by the 2.0 caret
  badge).
- **Technical terms and product names** in their usual casing (`PyThaiNLP`,
  `WangchanBERTa`, `RoBERTa`, `LoRA`, `JavaScript`, `GitHub`, … ~300 in
  `assets/tech_terms.txt`) come back when typed on the Thai layout. In the
  typing benchmark RightType now fixes 127 of 131 wrong-layout words (was 118),
  still with no correctly typed word changed.
- If the keyboard hook is lost and cannot be reinstalled, RightType says so
  (message and grey tray icon with a warning tooltip) and keeps retrying, then
  tells you when it is back.
- The installer is available in Thai (chosen from the Windows language).

### Build and release

- CI quality gate: the false-positive audit and the typing benchmark run on
  every PR and fail if any correctly typed dictionary word, phrase or
  benchmark word would be changed, or recall drops below its floor.
- CI builds the installer on every PR, so a broken installer script is caught
  before release.

## [1.1.0] — 2026-09-26

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
- **Seed-phrase guard:** every BIP39 word now counts toward the guard on either
  layout (words like `cat`/`ski`, which are real Thai words on the Thai layout,
  used to reset it); mid-word conversion is held once three seed words are in a
  row; and when the guard trips, the Undo record, last word and any Suggest hint
  are wiped from memory.
- Converting a selection while another program holds the clipboard retries
  briefly and then says the clipboard is busy, instead of wrongly saying it
  needs plain text.
- If Windows silently drops the keyboard hook (no sleep/lock event involved),
  RightType notices the silence while you type and reinstalls it within ~30 s.

- **Backspace inside a word** could leave raw keys mixed into the Thai
  (`mujouj1` then Backspace showed `muที่นี่`). Found by the new typing
  simulation; a Backspace now re-renders only a run RightType already owns.
- **Fewer wrong conversions of Thai typed on the Thai layout.** Punctuation
  keys that are Thai letters on Kedmanee (`;` ว, `[` บ, `'` ง, `,` ม) are no
  longer stripped before checking for English, and English read from Thai
  keys must look like English. Across the bundled dictionaries: Thai words and
  phrases wrongly converted 1,994 → 0, English words with punctuation 146 → 0,
  random unknown Thai 2.6 % → 0.18 % (see `docs/TYPING_BENCHMARK.md`).

### Changed

- **Technical English typed on the Thai layout comes back**: numbers
  (`12,480`, `0.912`, `64%`, `2e-5`), acronyms (`GPU`, `A100`), derived words
  (`tokenization`) and hyphenated terms (`fine-tuning`, `F1-score`, `TF-IDF`),
  also inside quotes or brackets. Thai typed on the English layout keeps a
  `:`/`?`/`"` typed after it. In a simulated academic article typed without
  ever switching layout, RightType now fixes 90 % of wrong-layout words
  (was 70 %), with no correctly typed word changed.

- **New look.** Settings, Welcome and Statistics were rebuilt: a Settings
  sidebar with General / Hotkeys / Blocked apps / Privacy & about pages, cards,
  toggle switches, a segmented mode picker, key caps for hotkeys, and an accent
  colour. Changes in Settings apply immediately (no Apply/OK).
- **Learned words are editable.** Settings → Learned words lists every word
  RightType has learned; add, remove or clear them and Save — changes take
  effect at once.
- **Typeface:** IBM Plex Sans Thai is embedded and used for every window and
  the toast, so Thai and English share one modern design.
- **Thai interface.** Every window, the tray menu and all messages are available
  in Thai; the language follows Windows and can be switched in Settings.
- **Light and dark** — the windows follow the Windows app theme (previously
  always dark).
- **Sharp at any display scaling** — RightType is DPI-aware; at 125–200 %
  Windows used to stretch its windows and toast into a blur.
- **New icon**, with a grey tray icon while RightType is off; the `.exe` now
  carries the icon and version information.
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
- Release artifacts are no longer committed to the repository: the Release
  workflow builds them on Windows and publishes them as GitHub Release assets,
  with install instructions in Thai and English.
- CI now builds, lints and tests the Windows integration layer and runs
  `cargo audit`.

## [1.0.0] — 2026-08-31

First release: Thai Kedmanee ↔ US English QWERTY, Manual/Auto/Suggest modes,
sensitive-context guards, per-user installer and portable zip (unsigned).
