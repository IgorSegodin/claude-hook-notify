#!/bin/sh
# Remove the wrapper script and the installed bundle.
set -eu

DEST="$HOME/Applications/ClaudeHookNotify.app"
WRAPPER="$HOME/.local/bin/claude-hook-notify"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

if [ -e "$WRAPPER" ]; then
  rm -f "$WRAPPER"
  echo "removed $WRAPPER"
fi
if [ -d "$DEST" ]; then
  "$LSREGISTER" -u "$DEST" || true
  rm -rf "$DEST"
  echo "removed $DEST"
fi
echo "Hook entries in ~/.claude/settings.json are left for you to remove."
