#!/bin/sh
# Copy the bundle to ~/Applications, register it with Launch Services and write
# the wrapper script that puts claude-hook-notify on PATH. The wrapper execs the
# binary by its bundle path: a plain symlink would make NSBundle look in
# ~/.local/bin and find no bundle (docs/design.md section 10).
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/target/bundle/ClaudeHookNotify.app"
DEST="$HOME/Applications/ClaudeHookNotify.app"
WRAPPER="$HOME/.local/bin/claude-hook-notify"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

[ -d "$SRC" ] || { echo "install.sh: $SRC not found; run 'make bundle' first" >&2; exit 1; }

mkdir -p "$HOME/Applications" "$HOME/.local/bin"
rm -rf "$DEST"
cp -R "$SRC" "$DEST"
"$LSREGISTER" -f "$DEST"
# The build copy under target/ shares the bundle id. Only the installed copy
# may be known to Launch Services, or the system can resolve the id to the
# wrong path (icon, Finder, Notification Center).
"$LSREGISTER" -u "$SRC" || true

BIN="$DEST/Contents/MacOS/claude-hook-notify"
printf '#!/bin/sh\nexec "%s" "$@"\n' "$BIN" > "$WRAPPER"
chmod 755 "$WRAPPER"

cat <<MSG
Installed $DEST
Command:   $WRAPPER

Next step (first-time setup, shows the macOS permission dialog):
  $WRAPPER --request-permission

Hook configuration for ~/.claude/settings.json:
{
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$WRAPPER --stdin-json",
            "timeout": 10
          }
        ]
      }
    ],
    "Notification": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$WRAPPER --stdin-json",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
MSG
