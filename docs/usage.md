# Usage

## Command line

```
claude-hook-notify [OPTIONS] [MESSAGE]

Arguments:
  [MESSAGE]               Notification body. Required unless --stdin-json,
                          --check or --request-permission is given.

Options:
  -t, --title <TEXT>      Title line. In hook mode overrides the derived title.
  -s, --subtitle <TEXT>   Subtitle line. In hook mode overrides the derived one.
      --sound <NAME>      "default" (system default sound), "none", or the name
                          of a system sound such as "Glass". Default: default.
      --stdin-json        Hook mode: read one Claude Code hook JSON object from
                          stdin and derive title/subtitle/body from it.
      --dry-run           Print the notification that would be posted, as one
                          JSON object on stdout, and exit 0. Makes no macOS
                          calls, so it works outside the bundle.
      --check             Print bundle id and authorization status to stdout.
                          Exit 0 if the tool can post, 1 otherwise.
      --request-permission
                          Show the macOS permission dialog and wait for the
                          answer (up to 120 s). First-time setup.
      --timeout <SECS>    Max wait for each macOS reply, in seconds. Default 5.
  -v, --verbose           Diagnostics on stderr.
  -h, --help
  -V, --version
```

`MESSAGE` and `--stdin-json` are mutually exclusive. Everything meant for a
human goes to stderr; only `--dry-run` and `--check` write to stdout, so
scripts can consume them.

The notification header always shows the app name, "Claude Hook Notify". The
title line is therefore optional. In plain mode it is empty unless you pass
`--title`, so `claude-hook-notify "hello"` shows the app name and the body.

## Examples

```sh
claude-hook-notify "Build finished"
claude-hook-notify -t "Deploy" -s "production" "All green" --sound Glass
claude-hook-notify --sound none "quiet"
claude-hook-notify --stdin-json < payload.json            # hook mode from a file
claude-hook-notify --stdin-json --dry-run < payload.json  # what it would post, no macOS calls
claude-hook-notify --check
claude-hook-notify -v "hello"                              # timings and status on stderr
```

`--dry-run` output is one JSON object with the fields `title`, `subtitle`,
`body`, `sound` and `thread_id`:

```json
{"title":"Finished","subtitle":"demo","body":"I updated the README.","sound":"default","thread_id":"abc123"}
```

## Hook mode

With `--stdin-json` the tool reads one Claude Code hook payload from stdin.
Let `project` be the last path component of `cwd` (empty if `cwd` is missing
or is `/`), and `snippet(s)` be `s` with whitespace runs collapsed, trimmed,
and cut to 200 characters with "…" appended when cut.

| `hook_event_name` | Title | Subtitle | Body |
|---|---|---|---|
| `Stop` | Finished | project | `snippet(last_assistant_message)`, or "Done." if empty |
| `Notification` | payload `title` if non-empty, else by type (below) | project | `snippet(message)`, or empty |
| `SubagentStop` | Subagent finished | project | `agent_type` if present, else empty |
| anything else | the event name humanized (`PreToolUse` becomes "Pre tool use") | project | empty |
| not valid JSON, not an object, or no `hook_event_name` | Claude Code | empty | the raw stdin cut to 200 characters, or "(empty hook payload)" |

Title by `notification_type` when the payload has no `title`:

| `notification_type` | Title |
|---|---|
| `permission_prompt` | Permission needed |
| `idle_prompt` | Waiting for input |
| `elicitation_dialog`, `elicitation_url_dialog` | Input needed |
| `agent_needs_input` | Agent needs input |
| `agent_completed` | Agent finished |
| any other value | the type humanized (`_` to space, first letter capitalized) |
| missing | Claude Code |

Also in hook mode:

- The notification's thread identifier is the `session_id`, so notifications
  from one session stack together in Notification Center.
- `--title` and `--subtitle` override the derived values.
- The tool never stays silent for an event it recognises. Choosing which
  events notify is the job of hook matchers in `settings.json`.
- If stdin is a terminal (nothing piped), the payload counts as empty and a
  warning is printed instead of waiting for input.

## Sounds

`--sound default` plays the system default notification sound. `--sound none`
is silent. Any other value is passed to macOS as a system sound name, for
example `Glass`, `Ping`, `Submarine`. An unknown name falls back to the
default sound. An empty value counts as `default`.

The sound only plays when the notification is actually presented. Focus
modes mute it along with the banner.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | macOS confirmed delivery (or `--dry-run`, `--check` with a usable status, `--request-permission` granted, `--help`, `--version`) |
| 1 | anything else: usage error, not inside the bundle, permission not requested or denied, delivery error, timeout |

The tool never exits 2. On a `Stop` hook, exit 2 would block Claude Code
from stopping.

Every wait on macOS is bounded by `--timeout`: reading the authorization
status and waiting for delivery confirmation. The worst case for one run is
therefore twice the timeout, 10 seconds by default. The normal case is about
25 milliseconds.
