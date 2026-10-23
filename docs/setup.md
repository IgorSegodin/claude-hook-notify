# Setup

## Install

```sh
make install
```

This builds a release binary, renders the app icon, assembles and ad-hoc signs
`ClaudeHookNotify.app`, copies it to `~/Applications/ClaudeHookNotify.app`,
registers it with Launch Services, and writes a two-line wrapper script at
`~/.local/bin/claude-hook-notify`:

```sh
#!/bin/sh
exec "/Users/<you>/Applications/ClaudeHookNotify.app/Contents/MacOS/claude-hook-notify" "$@"
```

The wrapper exists because macOS only posts notifications for a process that
runs from inside an app bundle, and it finds the bundle from the path the
process was started with. A symlink on PATH would make the binary look for a
bundle in `~/.local/bin` and fail. The wrapper runs the binary by its bundle
path instead.

No Developer ID or notarization is needed. The bundle is ad-hoc signed and the
notification permission is keyed on the bundle id, so rebuilding and
reinstalling keeps the permission.

Make sure `~/.local/bin` is on your PATH if you want to call the tool by name
from a shell. The hook configuration below uses the absolute path, so hooks
work either way.

## Permission

```sh
claude-hook-notify --request-permission
```

This shows the macOS permission dialog once and waits up to two minutes for
your answer. Click Allow. Afterwards:

```sh
claude-hook-notify --check     # prints the bundle id and "authorization: authorized", exit 0
```

Two things to know about the dialog:

- Only `--request-permission` shows it. A plain or hook run that finds the
  permission still unrequested exits 1 with a message naming that command. It
  never flashes a dialog nobody is watching, because macOS treats a dialog
  that is still open when the process exits as declined.
- Answer within the two minutes. If the dialog is declined or left open until
  the tool gives up, macOS records "denied" for the app. The only way back is
  System Settings > Notifications > Claude Hook Notify > Allow Notifications.
  No reinstall is needed.

In System Settings > Notifications you can also pick the alert style (Banners
disappear by themselves, Alerts stay until dismissed) and the sound.

## Claude Code hooks

`make install` prints this block with your absolute path filled in. Merge it
into the `hooks` object of `~/.claude/settings.json`:

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command",
                     "command": "/Users/<you>/.local/bin/claude-hook-notify --stdin-json",
                     "timeout": 10 } ] }
    ],
    "Notification": [
      { "hooks": [ { "type": "command",
                     "command": "/Users/<you>/.local/bin/claude-hook-notify --stdin-json",
                     "timeout": 10 } ] }
    ]
  }
}
```

- `Stop` fires when Claude finishes a turn. The notification reads
  "Finished", the project folder name, and the first 200 characters of the
  reply.
- `Notification` fires when Claude needs you: permission prompts, idle
  reminders, input requests. The title follows the notification type
  ("Permission needed", "Waiting for input", ...).
- `timeout: 10` caps how long Claude Code waits for the hook. The tool
  normally returns in about 25 ms.

To notify on fewer `Notification` types, replace the empty matcher with one
entry per `notification_type` you want, for example `permission_prompt`
only. `SubagentStop` is supported too but not wired by default, because it
fires for every subagent during agent-heavy work.

Claude Code reads hook configuration when a session starts. A session that
was already running when you edited the file keeps its old hooks until you
open `/hooks` in that session and accept the change, or start a new session.

If you previously had an `osascript -e 'display notification ...'` hook,
remove it; otherwise you get two notifications per event.

## Focus modes

Focus modes (Do Not Disturb, Sleep, Work, ...) silence this tool like any
other app: the notification is delivered to Notification Center but no
banner is shown. To let it through, add the app to the Focus's allow-list:

1. System Settings > Focus > the Focus you use (each one has its own list).
2. Under Allowed Notifications, choose Apps, press the plus button, add
   "Claude Hook Notify".

The tool cannot do this for you. macOS's "time-sensitive" level that breaks
through Focus needs an entitlement issued through Apple's provisioning, which
an ad-hoc signed app cannot carry.

## Updating

Run `make install` again. The bundle id does not change, so the permission
and your notification settings stay. If the app icon changed, Notification
Center may keep showing the old one until you run `killall
NotificationCenter` or log out and in.

## Uninstall

```sh
make uninstall
```

This removes the wrapper script and the app bundle and unregisters it from
Launch Services. Then remove the hook entries from `~/.claude/settings.json`.
The entry in System Settings > Notifications disappears on its own after a
while; macOS owns it.
