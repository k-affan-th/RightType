# Plan: Rebuild RightLang as "RightType" — Rust / Windows

> **Execution status:** แผนนี้เก็บ vision และ architecture ระยะยาว ส่วนสถานะงาน,
> decision gates, verification evidence และ next action ปัจจุบันอยู่ที่
> [DYNAMIC_PLAN.md](DYNAMIC_PLAN.md) ซึ่งเป็น source of truth สำหรับการลงมือทำ

> **Name:** project **RightType**, Cargo package/crate `righttype`.

## Context

**What RightLang was.** [RightLang](https://www.rightlang.com/) (a free tool by a Chulalongkorn University CompEng student, now abandoned ~2012/2022) is a Thai/English *keyboard-layout-mismatch* corrector. With TH (Kedmanee) + EN (QWERTY) installed, forgetting to switch produces gibberish — `correct` → `แนพพำแะ`, `twitter` → `ะไระะำพ`, `www` → `ไไไ`. It fixes this without retyping, via **automatic** and **manual** modes. Hotkeys: `Ctrl+CapsLock` (toggle mode), `Shift+Backspace` (fix last word), `Shift+CapsLock` (fix selection). It also supported Pattachote/Dvorak/UK-AU-CA layouts, wrong-language numbers, and auto-learning a user dictionary.

**Why rebuild (the user's real bugs).** RightLang is unmaintained and its design *causes* two bugs the user hit:
1. **Dies after waking from sleep** — only fix is kill + reopen.
2. **Sometimes won't switch; manual switch emits `ggg…`** (repeated garbage, not the real word) — no known fix.

Plus the documented Word/Windows-layout bug class (dropped/reordered chars during correction).

**Goal (confirmed with user).** A from-scratch **Windows-only**, **open-source**, **portable single-exe** Rust reimplementation that fixes both bugs by design and injects corrections as **raw Unicode**. **v1 ships Thai Kedmanee ↔ US English QWERTY only**; full RightLang layout parity (Pattachote/Dvorak/UK-AU-CA) is a v1.x track. The engine remains N-layout-oriented so future layouts do not require replacing the core conversion model.

**Confirmed feature scope:**
- Core reliability fixes for both user bugs (below).
- Unicode atomic injection (fixes the Word/dropped-char class).
- **Undo last correction** (hotkey).
- **Smarter detection + auto-learn** user dictionary.
- **Per-app profiles & exclusions**.
- **Correction toast + tray stats**.
- **Password-field safety** + zero keystroke logging to disk, no telemetry.
- **N-layout-ready engine** (TH/EN content only in v1).
- **Suggest mode** (non-destructive hint, accept with a key).
- **Convert-on-demand** (convert any selected/copied text via hotkey).
- **Privacy/security hardening for a Bitcoiner threat model** (see dedicated section) — secret-shaped bail-out, RAM hardening, no-persistence, no-network, verifiable build. This is a first-class requirement, not an add-on.

**Prerequisite:** Rust **MSVC** toolchain (`rustup default stable-x86_64-pc-windows-msvc`) + VS Build Tools "Desktop C++" (required by the `windows` crate).

---

## How each bug is fixed (the heart of the rebuild)

**Bug 1 — hook dies after sleep/lock.** `WH_KEYBOARD_LL` is torn down by Windows on callback timeout (`LowLevelHooksTimeout`) and on session transitions (sleep/resume, lock/unlock, UAC secure desktop). Original never recovers.
→ **`session.rs`**: register `WM_POWERBROADCAST` (PBT_APMRESUMEAUTOMATIC) + WTS session-change notifications (`WTSRegisterSessionNotification`) and auto-reinstall the hook. The keyboard callback keeps only bounded in-memory classification and the synchronous atomic correction needed to preserve input order; clipboard waits, UI dispatch and disk I/O stay outside the hook. Hook liveness and latency remain release gates in the dynamic plan.

**Bug 2 — `ggg…` garbage on manual switch.** Synthetic injection collides with a physically-held key (hotkey pressed while a letter/modifier is still down) → OS sees auto-repeat / inherits a stuck modifier. Original replays virtual keys, which is exactly what repeats.
→ **`inject.rs`**: before any correction, **release all currently-held keys/modifiers** (send key-ups, clear modifier state); inject the corrected text as a **single atomic Unicode `SendInput` batch** (`KEYEVENTF_UNICODE`), never VK replay; set an `INJECTING` re-entrancy guard and ignore `LLKHF_INJECTED` events so we never reprocess our own input. An **Undo** hotkey reverts instantly if anything still looks wrong.

**Word/Windows-layout class.** Unicode codepoint injection means Word/Chrome/etc. receive `WM_CHAR` directly — **no virtual-key replay**, and Thai combining-char (vowel/tone) order is preserved. Selection fixes use Copy only to read selected text, restore the allowed plain clipboard, then inject Unicode directly. Word/Chrome reliability remains a Windows E2E gate rather than an implementation claim.

---

## Security & Privacy (Bitcoiner threat model) — core requirement

**Threat:** a global keyboard hook is architecturally a keylogger; it sees seed words (BIP39), private keys (hex/WIF/base58), addresses, and passwords. The risk is any of these (a) being analyzed/held, (b) escaping RAM (pagefile/hibernation/crash dump/another process), or (c) being persisted. Defenses below are mandatory.

**1. Secret-shaped bail-out (aggressive) — `secret.rs`.** Before the current token is ever analyzed/corrected/learned, classify its *shape* (never its value). Bail (stop, wipe buffer, do nothing) on:
- hex runs, base58/WIF, bech32 addresses (`bc1…`), high-entropy mixed case+digit+symbol, tokens longer than any real word, **and**
- **consecutive BIP39 words** — bundle the public 2,048-word BIP39 list (`assets/bip39.txt`) solely for a stream guard (≥N exact consecutive raw/candidate matches ⇒ bail from that point). Individual words cannot be denied safely because ordinary English overlaps the list; see `THREAT_MODEL.md` TM-001.

**2. Minimize exposure — `buffer.rs`.** Hold only the current word, hard length cap; **zeroize** on every word boundary (`zeroize`/`SecureString`). Undo keeps ≤1 entry, zeroized after use/short timeout.

**3. RAM hardening (full).** `VirtualLock` buffer pages (keep secrets out of the pagefile/hiberfil), **disable crash dumps** for the process (no minidump can capture the buffer), run **non-elevated** (the LL hook needs no admin — least privilege).

**4. No persistence of typed content.** Nothing typed is ever written to disk — only in-memory stat counters. **Auto-learn = whitelist-only + blacklist-wins + token guards, off by default:** learns only in explicitly-allowed apps; never in wallets/password-managers/terminals/password fields; never learns secret-shaped or long/uncommon tokens. A manual "pause learning" toggle (no time-window control).

**5. Default sensitive-context guards — `safety.rs`.** Disable in password/concealed (`ES_PASSWORD`) fields + a default blacklist (wallets: Electrum/Sparrow/desktop wallets; password managers; terminals). Global **pause/panic hotkey** + tray pause.

**6. Verifiable, network-free binary.** **Zero network code** — no HTTP/telemetry/auto-update crates anywhere in the dependency tree (verifiable by inspecting `Cargo.lock`). Open-source, **reproducible build**, **code-signed release + SHA-256 checksums**, minimal deps, `cargo-audit` + `cargo-vet` in CI.

---

## Tech stack
- **`windows`** (windows-rs) — `SetWindowsHookExW`/`WH_KEYBOARD_LL`, `SendInput`, `GetForegroundWindow`, `GetGUIThreadInfo`/`GetWindowThreadProcessId`, clipboard, power/WTS notifications.
- **`native-windows-gui` + `native-windows-derive`** — tray, menus, settings dialog, toast window, all on one Win32 message loop (a LL hook *requires* a message pump on its thread; nwg gives one cohesive loop with no ownership conflict).
- **`serde` + `toml`** — `%APPDATA%\RightType\config.toml` + user dictionary.
- **`crossbeam-channel`** — hook thread → worker thread queue (keeps callback instant).
- **`once_cell` / `std::sync`** — global state for the C callback.
- **`zeroize`** — wipe keystroke buffers/undo entries; plus `VirtualLock` + dump-disable via `windows`.
- Bundled wordlists via `include_str!` (incl. `bip39.txt` for *avoidance*); optional `fst`/bloom set for fast membership + n-gram scoring.
- **No network crate of any kind** (verifiable trust requirement).
- **`KeyboardBackend` trait** (`install`/`uninstall`/`inject`) abstracts OS integration; Windows impl behind `#[cfg(windows)]`. Pure-logic modules (`layout`, `detect`, `dict`, `secret`, `buffer`, `config`) are OS-free so a future macOS backend is additive, not a rewrite.

---

## Project layout
```
RightType/        # repo root
  Cargo.toml   README.md   LICENSE-MIT   LICENSE-APACHE
  src/
    main.rs          # nwg app, message loop, wire hook+worker+tray+session
    config.rs        # TOML, hotkeys, mode, per-app profiles, run-at-startup (HKCU Run)
    session.rs       # power/WTS notifications + one delayed retry -> reinstall hook
    hook.rs          # WH_KEYBOARD_LL: enqueue only, fast; injecting guard; skip LLKHF_INJECTED
    buffer.rs        # current-word buffer (zeroized on boundary), word boundaries, focus tracking
    secret.rs        # secret-shaped bail-out: hex/base58/WIF/bech32/entropy/long/BIP39-run
    layout/
      mod.rs         # Layout trait + registry + generic convert(from,to,&str)  (N-layout engine)
      kedmanee.rs    # TH Kedmanee table (normal+shift)
      qwerty.rs      # EN QWERTY table (normal+shift)
    dict.rs          # wordlists, membership, n-gram score; auto-learn (whitelist+guards, off by default)
    detect.rs        # generic: pick best target layout by validity+score; confidence; suggest vs auto
    inject.rs        # atomic Unicode SendInput; release-held-mods; undo stack       (Bug 2)
    manual.rs        # hotkeys: fix-word, fix-selection, toggle mode, undo, convert-on-demand, pause/panic
    safety.rs        # password/concealed-field + blacklist (wallets/pw-mgrs/terminals); RAM hardening; no-disk guarantee
    ui/
      tray.rs        # tray menu (Enable/Pause, Auto/Manual/Suggest, Settings, Stats, Quit)
      settings.rs    # nwg dialog: hotkeys, profiles/white-blacklist, startup, dictionary, language
      toast.rs       # correction toast w/ one-key undo
  assets/  en_words.txt  th_words.txt  bip39.txt  icon.ico
  tests/   layout_tests.rs  detect_tests.rs  safety_tests.rs  secret_tests.rs
```

### Key module notes
- **`layout/mod.rs`** — `LayoutId`, a registry, and pure `convert(from, to, &str)` keyed by physical key position so it's robust; round-trip-testable with no Win32. v1 registers only Kedmanee + QWERTY.
- **`detect.rs`** — given a completed token and the supported layout pair, produce evidence for an exact dictionary or full-segmentation candidate. Auto commits only at a boundary in v1; Suggest remains non-destructive and is tracked in the dynamic plan.
- **`safety.rs`** — before buffering/correcting, check the focused control: concealed/`ES_PASSWORD` field or excluded app ⇒ disable. **Never writes keystrokes to disk**; only counts (for stats) live in memory/config.
- **`manual.rs`** — v1 uses `Shift+Backspace` (fix buffered word), `Shift+CapsLock` (read selection through a plain-Unicode clipboard transaction, then inject Unicode), `Ctrl+CapsLock` (Manual/Auto/Suggest), and one-shot Undo. Configurable hotkeys and non-CapsLock alternatives are v1.x work.

---

## Build order (each step independently verifiable)
1. ✅ `cargo init`, Cargo.toml, README, dual license; commit skeleton.
2. ✅ `layout/` + tests — round-trip `correct`↔`แนพพำแะ`, `สวัสดี`↔`l;ylfu`, `www`↔`ไไไ`, shifted glyphs. *No OS.*
3. ✅ `dict.rs` + wordlists + `detect.rs` + tests — gibberish detected, real words untouched.
4. ✅ `secret.rs` + tests — bail-out classifier (hex/base58/WIF/bech32/entropy/long/BIP39-run). *No OS.*
5. ◑ `hook.rs` + `buffer.rs` — capture words and zeroize on boundary. Callback latency, disk-I/O removal and Windows capture evidence remain open in the dynamic plan.
6. ✅ `inject.rs` — release-held-mods + atomic Unicode `SendInput` batch; auto-mode wired (hook→detect→inject), boundary key re-emitted. *(Bug 2 fix. Undo stack + manual hotkeys still pending — step 9. Manual end-to-end test pending.)*
7. ◑ `session.rs` — power/WTS reinstall + one delayed retry. *(Transition E2E and independent liveness detection remain open.)*
8. ✅ `safety.rs` — password-field (ES_PASSWORD, + UIA for browsers via `focus.rs`) + app blacklist (wallets / password managers / terminals) gating. `ram.rs` — VirtualLock on the word buffer's stable allocation, WerSetFlags(NOHEAP) + SetErrorMode to keep the buffer out of crash dumps; non-elevated confirmed (no manifest).
9. ◑ Manual hotkeys are wired for word, selection, Auto/Manual, Undo and panic. Selection is plain-Unicode-clipboard-only and Auto is boundary-only in v1; remaining transaction/E2E evidence is tracked in the dynamic plan.
10. ✅ Tray app (windowless) — enable/disable, Auto/Manual, Learn, Start-with-Windows, Blocked apps... (settings), Stats..., Quit; status toast; config persistence (`%APPDATA%`); run-at-startup (HKCU Run); portable `--release` build **2.6 MB**.
11. ◑ Polish + trust: README updated; UIA browser password detection; opt-in auto-learn. *(Thai UI, RAM hardening (VirtualLock/dump-disable), `cargo-audit`/CI, reproducible build + signing + checksums still TODO.)*

**Status: implementation in progress, not release-certified.** The core and Windows
shell build, but privacy/manual/hook resilience, Windows E2E, clean-release and audit
gates remain blockers. Current status and evidence live in `DYNAMIC_PLAN.md`.

---

## Verification

**Automated (no OS hooks):** `cargo test` (`layout` round-trips incl. shifted cases; `detect` true-positives + no false-positives; `secret` keys/seed phrases dropped & normal words pass; `safety` field/blacklist gating), `cargo clippy -- -D warnings`, `cargo build --release`, `cargo-audit`. Inspect `Cargo.lock` to confirm **no network crates**.

**Manual end-to-end — the actual bug targets:**
- Type `l;ylfu` → `สวัสดี`; type Thai-layout `แนพพำแะ` → `correct`. Test in **Notepad, Microsoft Word, Chrome**.
- **Bug 1 regression:** sleep the PC, resume, immediately type — correction must work with **no kill/restart**. Repeat with lock/unlock and a UAC prompt.
- **Bug 2 regression:** trigger manual fixes rapidly while keys are held / at ~150 WPM — confirm **no `ggg…` / no repeated or reordered chars**; Undo reverts cleanly.
- **Word-specific:** rapid Thai words with tone marks — no dropped/reordered characters, no layout confusion.
- Hotkeys: `Shift+Backspace`, `Shift+CapsLock` (selection only, clipboard restored), `Ctrl+CapsLock` cycles Auto/Manual/Suggest **without** toggling the CapsLock light; Convert-on-demand on arbitrary copied text.
- **Safety:** focus a password field — no buffering/correction, nothing on disk. Blacklisted app (wallet/terminal) — disabled. Type a hex key / WIF / `bc1…` address in a normal field — tool must bail immediately. Test raw and wrong-layout BIP39 streams against the threshold/non-retrospective contract in `THREAT_MODEL.md`. Confirm `VirtualLock`/dump-disable active and process is non-elevated.
- **Performance:** `criterion` micro-bench for `convert`/`detect` (assert sub-ms); measure hook-callback time; working set against the ~5–15 MB target; ~0% idle CPU.

**Out of scope (v1):** macOS; non-TH/EN layouts (engine supports them, content not bundled); cloud/sync.

## Performance & resource budget
Event-driven native Rust (no GC / Electron / async runtime); sleeps at ~0% CPU until a key arrives.

| Metric | Target | How |
|---|---|---|
| Ordinary-key hook callback latency | < ~100 µs (never near ~300 ms `LowLevelHooksTimeout`) | bounded in-memory buffer/context checks; no waits or disk I/O |
| Typing throughput | ≥ 200 WPM, no perceptible lag | per-key work stays bounded; Windows stress evidence remains required |
| Per-word boundary policy | sub-ms | synchronous hash-set lookup/DP segmentation at a boundary; measured by `examples/policy_latency.rs` |
| Idle CPU | ~0% | event-driven; the 1.5 s retry timer performs only an atomic flag check |
| RAM working set | ~5–15 MB | `fst`-compressed wordlists (~1–3 MB); one worker thread; tiny zeroized buffers |
| Binary | ~2–5 MB single portable exe | static MSVC build; release profile LTO + `codegen-units=1` + `panic="abort"` + `strip=true` + size-opt |
| Hot-path allocations | zero | reused buffers in the callback |

## Future: macOS port
Pure-logic modules reused as-is; only the `KeyboardBackend` + OS layer is reimplemented:
- **Capture:** `CGEventTap` (Quartz) — Accessibility + Input-Monitoring permission; re-enable on `kCGEventTapDisabledByTimeout` (same watchdog pattern as Bug 1).
- **Inject:** `CGEventKeyboardSetUnicodeString` — same Unicode-not-layout strategy → same Word/layout bug fix.
- **RAM hardening:** `mlock` + disable core dumps (mirrors VirtualLock/dump-disable).
- **Secret/password safety:** `EnableSecureEventInput` makes macOS block taps in secure fields automatically.
- **UI:** native status-bar item (`tao`/`muda`).
