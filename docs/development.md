# Development

## Build and test

```sh
cargo test                                   # unit tests plus --dry-run integration tests; no bundle needed
cargo clippy --all-targets -- -D warnings    # kept clean
make bundle                                  # target/bundle/ClaudeHookNotify.app, signed, not installed
make icon                                    # only the icon, into target/icon/
make install                                 # bundle + install + wrapper + printed hook config
make uninstall
```

`cargo test` runs outside the bundle: everything that touches macOS is
behind the bundle guard or bypassed by `--dry-run`. Real delivery is checked
by the manual checklist at the end of this page.

## Project layout

| Path | Responsibility |
|---|---|
| `src/main.rs` | Entry point: parse, dispatch `--check` / `--request-permission` / post, map errors to exit codes 0 and 1. |
| `src/cli.rs` | clap definition, `try_parse` wrapper, remap of clap's exit code 2 to 1. |
| `src/spec.rs` | `NotificationSpec` (title, subtitle, body, sound, thread id), `Sound`, `build_spec` for plain and hook mode, JSON for `--dry-run`. |
| `src/hook.rs` | Hook payload deserialization and the event-to-notification mapping. |
| `src/text.rs` | `snippet`, humanization, project name from `cwd`. |
| `src/macos.rs` | Everything Objective-C: bundle guard, settings read, permission request, posting. The only module that imports `objc2*` or `block2`. |
| `src/verbose.rs` | `-v` logging to stderr. |
| `tests/cli.rs`, `tests/fixtures/` | Integration tests through the built binary with `--dry-run`, plus usage errors and the bundle guard. |
| `bundle/Info.plist.template` | Plist with `__VERSION__` filled from `Cargo.toml`. |
| `bundle/icon.svg` | App icon source. |
| `scripts/icon.sh` | SVG to `AppIcon.icns`. |
| `scripts/bundle.sh` | Release build, icon, bundle assembly, ad-hoc signing. |
| `scripts/install.sh`, `scripts/uninstall.sh` | Copy to `~/Applications`, Launch Services registration, wrapper script. |
| `docs/` | This documentation. `design.md` holds the decisions. |

## Rules that keep it working

These come from verified macOS behaviour, see [design.md](design.md)
sections 5 and 10. Do not relitigate them.

- The binary must run from inside `ClaudeHookNotify.app`. The PATH entry is
  a two-line `sh` wrapper that execs the bundle path. A symlink does not work.
- Use the UserNotifications framework through the `objc2` crates. Never
  `mac-notification-sys` or `notify-rust`.
- Only `src/macos.rs` may import `objc2*` or `block2`; everything else stays
  testable with `cargo test`.
- Every wait on a macOS completion handler is a `recv_timeout` bounded by
  `--timeout`. Nothing may block forever.
- Exit codes are 0 or 1 on every path, including usage errors. Never 2.
- `--dry-run` makes no macOS calls, so it works from `target/debug`.
- Plain and hook runs never show the permission dialog; only
  `--request-permission` does.
- Rebuilds must keep the bundle id, so the permission survives.

## Bundle

```
ClaudeHookNotify.app/
  Contents/
    Info.plist                  # from bundle/Info.plist.template
    MacOS/claude-hook-notify    # the release binary
    Resources/AppIcon.icns      # from bundle/icon.svg
```

`CFBundleVersion` and `CFBundleShortVersionString` come from the `version`
in `Cargo.toml`; bump it there. The bundle is ad-hoc signed
(`codesign --force --sign -`) after assembly, resources included, so the icon
must be copied in before signing.

## Icon

`bundle/icon.svg` is the only versioned image. `scripts/icon.sh` renders it
with `sips` (SVG to PNG at 16, 32, 128, 256, 512 px and their @2x doubles)
and packs the iconset with `iconutil`; both tools ship with macOS. The output
under `target/icon/` is ignored by git.

To change the icon, replace `bundle/icon.svg` with a 1024x1024 drawing and
run `make install`. Keep the artwork inside the macOS rounded square (824 px
at offset 100, corner radius 184) and clip to it, as the current file does;
macOS does not round the corners for you. If Notification Center keeps the
old icon, run `killall NotificationCenter` or log out and in.

The current icon was traced from a raster illustration (not kept in the
repository) with [vtracer](https://github.com/visioncortex/vtracer). To trace
a new one:

```sh
cargo install vtracer
vtracer --input original.jpg --output traced.svg \
  --colormode color --hierarchical stacked --mode spline \
  --filter_speckle 8 --color_precision 6 --gradient_step 24 \
  --corner_threshold 60 --segment_length 4 --splice_threshold 45 --path_precision 2
```

The paths from `traced.svg` were then wrapped in a `clipPath` group with a
`translate(100 100) scale(0.8046875)` transform (1024 to 824). Preview any
SVG the way the build sees it with
`sips -s format png -z 256 256 bundle/icon.svg --out preview.png`.

## Manual verification checklist

Run before calling a change done; `cargo test` cannot see real delivery.

1. `make install`, then `claude-hook-notify --check` prints the bundle id and
   a status. On a fresh machine it is "not determined".
2. `claude-hook-notify --request-permission` shows the dialog; after Allow,
   `--check` prints "authorized" and exits 0.
3. `time claude-hook-notify "hello"` with another app frontmost shows a
   banner under "Claude Hook Notify", exits 0, well under one second.
4. `claude-hook-notify --stdin-json < tests/fixtures/stop.json` shows
   "Finished / demo / I updated the README...".
5. `claude-hook-notify --stdin-json < tests/fixtures/notification_permission_prompt.json`
   shows "Permission needed / demo / Claude needs your permission to use Bash".
   Both are grouped under the same thread in Notification Center.
6. `--sound none` is silent; `--sound Glass` plays Glass.
7. `target/release/claude-hook-notify "hello"` fails with the "must run from
   inside ClaudeHookNotify.app" message and exit 1.
8. With notifications turned off in System Settings, a run exits 1 with the
   "disabled in System Settings" message; turned back on, `--check` exits 0.
9. `make install && claude-hook-notify --check` still reports "authorized".
10. Wired into `~/.claude/settings.json`, a Claude Code turn produces the Stop
    notification and a permission prompt produces the Notification one
    (`claude --debug` shows the hook ran with exit 0). Remember that a running
    session needs `/hooks` to pick up edited configuration.
