# RightType

A modern, safe **Rust** successor to [RightLang](https://www.rightlang.com/) — it fixes
text typed in the **wrong keyboard layout** (Thai Kedmanee typed while English QWERTY
is active, or vice-versa) **without retyping**.

> Type `correct` while Thai is active and you get `แนพพำแะ`. RightType turns it back
> into `correct` automatically — or on a hotkey.

## Why a rebuild?

RightLang is excellent but unmaintained, and its design causes real bugs:

| RightLang bug | RightType fix |
| --- | --- |
| Dies after the PC wakes from sleep (must kill & reopen) | Auto-reinstalls the keyboard hook on power/session events + watchdog |
| Manual switch sometimes emits `ggg…` garbage | Releases held modifiers + injects an atomic Unicode batch + Undo |
| Dropped/reordered characters in Word | Injects raw Unicode (no layout switching, no race) |

…plus a first-class **privacy/security layer** (see below) and a tiny footprint
(~5–15 MB RAM, ~2–5 MB single portable `.exe`).

## Status

🚧 Early development. The OS-free **core** (layout conversion engine) is in place and
tested; the Windows integration layer (hook, injection, session resilience, tray UI)
is being built incrementally. See [`docs`/the plan] for the roadmap.

## Privacy & security (built for sensitive typing)

A keyboard tool sees everything you type. RightType is designed so secrets never leak:

- **Secret-shaped bail-out** — stops and wipes its buffer the moment a token looks
  like a private key (hex/WIF/base58/bech32), a high-entropy password, or a BIP39
  seed phrase.
- **No on-disk keystroke logging**, no telemetry, **zero network code**.
- **RAM hardening** — buffers are zeroized, locked out of the pagefile, and crash
  dumps are disabled; runs non-elevated.
- **Sensitive-context guards** — disabled in password fields and a default blacklist
  (wallets, password managers, terminals).

## For end users

Download `RightType.exe` and run it. **No installer, no dependencies** — it needs only
the Universal C Runtime already present on Windows 10/11.

## For developers

Requires the Rust **MSVC** toolchain and the Visual Studio C++ Build Tools (Desktop C++).

```sh
# Pure-logic core (no OS calls) — runs anywhere:
cargo test

# Full Windows app:
cargo run --features winos
cargo build --release --features winos
```

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
