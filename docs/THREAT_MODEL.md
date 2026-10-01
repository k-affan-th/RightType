# RightType v1 Threat Model

> Scope: Windows v1, exact Thai Kedmanee ↔ US English QWERTY. This document records
> implemented controls and residual risks; it is not a claim that Windows E2E has passed.

## Trust boundaries

1. The low-level keyboard hook receives global key events inside the RightType process.
   A low-level mouse hook (2.0) looks only at whether a button went down — never
   where — so a click that moves the caret drops everything recorded about the
   text before it (word in progress, owned run, Undo, recent words, Suggest).
2. Focus/UI Automation and foreground-process queries decide whether the context is safe.
3. `SendInput` crosses from RightType into the focused application.
4. Selection conversion reads the selection from the focused app through UI
   Automation; it crosses the Windows clipboard boundary (`Ctrl+C`) only when the
   app does not share it that way **and** the user set `selection_via_clipboard`
   in `config.toml` (off by default). What RightType writes to the clipboard (the
   restore in that fallback, Fix text's "Copy") is marked not for clipboard
   history, not for cloud sync to other devices, and not for clipboard monitors
   that honour those formats.
5. Config and opt-in learned words cross the disk boundary under `%APPDATA%\RightType`.
6. "Check for updates" (2.0) asks Windows to open the Releases page in the default
   browser (`ShellExecuteW`); RightType itself still makes no network connection.

## Data and lifetime matrix

| Data | Location/lifetime | Disk/network | Cleanup/control | Residual risk / evidence needed |
| --- | --- | --- | --- | --- |
| Current token | Preallocated `WordBuffer`, until boundary/reset/context change | None | zeroize on clear/drop; buffer region is best-effort `VirtualLock` | verify `VirtualLock`; OS/hook E2E |
| Detection candidate | Temporary `String`, one boundary callback | None | zeroized after commit/failure | stack/allocator copies are not all locked; latency benchmark open |
| Undo text | One record, max 30 seconds or until context change/use | None | `Drop` zeroize; secret-shaped originals are not recorded | context/focus race E2E open |
| Suggest original/candidate | One boundary, until next non-modifier/context/mode change or accept | None | `Drop` zeroize | accept/reject E2E open |
| Manual selection | Worker-local strings during one command; for a standard Windows text box (Edit/RichEdit) the box's whole text for a moment, to cut the selection out of it | Read from the app through UI Automation, or for a standard Windows text box with `EM_GETSEL` + `WM_GETTEXT`: nothing on the clipboard; no network. Opt-in fallback (`selection_via_clipboard`): the app's own `Ctrl+C` puts the selection on the clipboard, where Windows clipboard history, cloud sync and any clipboard monitor can take it before RightType restores the previous clipboard | local strings/snapshot zeroized on drop (the UI Automation string the app returns is freed by Windows, not zeroized); fallback off by default, and apps without a UI Automation text pattern then get a message instead | e2e: `convert selection leaves the clipboard alone` (sequence number and text unchanged) |
| Learning pending word | In-memory repeat counter while learning is enabled | Only qualifying word after third sighting is appended to `learned.txt`; a word the user restores by reverting an automatic conversion (Undo / Shift+Backspace) is appended at once, English or Thai; no network | opt-in; letters-of-one-script shape guard, secret guard, length bounds; disable drains/zeroizes pending map; bounded persistence queue | whitelist is structural, not per-app; adversarial persistence audit open |
| Learned words | Loaded from `learned.txt` into the in-memory dictionary overlay for the process lifetime | Already on disk by the user's opt-in | "Clear learned words" empties file and overlay | overlay strings are not zeroized (they are the user's persisted vocabulary, not transient input) |
| Settings | Runtime state and custom blacklist | `config.toml`; no typed content/network | bounded async writes from hook-triggered changes | file permissions/atomic-write behavior not audited |
| Recent words (2.0) | Up to 8 completed words as on screen, with their boundaries, for Shift+Backspace pressed repeatedly | None | `Drop` zeroize; cleared by Backspace, navigation keys, any Ctrl/Alt/CapsLock chord, a mouse click, a focus/window/layout change, switching off, and the seed-phrase guard; only words separated by single spaces are kept together | more than one word of recent text is in memory at once (v1 kept one); same boundaries as the v1 last word |
| Per-app modes (2.0) | Executable name → mode; the last app typed in (its executable name only) | `app_modes` in `config.toml`; no typed content; no network | edited in Settings → Apps or the tray's "In this app"; the safety blacklist is checked first and cannot be overridden | exe names reveal which apps have a mode set |
| Pause (2.0) | End time of a pause, process-local | None; a pause is not saved as "off" | ends on the 1.5 s timer or when switched on by any means | none content-related |
| Learned-words export/import (2.0) | A file the user chooses in a dialog | The learned list written to / read from that file, only on the button press; imported lines pass the same shape and secret guards as the editor; files over 1 MB refused | user-initiated only | the exported file is outside `%APPDATA%\RightType` and outside RightType's "Clear" |
| Fix text window (2.0) | Text the user pastes or takes from the clipboard, in the window's two edit controls and short-lived strings while it is open | Clipboard read only on "Paste and fix", written only on "Copy" — marked not for clipboard history or cloud sync, so it stays on this PC; nothing on disk; no network | strings zeroized after each fix; edit controls emptied when the window closes (the controls' own heap is Windows') | same secret guards as the hook (`detect_token`); the seed-phrase stream guard does not apply to pasted text — it is fixed only on the user's explicit request and shown, never typed into another app |
| Caret hints (2.0) | Caret rectangle read at a layout switch or Suggest hint; tag text is `TH`/`EN`, the hint is the suggested word | None | nothing kept after the pill hides (hint text zeroized as before) | none beyond the existing Suggest hint |
| Field language habits (2.0, opt-in) | Per field (program file name + focused control's class name): two word counts, Thai and English; at most 500 fields | `contexts.toml` under `%APPDATA%\RightType`, saved about once a minute while it changes and at exit; no words, no window titles; no network | off by default; Settings → Apps turns it on and **Clear** deletes the counts and the file; nothing recorded or switched in blocked apps, apps switched off, or password fields (the switch runs after the focus password check) | reveals which programs/field types are used and in which language |
| Daily counts (2.0, opt-in) | Two numbers per local calendar day (automatic fixes, hotkey fixes), at most 56 days | `stats.toml`; no words, times of day or app names; saved about once a minute while changing and at exit; no network | off by default (Statistics window toggle); turning it off deletes the file | shows on which days RightType was used, and how much |
| Learned words in a sync folder (2.0, opt-in) | The learned list, in a folder the user picks (e.g. OneDrive) | `RightType learned words.txt` in that folder, synced by whatever syncs the folder — RightType itself makes no network connection; the file is re-read when another PC changes it | "This PC only" moves it back (the folder copy is left for the user to delete); "Clear all" also empties the shared file | the list leaves the PC through the user's own sync service; a file edited elsewhere is still filtered by the same shape/secret guards on load |
| Custom hotkeys (2.0) | The chords, in `config.toml` (only those changed from the defaults) | no typed content | Settings → Hotkeys; while "Change" waits, the hook takes the next chord (and nothing else) and hands it to Settings; Esc or closing Settings cancels | chords that type text or move the caret without Ctrl/Alt are refused, so a binding cannot swallow ordinary typing |
| Keyboard choice (2.0) | Which Thai keyboard (Kedmanee / Pattachote) the user picked; which English keyboard (US / UK) is active is read from Windows and kept in memory only | `thai_layout` in `config.toml`, only when Pattachote; no typed content | Settings → Hotkeys | a wrong choice converts with the wrong table; Auto still requires every word to pass the same dictionary rules |
| Command palette (2.0) | A list of commands; the app it acts on is the program file name of the window focused when it opened | None | commands only change settings or open windows — none reads or writes text | focus returns to the previous window when it closes |
| Caret position via UI Automation (2.0) | The focused element's selection rectangle, read only to place the TH/EN tag or the Suggest hint | None | read after the keyboard hook returns, never inside it; nothing kept | UIA is cross-process: a slow provider delays only the tag, not typing |
| Reversed words (2.0.2) | Up to 5 words the typist reversed (Shift+Backspace or undo of an automatic fix) while learning is off, in memory, newest first | Nothing on disk unless the typist picks "Never convert …" in the command palette, which adds that one word to `learned.txt` | same shape, secret and dictionary checks as learning; wiped when taught, pushed out, or at exit | the palette shows these words while it is open |
| Check-after-write (2.0.2) | After a correction typed as keys, the same number of characters before the caret, read back from the app on a worker thread about 120 ms later, compared with what was sent, then zeroized; the program names of apps that showed a correction differently, for the process lifetime | None; no network | skipped if the typist pressed a key since, the focus or window changed, text is selected, or the field is a password field (UI Automation `IsPassword`, `ES_PASSWORD`); only the verdict (and how many characters matched) reaches the problem report | the read-back is a cross-process call to the app, like reading a selection; the UI Automation string the app returns is freed by Windows, not zeroized |
| Problem report (2.0.2) | The last 400 decisions, process-local: fixed messages written into the program, numbers, yes/no, and a word only as letter counts (`[th 6 en 0 other 0]`), with the program file name and control class name when the caret moves to another window; timestamps are milliseconds since RightType started | Nothing on disk unless the user picks tray → "Save a problem report…" and a file; no network | `diag` takes only `'static` messages and numbers, so a typed word cannot be passed in; names that are not plain ASCII program/class names are replaced by `?`; nothing is recorded for keys in password fields or protected apps; gone at exit | word lengths and the time between words are in it; the programs used are named; e2e `problem report has no typed text` checks every word the sweep typed is absent |
| Snippets (2.1) | Triggers and texts the user writes in Settings → Snippets (at most 200, 1,000 characters each) | `snippets` in `config.toml` (and in the sync folder's `RightType settings.toml` when settings sync is on); never anything typed by RightType's own recording | Settings → Snippets (remove); not expanded in password fields or blocked apps | the user may put something private in a snippet: it is stored like any other setting (plain text in their profile) |
| Recent words in the palette (2.1) | The same up-to-8 recent words as Shift+Backspace (no new or longer buffer), shown in the palette while it is open; their screen boxes read once through UI Automation when it opens | None | words and labels zeroized when the palette closes; tints are click-through, excluded from screen capture and destroyed on close | the words are in the palette's controls while it is open |
| Off in this field (2.1) | Up to 32 field identities (a hash of the UI Automation runtime id — no names, no text) | None; gone when RightType exits | palette → "On again in this field" | a page that rebuilds its fields gives them new ids (RightType works there again) |
| Calmer-mode offer (2.1) | Per app (program file name): the times of the last fixes taken back within ten minutes, and whether it was offered this run | None; the choice "this time only" is memory only, "from now on" is a per-app mode in `config.toml` | counts reset after an offer; gone on exit | none content-related |
| Code mode context (2.1) | For a word Code mode would otherwise fix: up to 160 characters before the caret, read through UI Automation on a worker (80 ms at most) to tell code from comments/strings | None | zeroized right after the check | read only in code editors (or apps set to Code), only at such a word |
| Common misspellings (2.1, opt-in) | A fixed list compiled into the program; the misspellings the typist took back this run (from that list, no other text) | None | off by default; Settings → General | none content-related |
| Screen-reader notifications (2.1) | The message or fixed word, raised as a UI Automation notification only while a UI Automation client (screen reader) is listening | Goes to that program on this PC | string zeroized after raising | the screen reader may keep its own speech history |
| One instance / watchdog (2.1) | Process id, version and program path of the running RightType in a named shared-memory block (session-local); a watchdog process that only waits on RightType's process handle | None | gone when RightType exits; "start again after a crash" can be turned off | another program in the same session can read which RightType is running (no user data) |
| Settings sync (2.1, opt-in) | Modes, per-app modes, blocked apps, hotkeys, snippets, a few switches | `RightType settings.toml` in the user's sync folder (and whatever syncs that folder) | off by default; turn off in Settings → Learned words | the sync provider sees the settings and snippets |
| Stats | Two process-local counters (time saved is computed from them) | None | reset on process exit | none content-related |
| Toast/UI copy | Fixed status/error strings; the Suggest hint and the Auto preview show the candidate text | None; the pill is excluded from screen capture (`WDA_EXCLUDEFROMCAPTURE`: screen sharing, recordings, screenshots), so typed content does not reach a shared screen | UI-thread owned; the displayed text is zeroized when the toast hides | Windows UI evidence open |

## Deny policy

- Native password controls, UIA password controls, fixed/custom blacklisted processes
  (including remote-desktop and VM clients, whose keys go to another computer),
  a full-screen program without a text cursor and not focused on a text field
  (games: `wasd` spells ไฟหก),
  and process/focus-query failures deny buffering, correction, Suggest and learning.
- Hex private-key shapes, WIF/Base58 keys, Bech32 addresses and extended keys are hard
  token denies even if their converted text looks valid.
- High-entropy/overlong ASCII can be wrong-layout Thai. In a normal context it may be
  corrected only when the whole converted token is known Thai; it is never learned.
- A single BIP39 word cannot be a hard deny because ordinary words overlap the BIP39
  list (for example, `correct`). The stream tracker denies once a run reaches four.
  A token counts as a seed word by any of its readings: the wrong-layout candidate,
  the raw text, or the English its keys spell on the Thai layout (so `cat`/`ski`,
  which are real Thai words there, no longer break the run). Every one of the 2,048
  words extends the run on either layout (`secret::tests`).
- Mid-word (live) conversion is held as soon as three seed words are in a row, so a
  fourth cannot be converted before its boundary; the tracker itself counts each
  completed token exactly once.
- When the run trips, the Undo record, the recent completed words and any pending
  Suggest hint (including the toast text) are dropped and zeroized, so the words
  before the threshold do not outlive it in the process.

## Known residual risks

| ID | Risk | v1 handling | Release requirement |
| --- | --- | --- | --- |
| TM-001 | A seed phrase's first three words cannot be told apart from ordinary English (`about`, `able`, `correct` are BIP39 words); an online converter cannot know future words. | Sensitive apps/password fields hard deny; learning never stores BIP39 words (all are in the bundled English dictionary); from the threshold on, correction/Suggest/learning are denied and recent copies are wiped. Words 1–3 may still be converted on the wrong layout, which writes only what the typist meant to type into the focused app. | README must not claim every wrong-layout BIP39 phrase is untouched (it does not); Windows adversarial run: `e2e/release_gaps.py`. |
| TM-002 | UIA/process identity can be temporarily unavailable. | Tri-state/failure paths deny by default. | Native/browser/Electron and process-query E2E. |
| TM-003 | `VirtualLock` and WER hardening are best effort. | Buffer zeroization remains primary; process runs non-elevated. | Record runtime return values or diagnostic evidence without logging content. |
| TM-004 | Clipboard owners can be slow or locked and focus can change during manual copy. | Bounded worker, one-second command expiry, sequence/context checks, clipboard restore before injection; opening the clipboard retries ~100 ms and a still-held clipboard is refused as "busy" before anything is copied. | Focus race passed (`manual_selection.py`); locked clipboard: `release_gaps.py clipboard` on Windows. |
| TM-005 | A hook can be evicted without a resume/unlock event. | Resume/unlock reinstall plus delayed retry, and an independent liveness check: when the system reports input within 3 s but the hook has heard nothing for 30 s, it is reinstalled (rate-limited to once per 30 s). If a reinstall fails, the tray turns grey and says so, a toast tells the user, and it is retried every 1.5 s until it succeeds. | Sleep/resume run (`transition_probe.py`); lock/unlock and UAC passed (E-030/E-031). |

## No-network/dependency claim

The crate manifest has no HTTP, telemetry, updater or analytics dependency. This is a
source-level observation backed by CI: every push runs `cargo audit` and fails the
Windows job if `cargo tree --features winos` contains an HTTP/TLS/telemetry crate.
