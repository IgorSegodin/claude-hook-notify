# claude-hook-notify

A small macOS command line tool that posts a native notification (top-right
corner, Notification Center) from anywhere: Claude Code hooks, shell scripts,
cron. Its main job is to tell you when Claude Code has finished its turn or is
waiting for input, whichever window is in front.

The binary ships inside a minimal app bundle, `ClaudeHookNotify.app`, because
macOS only posts notifications for processes that run from a bundle. The
bundle gets its own entry in System Settings > Notifications, where you can
pick Alerts (persistent) or Banners and a sound.

## Requirements

- macOS 13 or newer (developed on 15.7, Apple Silicon)
- Rust toolchain (`cargo`)

## Install

```sh
make install
claude-hook-notify --request-permission   # shows the macOS permission dialog once
claude-hook-notify --check                # exit 0 means the tool can post
```

`make install` builds a release binary, assembles and ad-hoc signs the bundle,
copies it to `~/Applications/ClaudeHookNotify.app`, and writes a two-line
wrapper script at `~/.local/bin/claude-hook-notify`. The wrapper execs the
binary by its bundle path; a plain symlink does not work because macOS looks
for the bundle next to the path you invoked.

Answer the dialog within two minutes. If it is declined, left unanswered
until the tool gives up, or you turn notifications off later, macOS records
"denied" and the tool exits 1 with a message pointing at System Settings >
Notifications. Enabling "Claude Hook Notify" there fixes it; no reinstall
needed.

Only `--request-permission` shows the dialog. A plain or hook run that finds
the permission still unrequested exits 1 with a message naming that command,
instead of flashing a dialog nobody is watching: macOS treats a dialog that
is still open when the process exits as declined.

## Claude Code hooks

`make install` prints this block with your absolute path filled in. Add it to
`~/.claude/settings.json` (merge with any existing `hooks`):

```json
{
  "hooks": {
    "Stop": [
      { "hooks": [ { "type": "command",
                     "command": "/Users/<user>/.local/bin/claude-hook-notify --stdin-json",
                     "timeout": 10 } ] }
    ],
    "Notification": [
      { "hooks": [ { "type": "command",
                     "command": "/Users/<user>/.local/bin/claude-hook-notify --stdin-json",
                     "timeout": 10 } ] }
    ]
  }
}
```

In hook mode the tool reads the hook's JSON from stdin and derives the
notification:

| Event | Title | Subtitle | Body |
|---|---|---|---|
| `Stop` | Finished | project folder | first 200 chars of the last reply |
| `Notification` (`permission_prompt`) | Permission needed | project folder | the hook message |
| `Notification` (`idle_prompt`) | Waiting for input | project folder | the hook message |
| `SubagentStop` | Subagent finished | project folder | agent type |

Notifications from one session are grouped together in Notification Center.
To notify on fewer `Notification` types, replace the empty matcher with one
entry per `notification_type` you want (see the Claude Code hooks docs).

## Usage

```sh
claude-hook-notify "Build finished"
claude-hook-notify -t "Deploy" -s "production" "All green" --sound Glass
claude-hook-notify --sound none "quiet"
echo '{"hook_event_name":"Stop","cwd":"/p/demo"}' | claude-hook-notify --stdin-json
claude-hook-notify --stdin-json --dry-run < payload.json   # prints the JSON it would post, no macOS calls
claude-hook-notify --check
claude-hook-notify --help
```

Exit codes: 0 when macOS confirmed delivery, 1 for anything else (usage error,
not inside the bundle, permission missing, delivery error, timeout). The tool
never exits 2, which on a `Stop` hook would block Claude Code from stopping.

## Uninstall

```sh
make uninstall
```

Then remove the hook entries from `~/.claude/settings.json`.

## Development

```sh
cargo test        # unit tests plus --dry-run integration tests; no bundle needed
make bundle       # builds target/bundle/ClaudeHookNotify.app without installing
```

Design notes and the manual verification checklist live in
`docs/superpowers/specs/2026-10-23-claude-hook-notify-design.md`.
