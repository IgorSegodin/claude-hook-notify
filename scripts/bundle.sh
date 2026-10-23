#!/bin/sh
# Build the release binary, assemble ClaudeHookNotify.app under target/bundle/
# and ad-hoc sign it. The bundle id never changes between builds, so a granted
# notification permission survives a rebuild (docs/design.md section 10).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$ROOT/target/bundle/ClaudeHookNotify.app"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)"
[ -n "$VERSION" ] || { echo "bundle.sh: could not read version from Cargo.toml" >&2; exit 1; }

cargo build --release --manifest-path "$ROOT/Cargo.toml"
sh "$ROOT/scripts/icon.sh" "$ROOT/target/icon"

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$ROOT/target/release/claude-hook-notify" "$APP/Contents/MacOS/claude-hook-notify"
cp "$ROOT/target/icon/AppIcon.icns" "$APP/Contents/Resources/AppIcon.icns"
sed "s/__VERSION__/$VERSION/g" "$ROOT/bundle/Info.plist.template" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist"
codesign --force --sign - "$APP"
codesign --verify --verbose=2 "$APP"
echo "bundle ready: $APP"
