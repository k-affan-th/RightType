# macOS contribution plan

Started 2026-10-27 against upstream `ea4bce4` (RightType 2.6.2).
Working fork: <https://github.com/KitCJX/RightType>.

## Goal and contribution strategy

Add a native macOS application to RightType, reusing its Rust conversion and
correction policy. Ship reviewable milestones rather than copying the Windows
application or rewriting the language engine. Upstream maintainer acceptance
of the architecture and Mac maintenance is **not established** yet.

The first contribution is a Fix Text preview. It is useful on its own, exercises
the real core from a native app, and requires no system-wide keyboard access.
Later input capture and replacement must have their own evidence before they
can be enabled. No date or full Windows-feature parity is promised.

## Milestones and exit criteria

### M0 — Contribution baseline [complete]

- Fork the original, keep an `upstream` remote, work on `codex/macos-foundation`.
- Record the upstream commit and current behavior; run existing core tests on Mac.
- Keep the existing Windows entry point, default command, release and tests working.

### M1 — Native Fix Text preview [complete locally; CI pending]

- Native AppKit menu bar item, editable original and read-only preview.
- Preview explicitly supplied text through the existing `repair::repair` engine.
- Clipboard access only through Paste Text / Copy Result or normal edit commands.
- No keyboard capture, app inspection, automatic replacement, learning or network.
- Bounded UTF-8 pipe to a bundled Rust text tool; no typed text in arguments,
  temporary files or error messages. Reject oversized/invalid input before output.
- Invalidate results after edits and clear window contents/undo history on close.
- Build a local `.app`, verify its bundle/signature, test the actual Swift-to-Rust
  path and exercise the window. Add a Mac build check to CI.
- Exit: automated checks and local UI evidence recorded below. Local ad hoc
  signing is a development identity, not notarization or public release readiness.

### M2 — Explicit correction of selected text [planned]

- Agree the Mac integration approach with the upstream maintainer before merging.
- Choose Mac shortcuts after checking Caps Lock/input-source and Command conflicts.
- Permission onboarding and a visible status for missing/revoked permissions.
- Accessibility-based selection read/replacement only in a supported editable
  field; no clipboard fallback in the initial version.
- Deny password/protected fields, sensitive apps, unsupported roles/layouts, and
  unknown or changed focus. Recheck target identity and selected text before a write.
- Bounded undo in the same unchanged context; clear sensitive state on transition.
- Exit: TextEdit and browser/Electron fixtures verify selection, Thai marks,
  emoji/UTF-16 ranges, unsupported/read-only fields, password denial, focus races,
  permission denial and undo. A failure must leave the target untouched.

### M3 — Last-word correction [planned]

- Event tap capture with a short callback, own-event tagging and recovery after
  timeout, lock, sleep or permission changes. Unknown context means no correction.
- Map exact Mac input-source IDs and physical keys; do not reuse Windows key IDs.
- Preserve the current-word cap, secret/seed stream guards and memory lifetimes.
- Reconcile actual text before deletion, especially Thai combining marks.
- Exit: fast typing, repeat/modifier release, mouse/caret movement, focus/layout
  switches, mixed scripts and injected input tested in the supported apps.

### M4 — Suggest and Auto [planned]

- Reuse the production policy and share platform-independent state where useful;
  do not use the simulator as an OS event backend or duplicate the Windows hook.
- Non-destructive Suggest first; Auto stays opt-in until real-app evidence passes.
- Preserve undo, pause, layout switching, secret guards and no-telemetry behavior.
- Exit: shared false-positive/recall and latency gates pass, followed by actual
  Mac typing tests. Unit tests alone do not establish safe automatic replacement.

### M5 — Release and maintenance [planned]

- Confirm supported macOS releases, Apple silicon/Intel scope and supported layouts.
- Add native permission help, Thai/English UI, login behavior and accessibility QA.
- Developer ID signing/notarization and first-launch testing on a clean Mac.
- Decide who maintains Mac CI, fixes and release credentials with upstream.
- Exit: install/uninstall, denied/revoked permissions, update, sleep/wake, real
  typing and app compatibility matrix documented. No global rollout before this.

## Initial scope and implementation choice

The native UI lives in `macos/`; a feature-gated `righttype-text` binary calls the
existing Rust library. Swift uses bounded UTF-8 stdin/stdout with a bundled helper
so this milestone needs neither FFI unsafe code nor new runtime dependencies.
`default-run = "righttype"` preserves existing `cargo run` behavior. The helper
also runs on Linux for trust-boundary tests. No change to shared detection policy.

M1 uses the upstream US QWERTY/Thai Kedmanee tables for finished text. Actual Mac
keyboard parity, other layouts, global shortcuts, text injection, snippets,
training tools and Windows feature parity are deferred to their own milestones.
The preview has a 32 KiB passage limit and a 10-second worker timeout.

## Privacy limits of M1

The app processes only explicitly supplied text. The Rust bridge wipes its owned
input/output and change strings before normal exit, but AppKit/Swift strings,
OS pipe buffers and the shared engine can make transient copies. Do not claim
complete memory erasure or protection against swap/crash capture. Closing the
window clears the visible text and undo history; an explicit Copy Result remains
on the clipboard. Existing token guards are inherited, not a guarantee that all
secrets are recognized. Global password-field safety belongs to M2/M3.

## Evidence ledger

| Date | Check | Result and limits |
| --- | --- | --- |
| 2026-10-27 | Original `cargo test --locked --all-targets` on arm64 macOS | 303 tests passed; core only, no Mac backend existed. |
| 2026-10-27 | `cargo test --locked --offline --features text-tool --all-targets` | 307 tests passed, including four new input/output-boundary tests. No Windows integration was run locally. |
| 2026-10-27 | Strict Clippy, Rust format, whitespace, shell syntax and workflow YAML checks | Passed. Three existing keyboard mapping tests also passed after the equivalent boolean simplification. |
| 2026-10-27 | Local native build, bundled `--self-test`, plist lint and deep signature verification | Passed on Apple silicon macOS 27.0.1 with Swift 6.4 / Rust 1.95. These do not prove Intel, macOS 13, public notarization or clean-machine launch. |
| 2026-10-27 | Native UI automation and screenshot inspection | `l;ylfu` / `แนพพำแะ` became `สวัสดี` / `correct`; tabs, newlines, correct English/Thai and emoji remained. Editing cleared the result and disabled Copy Result; unchanged English, Clear and empty-input handling passed. No Copy Result click was performed, to preserve the user's clipboard; VoiceOver and physical typing are untested. |

The current Rust toolchain also flags an upstream boolean expression in
`keyboard::index_of`; its equivalent `scan != 0x36` form is a one-line prerequisite
for the existing strict Clippy gate. No keyboard mapping or detection policy changed.

Update this plan in each Mac contribution. Mark a milestone complete only when
its exit criteria have evidence; record untested behavior explicitly.
