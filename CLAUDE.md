# claude-hook-notify

A tiny macOS CLI, written in Rust, that posts a native notification (top-right
corner, Notification Center) from anywhere: Claude Code hooks, shell scripts,
other apps. Primary purpose: tell the user when Claude Code has finished its
turn or is waiting for input, regardless of which window is focused.

## Where things are

- Design spec (source of truth for scope and decisions):
  `docs/superpowers/specs/2026-10-23-claude-hook-notify-design.md`
- Implementation plan (written after the spec is approved):
  `docs/superpowers/plans/` (does not exist until planning starts)

## Status

- 2026-10-23: brainstormed, spec written, builder check passed with no open questions. Waiting for the user to approve the spec.
- Next step after approval: `superpowers:writing-plans`, then implement with
  `superpowers:test-driven-development` (or subagent-driven development).

## Environment facts (verified 2026-10-23)

- macOS 15.7.7 (Sequoia), Apple Silicon (arm64), Xcode installed at /Applications/Xcode.app.
- Rust 1.97.1 via rustup (Homebrew). `cargo` and `rustc` on PATH.
- Claude Code 2.1.292.
- The user already has a `Notification` hook in `~/.claude/settings.json` that
  runs `osascript -e 'display notification ...'`. This tool replaces it.
- Crates confirmed on crates.io: `objc2-user-notifications` 0.3.x,
  `objc2-foundation` 0.3.x, `block2` 0.6.x, `clap` 4.x.

## Hard constraints (do not relitigate)

- macOS will not post notifications from a bare executable. The binary MUST run
  from inside an `.app` bundle with a `CFBundleIdentifier`. The CLI is symlinked
  onto PATH from inside the bundle. See the spec for the install story.
- Use the modern UserNotifications framework via the `objc2` crate family.
  Do NOT use `mac-notification-sys` or `notify-rust` (deprecated
  NSUserNotification API plus a bundle-id swizzle hack).
- In hook mode (`--stdin-json`) the process must never exit with code 2.
  Exit 2 on a `Stop` hook blocks Claude from stopping. Only 0 or 1.
- The tool must exit quickly (well under the hook timeout) and must never hang.

## Assumptions that need a spike before the plan relies on them

1. UNUserNotificationCenter works when the bundle's binary is invoked through
   a symlink on PATH (bundle found from the real executable path).
2. An ad-hoc signed bundle in `~/Applications` gets the permission prompt and
   can post notifications without a Developer ID or notarization.
3. Completion handlers fire without running the main run loop, so a channel
   wait (with timeout) is enough and the process can exit right after.

## Testing notes

- Delivery of a real notification cannot be asserted programmatically.
  Keep the JSON-to-notification mapping and argument handling as pure
  functions with unit tests, and use `--dry-run` for end-to-end checks that
  do not touch macOS APIs.
- Manual verification: the spec has a checklist. Run it before claiming done.

## Working style

- Follow the superpowers skills (brainstorming -> writing-plans -> TDD ->
  verification-before-completion). Spec first, plan second, code third.
- Keep the CLI surface exactly as the spec says; propose changes in chat
  rather than silently widening scope.
