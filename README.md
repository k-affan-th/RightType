# RightType

A modern, safe **Rust** successor to [RightLang](https://www.rightlang.com/) — it fixes
text typed in the **wrong keyboard layout** (Thai Kedmanee typed while English QWERTY
is active, or vice-versa) **without retyping**.

> Type `correct` while Thai is active and you get `แนพพำแะ`. RightType turns it back
> into `correct` — automatically as you type, or on a hotkey.

## Why a rebuild?

RightLang is excellent but unmaintained, and its design causes real bugs:

| RightLang bug | RightType fix |
| --- | --- |
| Dies after the PC wakes from sleep (must kill & reopen) | Reinstalls the keyboard hook on power/session events, with one delayed retry |
| Manual switch sometimes emits `ggg…` garbage | Releases held modifiers + injects one atomic Unicode batch (never replays keys) |
| Dropped/reordered characters in Word | Injects raw Unicode codepoints — no layout switching, no race |

…plus a first-class **privacy/security layer** (see below) and a tiny footprint.

## How it works

RightType lives in the **system tray** (no window, no console). Right-click the tray
icon for: **Enable**, **Manual/Auto/Suggest** mode, **Learn new words**, **Start with Windows**,
and **Quit**.

**Manual mode (default)** — type freely; when you notice a wrong-layout word, fix it
with a hotkey:

| Hotkey | Action |
| --- | --- |
| `Shift`+`Backspace` | Convert the last word in place |
| `Shift`+`CapsLock` | Convert the current **selection** (v1 temporarily reads it with Copy, restores an empty/plain-Unicode clipboard, then injects Unicode; any non-text/app-specific clipboard format is refused) |
| `Ctrl`+`CapsLock` | Cycle **Manual** → **Auto** → **Suggest** |
| `Alt`+`CapsLock` | Accept the current Suggest hint |
| `Ctrl`+`Shift`+`CapsLock` | Undo the last correction (selection undo requires the same focused context) |
| `Ctrl`+`Alt`+`CapsLock` | Enable/disable RightType immediately |

**Auto mode** — direction-aware, built for how each language is actually written:

- **EN → TH (instant)**: the moment your in-flight keystrokes form a known Thai
  word with high confidence, RightType fixes it **and switches to Thai** — your
  remaining keystrokes finish the word natively. No spaces required, none
  inserted: Thai doesn't work that way.
- **TH → EN (at whitespace)**: English *is* space-delimited, so the token
  commits when you hit space.
- Ambiguous prefixes (a short valid word that begins a longer one) resolve in
  your favour as you keep typing; `Shift`+`Backspace` and 30-second Undo cover
  the rare miss. It deliberately does not destructively convert mid-word runs
  that aren't fully-known Thai — those stay available to Manual and Suggest.

**Suggest mode** uses the same completed-token policy as Auto's boundary path,
but only displays a hint. It changes text only after `Alt`+`CapsLock`, and
discards the hint when focus, layout, mode, or typing context changes.

Version 1 intentionally supports only the exact Thai Kedmanee ↔ US English QWERTY
pair and the fixed hotkeys above. Pattachote/Dvorak/UK-AU-CA layouts, remappable
hotkeys, and per-app mode profiles are tracked for v1.x.

Short, genuinely ambiguous words (e.g. `ok` vs Thai `นา`, which share keys) are left for
you to fix manually — no tool can resolve those without guessing.

## Privacy & security (built for sensitive typing)

A keyboard tool sees everything you type. RightType is designed so secrets never leak:

- **Sensitive-context guards** — it disables itself entirely in **password fields**
  (native `ES_PASSWORD`, and browser/Electron/UWP fields via UI Automation) and in a
  default blacklist of **wallets, password managers, and terminals**.
- **Secret-shaped bail-out** — even elsewhere, identifiable private keys and addresses
  are always ignored. Consecutive BIP39 words trigger a phrase-level stream guard.
  Password-like/long ASCII can be wrong-layout Thai;
  it is eligible only when the complete conversion is fully-known Thai, and it is never
  sent to the learning/persistence path.
- **Minimal in-memory footprint** — it holds only the current word, **zeroized on every
  word boundary**, and runs **non-elevated**.
- **No telemetry, zero network code** — verifiable in `Cargo.lock`; there is no HTTP,
  update, or analytics crate anywhere in the dependency tree.
- **Nothing typed is written to disk** — the only files are app settings
  (`%APPDATA%\RightType\config.toml`) and, *if you opt into* "Learn new words", the
  learned words themselves (`learned.txt`). Learning is **off by default** and never
  runs in the sensitive contexts above.

The detailed data lifetimes, controls, and known residual risks are documented in
[`docs/THREAT_MODEL.md`](docs/THREAT_MODEL.md). In particular, ordinary English
words overlap the BIP39 list, so phrase-level protection is contextual/stream-based
rather than a blanket ban on every individual BIP39 word.

## For end users

Download `RightType.exe` and run it — it appears in the tray. **No installer, no
dependencies** beyond the Universal C Runtime already present on Windows 10/11. Use
"Start with Windows" to launch it at login.

## For developers

Requires the Rust **MSVC** toolchain and the Visual Studio C++ Build Tools (Desktop C++).

```sh
# Pure-logic core (no OS calls) — runs anywhere:
cargo test

# Full Windows app:
cargo run --features winos
cargo build --release --features winos
```

The crate is split into an OS-free **core** (`layout`, `secret`, `dict`, `detect`,
`segment`, `buffer` — exhaustively unit-tested) and a Windows **integration layer**
behind the `winos` feature (`hook`, `inject`, `manual`, `safety`, `focus`, `session`,
`tray`, `toast`, `config`, `startup`, `learn`). Keeping the core OS-free makes it
testable anywhere and a future macOS backend additive. See the long-term
[`docs/PLAN.md`](docs/PLAN.md) and the current
[`docs/DYNAMIC_PLAN.md`](docs/DYNAMIC_PLAN.md) for active decisions, evidence, and
the next implementation step.

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
