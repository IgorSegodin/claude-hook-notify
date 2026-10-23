# claude-hook-notify

A tiny macOS CLI, written in Rust, that posts a native notification (top-right
corner, Notification Center) from anywhere: Claude Code hooks, shell scripts,
other apps. Primary purpose: tell the user when Claude Code has finished its
turn or is waiting for input, regardless of which window is focused.

## Where things are

- Design spec (source of truth for scope and decisions):
  `docs/superpowers/specs/2026-10-23-claude-hook-notify-design.md`
- Implementation plan: `docs/superpowers/plans/2026-10-23-claude-hook-notify.md`
- Spike results: spec section 12.
- App icon source: `bundle/icon.svg`; `scripts/icon.sh` renders it at build time
  into `target/icon/` (not versioned), spec section 7.3.

## Status

- 2026-10-23: brainstormed, spec written, builder check passed with no open questions.
- 2026-10-23: spec approved (user asked to start implementation). Spike run,
  all five assumptions answered (spec section 12). Plan written.
- 2026-10-23: plan executed inline (all 7 tasks, TDD). `cargo test` passes
  (45 unit + 16 integration), `cargo clippy --all-targets -- -D warnings` is
  clean. Manual checklist passed (spec section 9, items 1 to 9): permission
  granted, real notifications delivered in about 25 ms, permission survives
  `make install`, hooks wired into `~/.claude/settings.json`.
- 2026-10-23: app icon added (Task 8): SVG source, rendered during `make bundle`.
- Nothing is committed yet (user directive: do not commit).

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
  from inside an `.app` bundle with a `CFBundleIdentifier`. The PATH entry
  `~/.local/bin/claude-hook-notify` is a two-line `sh` wrapper that `exec`s the
  binary by its bundle path. A plain symlink does NOT work (verified: NSBundle
  resolves the bundle from the invoked path, so the bundle id is nil).
- Use the modern UserNotifications framework via the `objc2` crate family.
  Do NOT use `mac-notification-sys` or `notify-rust` (deprecated
  NSUserNotification API plus a bundle-id swizzle hack).
- In hook mode (`--stdin-json`) the process must never exit with code 2.
  Exit 2 on a `Stop` hook blocks Claude from stopping. Only 0 or 1.
- The tool must exit quickly (well under the hook timeout) and must never hang.

## Spike results (verified 2026-10-23, details in spec section 12)

1. Symlink on PATH: FAILS (bundle id nil). Shell wrapper that execs the
   bundle path: works.
2. Ad-hoc signed bundle in `~/Applications`: permission dialog appears and
   notifications are delivered. No Developer ID needed.
3. Completion handlers fire without a run loop; `mpsc::recv_timeout` is enough
   (3-5 ms per handler, ~20 ms for a whole post).
4. Outside a bundle `NSBundle.mainBundle().bundleIdentifier()` is `None`; the
   guard exits 1 before any UserNotifications call, no exception.
5. Permission survives rebuild + ad-hoc re-sign + reinstall (keyed on bundle
   id, not code hash).
6. Crate API facts: `block2::RcBlock::new(closure)` passes as
   `&DynBlock<dyn Fn(..)>`; error handlers receive `*mut NSError` (null = ok);
   settings handler receives `NonNull<UNNotificationSettings>`.

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
