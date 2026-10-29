# RightType Privacy Policy

_Last updated: 2026-10-29. Applies to RightType 2.6.3 and later, including the Microsoft Store version._

RightType is a Windows tray app that fixes text typed with the wrong keyboard layout (Thai/English).
To do that it has to see the keys you press, so this page says exactly what it does with them.

## Summary

- RightType **does not send anything over the network.** It has no telemetry, no analytics, no
  accounts and no advertising. It makes no network connection of its own.
- What you type is processed **in memory only**, to decide whether a word needs fixing. It is
  cleared as soon as it is no longer needed (for example when you move to another field or click).
- Anything RightType keeps on disk stays on your PC, in your user profile, and only comes from
  features you turn on or settings you change.

## What RightType reads

- **Keystrokes.** RightType uses a Windows keyboard hook to see the word you are typing. It does not
  record or log them. Passwords are not touched: RightType does nothing in password fields, in
  password managers or other apps on its safety list, or in remote-desktop and virtual-machine
  windows, and it skips text that looks like a secret (such as private keys or seed phrases).
- **Text near the caret or a selection**, only when you use a feature that needs it (for example
  "convert selection" or "fix this field"). It is read through Windows accessibility (UI Automation)
  and discarded after use.
- **The clipboard**, only when you use a command that says so (such as "Paste and fix"). A fallback
  that copies your selection through the clipboard is off unless you enable it.

## What RightType saves on your PC

Files are stored in `%APPDATA%\RightType` (for the Microsoft Store version, in the app's own
local data folder). All of them are plain files you can read and delete:

| File | What it holds | When |
| --- | --- | --- |
| `config.toml` | Your settings, hotkeys, per-app modes and snippets | Always, when you change a setting |
| `learned.txt` | Words you taught RightType, or reverted conversions | Only if word learning is on |
| `stats.toml` | Two counts per day (automatic and hotkey fixes). No words | Only if you turn on statistics |
| `contexts.toml` | Per program and field type: Thai and English word counts. No words | Only if you turn on field habits |
| `practice.txt` | Daily typing-practice scores | Only if you turn on score keeping |

If you pick a sync folder for learned words or settings, those files are written there and
synchronised by whatever service syncs that folder (for example OneDrive). RightType itself does
not upload them.

## What RightType shares

Nothing. RightType never transmits your typing, files, settings or usage to the developer or to
anyone else. The "Check for updates" button only asks Windows to open the GitHub Releases page in
your browser; that page is then governed by GitHub's privacy policy.

## Why the Store asks for "full trust"

The Microsoft Store listing declares the `runFullTrust` capability because a keyboard hook and
simulated keystrokes (`SendInput`) are not available to sandboxed apps. They are used only for the
purposes above.

## Your control

Switch RightType off or pause it from the tray menu, change what it does in Settings, clear learned
words in Settings, or uninstall it. Deleting `%APPDATA%\RightType` removes everything it saved.

## More detail and contact

- Technical description of every kind of data and its lifetime: [THREAT_MODEL.md](THREAT_MODEL.md)
- Source code (MIT OR Apache-2.0): <https://github.com/k-affan-th/RightType>
- Questions or concerns: open an issue at <https://github.com/k-affan-th/RightType/issues>

If this policy changes, the new version will be published here with a new date.
