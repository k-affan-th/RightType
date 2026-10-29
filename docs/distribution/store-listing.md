# Microsoft Store listing text (draft)

Paste-ready text for the Partner Center MSIX submission. Review before submitting.

## Short description (en-US)

Fix text typed with the wrong keyboard layout (Thai/English), automatically or with a hotkey.

## Description (en-US)

RightType is a small tray app for people who type in both Thai and English. When you type a
word with the wrong keyboard layout active (for example "l;ylfu" instead of "สวัสดี"),
RightType detects it and converts the text for you, either automatically or when you press a
hotkey (Shift+Backspace by default).

- Automatic and manual correction between Thai and English layouts
- Undo for every correction
- Practice mode, typing statistics and a learned-words dictionary, all stored locally
- No telemetry and no network access: everything stays on your PC

## คำอธิบาย (th-TH)

RightType คือโปรแกรมเล็กๆ บน system tray สำหรับคนที่พิมพ์ทั้งไทยและอังกฤษ เมื่อคุณพิมพ์ผิดภาษา
(เช่น "l;ylfu" แทน "สวัสดี") โปรแกรมจะตรวจจับและแปลงข้อความให้ ทั้งแบบอัตโนมัติและแบบกดปุ่มลัด
(ค่าเริ่มต้น Shift+Backspace)

- แก้ข้อความผิดภาษาไทย/อังกฤษ ทั้งอัตโนมัติและด้วยปุ่มลัด
- ย้อนกลับได้ทุกครั้งที่แก้
- โหมดฝึกพิมพ์ สถิติการพิมพ์ และพจนานุกรมคำที่เรียนรู้ เก็บไว้ในเครื่องเท่านั้น
- ไม่มี telemetry และไม่เชื่อมต่อเครือข่าย

## Notes for certification (justify `runFullTrust`)

RightType needs the `runFullTrust` capability because it:

1. Installs a low-level keyboard hook (`SetWindowsHookExW`, `WH_KEYBOARD_LL`) to notice when a
   word was typed with the wrong layout. Typed characters are processed in memory only, are
   never written to disk or sent anywhere, and password fields are skipped.
2. Replaces the wrongly typed word with `SendInput`.

The app has no network code and collects no telemetry. Source code (MIT OR Apache-2.0):
https://github.com/k-affan-th/RightType. Security design: `docs/THREAT_MODEL.md`.

Verify these claims against `docs/THREAT_MODEL.md` and the code before submitting; fix this
text rather than the claim if they disagree.

## Properties / values to fill

- Category: Productivity (or Utilities & tools)
- Minimum OS: Windows 10 1809 (build 17763)
- Package identity (Partner Center > Product identity, Store ID 9P0XS6XQLMNC):
  `pwsh -File packaging/msix.ps1 -IdentityName "theKaf.RightType" -Publisher "CN=1083B6B6-BE7E-4B8B-8F88-EBD2826E03EA" -PublisherDisplayName "theKaf" -Arch x64`
  (run `packaging/build_release.ps1 -Arch <arch>` or `cargo build --release --features winos` first)
- Screenshots: `docs/img/*.png` (check the size requirements in Partner Center)
- Privacy policy URL: link to `docs/THREAT_MODEL.md` on GitHub, or a dedicated page
