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
| Dies after the PC wakes from sleep (must kill & reopen) | Auto-reinstalls the keyboard hook on power/session events + a watchdog |
| Manual switch sometimes emits `ggg…` garbage | Releases held modifiers + injects one atomic Unicode batch (never replays keys) |
| Dropped/reordered characters in Word | Injects raw Unicode codepoints — no layout switching, no race |

…plus a first-class **privacy/security layer** (see below) and a tiny footprint.

## How it works

RightType lives in the **system tray** (no window, no console). Right-click the tray
icon for: **Enable**, **Auto/Manual** mode, **Learn new words**, **Start with Windows**,
and **Quit**.

**Manual mode (default)** — type freely; when you notice a wrong-layout word, fix it
with a hotkey:

| Hotkey | Action |
| --- | --- |
| `Shift`+`Backspace` | Convert the last word in place |
| `Shift`+`CapsLock` | Convert the current **selection** (any length — works for whole sentences, via the clipboard) |
| `Ctrl`+`CapsLock` | Toggle **Auto** / **Manual** |

**Auto mode** — corrects as you type, in **both** directions, even for Thai (which has
no spaces between words): it segments the text against a dictionary, and the moment a
run is recognised as a real word in the *other* layout it's converted in place. When it
detects you've started a language, it also **switches the active keyboard layout** for
you, so the rest of the sentence is typed natively.

Short, genuinely ambiguous words (e.g. `ok` vs Thai `นา`, which share keys) are left for
you to fix manually — no tool can resolve those without guessing.

## Privacy & security (built for sensitive typing)

A keyboard tool sees everything you type. RightType is designed so secrets never leak:

- **Sensitive-context guards** — it disables itself entirely in **password fields**
  (native `ES_PASSWORD`, and browser/Electron/UWP fields via UI Automation) and in a
  default blacklist of **wallets, password managers, and terminals**.
- **Secret-shaped bail-out** — even elsewhere, it ignores any token shaped like a
  private key (hex/WIF/base58/bech32), a high-entropy password, or a BIP39 seed phrase.
- **Minimal in-memory footprint** — it holds only the current word, **zeroized on every
  word boundary**, and runs **non-elevated**.
- **No telemetry, zero network code** — verifiable in `Cargo.lock`; there is no HTTP,
  update, or analytics crate anywhere in the dependency tree.
- **Nothing typed is written to disk** — the only files are app settings
  (`%APPDATA%\RightType\config.toml`) and, *if you opt into* "Learn new words", the
  learned words themselves (`learned.txt`). Learning is **off by default** and never
  runs in the sensitive contexts above.

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
testable anywhere and a future macOS backend additive. See [`docs/PLAN.md`](docs/PLAN.md).

## License

Dual-licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
