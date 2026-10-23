# claude-hook-notify

A small macOS command line tool that posts a native notification (top-right
corner, Notification Center) from anywhere: Claude Code hooks, shell scripts,
cron. Its main job is to tell you when Claude Code has finished its turn or is
waiting for input, whichever window is in front.

- Native notifications through the UserNotifications framework, with the
  tool's own entry in System Settings > Notifications: alert style, sound,
  Focus allow-list.
- Hook mode reads the Claude Code hook JSON from stdin and turns it into a
  sensible title, subtitle and body.
- Fast and honest: about 25 ms per notification, exit 0 only when macOS
  confirmed delivery, never exit 2 (which would block a `Stop` hook).

## Requirements

- macOS 13 or newer (developed on macOS 15, Apple Silicon)
- Rust toolchain (`cargo`)

## Quick start

```sh
make install                              # build, sign, install; prints the hook config
claude-hook-notify --request-permission   # answer the macOS dialog, once
claude-hook-notify "hello"                # a banner appears
```

Then add the hook entries that `make install` printed to
`~/.claude/settings.json`. Details in the setup guide.

## Documentation

- [Setup](docs/setup.md): install, permission, Claude Code hooks, Focus
  modes, updating, uninstall.
- [Usage](docs/usage.md): command line reference, hook mode mapping, sounds,
  exit codes.
- [Troubleshooting](docs/troubleshooting.md): no banner, permission states,
  hooks not firing, stale icon.
- [Development](docs/development.md): build, test, project layout, icon
  pipeline, verification checklist.
- [Design](docs/design.md): goals, constraints, decisions and verified macOS
  behaviour.
