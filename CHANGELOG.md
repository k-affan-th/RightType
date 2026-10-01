# Changelog

All notable changes to RightType. Versions follow [Semantic Versioning](https://semver.org/).

## [2.1.0]

### Added

- **Code mode** for code editors (VS Code, Cursor, Visual Studio,
  JetBrains IDEs, Sublime Text, Notepad++ and others; on by default there,
  changeable per app):
  - names are never touched: `camelCase`, `snake_case`, `CONSTANT`, and
    anything with digits or `_ . :: ->`;
  - Thai keys typed for code come back as the English typed (`ฟหกด` →
    `asdf`), even when that is not a dictionary word;
  - English typed for Thai becomes Thai only inside a comment or a string;
    where the editor does not share its text, it is offered as a hint;
  - CapsLock is not treated as an accident (`MAX_SIZE` is meant).
- **A calmer mode offered where fixes keep being taken back**: three in ten
  minutes in one app, and RightType offers Suggest (or Manual) there —
  **this time only** or **from now on** — from the palette (1 or 2).
- **Settings → Apps is a table**: app, mode, set by (you, this time only,
  default, safety), and where the program is. Select a row and pick a mode,
  or right-click it (keep for good, remove, show the program file);
  **+ Add an app that is open** lists running programs instead of typing a
  name. Built-in safety apps are listed, locked.
- **Recent words in the palette**: the last words typed, each with what it
  would flip to. Tick several with Space and they are **tinted where they
  are in the app** (when the app says where), then Enter flips exactly
  those and leaves the words between them alone.
- **Off in this field** (palette): RightType stays out of one field — a
  search box, a code cell — and keeps working everywhere else in the app,
  until it restarts.
- **Snippets** (Settings → Snippets): a short trigger and Space becomes a
  longer text, line breaks included. Each works on the Thai keyboard, the
  English one, or **either** — matched by the keys pressed, so `;addr`
  works with the Thai keyboard on too. Shift+Backspace right after puts the
  trigger back. Not in password fields.
- **Common Thai misspellings put right** (opt-in, Settings → General or the
  palette): `อนุญาติ` → `อนุญาต`, `ผลลัพท์` → `ผลลัพธ์` and about 60 more,
  only where the result reads better to the dictionary and never a word the
  dictionary knows. Each fix shows in its own colour (purple) with what
  changed; **Backspace right after puts the word back** instead of
  deleting into it, and that word is left alone from then on.
- **Screen readers** (Narrator, NVDA, JAWS) hear each fix ("Fixed:
  สวัสดี"), tag and message, only while one is running.
- **High Contrast**: Settings, the palette, tags and highlights use the
  contrast theme's own colours.
- **One RightType at a time, with no dialog**: opening it again shows
  "already running" for a moment; opening another version or copy closes
  the running one and takes over ("Now running 2.1.0, closed 2.0.1").
- **Start again after a crash** (on by default, Settings → Privacy & about):
  not after you quit it or end it in Task Manager, at most three times in a
  row.
- **Sync settings and snippets too** (opt-in): with a sync folder for the
  learned words, modes, apps, hotkeys and snippets are shared by every PC
  using that folder.
- **Windows on ARM**: an arm64 installer and zip.
- **A preview while you type (Auto)**: before Auto is sure enough to fix a
  word, the cursor tag shows where the keys are heading — `l;yl` shows
  `→ สวัส` — so you can see a fix is coming without anything changing yet.
- **Command palette** (`Ctrl`+`Alt`+`Space`) gains:
  - **Fix this field**: every wrong-layout word in the field, fixed in one
    go (read from the app, never through the clipboard; Ctrl+Z undoes it).
  - **Never convert “word”**: for words you took back lately while learning
    is off — one click and RightType leaves that word alone from then on.
  - **Thai digits ↔ 0–9**, **UPPER CASE**, **lower case**, **Title Case**
    and **sWAP cASE** (CapsLock undone) for the selection.
  - **Works without a mouse**: ↑↓ (or Tab) and Enter, 1–9 to run a line,
    type to search (on either keyboard: `fxw` finds ซ่อม), Esc to close.
  - **CapsLock switches Thai/English** (off by default): tap CapsLock to
    switch, hold it half a second for CAPS.
  - **Tray icon shows TH / EN** (off by default).
- **Thai typed in a wrong order that looks right is put right** (Auto):
  `เเ` (two เ) for `แ`, `ํา` for `ำ`, a tone mark typed before the vowel
  above it, the same mark twice — when the result is a Thai word.
- **Tags and hints stay off shared screens**: Teams, Zoom, recordings and
  screenshots do not show them (a Suggest hint or preview is what you
  typed); you still see them.
- **Portable mode**: an empty file named `portable` next to
  `righttype.exe` keeps settings and learned words in a `data` folder beside
  it instead of `%APPDATA%`.
- **The first three fixes of a session show `↶ Shift+Backspace`** at the
  cursor, and the Welcome window has a box to try a fix in.

- **Tray → "Save a problem report…"** writes what RightType did lately (the
  last 400 steps: a word ended and was or wasn't fixed, how the correction
  was typed, Shift+Backspace, which program the caret moved to) to a file
  you choose, to attach to a bug report. There is no typed text in it:
  a word appears only as how many Thai and English letters it had. It is
  kept in memory only and written only when you save it.

- **Check-after-write**: after RightType types a correction, it reads back
  the word before the cursor (when the app shares its text) and compares.
  If an app shows something other than what was sent — the way Windows 11
  Notepad turned สวัสดี into `ีีีีีี` — it tells you, notes it in the problem
  report, and from then on waits longer before typing in that app.

- **Stays out of full-screen games, Remote Desktop and virtual machines.**
  Movement keys spell Thai (`wasd` is ไฟหก), so a game could get
  Backspaces; and in a remote session or VM the keys belong to the other
  computer. Games in exclusive full screen, slide shows, and any full-screen
  window without a text cursor are left alone (a full-screen browser or
  editor still works); Remote Desktop, the Windows App, Hyper-V, VirtualBox,
  VMware, AnyDesk, TeamViewer, RustDesk, Parsec and VNC viewers are on the
  built-in list.


### Fixed

- **Notepad, WordPad and other standard text boxes: English typed on the
  Thai layout could take two more characters with it when fixed**
  (`;yoouh there` came out as `วันนีthere`). These boxes drop Thai vowels
  and tone marks that cannot follow the letter before (`there` on the Thai
  layout is `ะ้ำพำ`; the box keeps `ะพำ`), so there were fewer characters to
  delete than were typed. RightType now looks at what the box really holds
  before replacing.
- **A slow text box could get a correction in the wrong place**: Windows 11
  Notepad sometimes answers before it has handled the latest keys. If what
  is before the caret is not what RightType expects yet, the correction is
  typed as keys instead, which arrive after them.

### Changed

- **Smoother tags and messages**: the pill now rises a few pixels into
  place while fading in (140 ms, ease-out) and fades out with an ease-in
  (220 ms), each frame computed from the time elapsed at about 60 frames a
  second — it used to appear at once and fade in coarse 30 ms steps. A tag
  that is already showing moves and stays instead of fading in again. The
  TH, EN and CAPS tags each have a colour of their own (teal, blue, amber).
- **A `CAPS` tag at the text cursor** when CapsLock is switched on (with
  the TH/EN tags on), before a sentence comes out in capitals.
- **CapsLock left on by accident is put right, with a way back**: in Auto,
  `hELLO` — Shift on the first letter, so capitals were not meant — becomes
  `Hello`, and a Thai word typed on the Thai layout with CapsLock on (every
  key comes out shifted: สวัสดี as `ศซํศโ๊`) becomes the word meant;
  CapsLock is turned off and a note says so. Meant the capitals? One
  Shift+Backspace (or Ctrl+Shift+CapsLock) puts them back and CapsLock on
  again. In Manual and Suggest it is only offered (`⇪ Hello · Tab`). Words
  in capitals (`VARIABLE`, `NASA`) are never touched.
- **Thai typed with CapsLock left on is still fixed**: the English layout
  shows `L;YLFU`, but the keys are the ones for สวัสดี, and RightType now
  reads them that way. English typed with CapsLock on stays as it is
  (`HELLO`), and anything RightType puts back as typed — Shift+Backspace,
  undo, a word that goes back at the space — comes back in capitals as it
  was shown. Pressing CapsLock mid-word leaves that word alone.
- **English words with a prefix or suffix stay English**: a known word with
  `re`, `un`, `pre`, `dis`, `multi` … in front or `ing`, `ed`, `ness`,
  `able`, `s` … behind (`rerise`, `resit`, `multiholes`) is no longer
  turned into Thai, even when it is not in the word list. Unknown English
  words wrongly converted in the live study: 170 → 112 of 20,000; Thai is
  unchanged (a Thai dictionary word always wins).

## [2.0.1] — 2026-09-30

### Changed

- **Settings, Statistics, Fix text and Welcome appear fully drawn**: they
  used to show their frame first and fill in control by control; switching
  a Settings page repainted it dozens of times.
- **Convert selection no longer goes through the clipboard.** It pressed
  Ctrl+C for you, so the selected text went into Windows clipboard history,
  could be synced to your other devices (an Android phone) and was visible
  to any program watching the clipboard — even though RightType put the old
  clipboard back. It now asks the app for the selection through UI
  Automation. Apps that do not share it say so; `selection_via_clipboard =
  true` in `config.toml` brings back the copy for them.
- What RightType puts on the clipboard (Fix text's **Copy**) is marked to
  stay out of clipboard history and cloud sync.
- **Every Shift+Backspace says what it did**: "Flipped 1 word · again for
  the one before", "Flipped back 3 words", "Undone", or "Nothing to flip
  here" when the cursor moved and RightType no longer knows the text before
  it (a press that did nothing used to look like one that failed). What is
  kept for this, and what each press does, is in `docs/UNDO.md`.
- **Shift+Backspace pressed once too often puts things back**: after the
  oldest word was flipped (or the only one — `reload` → `พำสนฟก`), the next
  press restores every word the run changed, instead of saying there is
  nothing to flip.

### Fixed

- **Corrections in Notepad (Windows 11 and classic), WordPad and other
  standard text boxes are made by the box itself**: RightType tells it to
  replace the word in one edit (Ctrl+Z undoes it) instead of typing
  Backspaces and characters. Typed corrections came out garbled in Windows
  11 Notepad (`สวัสดี` became `ีีีีีี`) and keys typed while a word was being
  rewritten were lost. Elsewhere, text typed after Backspaces now waits
  40 ms for them to land.
- **RightType could crash in Edge's address bar** (2.0.0), after which
  nothing was corrected and Shift+Backspace was a plain Backspace until it
  was restarted: the small TH/EN tag could be painted after its text had
  been wiped, and drawing empty text read an invalid pointer. Found with
  four Edge rounds per CI run and a crash report in the debug build.
- **Shift+Backspace right after an automatic fix did nothing** (2.0.0): the
  Shift key on its own made RightType forget the words it keeps for flipping
  back, so the flip found nothing. It flips the fixed word back again.
- **Corrections in the browser's address/search bar left a stray letter**
  (`giupo` came out as `gเรียน`): when the bar completes what you type and
  selects the rest, the first Backspace only removed that selection.
  RightType now clears it first in Chrome, Edge and other Chromium browsers
  and in Firefox.
- **Edge's address bar forgot the start of the word** (`l;ylfu` came out as
  `l;ัสดี`): its suggestion list gives the highlighted row accessibility
  focus on almost every keystroke, and RightType took each of those as a
  move to another field. Focus on the same field or on a list/menu row no
  longer counts (and leaves the password-field status as it was).
- **An English word RightType does not know could turn Thai** (`relogin`
  became `พำสนเรื`): its first letters read as short Thai words. At the
  space, a reading that cannot end a Thai word goes back to the keys typed
  (when they were all letters), and 24 everyday computer words (`relogin`,
  `logout`, `signin`, `dropdown`, …) were added.
- **Fewer unknown English words turn Thai** (`reavik` became `พำฟอรา`):
  a reading made of nothing but three or more one- and two-letter Thai
  words (พำ + ฟ + อ + รา) is no longer enough to convert a word, mid-way or
  at the space. On 20,000 unknown English words, wrong conversions fell
  from 246 to 170; of 20,000 unknown Thai words, one fewer arrives as Thai.
- With both US and UK English keyboards installed, RightType starts with the
  one in use rather than the first installed, so **Fix text** opened from the
  tray before any typing uses the right punctuation (`"` `@` `£` `#`).

## [2.0.0] — 2026-09-27

RightType 2.0 shows what it does where you are typing, lets you steer it from
the keyboard, and works with more keyboards. The highlights:
a `TH` / `EN` tag and Suggest hints at the text cursor (Tab takes a hint),
Pause, a mode per app, flipping back several words, a Fix text window, a
command palette, hotkeys you can change, Thai Pattachote and UK English, an
opt-in 7-day chart and learned-words sync folder, and install/upgrade through
winget. As always, it never goes online and never saves what you type.

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
- **Fix text** window (tray → Fix text…): paste a paragraph typed in the wrong
  layout, or take it from the clipboard with **Paste and fix**, and copy back
  the fixed text. Every word is judged by the same rules as while typing; in
  a test of 20,000 correctly typed mixed Thai/English sentences, none was
  changed.
- **Hints at the text cursor**: a small `TH` / `EN` tag flashes under the text
  cursor when RightType switches the language, and the Suggest hint appears
  there too. **Tab** takes a Suggest hint right after it appears (Tab is
  otherwise untouched; `Alt`+`CapsLock` still works). Can be turned off in
  Settings → General.
- **Guess each field's language** (opt-in, Settings → Apps): RightType counts
  how many Thai and English words you finish in each kind of field (the program
  and the kind of control), and once a field clearly has one language (20+
  words, 80 %+), clicking into it switches the keyboard before you type. Only
  the counts are stored (`contexts.toml`), and **Clear** removes them.
- A colon or question mark typed after a Thai phrase on the Thai layout (the
  keys give ซ and ฦ there) comes back as `:` / `?` when the phrase is known
  Thai (a dictionary word or three or more known words).
- **Last 7 days** in Statistics (opt-in): a bar per day and the week's total
  words fixed and time saved. Only two numbers per day are kept
  (`stats.toml`); turning it off deletes them.
- **Sync folder for learned words** (Settings → Learned words): keep the list
  in a OneDrive / Google Drive / team folder; both lists are merged when you
  choose it, and a change made on another PC is picked up within seconds.
- **Change the hotkeys** (Settings → Hotkeys): click Change and press the new
  keys. Chords that would get in the way of typing, and chords already in use,
  are refused; Reset all brings back the defaults.
- **Command palette** (`Ctrl`+`Alt`+`Space`): a short list next to the text
  cursor — Fix text, pause, turn RightType off in this app, switch mode,
  settings. Arrow keys and Enter, Esc to close.
- The TH / EN tag and the Suggest hint now also find the text cursor in apps
  that draw their own (through UI Automation).
- **Suggest while typing**: in Suggest mode the Thai reading appears next to
  the cursor as soon as the keys typed so far clearly spell it (the bar Auto
  uses); Tab flips the word there and then.
- Thai words whose keys give no letters on the English layout come back:
  `57'` → ถึง, `]'` → ลง, `[688]` → บุคคล. Numbers, dates, times, prices and
  emoticons never change (0 of 750,043 tested); 163 more Thai dictionary words
  are recovered.
- **Thai Pattachote keyboard**: choose it in Settings → Hotkeys → Thai
  keyboard. Its table comes from the maintained xkeyboard-config layout; the
  same quality gate runs on it (no Thai or English dictionary word changed;
  60,213 of 60,964 Thai words recovered).
- **English on the UK keyboard, and English of other countries** (Australia,
  New Zealand, Canada set to the US keyboard, …): RightType now works with
  them instead of staying off. Which English keyboard you use is found
  automatically, so Thai text converted back to English gets the UK
  punctuation (`"` `@` `£` `#`) when that is your keyboard.
- **Check for updates** in Settings → Privacy & about: opens the Releases page
  in your browser; RightType itself still never goes online.

### Changed

- Hotkey names in Settings and Welcome are no longer hidden under long key
  combinations; they are cut short with "…" only when there is truly no room.
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
- **winget**: each release carries winget manifests
  (`winget-manifests-<version>.zip`, package `k-affan-th.RightType`), and the
  Release workflow submits them to winget when a `WINGET_TOKEN` secret is set.
- Release checklist: a 2.0 section of checks that need a real Windows desktop.

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
