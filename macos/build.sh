#!/bin/bash
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
if [[ "$(uname -s)" != Darwin ]]; then
    echo "The native preview must be built on macOS." >&2
    exit 1
fi
export MACOSX_DEPLOYMENT_TARGET=13.0
target_dir="${CARGO_TARGET_DIR:-$root/target}"
app="$root/dist/macos/RightType.app"
cargo build --locked --release --features text-tool --bin righttype-text --target-dir "$target_dir"
mkdir -p "$app/Contents/MacOS"
cp macos/Info.plist "$app/Contents/Info.plist"
cp "$target_dir/release/righttype-text" "$app/Contents/MacOS/righttype-text"
xcrun swiftc -swift-version 6 -warnings-as-errors -O -target "$(uname -m)-apple-macos13.0" \
    -module-cache-path "$target_dir/swift-module-cache" \
    macos/RightType.swift -o "$app/Contents/MacOS/RightType"
codesign --force --sign - "$app/Contents/MacOS/righttype-text"
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
echo "Built $app (local preview; not notarized)."
