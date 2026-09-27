# RightType Release Checklist

> A release is complete only when every required row below has evidence in
> `DYNAMIC_PLAN.md`. A successful local build from a dirty tree is a test artifact,
> not a release.

## 1. Preconditions

- [ ] Product decisions D-001–D-005 remain accepted and unchanged.
- [ ] Working tree is clean at an intentional commit; unrelated user files are excluded.
- [ ] Release version and signed Git tag are chosen by the product owner.
- [ ] Windows test machine has exact Thai Kedmanee and US English QWERTY installed.
- [ ] Code-signing certificate/thumbprint and RFC 3161 timestamp service are available. **2.0: deferred by the product owner (2026-09-26);** releases stay unsigned with `SHA256.txt`, and the release notes say how to get past SmartScreen.

## 2. Clean automated gates

Run from a fresh checkout of the release commit:

```powershell
cargo test --features winos --all-targets
cargo clippy --features winos --all-targets -- -D warnings
cargo fmt --all -- --check
cargo build --release --features winos
cargo audit
cargo run --release --example policy_latency
git diff --check
```

CI (`.github/workflows/rust.yml`) runs all of these except the signed build on
every push/PR to `main` (Linux core + `windows-latest` with `--features winos` +
`cargo audit`); a green run on the release commit is the record for this section.

- [ ] Record command output, Rust version, target triple and Windows build.
- [ ] Confirm `cargo tree --features winos --edges normal` has no unintended network/telemetry crate.
- [ ] Confirm release build has no warning and the latency gate remains below 1 ms/token.

## 3. Windows E2E gates

- [x] Auto boundary correction in both directions with exact supported HKLs. (matrix 9/9: then_basic_boundary, enth_live_full)
- [x] Manual current/last word, selection conversion and one-shot Undo. (`word_roundtrip.py` 4/4: `dy[` selected and converted to the Thai word in place; Undo covered there and in the matrix)
- [~] Empty and Unicode-only clipboards are both snapshotted and restored exactly (word_roundtrip.py 7/7), and one carrying private .NET formats is refused with the documented message rather than touched. A LOCKED clipboard is now retried for ~100 ms and then refused with a distinct "clipboard is busy" message (1.1.0); `release_gaps.py clipboard` holds it from a helper process with its own window and first proves from a third process that it really is held, so the case can no longer pass vacuously. **Pending: a Windows run of that script.**
- [x] Manual focus race and stale command; no cross-control injection. (`manual_selection.py`: a focus change mid-conversion aborts it and the page's password field stays empty)
- [x] Manual/Auto/Suggest mode cycle, Suggest reject/accept/context invalidation. (matrix: suggest_no_touch, suggest_accept)
- [~] Word 3/3 and Chrome 5/5 and Edge 5/5 on 2026-08-31 (`word_roundtrip.py`, `d008_revision.py <browser>`). Notepad is WinUI and stays manual-only per the harness note. Keys typed faster than the layout switch after a mid-word anchor (D-009 pending layout) and Undo right after such an anchor are covered by `release_gaps.py chrome word`; **pending a Windows run**.
- [~] Native and browser password fields plus a blacklisted terminal pass (matrix: guard_password_field, guard_blacklisted_terminal; manual_selection.py also shows nothing injected into a password input during a focus race). ELECTRON IS NOT COVERED: it is detected through the same UI Automation IsPassword path in focus.rs that the Chromium tests already exercise, so the risk is that a specific Electron app behaves differently, not that the mechanism is unproven. Treat as residual risk, not coverage. `release_gaps.py electron` now runs the same password-field check inside a real Electron window (needs Node.js); **pending a Windows run**.
- [~] Fast typing (matrix: fast_typing_live), Thai combining marks (d008_revision.py 8/8 on Edge and Chrome) and a held modifier during a correction (word_held_shift_no_garbage: exactly one conversion, no repeat) all pass. The partial-failure seam is covered on both correction paths: a failed injection adds no stats, no Undo record and no layout switch (failed_auto_injection_has_no_success_side_effects), and while a run is owned it also releases ownership rather than diffing against a screen state that was never reached (failed_reconcile_injection_releases_ownership, seen red without the fix). true key repeat comes from the keyboard driver and SendInput cannot reproduce it.
- [~] Lock/unlock (E-031) and UAC secure-desktop (E-030) transitions pass without a process restart. **Sleep/resume has no recorded run** (DYNAMIC_PLAN S5 lists it as BLOCKED-interactive); run `transition_probe.py` and sleep the machine during its window. Since 1.1.0 a liveness check also reinstalls a hook that falls silent while input continues, without any power/session event (TM-005; `session::tests`).
- [x] Process runs at Medium Integrity (launched from a Medium shell, no manifest, no elevation prompt) and the WER/VirtualLock controls are active in a running process, not merely called in source: ram_probe.py reads back SetErrorMode applied, WerSetFlags(NOHEAP) -> true and VirtualLock(260 bytes) -> true, 260 being exactly the WordBuffer stable region.
- [ ] Seed phrase on the Thai layout: the fourth word is never offered, `cat`/`ski` do not break the run, an ordinary word releases the guard (`release_gaps.py chrome`, seed cases; logic proven for all 2,048 words in `secret::tests`).

## 3b. 2.0 on a real Windows desktop

Unit tests, the simulator (`sim.rs`), the quality gate and Wine renders cover
the logic and the look of every 2.0 feature. What only a real desktop shows is
listed here; tick each with the Windows build and app version in
`DYNAMIC_PLAN.md`. `release_gaps.py chrome` already scripts `flip` and `tab`.

- [ ] **Pause** 10 min from the tray: the tooltip counts down and RightType comes back on by itself; quitting while paused starts enabled next time.
- [ ] **A mode per app**: Manual in one editor, Auto in a browser; switching windows switches behaviour; the safety list still wins (a terminal set to Auto stays off).
- [ ] **Flip back several words** (`release_gaps.py chrome`, `flip`), and the same in an Electron app (VS Code or Discord).
- [ ] **Caret tag and Suggest hint** appear under the text cursor in Notepad, Word, Chrome and one app without a system caret (VS Code or Windows Terminal via UI Automation); on a second monitor with another scale they land on that monitor.
- [ ] **Tab** takes a hint (`release_gaps.py chrome`, `tab`); Tab in a password field or after switching window is an ordinary Tab.
- [ ] **Fix text** window: Paste and fix, Copy; the clipboard is left as it was when the window is closed without copying.
- [ ] **Per-field prediction** (opt-in): after 20 words in a chat box, focusing it switches to its usual language; a password field never switches.
- [ ] **Command palette** `Ctrl`+`Alt`+`Space`: opens next to the caret (and above it at the bottom of the screen), Esc returns focus to the same field, each command works.
- [ ] **Custom hotkeys**: change Flip to `F8`, swap two hotkeys, restart RightType, both still work; `Shift`+`A` is refused.
- [ ] **Last 7 days** (opt-in): numbers survive a restart; turning it off deletes `stats.toml`.
- [ ] **Sync folder**: two PCs (or two data dirs) on one OneDrive folder see each other's learned words within a few seconds.
- [ ] **Pattachote**: with Thai Pattachote installed and chosen in Settings, `สวัสดี` typed on English (`mogmuh`) comes back, and the layout switches to Pattachote, not Kedmanee.
- [ ] **UK English**: with English (United Kingdom) active, Thai typed by mistake turns into English with `"` and `@` in the UK places; English (Australia) works the same as US.
- [ ] **DPI**: drag Settings between a 100 % and a 150 % monitor; text and controls re-lay out sharply.
- [ ] **winget**: `winget install --manifest <unzipped winget-manifests-<version>>` installs silently per user, and `winget uninstall k-affan-th.RightType` removes it.

## 4. Artifact, checksum and signature

```powershell
$artifact = Resolve-Path target\release\righttype.exe
Get-FileHash -Algorithm SHA256 -LiteralPath $artifact
signtool sign /fd SHA256 /td SHA256 /tr <RFC3161-URL> /sha1 <CERT-THUMBPRINT> $artifact
signtool verify /pa /all $artifact
Get-AuthenticodeSignature -LiteralPath $artifact
```

- [ ] Sign the exact clean-checkout artifact; signing changes its checksum, so compute the published SHA-256 **after** signing. **This is the only remaining step and it needs the product owner's certificate.** Everything else in this section is done: `packaging/build_release.ps1` produces `RightType-<version>-setup.exe`, `RightType-<version>-x64.zip` (with licenses and changelog) and a `SHA256.txt` covering only that version's files, taking the version from `Cargo.toml`; the 1.0.0 installer was verified end to end on 2026-08-31 (silent install to a scratch directory, installed binary launched and stayed running, uninstaller removed the directory, an existing install elsewhere untouched). Re-run `build_release.ps1` after signing to regenerate the checksums.
- [ ] Publish binary, checksum, license files, changelog and known limitations together as a GitHub Release on the signed tag. Artifacts are not committed to the repository (`dist/` is git-ignored).
- [ ] Re-download the published files and verify signature/checksum independently.

## 5. Current local test artifact (not releasable)

None. Since 1.1.0 releases are built by `.github/workflows/release.yml` on a
clean runner (from 2.0.0 also with winget manifests); nothing built locally is
published. The 1.0.0 artifacts (2026-08-31, unsigned) remain in git
history at `69a54e9` under `dist/`; they predate D-009 and the fixes listed in
`CHANGELOG.md` and must not be republished.
