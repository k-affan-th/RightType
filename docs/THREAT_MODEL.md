# RightType v1 Threat Model

> Scope: Windows v1, exact Thai Kedmanee ↔ US English QWERTY. This document records
> implemented controls and residual risks; it is not a claim that Windows E2E has passed.

## Trust boundaries

1. The low-level keyboard hook receives global key events inside the RightType process.
2. Focus/UI Automation and foreground-process queries decide whether the context is safe.
3. `SendInput` crosses from RightType into the focused application.
4. Selection conversion temporarily crosses the Windows clipboard boundary for `Ctrl+C`.
5. Config and opt-in learned words cross the disk boundary under `%APPDATA%\RightType`.

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
| Stats | Two process-local counters | None | reset on process exit | none content-related |
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

## Known residual risks

| ID | Risk | v1 handling | Release requirement |
| --- | --- | --- | --- |
| TM-001 | A seed phrase typed on the wrong layout is not recognizable from its raw first words; an online boundary converter cannot know future words. | Sensitive apps/password fields hard deny; candidate learning is denied. The four-word stream guard is not retrospective. | README/spec must not claim every wrong-layout BIP39 phrase is untouched; add Windows adversarial test. |
| TM-002 | UIA/process identity can be temporarily unavailable. | Tri-state/failure paths deny by default. | Native/browser/Electron and process-query E2E. |
| TM-003 | `VirtualLock` and WER hardening are best effort. | Buffer zeroization remains primary; process runs non-elevated. | Record runtime return values or diagnostic evidence without logging content. |
| TM-004 | Clipboard owners can be slow or locked and focus can change during manual copy. | Bounded worker, one-second command expiry, sequence/context checks, clipboard restore before injection. | Notepad/Word/Chrome, locked clipboard and focus-race E2E. |
| TM-005 | A hook can be evicted without a resume/unlock event. | Resume/unlock reinstall plus delayed retry currently exists. | Add independent liveness signal or document event-driven limitation; transition E2E. |

## No-network/dependency claim

The crate manifest has no HTTP, telemetry, updater or analytics dependency. This is a
source-level observation only until `cargo audit` and dependency-tree inspection are
recorded in `DYNAMIC_PLAN.md`.
