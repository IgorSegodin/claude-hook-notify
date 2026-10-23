#!/bin/sh
# Render bundle/icon.svg into AppIcon.icns with tools that ship with macOS:
# sips rasterises the SVG at each iconset size, iconutil packs the set.
# Output goes under target/ (git-ignored); the SVG is the only versioned file.
# Usage: icon.sh [OUT_DIR]   (default: <repo>/target/icon)
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SVG="$ROOT/bundle/icon.svg"
OUT="${1:-$ROOT/target/icon}"
ICONSET="$OUT/AppIcon.iconset"
ICNS="$OUT/AppIcon.icns"

[ -f "$SVG" ] || { echo "icon.sh: missing $SVG" >&2; exit 1; }
command -v sips >/dev/null 2>&1 || { echo "icon.sh: sips not found (it ships with macOS)" >&2; exit 1; }
command -v iconutil >/dev/null 2>&1 || { echo "icon.sh: iconutil not found (it ships with macOS)" >&2; exit 1; }

render() { # render SIZE FILE
  sips -s format png -z "$1" "$1" "$SVG" --out "$2" >/dev/null
  [ -s "$2" ] || { echo "icon.sh: sips produced no output for $2" >&2; exit 1; }
}

rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  render "$s" "$ICONSET/icon_${s}x${s}.png"
  render $((s * 2)) "$ICONSET/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$ICNS"
echo "icon ready: $ICNS"
