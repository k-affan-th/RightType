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
icon (left- or right-click) for: **Enable**, **Manual/Auto/Suggest** mode, **Learn new
words**, **Start with Windows**, **Hotkeys & help**, and **Quit**. The tooltip shows the
current mode.

**Manual mode (default)** — type freely; when you notice a wrong-layout word, fix it
with a hotkey:

| Hotkey | Action |
| --- | --- |
| `Shift`+`Backspace` | Convert the last word in place — or, right after Auto changed a word, flip it back |
| `Shift`+`CapsLock` | Convert the current **selection** (v1 temporarily reads it with Copy, restores an empty/plain-Unicode clipboard, then injects Unicode; any non-text/app-specific clipboard format is refused) |
| `Ctrl`+`CapsLock` | Cycle **Manual** → **Auto** → **Suggest** (works in every app, including ones RightType otherwise stays out of) |
| `Alt`+`CapsLock` | Accept the current Suggest hint |
| `Ctrl`+`Shift`+`CapsLock` | Undo the last correction (selection undo requires the same focused context) |
| `Ctrl`+`Alt`+`CapsLock` | Enable/disable RightType immediately |

**Auto mode** — direction-aware, built for how each language is actually written:

- **EN → TH (instant)**: the moment your in-flight keystrokes form a known Thai
  word with high confidence, RightType fixes it **and switches to Thai** — your
  remaining keystrokes finish the word natively. No spaces required, none
  inserted: Thai doesn't work that way. It **holds** while the same keystrokes
  could still become an English word (`diffe` is on its way to `different`), so
  ordinary English typing is never converted mid-word — and for a few keystrokes
  after a conversion it keeps watching, putting the letters back by itself if the
  run turns out not to be Thai after all.
- **TH → EN (at whitespace)**: English *is* space-delimited, so the token
  commits when you hit space.
- **The whole word gets the last say.** A conversion made mid-word is not final:
  when you reach the space, RightType looks at the *entire* word — including the
  part typed after it switched to Thai — and if your keystrokes spell English it
  puts the whole word back and returns to English. (Before 1.1 only the part after
  the switch was judged, which could leave half-Thai, half-English words.)
- English is more than the dictionary: compounds of everyday words
  (`middleware`, `workflow`, `frontend`, `codebase`) and words you have taught it
  are treated as English everywhere.
- Ambiguous prefixes (a short valid word that begins a longer one) resolve in
  your favour as you keep typing; `Shift`+`Backspace` and Undo cover the rare
  miss. It deliberately does not destructively convert mid-word runs that aren't
  fully-known Thai — those stay available to Manual and Suggest.

**Suggest mode** uses the same completed-token policy as Auto's boundary path,
but only displays a hint showing the suggested text. It changes text only after
`Alt`+`CapsLock`, and discards the hint when focus, layout, mode, or typing
context changes.

**Learning** (opt-in, "Learn new words") — an English word you type three times is
remembered, and so is any word you *flip back* after RightType changed it
(`Shift`+`Backspace` or Undo), immediately and in either language. Learned words
take effect at once for every decision.

Version 1 intentionally supports only the exact Thai Kedmanee ↔ US English QWERTY
pair and the fixed hotkeys above. Pattachote/Dvorak/UK-AU-CA layouts, remappable
hotkeys, and per-app mode profiles are tracked for v1.x.

Short, genuinely ambiguous words (e.g. `ok` vs Thai `นา`, which share keys) are left for
you to fix manually — no tool can resolve those without guessing.

The same limit applies to words RightType has never seen. A **typo or a name**
that is not in the English dictionary cannot be recognised as an unfinished
English word, so Auto mode may start converting one — but that decision stays
under review for the next few keystrokes and undoes itself as soon as the Thai
reading stops making sense. What survives is the narrow case where a mistyped
word's *entire* conversion keeps reading as valid Thai; `Shift`+`Backspace` flips
the whole word back — and, with learning on, remembers it so it does not happen
again.

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

**Installer (recommended, per-user, no admin):** run
`RightType-1.0.0-setup.exe` from the release. It installs RightType, offers a
desktop shortcut and start-at-login, and registers a normal Windows uninstaller
(Settings -> Apps -> RightType).

**Portable zip:** extract `RightType-1.0.0-x64.zip` and either run
`righttype.exe` where it sits, or install it per-user:

```powershell
pwsh -File install.ps1 -Autostart
```

- Installs to `%LOCALAPPDATA%\RightType`, adds a **Start Menu shortcut**, and
  (with `-Autostart`) launches at login.
- Uninstall anytime: `pwsh -File uninstall.ps1` (add `-KeepSettings`
  to preserve your config/learned words).
- Verify what you downloaded against `SHA256.txt` before running it:

```powershell
Get-FileHash -Algorithm SHA256 .\RightType-1.0.0-setup.exe
```
- Unsigned builds show a SmartScreen prompt — "More info → Run anyway".
  Signed releases will ship under Azure Trusted Signing in v1.x.

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
