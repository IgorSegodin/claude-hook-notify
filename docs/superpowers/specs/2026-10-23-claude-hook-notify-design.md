# claude-hook-notify: design

Date: 2026-10-23
Status: approved 2026-10-23 (user asked to start implementation). Spike
results in section 12; they change the install story from a symlink to a
shell wrapper.

## 1. Intent

A small Rust command-line tool for macOS that posts a native notification
(top-right corner, Notification Center) from anywhere it can be executed:
Claude Code hooks, shell scripts, cron, other applications.

The reason it exists: the user wants to see a notification whenever Claude
Code has finished its turn or is waiting for input, without having to watch
the terminal. It must show regardless of which window is focused.

The user already has a `Notification` hook in `~/.claude/settings.json` that
runs `osascript -e 'display notification ...'`. That works but shows up as
"Script Editor", cannot be given its own notification settings (persistent
alerts, sound), and cannot carry per-event context. This tool replaces it.

## 2. Goals

- One command, callable from any shell or process, posts a notification.
- Works from Claude Code hooks with a one-line hook command, reading the
  hook's JSON from stdin and producing a sensible title, subtitle and body.
- Appears in Notification Center under its own name and icon, with its own
  entry in System Settings > Notifications, so the user can choose Alerts
  (persistent) vs Banners and pick a sound.
- Fast: returns in well under a second in the normal case; never hangs.
- Honest: non-zero exit and a one-line stderr message whenever a
  notification was not delivered. Never a silent failure.
- Safe for hooks: never produces an exit code that changes Claude's behaviour.

## 3. Non-goals (version 1)

- Click actions (focus the terminal, open a URL, run a command). Clicking
  the notification just dismisses it.
- Buttons, reply fields, images or attachments.
- Breaking through Focus modes / Do Not Disturb. The user controls that in
  System Settings. (Time-sensitive interruption level needs an entitlement and
  is a possible later addition.)
- Suppressing the notification when the terminal is frontmost. The user
  explicitly wants it shown in every case.
- Linux or Windows support.
- Homebrew formula, notarization, Developer ID signing. This is a personal
  tool installed from source.
- Filtering which hook events deserve a notification. That is the job of the
  hook `matcher` in `settings.json`; the tool posts whatever it is given.

## 4. Anti-goals (things that would make this a failure)

- Exiting 0 when nothing was shown.
- Exiting 2 from a hook. On a `Stop` hook exit 2 blocks Claude from stopping.
- Blocking a hook for longer than a few seconds under any circumstance.
- Depending on the deprecated `NSUserNotification` API or on bundle-id
  swizzling tricks (`mac-notification-sys`, `notify-rust`, old
  `terminal-notifier`).

## 5. Constraints and facts the design rests on

### 5.1 macOS

- The modern `UserNotifications` framework (`UNUserNotificationCenter`) only
  works for a process whose executable lives inside an `.app` bundle with a
  `CFBundleIdentifier`. A bare binary raises an Objective-C exception
  ("bundleProxyForCurrentProcess is nil"). Therefore the CLI binary ships
  inside a minimal bundle and is reached through a two-line shell wrapper on
  PATH that `exec`s the binary by its bundle path. A plain symlink does not
  work: `NSBundle.mainBundle` resolves the bundle from the path the process
  was invoked by, not from the symlink target (spike result 1, section 12).
- First use of a new bundle id triggers the system permission dialog. If the
  user never answers it, notifications fail silently at the OS level.
- Posting is asynchronous. The process must wait for the completion handler
  of `addNotificationRequest` before exiting.
- Notifications are system-level; the focus state of the terminal does not
  matter. Focus modes can still hide them; that is user configuration.
- Development machine: macOS 15.7 (Sequoia), Apple Silicon, Xcode installed.
  On arm64 the Rust linker ad-hoc signs binaries automatically; the bundle is
  re-signed ad-hoc after assembly.

### 5.2 Claude Code hooks (verified against docs for v2.1.292)

Hook commands run under non-interactive bash, inherit the user's PATH, run
with cwd = project directory, and receive a JSON object on stdin. Default
timeout for command hooks is 600 s; a per-hook `timeout` can be set.

Common fields on every event: `session_id`, `transcript_path`, `cwd`,
`hook_event_name`, `permission_mode`.

`Stop` (Claude finished its turn) adds:
- `last_assistant_message`: the final response text
- `stop_hook_active`: bool
- `background_tasks`, `session_crons`: arrays (ignored by this tool)

`Notification` adds:
- `message`: notification text
- `title`: optional
- `notification_type`: one of `permission_prompt`, `idle_prompt`,
  `auth_success`, `elicitation_dialog`, `elicitation_url_dialog`,
  `elicitation_complete`, `elicitation_response`, `agent_needs_input`,
  `agent_completed`, `quota_auto_resume_fired`, `quota_auto_resume_stale`,
  `quota_auto_resume_disabled`, `plugin_notification`.
  `idle_prompt` fires about 60 s after Claude finished if the user is idle;
  `permission_prompt` fires about 6 s after a permission request is shown.

`SubagentStop` has the same shape as `Stop` plus `agent_type`.

Exit codes as seen by Claude Code: 0 = fine; 1 (or any non-zero other than 2)
= non-blocking error, stderr not shown to the user; 2 = blocking, and on
`Stop` it prevents Claude from stopping.

Notification hook matchers are exact-string matches on `notification_type`;
an empty matcher matches all types.

## 6. The tool

### 6.1 Name and identity

- Cargo package and binary: `claude-hook-notify`.
- Bundle: `ClaudeHookNotify.app`, display name "Claude Hook Notify",
  bundle id `com.isegodin.claude-hook-notify`. (My call; both are a one-line
  change in `Info.plist` if the user prefers something else.)
- The notification header shows the bundle display name automatically; the
  title line is therefore free to carry event information.

### 6.2 Command-line surface

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
                          Trigger the macOS permission dialog and wait for the
                          answer (up to 120 s). First-time setup.
      --timeout <SECS>    Max wait for delivery confirmation. Default 5.
  -v, --verbose           Diagnostics on stderr.
  -h, --help
  -V, --version
```

Design rules:
- Positional `MESSAGE` and `--stdin-json` are mutually exclusive.
- All human-readable output goes to stderr except `--dry-run` and `--check`
  output, which goes to stdout so scripts can consume it.
- Launched with no arguments (which is what happens when the user clicks a
  notification and macOS relaunches the bundle): print usage to stderr,
  exit 1, no notification. `LSUIElement` in `Info.plist` keeps this invisible.
- Usage errors exit 1, not 2. clap's default for usage errors is exit code 2,
  which would be a blocking error if a hook was misconfigured; the builder
  must remap it (for example via `Command::try_get_matches` or
  `Error::exit` handling) and a test must cover it.
- In plain CLI mode the default title is empty: the notification header
  already shows the app name, so `claude-hook-notify "hello"` shows just
  the app name and the body.

### 6.3 Hook mode mapping (`--stdin-json`)

Let `project` = last path component of `cwd` (omitted if `cwd` is missing).
Let `snippet(s)` = `s` with all whitespace runs collapsed to one space,
trimmed, cut to 200 characters with "…" appended if cut. (macOS truncates
visually anyway; the limit just keeps the payload sane.)

| hook_event_name | title | subtitle | body |
|---|---|---|---|
| `Stop` | `Finished` | `project` | `snippet(last_assistant_message)`, or `Done.` if empty |
| `Notification` | payload `title` if present, else by type (below) | `project` | payload `message`, or empty |
| `SubagentStop` | `Subagent finished` | `project` | `agent_type` if present, else empty |
| anything else | humanized `hook_event_name` | `project` | empty |
| not valid JSON / no `hook_event_name` | `Claude Code` | empty | the raw stdin, cut to 200 chars, or `(empty hook payload)` |

Title by `notification_type` when the payload has no `title`:

| notification_type | title |
|---|---|
| `permission_prompt` | `Permission needed` |
| `idle_prompt` | `Waiting for input` |
| `elicitation_dialog`, `elicitation_url_dialog` | `Input needed` |
| `agent_needs_input` | `Agent needs input` |
| `agent_completed` | `Agent finished` |
| any other value | humanized type (`_` to space, first letter capitalized) |
| missing | `Claude Code` |

Additional behaviour in hook mode:
- `threadIdentifier` of the notification is set to `session_id` so
  notifications from one session stack together in Notification Center.
- `--title` / `--subtitle` on the command line override the derived values.
- The tool never decides to stay silent for a recognised event. Choosing
  which events notify is done with hook matchers in `settings.json`.
- Exit code is 0 or 1 only. Never 2.

### 6.4 Recommended hook configuration

The install step prints this, with the absolute path filled in, so the user
can paste it into `~/.claude/settings.json`:

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

Absolute paths are used on purpose: the hook shell inherits PATH, but
`~/.local/bin` is not guaranteed to be on it. `SubagentStop` is intentionally
not wired by default; it would fire for every subagent during agent-heavy
work. If `idle_prompt` reminders feel redundant after the `Stop`
notification, the user can replace the empty matcher with one matcher entry
per wanted `notification_type`.

### 6.5 Runtime flow

1. Parse arguments. `--dry-run` builds the notification spec, prints it, exits.
2. Confirm the process is inside a bundle: `NSBundle.mainBundle().bundleIdentifier`
   must be non-nil. If nil: stderr "claude-hook-notify must run from inside
   ClaudeHookNotify.app; run `make install` and call
   ~/.local/bin/claude-hook-notify", exit 1.
   This check happens before any UserNotifications call, so the Objective-C
   exception is never reached.
3. Get the `UNUserNotificationCenter` and read its settings
   (`getNotificationSettingsWithCompletionHandler`).
   - authorized or provisional: continue.
   - not determined: stderr "notification permission has not been requested
     yet; run `~/.local/bin/claude-hook-notify --request-permission` once and
     answer the dialog", exit 1. Only `--request-permission` calls
     `requestAuthorizationWithOptions(alert | sound)` (120 s wait); a plain or
     hook run never shows the dialog (see the section 12 addendum).
   - denied: stderr "notifications are disabled for Claude Hook Notify in
     System Settings > Notifications", exit 1.
4. Build `UNMutableNotificationContent` (title, subtitle, body, sound,
   threadIdentifier), wrap it in a `UNNotificationRequest` with a fresh UUID
   identifier and no trigger (deliver now).
5. `addNotificationRequest:withCompletionHandler:`; wait on a channel for the
   handler, up to `--timeout`. Handler with no error: exit 0. Handler with
   error: print it, exit 1. Timeout: stderr "timed out waiting for delivery
   confirmation", exit 1.

Completion handlers arrive on a background queue; a `std::sync::mpsc`
channel with `recv_timeout` is expected to be enough, with no run loop
required. This is one of the assumptions the spike verifies (see 8).

### 6.6 Sound

`--sound default` maps to `UNNotificationSound.defaultSound`. `--sound none`
sets no sound. Any other value is passed to `UNNotificationSound soundNamed:`;
macOS silently falls back to the default sound if the name does not exist,
which is acceptable.

## 7. Bundle, build and install

### 7.1 Layout

```
ClaudeHookNotify.app/
  Contents/
    Info.plist
    MacOS/
      claude-hook-notify        # the Rust release binary
    Resources/
      AppIcon.icns              # optional in v1, see below
```

`Info.plist` keys: `CFBundleIdentifier`, `CFBundleName`, `CFBundleDisplayName`,
`CFBundleExecutable`, `CFBundlePackageType` = `APPL`, `CFBundleVersion`,
`CFBundleShortVersionString` (from `Cargo.toml` version), `LSUIElement` = true
(no Dock icon, no menu bar), `LSMinimumSystemVersion` = `13.0`,
`CFBundleIconFile` when an icon is present.

### 7.2 Build and install steps

A `Makefile` (thin wrapper around a shell script under `scripts/`) with:

- `make bundle`: `cargo build --release`, assemble the bundle under
  `target/bundle/`, `codesign --force --sign - ClaudeHookNotify.app`.
- `make install`: copy the bundle to `~/Applications/ClaudeHookNotify.app`
  (replacing any previous copy), register it with LaunchServices
  (`lsregister -f`), write the wrapper script `~/.local/bin/claude-hook-notify`
  (two lines: `#!/bin/sh` and
  `exec "$HOME/Applications/ClaudeHookNotify.app/Contents/MacOS/claude-hook-notify" "$@"`,
  mode 755), then print: the absolute wrapper path, the next step
  (`claude-hook-notify --request-permission`), and the hook JSON from 6.4.
- `make uninstall`: remove wrapper script and bundle.

Rebuilding and reinstalling must not change the bundle id, so the
notification permission granted once stays granted.

### 7.3 Icon

A custom icon is a stretch goal, not a requirement for v1. If it is cheap
(a flat PNG converted with `sips` + `iconutil` during `make bundle`), include
it; otherwise macOS shows a generic app icon and that is fine. Builder's call.

## 8. Spike before planning the implementation

A throwaway spike (minimal Rust binary in a hand-made bundle, no tests, not
kept) must answer, in this order:

1. Does `UNUserNotificationCenter` work when the binary is invoked through a
   symlink on PATH, not via the bundle path?
2. Does an ad-hoc signed bundle in `~/Applications` get the permission dialog
   and then deliver notifications, with no Developer ID?
3. Do the completion handlers fire without a running main run loop, so a
   channel wait is sufficient?
4. Does `--dry-run`-free invocation outside the bundle get caught by the
   `bundleIdentifier` check before any exception?
5. Does the granted permission survive a rebuild and reinstall? Each ad-hoc
   signature is different, so this checks that macOS keys the permission on
   the bundle id, not on the code signature.

If 1 fails, fallback: the symlink becomes a tiny shell wrapper that `exec`s
the binary by its bundle path. If 3 fails, run a `CFRunLoop` until the
handler fires (with the same timeout). If 2 or 5 fails, stop and report to
the user; either would change the install story.

The spike uses a throwaway bundle id (for example
`com.isegodin.claude-hook-notify.spike`) so the real bundle id stays in the
"not determined" state for step 1 of the manual checklist in section 9.

## 9. Testing

Notification delivery cannot be asserted programmatically, so the design
separates the pure part from the macOS part.

- The mapping from (arguments, optional hook JSON) to a `NotificationSpec`
  struct {title, subtitle, body, sound, thread_id} is a pure function with
  unit tests covering every row of the tables in 6.3, the snippet rules,
  humanization, missing fields, invalid JSON and empty stdin.
- `--dry-run` serialises that struct, so integration tests can run the real
  binary with fixture payloads (copied from the Claude Code docs) and
  compare stdout, without a bundle and without macOS calls.
- Argument validation (mutual exclusion, missing message) is tested through
  the binary's exit code and stderr.
- Manual verification checklist, run before declaring done:
  1. `make install`, then `claude-hook-notify --check` reports the bundle id
     and "not determined".
  2. `claude-hook-notify --request-permission` shows the dialog; after Allow,
     `--check` exits 0.
  3. `claude-hook-notify "hello"` with another app frontmost shows a banner
     under "Claude Hook Notify" and exits 0 in under one second.
  4. `echo '<Stop payload from docs>' | claude-hook-notify --stdin-json` shows
     "Finished / <project> / <snippet>".
  5. Same with a `permission_prompt` Notification payload shows
     "Permission needed / <project> / <message>".
  6. `--sound none` is silent; `--sound Glass` plays Glass.
  7. Running the binary from `target/release/` directly fails with the
     "must run from inside the bundle" message and exit 1.
  8. With permission denied in System Settings, exit 1 and a clear message.
  9. Wired into `~/.claude/settings.json`, a real Claude Code turn produces
     the Stop notification, and a permission prompt produces the
     Notification one.

## 10. Implementation notes (left to the builder unless stated)

- Crates: `clap` (derive) for arguments, `serde` + `serde_json` for hook
  payloads and `--dry-run` output, `objc2`, `objc2-foundation`,
  `objc2-user-notifications`, `block2` for the completion handlers.
  Versions 0.3.x of the objc2 framework crates and 0.6.x of `block2` are
  current on crates.io.
- Rust edition and MSRV: builder's call; the machine has Rust 1.97.
- Module split suggestion, not a requirement: `cli.rs` (clap types),
  `spec.rs` (NotificationSpec + pure mapping), `hook.rs` (hook JSON types),
  `macos.rs` (everything that touches Objective-C). Only `macos.rs` is
  untestable.
- Keep `main` thin: parse, build spec, dispatch.
- Error messages name the fix, not just the failure.

## 11. Decisions made by Claude in this design (user may override)

- Bundle id `com.isegodin.claude-hook-notify`, display name "Claude Hook Notify".
- Title/subtitle/body mapping in 6.3, including "Finished" as the Stop title
  and the project folder name as subtitle.
- Default sound: the system default, not "Glass" as in the current osascript
  hook. `--sound Glass` restores it.
- Default delivery timeout 5 s; hook `timeout` 10 s in the recommended config.
- Install location `~/Applications` and symlink in `~/.local/bin`.
- Notifications grouped per session via `threadIdentifier`.
- `SubagentStop` supported in the mapping but not wired in the recommended config.
- No icon required for v1.
- Usage errors exit 1 (clap's default of 2 is remapped).
- Empty default title in plain CLI mode.
- Spike uses a throwaway bundle id and also checks that permission survives
  a rebuild.
- After the spike: the PATH entry is a shell wrapper, not a symlink (section 12).
- `snippet()` is applied to every body taken from the hook payload
  (`last_assistant_message`, `message`), not only to the Stop body; it is a
  normalisation, not a content change.
- A `title` field in a `Notification` payload counts as present only when it
  is non-empty after trimming; an empty title falls back to the type table.
- "Humanized" for `hook_event_name` also splits CamelCase: `PreToolUse`
  becomes `Pre tool use`. For `notification_type` only `_` is replaced.
- `--sound ""` is treated as `default`.

## 12. Spike results (2026-10-23)

Throwaway bundle `ClaudeHookNotifySpike.app`, bundle id
`com.isegodin.claude-hook-notify.spike`, ad-hoc signed, in `~/Applications`,
built with objc2 0.6.5 / objc2-foundation 0.3.2 /
objc2-user-notifications 0.3.2 / block2 0.6.2 on macOS 15.7.7.

| # | Question | Result |
|---|---|---|
| 1 | Works through a symlink on PATH? | **No.** `NSBundle.mainBundle().bundlePath` was `~/.local/bin` and `bundleIdentifier` nil. The spec's fallback applies: a two-line `sh` wrapper that `exec`s the bundle path works (bundle found, 170 ms cold, 20 ms warm). |
| 2 | Ad-hoc bundle gets the dialog and delivers? | **Yes.** `usernoted` presented the permission alert. The first answer came back `granted=false`, error "Notifications are not allowed for this application"; the user then enabled the app in System Settings > Notifications and `addNotificationRequest` completed with no error. |
| 3 | Handlers fire without a run loop? | **Yes.** `getNotificationSettings` handler in 3-5 ms, `addNotificationRequest` handler in 3 ms, waiting on `std::sync::mpsc::recv_timeout` only. No `CFRunLoop`. |
| 4 | Bare binary caught by the bundle check? | **Yes.** `bundleIdentifier` is `None` outside a bundle, exit 1, no Objective-C exception. |
| 5 | Permission survives rebuild + re-sign + reinstall? | **Yes.** CDHash changed (`fd93a8ee…` to `d97a9809…`), status stayed `authorized`, delivery worked. macOS keys the grant on the bundle id and path. |

Timing: a full post (bundle check, settings read, add request, wait) takes
about 20 ms wall-clock through the wrapper.

Consequence for the code: the `--request-permission` flow must treat
`granted=false` with the "not allowed" error as "denied; enable the app in
System Settings > Notifications", not as a transient failure.

Addendum (real bundle, 2026-10-23 16:00): a permission dialog left unanswered
until `--request-permission` gave up after 120 s was removed by macOS the
moment the process exited, and the status changed from "not determined" to
"denied". The only way back is System Settings > Notifications. The README
tells the user to answer within two minutes.

Consequence for the code (final review, 2026-10-23): plain and hook runs never
request permission. A "not determined" status is an error naming
`--request-permission` (section 6.5 step 3). Otherwise a hook firing before
the first `--request-permission`, with the user looking elsewhere, would show
the dialog for `--timeout` seconds and leave the tool permanently denied.
