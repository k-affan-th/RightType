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
4. Selection conversion temporarily crosses the Windows clipboard boundary for `Ctrl+C`.
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
| Manual selection | Worker-local strings during one command | Clipboard read only; no network | clipboard restored before Unicode injection; local strings/snapshot zeroized on drop | locked clipboard, slow copy and focus-ABA E2E open |
| Learning pending word | In-memory repeat counter while learning is enabled | Only qualifying word after third sighting is appended to `learned.txt`; a word the user restores by reverting an automatic conversion (Undo / Shift+Backspace) is appended at once, English or Thai; no network | opt-in; letters-of-one-script shape guard, secret guard, length bounds; disable drains/zeroizes pending map; bounded persistence queue | whitelist is structural, not per-app; adversarial persistence audit open |
| Learned words | Loaded from `learned.txt` into the in-memory dictionary overlay for the process lifetime | Already on disk by the user's opt-in | "Clear learned words" empties file and overlay | overlay strings are not zeroized (they are the user's persisted vocabulary, not transient input) |
| Settings | Runtime state and custom blacklist | `config.toml`; no typed content/network | bounded async writes from hook-triggered changes | file permissions/atomic-write behavior not audited |
| Recent words (2.0) | Up to 8 completed words as on screen, with their boundaries, for Shift+Backspace pressed repeatedly | None | `Drop` zeroize; cleared by Backspace, navigation keys, any Ctrl/Alt/CapsLock chord, a mouse click, a focus/window/layout change, switching off, and the seed-phrase guard; only words separated by single spaces are kept together | more than one word of recent text is in memory at once (v1 kept one); same boundaries as the v1 last word |
| Per-app modes (2.0) | Executable name → mode; the last app typed in (its executable name only) | `app_modes` in `config.toml`; no typed content; no network | edited in Settings → Apps or the tray's "In this app"; the safety blacklist is checked first and cannot be overridden | exe names reveal which apps have a mode set |
| Pause (2.0) | End time of a pause, process-local | None; a pause is not saved as "off" | ends on the 1.5 s timer or when switched on by any means | none content-related |
| Learned-words export/import (2.0) | A file the user chooses in a dialog | The learned list written to / read from that file, only on the button press; imported lines pass the same shape and secret guards as the editor; files over 1 MB refused | user-initiated only | the exported file is outside `%APPDATA%\RightType` and outside RightType's "Clear" |
| Fix text window (2.0) | Text the user pastes or takes from the clipboard, in the window's two edit controls and short-lived strings while it is open | Clipboard read only on "Paste and fix", written only on "Copy"; nothing on disk; no network | strings zeroized after each fix; edit controls emptied when the window closes (the controls' own heap is Windows') | same secret guards as the hook (`detect_token`); the seed-phrase stream guard does not apply to pasted text — it is fixed only on the user's explicit request and shown, never typed into another app |
| Caret hints (2.0) | Caret rectangle read at a layout switch or Suggest hint; tag text is `TH`/`EN`, the hint is the suggested word | None | nothing kept after the pill hides (hint text zeroized as before) | none beyond the existing Suggest hint |
| Stats | Two process-local counters (time saved is computed from them) | None | reset on process exit | none content-related |
| Toast/UI copy | Fixed status/error strings; the Suggest hint shows the candidate text | None | UI-thread owned; the displayed text is zeroized when the toast hides | Windows UI evidence open |

## Deny policy

- Native password controls, UIA password controls, fixed/custom blacklisted processes,
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
