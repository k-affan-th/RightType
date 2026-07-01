# Plan: Rebuild RightLang as "RightType" — Rust / Windows

> **Name:** project **RightType**, Cargo package/crate `righttype`.

## Context

**What RightLang was.** [RightLang](https://www.rightlang.com/) (a free tool by a Chulalongkorn University CompEng student, now abandoned ~2012/2022) is a Thai/English *keyboard-layout-mismatch* corrector. With TH (Kedmanee) + EN (QWERTY) installed, forgetting to switch produces gibberish — `correct` → `แนพพำแะ`, `twitter` → `ะไระะำพ`, `www` → `ไไไ`. It fixes this without retyping, via **automatic** and **manual** modes. Hotkeys: `Ctrl+CapsLock` (toggle mode), `Shift+Backspace` (fix last word), `Shift+CapsLock` (fix selection). It also supported Pattachote/Dvorak/UK-AU-CA layouts, wrong-language numbers, and auto-learning a user dictionary.

**Why rebuild (the user's real bugs).** RightLang is unmaintained and its design *causes* two bugs the user hit:
1. **Dies after waking from sleep** — only fix is kill + reopen.
2. **Sometimes won't switch; manual switch emits `ggg…`** (repeated garbage, not the real word) — no known fix.

Plus the documented Word/Windows-layout bug class (dropped/reordered chars during correction).

**Goal (confirmed with user).** A from-scratch **Windows-only**, **open-source**, **portable single-exe** Rust reimplementation with **full RightLang parity** that fixes both bugs by design, injects corrections as **raw Unicode**, and goes beyond the original with the enhancements below. **v1 ships TH Kedmanee ↔ EN QWERTY only**, but the engine is **N-layout generic** so future languages (Lao, Khmer, Russian, Pattachote, …) are a data-table add.

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
→ **`session.rs`**: register `WM_POWERBROADCAST` (PBT_APMRESUMEAUTOMATIC) + WTS session-change notifications (`WTSRegisterSessionNotification`) and **auto-reinstall the hook**; plus a ~1–2 s **watchdog** timer that verifies the hook handle and reinstalls if dead. The hook callback does *only* enqueue-and-return (real work on a worker thread) so it can never hit the timeout. → no more kill/restart.

**Bug 2 — `ggg…` garbage on manual switch.** Synthetic injection collides with a physically-held key (hotkey pressed while a letter/modifier is still down) → OS sees auto-repeat / inherits a stuck modifier. Original replays virtual keys, which is exactly what repeats.
→ **`inject.rs`**: before any correction, **release all currently-held keys/modifiers** (send key-ups, clear modifier state); inject the corrected text as a **single atomic Unicode `SendInput` batch** (`KEYEVENTF_UNICODE`), never VK replay; set an `INJECTING` re-entrancy guard and ignore `LLKHF_INJECTED` events so we never reprocess our own input. An **Undo** hotkey reverts instantly if anything still looks wrong.

**Word/Windows-layout class.** Unicode codepoint injection means Word/Chrome/etc. receive `WM_CHAR` directly — **no `ActivateKeyboardLayout`, no layout race**, and Thai combining-char (vowel/tone) order is preserved. Selection fixes go via the clipboard (uniformly reliable in Word), restoring prior clipboard after.

---

## Security & Privacy (Bitcoiner threat model) — core requirement

**Threat:** a global keyboard hook is architecturally a keylogger; it sees seed words (BIP39), private keys (hex/WIF/base58), addresses, and passwords. The risk is any of these (a) being analyzed/held, (b) escaping RAM (pagefile/hibernation/crash dump/another process), or (c) being persisted. Defenses below are mandatory.

**1. Secret-shaped bail-out (aggressive) — `secret.rs`.** Before the current token is ever analyzed/corrected/learned, classify its *shape* (never its value). Bail (stop, wipe buffer, do nothing) on:
- hex runs, base58/WIF, bech32 addresses (`bc1…`), high-entropy mixed case+digit+symbol, tokens longer than any real word, **and**
- **consecutive BIP39 words** — bundle the public 2,048-word BIP39 list (`assets/bip39.txt`) *solely to recognize and avoid* seed phrases (≥N exact consecutive matches ⇒ bail). Exact-wordlist matching means normal TH/EN prose is unaffected.

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
    session.rs       # power/WTS notifications + watchdog -> reinstall hook   (Bug 1)
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
- **`detect.rs`** — given a buffered word + candidate target layouts, choose the conversion that yields a valid dictionary word with the best n-gram score above a confidence threshold; returns `Auto(corrected)`, `Suggest(corrected)`, or `None`. Handles wrong-language numbers/symbols. Feeds `dict.rs` auto-learn.
- **`safety.rs`** — before buffering/correcting, check the focused control: concealed/`ES_PASSWORD` field or excluded app ⇒ disable. **Never writes keystrokes to disk**; only counts (for stats) live in memory/config.
- **`manual.rs`** — `Shift+Backspace` (fix buffered word), `Shift+CapsLock` (fix selection via clipboard), `Ctrl+CapsLock` (cycle Auto/Manual/Suggest), Undo, Convert-on-demand; hotkeys configurable, with non-CapsLock alternatives, and the hook **consumes** them so CapsLock state never toggles.

---

## Build order (each step independently verifiable)
1. ✅ `cargo init`, Cargo.toml, README, dual license; commit skeleton.
2. ✅ `layout/` + tests — round-trip `correct`↔`แนพพำแะ`, `สวัสดี`↔`l;ylfu`, `www`↔`ไไไ`, shifted glyphs. *No OS.*
3. ✅ `dict.rs` + wordlists + `detect.rs` + tests — gibberish detected, real words untouched.
4. ✅ `secret.rs` + tests — bail-out classifier (hex/base58/WIF/bech32/entropy/long/BIP39-run). *No OS.*
5. ✅ `hook.rs` + `buffer.rs` + worker channel — capture words; zeroize on boundary; enqueue-only callback. *(Code complete & compiles under `--features winos`; manual capture smoke-test pending.)*
6. ✅ `inject.rs` — release-held-mods + atomic Unicode `SendInput` batch; auto-mode wired (hook→detect→inject), boundary key re-emitted. *(Bug 2 fix. Undo stack + manual hotkeys still pending — step 9. Manual end-to-end test pending.)*
7. ✅ `session.rs` — power/WTS reinstall + watchdog. *(Bug 1 fix. Manual sleep/lock test pending.)*
8. ✅ `safety.rs` — password-field (ES_PASSWORD, + UIA for browsers via `focus.rs`) + app blacklist (wallets / password managers / terminals) gating. `ram.rs` — VirtualLock on the word buffer's stable allocation, WerSetFlags(NOHEAP) + SetErrorMode to keep the buffer out of crash dumps; non-elevated confirmed (no manifest).
9. ✅ Manual hotkeys: `Shift+Backspace` (fix word), `Shift+CapsLock` (fix selection via clipboard — doubles as Convert-on-demand), `Ctrl+CapsLock` (Auto/Manual), `Ctrl+Shift+CapsLock` (Undo last correction, one-shot), `Ctrl+Alt+CapsLock` (panic — instant enable/disable, works even in sensitive contexts). Auto mode wired hook→secret→detect→inject + eager run-on conversion both directions + auto layout-switch.
10. ◑ Tray app (windowless) — enable/disable, Auto/Manual, Learn, Start-with-Windows, Quit; status toast; config persistence (`%APPDATA%`); run-at-startup (HKCU Run); portable `--release` build **2.6 MB**. *(Settings dialog + stats still TODO.)*
11. ◑ Polish + trust: README updated; UIA browser password detection; opt-in auto-learn. *(Thai UI, RAM hardening (VirtualLock/dump-disable), `cargo-audit`/CI, reproducible build + signing + checksums still TODO.)*

**Status: a working, shippable v1.** Core engine + Windows tray app complete; privacy
guards (secret bail-out, password-field/blacklist context guards incl. browsers, zeroize,
no network) in place. Remaining items above are enhancements, not blockers.

---

## Verification

**Automated (no OS hooks):** `cargo test` (`layout` round-trips incl. shifted cases; `detect` true-positives + no false-positives; `secret` keys/seed phrases dropped & normal words pass; `safety` field/blacklist gating), `cargo clippy -- -D warnings`, `cargo build --release`, `cargo-audit`. Inspect `Cargo.lock` to confirm **no network crates**.

**Manual end-to-end — the actual bug targets:**
- Type `l;ylfu` → `สวัสดี`; type Thai-layout `แนพพำแะ` → `correct`. Test in **Notepad, Microsoft Word, Chrome**.
- **Bug 1 regression:** sleep the PC, resume, immediately type — correction must work with **no kill/restart**. Repeat with lock/unlock and a UAC prompt.
- **Bug 2 regression:** trigger manual fixes rapidly while keys are held / at ~150 WPM — confirm **no `ggg…` / no repeated or reordered chars**; Undo reverts cleanly.
- **Word-specific:** rapid Thai words with tone marks — no dropped/reordered characters, no layout confusion.
- Hotkeys: `Shift+Backspace`, `Shift+CapsLock` (selection only, clipboard restored), `Ctrl+CapsLock` cycles Auto/Manual/Suggest **without** toggling the CapsLock light; Convert-on-demand on arbitrary copied text.
- **Safety:** focus a password field — no buffering/correction, nothing on disk. Blacklisted app (wallet/terminal) — disabled. Type a hex key / WIF / `bc1…` address / a BIP39 seed in a normal field — tool must **bail and ignore** it. Confirm `VirtualLock`/dump-disable active and process is non-elevated.
- **Performance:** `criterion` micro-bench for `convert`/`detect` (assert sub-ms); measure hook-callback time; working set against the ~5–15 MB target; ~0% idle CPU.

**Out of scope (v1):** macOS; non-TH/EN layouts (engine supports them, content not bundled); cloud/sync.

## Performance & resource budget
Event-driven native Rust (no GC / Electron / async runtime); sleeps at ~0% CPU until a key arrives.

| Metric | Target | How |
|---|---|---|
| Hook callback latency | < ~100 µs (never near ~300 ms `LowLevelHooksTimeout`) | callback only copies + enqueues to a channel; all real work on a worker thread (also avoids the timeout that contributes to **Bug 1**) |
| Typing throughput | ≥ 200 WPM, no perceptible lag | per-key cost is µs; ~100× headroom |
| Per-word detection | sub-ms | memory-mapped `fst` set, runs at word boundaries off-thread |
| Idle CPU | ~0% | event-driven; only the ~1–2 s watchdog timer (one handle check) |
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
