# Troubleshooting

Start with:

```sh
claude-hook-notify --check          # bundle id and authorization status
claude-hook-notify -v "test"        # timings and each macOS reply on stderr
```

## Exit 0, but no banner appears

The tool reports "delivery confirmed" and exits 0, yet nothing shows.

**Cause:** a Focus mode (Do Not Disturb, Sleep, Work, ...) is active. macOS
delivers the notification to Notification Center but shows no banner and
plays no sound. Click the clock in the menu bar: the notifications are listed
there.

**Confirm it** from the system log (the lines mention the bundle id):

```sh
log show --last 5m --info --predicate 'process == "NotificationCenter"' | grep claude-hook-notify
```

A line like `muted by DND suppression` means Focus silenced it.

**Fix:** add the app to that Focus's allow-list: System Settings > Focus >
the active Focus > Allowed Notifications > Apps > plus > "Claude Hook
Notify". Each Focus has its own list.

Also check System Settings > Notifications > Claude Hook Notify: the alert
style must not be "None".

## "notification permission has not been requested yet"

**Cause:** the app has never asked for permission. Plain and hook runs never
show the dialog themselves.

**Fix:** run `claude-hook-notify --request-permission` and click Allow within
two minutes.

## "notifications are disabled for Claude Hook Notify in System Settings"

**Cause:** macOS recorded "denied" for the app. That happens when the dialog
was declined, when it was left open until `--request-permission` gave up
after two minutes, or when notifications were turned off later.

**Fix:** System Settings > Notifications > Claude Hook Notify > Allow
Notifications. No reinstall needed. `--check` then exits 0.

## "claude-hook-notify must run from inside ClaudeHookNotify.app"

**Cause:** the binary was started by a path outside the bundle, for example
`target/release/claude-hook-notify`, or through a symlink. macOS resolves the
bundle from the path the process was started with.

**Fix:** run `make install` and call `~/.local/bin/claude-hook-notify`, the
wrapper that execs the bundle path. Never replace the wrapper with a symlink.

## "command not found"

**Cause:** `~/.local/bin` is not on the PATH of that shell.

**Fix:** add it to your shell profile, or call
`~/.local/bin/claude-hook-notify` by its full path. Hooks are unaffected:
the printed hook configuration uses the absolute path.

## Nothing happens when Claude Code finishes a turn

1. Check the hook entries in `~/.claude/settings.json`: a `Stop` entry whose
   command is the absolute wrapper path with `--stdin-json`.
2. Claude Code reads hooks when a session starts. A session that was running
   when you edited the file keeps the old hooks. Open `/hooks` in that
   session and accept the change, or start a new session.
3. Test the hook path by hand:

   ```sh
   echo '{"hook_event_name":"Stop","cwd":"/tmp/demo","last_assistant_message":"done"}' \
     | ~/.local/bin/claude-hook-notify --stdin-json; echo "exit=$?"
   ```

   Exit 0 and a banner means the tool is fine and the problem is the hook
   wiring or a Focus mode (see above).
4. Hook stderr is not shown by Claude Code. Run `claude --debug` to see hook
   execution and exit codes.

## The banner shows the old or generic icon

**Cause:** Launch Services and Notification Center cache app icons. After an
install that changed the icon they may keep the old one.

**Fix:** `killall NotificationCenter` (it restarts by itself), or log out
and in. The permission is not affected.

## "timed out waiting for ..."

**Cause:** macOS did not answer within `--timeout` seconds (5 by default).
That is unusual; the normal reply takes a few milliseconds.

**Fix:** retry. If it persists, run `claude-hook-notify --check` and
`-v "test"` to see which step stalls, and try a larger `--timeout`. A
hung notification daemon is fixed by logging out and in.

## Two copies of the app are registered

**Symptom:** the Finder or Notification Center resolves the app to the
build copy under `target/bundle/` instead of `~/Applications`.

**Cause:** both copies share the bundle id, and the build copy got
registered with Launch Services (for example by browsing to it in the
Finder).

**Fix:** `make install` unregisters the build copy. To do it by hand:

```sh
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
  -u target/bundle/ClaudeHookNotify.app
```

## A hook run feels slow

Each run makes two bounded waits on macOS, so the worst case is twice
`--timeout` (10 seconds by default), always under the recommended hook
`timeout` of 10 seconds plus Claude Code's own margin. If a hook is killed by
its timeout, that is not exit 2, so Claude Code is never blocked. The normal
run takes about 25 milliseconds; anything slower points at the notification
daemon, see the timeout section above.
