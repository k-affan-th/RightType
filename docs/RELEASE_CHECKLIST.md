# RightType v1 Release Checklist

> A release is complete only when every required row below has evidence in
> `DYNAMIC_PLAN.md`. A successful local build from a dirty tree is a test artifact,
> not a release.

## 1. Preconditions

- [ ] Product decisions D-001–D-005 remain accepted and unchanged.
- [ ] Working tree is clean at an intentional commit; unrelated user files are excluded.
- [ ] Release version and signed Git tag are chosen by the product owner.
- [ ] Windows test machine has exact Thai Kedmanee and US English QWERTY installed.
- [ ] Code-signing certificate/thumbprint and RFC 3161 timestamp service are available.

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

- [ ] Record command output, Rust version, target triple and Windows build.
- [ ] Confirm `cargo tree --features winos --edges normal` has no unintended network/telemetry crate.
- [ ] Confirm release build has no warning and the latency gate remains below 1 ms/token.

## 3. Windows E2E gates

- [x] Auto boundary correction in both directions with exact supported HKLs. (matrix 9/9: then_basic_boundary, enth_live_full)
- [ ] Manual current/last word, selection conversion and one-shot Undo.
- [ ] Manual clipboard: empty, Unicode-only, locked and rejected rich/app-specific formats.
- [ ] Manual focus race and stale command; no cross-control injection.
- [x] Manual/Auto/Suggest mode cycle, Suggest reject/accept/context invalidation. (matrix: suggest_no_touch, suggest_accept)
- [~] Word 3/3 and Chrome 5/5 and Edge 5/5 on 2026-08-31 (`word_roundtrip.py`, `d008_revision.py <browser>`). Notepad is WinUI and stays manual-only per the harness note.
- [~] Native and browser password fields plus a blacklisted terminal pass (matrix: guard_password_field, guard_blacklisted_terminal). Electron untested.
- [~] Fast typing passes (matrix: fast_typing_live). Held modifiers, key repeat, Thai combining marks and partial-failure seams still untested.
- [x] Sleep/resume, lock/unlock and UAC secure-desktop transitions without process restart. (recorded in DYNAMIC_PLAN, verified interactively)
- [~] Process runs at Medium Integrity: launched from a Medium shell with no manifest and no elevation prompt, confirmed 2026-08-31. WER/VirtualLock are called at startup but have no runtime probe yet.

## 4. Artifact, checksum and signature

```powershell
$artifact = Resolve-Path target\release\righttype.exe
Get-FileHash -Algorithm SHA256 -LiteralPath $artifact
signtool sign /fd SHA256 /td SHA256 /tr <RFC3161-URL> /sha1 <CERT-THUMBPRINT> $artifact
signtool verify /pa /all $artifact
Get-AuthenticodeSignature -LiteralPath $artifact
```

- [ ] Sign the exact clean-checkout artifact; signing changes its checksum, so compute the published SHA-256 **after** signing.
- [ ] Publish binary, checksum, license files, changelog and known limitations together.
- [ ] Re-download the published files and verify signature/checksum independently.

## 5. Current local test artifact (not releasable)

| Field | Value |
| --- | --- |
| Date | 2026-08-04 |
| Path | `target/release/righttype.exe` |
| Size | 2,719,744 bytes |
| Pre-sign SHA-256 | `4BC8AF40DAE81C75B4797DED823837E1E23E1466C497A9F78F28B977FAEECB31` |
| Status | Dirty-working-tree test artifact; unsigned; incomplete Windows matrix |

This checksum is evidence for this one local build only. Rebuilds or signing invalidate it.
