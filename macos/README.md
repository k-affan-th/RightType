# RightType for Mac — development preview

This first milestone provides a native **Fix Text** window and menu bar item.
It uses RightType's existing Rust policy to fix known Thai/English text typed on
the wrong layout, preserving correct words and whitespace. It currently uses
US QWERTY / Thai Kedmanee tables. Ambiguous or unknown words can stay unchanged.

This preview does not capture global keys, correct another app or change the
Mac keyboard language. The [contribution plan](../docs/PLAN_MACOS.md) tracks that
work, its privacy requirements and the tests needed before enabling it.

## Build and try

On a Mac with Rust and Xcode 16+ (or command-line tools with Swift 6 and the
macOS SDK):

```sh
bash macos/build.sh
dist/macos/RightType.app/Contents/MacOS/RightType --self-test
open dist/macos/RightType.app
```

The script builds for the current Mac's architecture with a macOS 13 deployment
target. Only the tested host is verified; the target does not prove behavior on
every older release or on Intel. It creates an ad hoc signed development app,
not a Developer ID signed or notarized release.

1. Paste/type a short passage into **Original**, such as `l;ylfu`.
2. Choose **Preview Fix**. The preview should show `สวัสดี`.
3. Review it, then choose **Copy Result** if you want to use it elsewhere.
4. Editing the original invalidates the old result. **Clear** or closing the
   window clears its text and undo history. Reopen from the **Aก** menu bar item.

Passages are limited to 32 KiB of UTF-8. No permission prompts are needed for this
milestone. Previewing does not read or write the clipboard; Paste Text and Copy
Result do so only when selected. Copied results stay on the clipboard until you
replace them. No supplied text is saved by the app or used for learning. macOS,
AppKit and the engine can retain transient memory copies; this is not a promise
of secure memory erasure.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --locked --features text-tool --all-targets -- -D warnings
cargo test --locked --features text-tool --all-targets
bash -n macos/build.sh
plutil -lint macos/Info.plist
bash macos/build.sh
dist/macos/RightType.app/Contents/MacOS/RightType --self-test
codesign --verify --deep --strict dist/macos/RightType.app
```

The Rust tests cover invalid UTF-8, the input cap, I/O failure, spacing,
bidirectional repair and a known secret shape. The bundled Swift self-check
exercises the real helper and its packaging, including oversized input and a
missing helper. It does not replace keyboard, screen reader or real-app tests
required by later milestones. The GitHub Mac job builds this preview and uploads
a ZIP; a successful CI build is not evidence of global correction behavior.
