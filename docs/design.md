# claude-hook-notify: design

Last updated 2026-10-23. This document records what the tool must do, the
constraints it rests on, the decisions taken, and the macOS behaviour verified
on the development machine. How-to material lives in [setup](setup.md),
[usage](usage.md), [troubleshooting](troubleshooting.md) and
[development](development.md).

## 1. Intent

A small Rust command-line tool for macOS that posts a native notification
(top-right corner, Notification Center) from anywhere it can be executed:
Claude Code hooks, shell scripts, cron, other applications.

The reason it exists: the user wants to see a notification whenever Claude
Code has finished its turn or is waiting for input, without having to watch
the terminal. It must show regardless of which window is focused.

The previous setup was a `Notification` hook running
`osascript -e 'display notification ...'`. That works but shows up as
"Script Editor", cannot be given its own notification settings (persistent
alerts, sound, Focus allow-list), and cannot carry per-event context. This
tool replaced it.

## 2. Goals

- One command, callable from any shell or process, posts a notification.
- Works from Claude Code hooks with a one-line hook command, reading the
  hook's JSON from stdin and producing a sensible title, subtitle and body.
- Appears in Notification Center under its own name and icon, with its own
  entry in System Settings > Notifications, so the user can choose Alerts
  (persistent) vs Banners, pick a sound, and allow it through Focus modes.
- Fast: returns in well under a second in the normal case; never hangs.
- Honest: non-zero exit and a one-line stderr message whenever a
  notification was not delivered. Never a silent failure.
- Safe for hooks: never produces an exit code that changes Claude's behaviour.

## 3. Non-goals

- Click actions (focus the terminal, open a URL, run a command). Clicking
  the notification just dismisses it.
- Buttons, reply fields, images or attachments.
- Breaking through Focus modes. The user adds the app to a Focus's allow-list
  in System Settings. The "time-sensitive" interruption level that bypasses
  Focus needs an entitlement issued through Apple's provisioning, which an
  ad-hoc signed app cannot carry; macOS downgrades the request silently.
- Suppressing the notification when the terminal is frontmost. The user
  explicitly wants it shown in every case.
- Linux or Windows support.
- Homebrew formula, notarization, Developer ID signing. This is a personal
  tool installed from source.
- Filtering which hook events deserve a notification. That is the job of the
  hook `matcher` in `settings.json`; the tool posts whatever it is given.

## 4. Anti-goals (things that would make this a failure)

- Exiting 0 when nothing was posted.
- Exiting 2 from a hook. On a `Stop` hook exit 2 blocks Claude from stopping.
- Blocking a hook for longer than a few seconds under any circumstance.
- Depending on the deprecated `NSUserNotification` API or on bundle-id
  swizzling tricks (`mac-notification-sys`, `notify-rust`, old
  `terminal-notifier`).

## 5. Constraints and facts the design rests on

### 5.1 macOS

Everything in this list was verified on the development machine; details and
measurements are in section 10.

- The modern `UserNotifications` framework (`UNUserNotificationCenter`) only
  works for a process whose executable lives inside an `.app` bundle with a
  `CFBundleIdentifier`. A bare binary raises an Objective-C exception
  ("bundleProxyForCurrentProcess is nil"). Therefore the CLI binary ships
  inside a minimal bundle and is reached through a two-line shell wrapper on
  PATH that `exec`s the binary by its bundle path. A plain symlink does not
  work: `NSBundle.mainBundle` resolves the bundle from the path the process
  was invoked by, not from the symlink target.
- An ad-hoc signed bundle in `~/Applications` gets the permission dialog and
  delivers notifications. No Developer ID is needed.
- The permission is keyed on the bundle id, so rebuilding, re-signing and
  reinstalling keeps it.
- First use of a new bundle id requires the system permission dialog. A
  dialog that is still open when the requesting process exits is recorded as
  declined; the only way back is System Settings > Notifications.
- Posting is asynchronous. The process must wait for the completion handler
  of `addNotificationRequest` before exiting. Completion handlers fire on a
  background queue without a run loop, so a channel wait with a timeout is
  enough.
- Notifications are system-level; the focus state of the terminal does not
  matter. Focus modes (Do Not Disturb, Sleep, ...) still mute them: the
  notification lands in Notification Center history without a banner or
  sound. That is user configuration (Focus allow-list).
- Notification Center shows the bundle's `CFBundleName` as the header and
  the bundle's icon on the left of every banner. There is no public way to
  change the icon per notification.
- Launch Services caches icons and registers every copy of a bundle it
  encounters. Two registered copies with the same bundle id (installed copy
  and build copy) let the system resolve the id to the wrong path.
- Development machine: macOS 15.7 (Sequoia), Apple Silicon, Xcode installed.
  On arm64 the Rust linker ad-hoc signs binaries automatically; the bundle is
  re-signed ad-hoc after assembly. Minimum supported macOS is 13.

### 5.2 Claude Code hooks (verified against the docs for v2.1.292)

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

Claude Code snapshots the hook configuration when a session starts. Edits to
`settings.json` reach a running session only after the user accepts them in
`/hooks`, or in the next session.

## 6. The tool

### 6.1 Name and identity

- Cargo package and binary: `claude-hook-notify`.
- Bundle: `ClaudeHookNotify.app`, display name "Claude Hook Notify",
  bundle id `com.isegodin.claude-hook-notify`. Both are a one-line change in
  the plist template.
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
                          Show the macOS permission dialog and wait for the
                          answer (up to 120 s). First-time setup.
      --timeout <SECS>    Max wait for each macOS reply, in seconds. Default 5.
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
  which would be a blocking error if a hook was misconfigured; it is remapped
  and a test covers it. `--timeout 0` is a usage error.
- In plain CLI mode the default title is empty: the notification header
  already shows the app name, so `claude-hook-notify "hello"` shows just
  the app name and the body.

### 6.3 Hook mode mapping (`--stdin-json`)

Let `project` = last path component of `cwd` (empty if `cwd` is missing or
has no file name, such as `/`). Let `snippet(s)` = `s` with all whitespace
runs collapsed to one space, trimmed, cut to 200 characters (characters, not
bytes) with "…" appended if cut. macOS truncates visually anyway; the limit
keeps the payload sane. `snippet` applies to every body taken from the
payload.

| hook_event_name | title | subtitle | body |
|---|---|---|---|
| `Stop` | `Finished` | `project` | `snippet(last_assistant_message)`, or `Done.` if empty |
| `Notification` | payload `title` if non-empty after trimming, else by type (below) | `project` | `snippet(message)`, or empty |
| `SubagentStop` | `Subagent finished` | `project` | `agent_type` if present, else empty |
| anything else | humanized `hook_event_name` | `project` | empty |
| not valid JSON, not a JSON object, or no `hook_event_name` | `Claude Code` | empty | the raw stdin, cut to 200 chars, or `(empty hook payload)` |

Title by `notification_type` when the payload has no usable `title`:

| notification_type | title |
|---|---|
| `permission_prompt` | `Permission needed` |
| `idle_prompt` | `Waiting for input` |
| `elicitation_dialog`, `elicitation_url_dialog` | `Input needed` |
| `agent_needs_input` | `Agent needs input` |
| `agent_completed` | `Agent finished` |
| any other value | humanized type (`_` to space, first letter capitalized) |
| missing | `Claude Code` |

"Humanized" for `hook_event_name` also splits CamelCase: `PreToolUse`
becomes `Pre tool use`. For `notification_type` only `_` is replaced.

Additional behaviour in hook mode:
- `threadIdentifier` of the notification is set to `session_id` so
  notifications from one session stack together in Notification Center.
- `--title` / `--subtitle` on the command line override the derived values.
- The tool never decides to stay silent for a recognised event. Choosing
  which events notify is done with hook matchers in `settings.json`.
- A terminal on stdin counts as an empty payload (with a warning) rather
  than waiting for input.
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
     hook run never shows the dialog, because an unanswered dialog becomes a
     permanent "denied" when the process exits, and a hook runs while the
     user is looking elsewhere.
   - denied (or any other status): stderr "notifications are disabled for
     Claude Hook Notify in System Settings > Notifications", exit 1.
4. Build `UNMutableNotificationContent` (title, subtitle, body, sound,
   threadIdentifier), wrap it in a `UNNotificationRequest` with a fresh UUID
   identifier and no trigger (deliver now).
5. `addNotificationRequest:withCompletionHandler:`; wait on a channel for the
   handler, up to `--timeout`. Handler with no error: exit 0. Handler with
   error: print it, exit 1. Timeout: stderr "timed out waiting for delivery
   confirmation", exit 1.

Completion handlers arrive on a background queue; a `std::sync::mpsc`
channel with `recv_timeout` is enough, no run loop is required. Every wait
is bounded by `--timeout`, so one run takes at most twice the timeout.

`--request-permission` itself: bundle check, settings read; if already
authorized say so and exit 0; if denied print the denied message and exit 1;
otherwise request with a 120 s wait and report granted (exit 0) or not
granted (exit 1, pointing at System Settings). A `granted=false` reply with
the error "Notifications are not allowed for this application" means the
user declined; it is reported as not granted, not as a transient failure.

### 6.6 Sound

`--sound default` maps to `UNNotificationSound.defaultSound`. `--sound none`
sets no sound. Any other value is passed to `UNNotificationSound soundNamed:`;
macOS silently falls back to the default sound if the name does not exist,
which is acceptable. An empty `--sound ""` is treated as `default`.

## 7. Bundle, build and install

### 7.1 Layout

```
ClaudeHookNotify.app/
  Contents/
    Info.plist
    MacOS/
      claude-hook-notify        # the Rust release binary
    Resources/
      AppIcon.icns              # rendered from bundle/icon.svg, see 7.3
```

`Info.plist` keys: `CFBundleIdentifier`, `CFBundleName`, `CFBundleDisplayName`,
`CFBundleExecutable`, `CFBundlePackageType` = `APPL`, `CFBundleVersion`,
`CFBundleShortVersionString` (from `Cargo.toml` version), `LSUIElement` = true
(no Dock icon, no menu bar), `LSMinimumSystemVersion` = `13.0`,
`CFBundleIconFile` = `AppIcon`.

### 7.2 Build and install steps

A `Makefile` (thin wrapper around shell scripts under `scripts/`) with:

- `make icon`: render `bundle/icon.svg` into `target/icon/AppIcon.icns`
  (see 7.3).
- `make bundle`: `cargo build --release`, render the icon, assemble the bundle
  under `target/bundle/`, `codesign --force --sign - ClaudeHookNotify.app`.
- `make install`: copy the bundle to `~/Applications/ClaudeHookNotify.app`
  (replacing any previous copy), register it with Launch Services
  (`lsregister -f`), unregister the build copy under `target/bundle/`
  (`lsregister -u`; it shares the bundle id and must not be the one the system
  resolves), write the wrapper script `~/.local/bin/claude-hook-notify`
  (two lines: `#!/bin/sh` and
  `exec "<absolute bundle path>/Contents/MacOS/claude-hook-notify" "$@"`,
  mode 755), then print: the absolute wrapper path, the next step
  (`claude-hook-notify --request-permission`), and the hook JSON from 6.4.
- `make uninstall`: remove the wrapper script, unregister and remove the
  bundle. Hook entries in `settings.json` are left to the user.

Rebuilding and reinstalling must not change the bundle id, so the
notification permission granted once stays granted.

### 7.3 Icon

The app icon is the only image macOS shows in the notification banner (left
side) and next to the app in System Settings > Notifications.

Source of truth: `bundle/icon.svg`, a 1024x1024 drawing clipped to the macOS
rounded-square shape (824 px square at offset 100, corner radius 184): a
hooded monk holding a candle on a parchment background, a vector trace of a
raster illustration (the raster original is not kept in the repository). The
SVG is self-contained. Nothing rendered from it is versioned; rendered files
live under `target/`, which `.gitignore` already covers.

`scripts/icon.sh [OUT_DIR]` (default `target/icon`) renders it with tools that
ship with macOS: `sips -s format png -z N N` for the ten iconset sizes
(16, 32, 128, 256, 512 and their @2x doubles) into `AppIcon.iconset/`, then
`iconutil -c icns` to `AppIcon.icns`. `sips` reads SVG through ImageIO on
macOS 11 and later. `scripts/bundle.sh` runs it and copies the result to
`Contents/Resources/AppIcon.icns` before signing.

Launch Services caches icons, so after an install that changed the icon the
banner may keep the old one until `killall NotificationCenter` or a logout.
The permission is unaffected.

## 8. Testing strategy

Notification delivery cannot be asserted programmatically, so the design
separates the pure part from the macOS part.

- The mapping from (arguments, optional hook JSON) to a `NotificationSpec`
  struct {title, subtitle, body, sound, thread_id} is a pure function with
  unit tests covering every row of the tables in 6.3, the snippet rules
  (including multibyte text), humanization, missing fields, invalid and
  non-object JSON, and empty stdin.
- `--dry-run` serialises that struct, so integration tests run the real
  binary with fixture payloads and compare stdout, without a bundle and
  without macOS calls.
- Argument validation (mutual exclusion, missing message, `--timeout 0`) and
  the bundle guard are tested through the binary's exit code and stderr.
- The decision from an authorization status to "may post / not requested /
  denied" is a pure function with unit tests. Only the Objective-C calls
  themselves are untested by `cargo test`.
- Real delivery is checked by the manual checklist in
  [development.md](development.md).

## 9. Decisions

- Bundle id `com.isegodin.claude-hook-notify`, display name "Claude Hook Notify".
- Title/subtitle/body mapping in 6.3, including "Finished" as the Stop title
  and the project folder name as subtitle.
- Default sound: the system default, not "Glass" as in the old osascript
  hook. `--sound Glass` restores it.
- Default timeout 5 s per macOS wait; hook `timeout` 10 s in the recommended
  config.
- Install location `~/Applications`; the PATH entry is a shell wrapper in
  `~/.local/bin`, never a symlink.
- Notifications grouped per session via `threadIdentifier`.
- `SubagentStop` supported in the mapping but not wired in the recommended
  config.
- Only `--request-permission` shows the permission dialog; plain and hook
  runs report "not requested" instead (6.5).
- Icon: SVG source in `bundle/icon.svg`, rendered at build time with `sips` +
  `iconutil`; rendered files are not versioned (7.3).
- Usage errors exit 1 (clap's default of 2 is remapped).
- Empty default title in plain CLI mode.
- `snippet()` is applied to every body taken from the hook payload
  (`last_assistant_message`, `message`); it is a normalisation, not a content
  change.
- A `title` field in a `Notification` payload counts as present only when it
  is non-empty after trimming; an empty title falls back to the type table.
- `--sound ""` is treated as `default`.
- The installer unregisters the build copy from Launch Services so only the
  installed bundle is known to the system.

## 10. Verified macOS behaviour (2026-10-23)

Verified on macOS 15.7.7 with objc2 0.6.5, objc2-foundation 0.3.2,
objc2-user-notifications 0.3.2 and block2 0.6.2, first with a throwaway
bundle id and then with the real one.

| # | Question | Result |
|---|---|---|
| 1 | Works through a symlink on PATH? | **No.** `NSBundle.mainBundle().bundlePath` was `~/.local/bin` and `bundleIdentifier` nil. A two-line `sh` wrapper that `exec`s the bundle path works (bundle found, 170 ms cold, 20 ms warm). |
| 2 | Ad-hoc bundle gets the dialog and delivers? | **Yes.** `usernoted` presented the permission alert and `addNotificationRequest` completed with no error. |
| 3 | Handlers fire without a run loop? | **Yes.** `getNotificationSettings` handler in 3-5 ms, `addNotificationRequest` handler in 3 ms, waiting on `std::sync::mpsc::recv_timeout` only. No `CFRunLoop`. |
| 4 | Bare binary caught by the bundle check? | **Yes.** `bundleIdentifier` is `None` outside a bundle, exit 1, no Objective-C exception. |
| 5 | Permission survives rebuild + re-sign + reinstall? | **Yes.** The code hash changed, status stayed `authorized`, delivery worked. macOS keys the grant on the bundle id. |

Further observations on the real bundle:

- A full post (bundle check, settings read, add request, wait) takes about
  20-25 ms wall-clock through the wrapper.
- A `requestAuthorization` reply of `granted=false` with the error
  "Notifications are not allowed for this application" is what a declined
  dialog looks like.
- A permission dialog left unanswered until `--request-permission` gave up
  after 120 s was removed by macOS the moment the process exited, and the
  status changed from "not determined" to "denied". The only way back is
  System Settings > Notifications. This is why plain and hook runs never
  request permission themselves.
- With a Focus mode active, the daemon logs "Delivering" and "Presenting"
  and the tool gets a successful completion, but Notification Center logs
  `muted by DND suppression` and shows no banner. The notification is in
  Notification Center history. Adding the app to the Focus allow-list fixes
  it.
- Launch Services had registered both `~/Applications/ClaudeHookNotify.app`
  and the build copy under `target/bundle/` under the same bundle id; the
  installer now unregisters the build copy.
- `sips` on macOS 15 renders SVG (including gradients and clip paths) at the
  requested pixel size, and `iconutil` accepts the resulting iconset.
