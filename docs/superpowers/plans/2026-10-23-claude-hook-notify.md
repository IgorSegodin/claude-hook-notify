# claude-hook-notify Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Rust macOS CLI, shipped inside a minimal `.app` bundle, that posts a native notification from a plain command line or from a Claude Code hook's stdin JSON, exiting 0 only when macOS confirmed delivery.

**Architecture:** Pure Rust modules turn (arguments, optional hook JSON) into a `NotificationSpec`; only `macos.rs` touches Objective-C through the `objc2` crates and waits on `mpsc` channels for completion handlers (no run loop; verified by the spike). A shell script assembles and ad-hoc signs `ClaudeHookNotify.app`; `make install` copies it to `~/Applications` and writes a two-line `sh` wrapper to `~/.local/bin` (a symlink does not work, spec section 12).

**Tech Stack:** Rust 1.97 (edition 2024), clap 4 (derive), serde + serde_json, objc2 0.6, objc2-foundation 0.3, objc2-user-notifications 0.3, block2 0.6; dev: assert_cmd 2, predicates 3. POSIX `sh` scripts, `Makefile`, `codesign`, `lsregister`.

**Spec:** `docs/superpowers/specs/2026-10-23-claude-hook-notify-design.md` (read sections 6, 7, 9 and 12 before any task).

## Global Constraints

- Package and binary name `claude-hook-notify`, version `0.1.0`. Bundle `ClaudeHookNotify.app`, bundle id `com.isegodin.claude-hook-notify`, `CFBundleName` and `CFBundleDisplayName` both `Claude Hook Notify`, `LSUIElement` true, `LSMinimumSystemVersion` `13.0`.
- Process exit code is 0 or 1 on every path, including clap usage errors (clap's default 2 is remapped). Never 2.
- Every wait on macOS has a timeout: `--timeout` (default 5 s) for settings, authorization in normal mode and delivery; 120 s for `--request-permission`.
- stdout carries only `--dry-run` JSON and `--check` output. All other human-readable text goes to stderr. Error messages name the fix.
- `snippet()` limit is 200 characters (chars, not bytes) plus `…` (U+2026) when cut.
- Default `--sound` is `default`; `none` means no sound; anything else is a system sound name.
- Only `src/macos.rs` may import `objc2*` or `block2`. Everything else is testable with `cargo test` outside a bundle.
- `--dry-run` must make no macOS calls so it works from `target/debug`.
- The PATH entry is a wrapper script, never a symlink. Rebuild and reinstall must keep the bundle id unchanged.
- Hook JSON field names and the title/subtitle/body tables are spec section 6.3, with the clarifications in spec section 11 (snippet applies to every payload-derived body; empty payload `title` counts as absent; CamelCase event names are split; `--sound ""` means default).

## Review Focus

1. Multibyte text (accents, emoji, CJK) in `last_assistant_message` longer than 200 characters: cut on a char boundary, never panic. Test `snippet_cuts_on_char_boundary` in Task 1.
2. stdin that is valid JSON but not an object (`[1,2]`, `"text"`, `42`): fallback row with the raw text as body, no panic. Test `non_object_json_falls_back` in Task 2.
3. `--timeout 0` or a non-number: usage error, exit 1, never an infinite wait. Tests `timeout_zero_is_usage_error` in Task 5.
4. `--sound ""`: default sound, not a `soundNamed("")` call. Test `sound_empty_is_default` in Task 3.
5. `cwd` of `/`, empty, or missing: empty subtitle, no panic. Tests `project_name_root_is_empty` and `project_name_missing_is_empty` in Task 1.

---

## File structure

| Path | Responsibility |
|---|---|
| `Cargo.toml`, `Cargo.lock`, `.gitignore` | Package, pinned deps, ignore `target/` |
| `src/main.rs` | Module list, `run() -> i32`, dispatch, stdin read, exit codes |
| `src/cli.rs` | clap `Cli` struct, `parse_from`, `exit_code_for` |
| `src/text.rs` | Pure helpers: `snippet`, `humanize_type`, `humanize_event`, `project_name` |
| `src/hook.rs` | `HookPayload` (serde), `Derived`, `derive(stdin) -> Derived` (spec 6.3 tables) |
| `src/spec.rs` | `Sound`, `NotificationSpec` (Serialize), `Inputs`, `build_spec`, `to_json` |
| `src/verbose.rs` | Process-wide verbose flag and `log()` to stderr |
| `src/macos.rs` | `AuthStatus`, `MacError`, bundle check, settings, authorization, post |
| `tests/cli.rs` + `tests/fixtures/*.json` | Integration tests through the real binary (`--dry-run`, exit codes, bundle guard) |
| `bundle/Info.plist.template` | Bundle metadata with `__VERSION__` placeholder |
| `scripts/bundle.sh`, `scripts/install.sh`, `scripts/uninstall.sh`, `Makefile` | Build, sign, install, uninstall |
| `README.md` | Install, permission, hook config, usage |

---

### Task 1: Cargo scaffold and text helpers

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/text.rs`
- Modify: `.gitignore` (append `target/`)
- Test: unit tests inside `src/text.rs`

**Interfaces:**
- Produces: `text::snippet(s: &str) -> String`, `text::humanize_type(s: &str) -> String`, `text::humanize_event(s: &str) -> String`, `text::project_name(cwd: Option<&str>) -> String`, `text::SNIPPET_MAX_CHARS: usize = 200`.

- [ ] **Step 1: Create the package**

`cargo init --name claude-hook-notify` in the repo root (edition 2024). Then add dependencies with these exact lines in `Cargo.toml`:

```toml
[dependencies]
clap = { version = "4.6", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
objc2 = "0.6"
objc2-foundation = "0.3"
objc2-user-notifications = "0.3"
block2 = "0.6"

[dev-dependencies]
assert_cmd = "2"
predicates = "3"
```

Keep the framework crates' default features (the spike built with them). `src/main.rs` for now: `mod text;` and an empty `fn main() {}`. Make sure `.gitignore` contains `target/` (`cargo init` may add `/target` itself; keep one line). Run `cargo build` and confirm it finishes.

- [ ] **Step 2: Write the failing tests in `src/text.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn snippet_collapses_whitespace_and_trims() { assert_eq!(snippet("  a \n\n b\t c  "), "a b c"); }
    #[test] fn snippet_empty_or_blank_is_empty() { assert_eq!(snippet(""), ""); assert_eq!(snippet(" \n\t"), ""); }
    #[test] fn snippet_exactly_200_is_unchanged() { let s = "x".repeat(200); assert_eq!(snippet(&s), s); }
    #[test] fn snippet_cuts_at_200_and_appends_ellipsis() {
        let out = snippet(&"x".repeat(250));
        assert_eq!(out.chars().count(), 201);
        assert!(out.ends_with('…'));
        assert!(out.starts_with(&"x".repeat(200)));
    }
    #[test] fn snippet_cuts_on_char_boundary() {
        let out = snippet(&"é".repeat(250));
        assert_eq!(out, format!("{}…", "é".repeat(200)));
    }
    #[test] fn humanize_type_replaces_underscores_and_capitalizes() {
        assert_eq!(humanize_type("permission_prompt"), "Permission prompt");
        assert_eq!(humanize_type("quota_auto_resume_fired"), "Quota auto resume fired");
        assert_eq!(humanize_type(""), "");
    }
    #[test] fn humanize_event_splits_camel_case() {
        assert_eq!(humanize_event("PreToolUse"), "Pre tool use");
        assert_eq!(humanize_event("Stop"), "Stop");
        assert_eq!(humanize_event("SessionStart"), "Session start");
        assert_eq!(humanize_event("user_prompt"), "User prompt");
    }
    #[test] fn project_name_is_last_component() {
        assert_eq!(project_name(Some("/Users/me/projects/demo")), "demo");
        assert_eq!(project_name(Some("/Users/me/projects/demo/")), "demo");
    }
    #[test] fn project_name_root_is_empty() { assert_eq!(project_name(Some("/")), ""); assert_eq!(project_name(Some("")), ""); }
    #[test] fn project_name_missing_is_empty() { assert_eq!(project_name(None), ""); }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test text::`
Expected: compile error, functions not defined.

- [ ] **Step 4: Implement the four functions in `src/text.rs`**

- `pub const SNIPPET_MAX_CHARS: usize = 200;`
- `pub fn snippet(s: &str) -> String`: join `split_whitespace()` with one space; if `chars().count() > SNIPPET_MAX_CHARS`, take the first 200 chars and push `'…'`.
- `pub fn humanize_type(s: &str) -> String`: replace `_` with space, uppercase the first char, leave the rest as is.
- `pub fn humanize_event(s: &str) -> String`: insert a space before every uppercase letter that follows a lowercase letter or digit, lowercase everything, then apply `humanize_type`.
- `pub fn project_name(cwd: Option<&str>) -> String`: `Path::new(cwd).file_name()` as a String, else empty.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test text::`
Expected: 10 passed.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/main.rs src/text.rs
git commit -m "feat: scaffold crate and add text helpers"
```

---

### Task 2: Hook payload derivation

**Files:**
- Create: `src/hook.rs`
- Modify: `src/main.rs` (add `mod hook;`)
- Test: unit tests inside `src/hook.rs`

**Interfaces:**
- Consumes: `text::{snippet, humanize_type, humanize_event, project_name}`.
- Produces: `hook::Derived { pub title: String, pub subtitle: String, pub body: String, pub thread_id: Option<String> }` and `hook::derive(stdin: &str) -> Derived`.

- [ ] **Step 1: Write the failing tests in `src/hook.rs`**

Use a helper `fn d(json: &str) -> Derived { derive(json) }` and the fixture base `{"session_id":"abc123","cwd":"/Users/me/projects/demo"}` merged into each case.

```rust
#[test] fn stop_uses_finished_project_and_snippet() {
    let r = d(r#"{"session_id":"abc123","cwd":"/Users/me/projects/demo","hook_event_name":"Stop","last_assistant_message":"I updated the README.\n\nAll 12 tests pass."}"#);
    assert_eq!((r.title.as_str(), r.subtitle.as_str(), r.body.as_str()), ("Finished", "demo", "I updated the README. All 12 tests pass."));
    assert_eq!(r.thread_id.as_deref(), Some("abc123"));
}
#[test] fn stop_without_message_says_done() {
    for payload in [r#"{"hook_event_name":"Stop"}"#, r#"{"hook_event_name":"Stop","last_assistant_message":""}"#, r#"{"hook_event_name":"Stop","last_assistant_message":" \n "}"#] {
        assert_eq!(d(payload).body, "Done.");
    }
}
#[test] fn notification_uses_payload_title_when_non_empty() {
    let r = d(r#"{"cwd":"/x/demo","hook_event_name":"Notification","title":"Custom","message":"hi","notification_type":"permission_prompt"}"#);
    assert_eq!((r.title.as_str(), r.subtitle.as_str(), r.body.as_str()), ("Custom", "demo", "hi"));
}
#[test] fn notification_empty_title_falls_back_to_type() {
    let r = d(r#"{"hook_event_name":"Notification","title":"  ","notification_type":"permission_prompt"}"#);
    assert_eq!(r.title, "Permission needed");
}
#[test] fn notification_titles_by_type() {
    let cases = [
        ("permission_prompt", "Permission needed"), ("idle_prompt", "Waiting for input"),
        ("elicitation_dialog", "Input needed"), ("elicitation_url_dialog", "Input needed"),
        ("agent_needs_input", "Agent needs input"), ("agent_completed", "Agent finished"),
        ("plugin_notification", "Plugin notification"), ("quota_auto_resume_fired", "Quota auto resume fired"),
    ];
    for (t, want) in cases {
        assert_eq!(d(&format!(r#"{{"hook_event_name":"Notification","notification_type":"{t}"}}"#)).title, want, "{t}");
    }
}
#[test] fn notification_without_type_is_claude_code() { assert_eq!(d(r#"{"hook_event_name":"Notification"}"#).title, "Claude Code"); }
#[test] fn notification_body_is_message_or_empty() {
    assert_eq!(d(r#"{"hook_event_name":"Notification","message":"Claude needs your permission to use Bash"}"#).body, "Claude needs your permission to use Bash");
    assert_eq!(d(r#"{"hook_event_name":"Notification"}"#).body, "");
}
#[test] fn subagent_stop_body_is_agent_type() {
    let r = d(r#"{"cwd":"/x/demo","hook_event_name":"SubagentStop","agent_type":"Explore"}"#);
    assert_eq!((r.title.as_str(), r.subtitle.as_str(), r.body.as_str()), ("Subagent finished", "demo", "Explore"));
    assert_eq!(d(r#"{"hook_event_name":"SubagentStop"}"#).body, "");
}
#[test] fn unknown_event_is_humanized_with_empty_body() {
    let r = d(r#"{"cwd":"/x/demo","hook_event_name":"PreToolUse","tool_name":"Bash"}"#);
    assert_eq!((r.title.as_str(), r.subtitle.as_str(), r.body.as_str()), ("Pre tool use", "demo", ""));
}
#[test] fn missing_cwd_gives_empty_subtitle() { assert_eq!(d(r#"{"hook_event_name":"Stop"}"#).subtitle, ""); }
#[test] fn invalid_json_falls_back_to_raw_body() {
    let r = d("not json at all");
    assert_eq!((r.title.as_str(), r.subtitle.as_str(), r.body.as_str()), ("Claude Code", "", "not json at all"));
    assert_eq!(r.thread_id, None);
}
#[test] fn non_object_json_falls_back() {
    for raw in ["[1,2]", "\"text\"", "42"] { let r = d(raw); assert_eq!(r.title, "Claude Code"); assert_eq!(r.body, raw); }
}
#[test] fn object_without_event_name_falls_back_but_keeps_session() {
    let r = d(r#"{"session_id":"abc123","cwd":"/x/demo"}"#);
    assert_eq!((r.title.as_str(), r.subtitle.as_str()), ("Claude Code", ""));
    assert_eq!(r.body, r#"{"session_id":"abc123","cwd":"/x/demo"}"#);
    assert_eq!(r.thread_id.as_deref(), Some("abc123"));
}
#[test] fn empty_event_name_falls_back() { assert_eq!(d(r#"{"hook_event_name":""}"#).title, "Claude Code"); }
#[test] fn empty_stdin_has_placeholder_body() {
    for raw in ["", "  \n"] { let r = d(raw); assert_eq!((r.title.as_str(), r.body.as_str()), ("Claude Code", "(empty hook payload)")); }
}
#[test] fn long_raw_stdin_is_cut() { let r = d(&"y".repeat(300)); assert_eq!(r.body.chars().count(), 201); assert!(r.body.ends_with('…')); }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test hook::`
Expected: compile error, `derive`/`Derived` not defined.

- [ ] **Step 3: Implement `src/hook.rs`**

- `#[derive(Deserialize)] struct HookPayload` with `Option<String>` fields `hook_event_name, session_id, cwd, last_assistant_message, message, title, notification_type, agent_type` (serde ignores unknown fields; wrong-typed fields make the whole parse fail, which lands in the fallback row).
- `pub struct Derived { .. }` as in Interfaces.
- `pub fn derive(stdin: &str) -> Derived`: parse with `serde_json::from_str::<HookPayload>(stdin).ok()`. Event = `hook_event_name` filtered non-empty. With an event: `subtitle = project_name(cwd)`, `thread_id = session_id`, then match `"Stop"` / `"Notification"` / `"SubagentStop"` / other per spec 6.3, bodies through `snippet`. Without an event: title `Claude Code`, subtitle empty, body `snippet(stdin)` or `(empty hook payload)` when that is empty, `thread_id` = `session_id` if the object parsed.
- `fn title_for_type(t: Option<&str>) -> String` implements the second table in 6.3 (`None` gives `Claude Code`, unlisted values go through `humanize_type`).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test hook::`
Expected: 16 passed.

- [ ] **Step 5: Commit**

```bash
git add src/hook.rs src/main.rs
git commit -m "feat: derive notification fields from Claude Code hook JSON"
```

---

### Task 3: NotificationSpec and the pure build step

**Files:**
- Create: `src/spec.rs`
- Modify: `src/main.rs` (add `mod spec;`)
- Test: unit tests inside `src/spec.rs`

**Interfaces:**
- Consumes: `hook::derive`.
- Produces:
  - `spec::Sound { Default, None, Named(String) }` with `Sound::parse(s: &str) -> Sound`, `Sound::as_str(&self) -> &str`, and `impl Serialize` that emits `as_str()`.
  - `spec::NotificationSpec { pub title: String, pub subtitle: String, pub body: String, pub sound: Sound, pub thread_id: Option<String> }` deriving `Serialize, Debug, PartialEq`, field order exactly as written; `NotificationSpec::to_json(&self) -> String` (compact, one line, no trailing newline).
  - `spec::Inputs<'a> { pub message: Option<&'a str>, pub title: Option<&'a str>, pub subtitle: Option<&'a str>, pub sound: &'a str, pub stdin_json: Option<&'a str> }` where `stdin_json = Some(raw stdin)` means hook mode.
  - `spec::build_spec(inputs: &Inputs) -> NotificationSpec`.

- [ ] **Step 1: Write the failing tests in `src/spec.rs`**

```rust
#[test] fn sound_parse_keywords_are_case_insensitive() {
    assert_eq!(Sound::parse("default"), Sound::Default); assert_eq!(Sound::parse("DEFAULT"), Sound::Default);
    assert_eq!(Sound::parse("none"), Sound::None); assert_eq!(Sound::parse("None"), Sound::None);
    assert_eq!(Sound::parse("Glass"), Sound::Named("Glass".into()));
}
#[test] fn sound_empty_is_default() { assert_eq!(Sound::parse(""), Sound::Default); }
#[test] fn plain_mode_has_empty_title_and_no_thread() {
    let s = build_spec(&Inputs { message: Some("hello"), title: None, subtitle: None, sound: "default", stdin_json: None });
    assert_eq!(s, NotificationSpec { title: "".into(), subtitle: "".into(), body: "hello".into(), sound: Sound::Default, thread_id: None });
}
#[test] fn plain_mode_uses_title_subtitle_and_sound() {
    let s = build_spec(&Inputs { message: Some("b"), title: Some("T"), subtitle: Some("S"), sound: "Glass", stdin_json: None });
    assert_eq!((s.title.as_str(), s.subtitle.as_str(), s.sound.as_str()), ("T", "S", "Glass"));
}
#[test] fn hook_mode_derives_from_json() {
    let s = build_spec(&Inputs { message: None, title: None, subtitle: None, sound: "none", stdin_json: Some(r#"{"session_id":"abc","cwd":"/p/demo","hook_event_name":"Stop","last_assistant_message":"ok"}"#) });
    assert_eq!(s, NotificationSpec { title: "Finished".into(), subtitle: "demo".into(), body: "ok".into(), sound: Sound::None, thread_id: Some("abc".into()) });
}
#[test] fn hook_mode_cli_overrides_win() {
    let s = build_spec(&Inputs { message: None, title: Some("T"), subtitle: Some("S"), sound: "default", stdin_json: Some(r#"{"cwd":"/p/demo","hook_event_name":"Stop"}"#) });
    assert_eq!((s.title.as_str(), s.subtitle.as_str(), s.body.as_str()), ("T", "S", "Done."));
}
#[test] fn to_json_shape_and_key_order() {
    let s = build_spec(&Inputs { message: Some("hello"), title: None, subtitle: None, sound: "default", stdin_json: None });
    assert_eq!(s.to_json(), r#"{"title":"","subtitle":"","body":"hello","sound":"default","thread_id":null}"#);
    let n = NotificationSpec { title: "a".into(), subtitle: "b".into(), body: "c".into(), sound: Sound::Named("Glass".into()), thread_id: Some("t".into()) };
    assert_eq!(n.to_json(), r#"{"title":"a","subtitle":"b","body":"c","sound":"Glass","thread_id":"t"}"#);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test spec::`
Expected: compile error.

- [ ] **Step 3: Implement `src/spec.rs`**

`Sound::parse` compares `eq_ignore_ascii_case` for `default`/`none`, empty string is `Default`. `build_spec`: in plain mode body is `message.unwrap_or("")`, title/subtitle are the overrides or empty, `thread_id` None; in hook mode call `hook::derive` and let `title`/`subtitle` overrides replace the derived values. `to_json` is `serde_json::to_string(self).expect("spec is serializable")`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test spec::`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add src/spec.rs src/main.rs
git commit -m "feat: add NotificationSpec with sound parsing and JSON output"
```

---

### Task 4: macOS layer

**Files:**
- Create: `src/macos.rs`, `src/verbose.rs`
- Modify: `src/main.rs` (add `mod macos; mod verbose;`)
- Test: none possible outside a bundle; verified by `cargo build`, `cargo clippy`, and the Task 5 bundle-guard integration test, then the Task 7 checklist.

**Interfaces:**
- Consumes: `spec::{NotificationSpec, Sound}`.
- Produces:
  - `verbose::set(on: bool)`, `verbose::log(msg: impl std::fmt::Display)` printing `[claude-hook-notify] {msg}` to stderr only when on.
  - `macos::AuthStatus { NotDetermined, Denied, Authorized, Provisional, Ephemeral, Unknown(i64) }` with `fn can_post(&self) -> bool` (true for `Authorized` and `Provisional`) and `fn label(&self) -> &'static str` returning `not determined`, `denied`, `authorized`, `provisional`, `ephemeral`, `unknown`.
  - `macos::MacError { NotInBundle, Timeout(&'static str), Denied, NotGranted, Delivery(String) }` implementing `Display` with these messages:
    - `NotInBundle`: `claude-hook-notify must run from inside ClaudeHookNotify.app; run \`make install\` and call ~/.local/bin/claude-hook-notify`
    - `Timeout(what)`: `timed out waiting for {what}`
    - `Denied`: `notifications are disabled for Claude Hook Notify in System Settings > Notifications; enable them there and retry`
    - `NotGranted`: `notification permission was not granted; enable Claude Hook Notify in System Settings > Notifications and retry`
    - `Delivery(e)`: `macOS refused the notification: {e}`
  - `macos::bundle_identifier() -> Option<String>`
  - `macos::require_bundle() -> Result<String, MacError>` (`NotInBundle` when `None`)
  - `macos::authorization_status(timeout: Duration) -> Result<AuthStatus, MacError>` (`Timeout("notification settings")`)
  - `macos::request_authorization(timeout: Duration) -> Result<bool, MacError>` (`Ok(granted)`; an error object from the handler is logged via `verbose::log` and treated as `Ok(false)`; `Timeout("the permission dialog")`)
  - `macos::ensure_authorized(timeout: Duration) -> Result<(), MacError>`: `require_bundle`, then status: `can_post` → `Ok`; `NotDetermined` → `request_authorization(timeout)`, `true` → `Ok`, `false` → `Err(NotGranted)`; anything else → `Err(Denied)`.
  - `macos::post(spec: &NotificationSpec, timeout: Duration) -> Result<(), MacError>` (`Delivery(localizedDescription)` on handler error, `Timeout("delivery confirmation")`).

- [ ] **Step 1: Create `src/verbose.rs`**

A `static VERBOSE: AtomicBool`, `set`, `log` as in Interfaces.

- [ ] **Step 2: Implement `src/macos.rs`**

The spike-verified pattern for every asynchronous call (handlers run on a background queue, no run loop needed):

```rust
use block2::RcBlock;
use objc2::runtime::Bool;
use objc2_foundation::{NSBundle, NSError, NSString, NSUUID};
use objc2_user_notifications::{UNAuthorizationOptions, UNAuthorizationStatus, UNMutableNotificationContent,
    UNNotificationRequest, UNNotificationSettings, UNNotificationSound, UNUserNotificationCenter};
use std::{ptr::NonNull, sync::mpsc, time::Duration};

// settings
let (tx, rx) = mpsc::channel::<UNAuthorizationStatus>();
let block = RcBlock::new(move |s: NonNull<UNNotificationSettings>| { let _ = tx.send(unsafe { s.as_ref() }.authorizationStatus()); });
center.getNotificationSettingsWithCompletionHandler(&block);
rx.recv_timeout(timeout) // Err => MacError::Timeout("notification settings")

// authorization
let block = RcBlock::new(move |granted: Bool, err: *mut NSError| { /* send (granted.as_bool(), err description if !err.is_null()) */ });
center.requestAuthorizationWithOptions_completionHandler(UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound, &block);

// post
let content = UNMutableNotificationContent::new();
content.setTitle(&NSString::from_str(&spec.title)); // same for subtitle, body
match &spec.sound { Sound::Default => content.setSound(Some(&UNNotificationSound::defaultSound())),
                    Sound::None => content.setSound(None),
                    Sound::Named(n) => content.setSound(Some(&UNNotificationSound::soundNamed(&NSString::from_str(n)))) }
if let Some(t) = &spec.thread_id { content.setThreadIdentifier(&NSString::from_str(t)); }
let request = UNNotificationRequest::requestWithIdentifier_content_trigger(&NSUUID::UUID().UUIDString(), &content, None);
let block = RcBlock::new(move |err: *mut NSError| { /* send None or Some(localizedDescription) */ });
center.addNotificationRequest_withCompletionHandler(&request, Some(&block));
```

`bundle_identifier()` is `NSBundle::mainBundle().bundleIdentifier().map(|s| s.to_string())`. Map `UNAuthorizationStatus` constants to `AuthStatus` by comparing with `UNAuthorizationStatus::NotDetermined` etc.; unknown values become `Unknown(raw.0 as i64)`. `verbose::log` the bundle id, the status label and each handler's elapsed time.

- [ ] **Step 3: Build and lint**

Run: `cargo build && cargo clippy --all-targets -- -D warnings`
Expected: builds, no warnings. (Unused-function warnings are expected until Task 5 wires main; silence them for this commit with `#![allow(dead_code)]` at the top of `macos.rs` and remove it in Task 5.)

- [ ] **Step 4: Commit**

```bash
git add src/macos.rs src/verbose.rs src/main.rs
git commit -m "feat: add UserNotifications layer with channel-based waits"
```

---

### Task 5: CLI parsing, dispatch and integration tests

**Files:**
- Create: `src/cli.rs`, `tests/cli.rs`, `tests/fixtures/stop.json`, `tests/fixtures/notification_permission_prompt.json`, `tests/fixtures/subagent_stop.json`, `tests/fixtures/unknown_event.json`
- Modify: `src/main.rs` (full dispatch), `src/macos.rs` (remove the `allow(dead_code)`)
- Test: unit tests in `src/cli.rs`; `tests/cli.rs`

**Interfaces:**
- Consumes: `spec::{build_spec, Inputs}`, `macos::*`, `verbose::set`.
- Produces: `cli::Cli` (fields `message: Option<String>, title: Option<String>, subtitle: Option<String>, sound: String, stdin_json: bool, dry_run: bool, check: bool, request_permission: bool, timeout: u64, verbose: bool`), `cli::parse_from<I, T>(args: I) -> Result<Cli, clap::Error>` where `I: IntoIterator<Item = T>, T: Into<OsString> + Clone`, and `cli::exit_code_for(err: &clap::Error) -> i32` (0 when `!err.use_stderr()`, i.e. help/version; otherwise 1).

- [ ] **Step 1: Write the fixtures**

`tests/fixtures/stop.json`:
```json
{"session_id":"abc123","transcript_path":"/Users/me/.claude/projects/demo/abc123.jsonl","cwd":"/Users/me/projects/demo","permission_mode":"default","hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"I updated the README and ran the tests.\n\nAll 12 tests pass.","background_tasks":[],"session_crons":[]}
```
`tests/fixtures/notification_permission_prompt.json`:
```json
{"session_id":"abc123","transcript_path":"/Users/me/.claude/projects/demo/abc123.jsonl","cwd":"/Users/me/projects/demo","permission_mode":"default","hook_event_name":"Notification","message":"Claude needs your permission to use Bash","notification_type":"permission_prompt"}
```
`tests/fixtures/subagent_stop.json`:
```json
{"session_id":"abc123","cwd":"/Users/me/projects/demo","permission_mode":"default","hook_event_name":"SubagentStop","agent_type":"Explore","stop_hook_active":false,"last_assistant_message":"Found it."}
```
`tests/fixtures/unknown_event.json`:
```json
{"session_id":"abc123","cwd":"/Users/me/projects/demo","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"ls"}}
```

- [ ] **Step 2: Write the failing unit tests in `src/cli.rs`**

```rust
fn p(args: &[&str]) -> Result<Cli, clap::Error> { parse_from(std::iter::once("claude-hook-notify").chain(args.iter().copied())) }
#[test] fn message_only_parses_with_defaults() { let c = p(&["hello"]).unwrap(); assert_eq!(c.message.as_deref(), Some("hello")); assert_eq!(c.sound, "default"); assert_eq!(c.timeout, 5); assert!(!c.stdin_json && !c.dry_run && !c.check && !c.request_permission && !c.verbose); }
#[test] fn no_args_is_usage_error_exit_1() { let e = p(&[]).unwrap_err(); assert!(e.use_stderr()); assert_eq!(exit_code_for(&e), 1); }
#[test] fn help_and_version_exit_0() { for a in ["--help", "--version"] { let e = p(&[a]).unwrap_err(); assert_eq!(exit_code_for(&e), 0, "{a}"); } }
#[test] fn message_conflicts_with_stdin_json() { assert_eq!(exit_code_for(&p(&["--stdin-json", "hello"]).unwrap_err()), 1); }
#[test] fn flags_that_do_not_need_a_message() { for a in ["--stdin-json", "--check", "--request-permission"] { assert!(p(&[a]).is_ok(), "{a}"); } }
#[test] fn dry_run_alone_still_needs_a_message() { assert_eq!(exit_code_for(&p(&["--dry-run"]).unwrap_err()), 1); }
#[test] fn timeout_zero_is_usage_error() { assert_eq!(exit_code_for(&p(&["--timeout", "0", "hi"]).unwrap_err()), 1); assert_eq!(exit_code_for(&p(&["--timeout", "x", "hi"]).unwrap_err()), 1); }
#[test] fn short_flags_parse() { let c = p(&["-t", "T", "-s", "S", "-v", "--sound", "Glass", "--timeout", "9", "m"]).unwrap(); assert_eq!((c.title.as_deref(), c.subtitle.as_deref(), c.sound.as_str(), c.timeout, c.verbose), (Some("T"), Some("S"), "Glass", 9, true)); }
```

- [ ] **Step 3: Write the failing integration tests in `tests/cli.rs`**

Helper: `fn bin() -> assert_cmd::Command { assert_cmd::Command::cargo_bin("claude-hook-notify").unwrap() }` and `fn fixture(name: &str) -> String` reading `tests/fixtures/{name}` relative to `env!("CARGO_MANIFEST_DIR")`. Use `predicates::str::contains`.

```rust
#[test] fn dry_run_stop_fixture() { bin().args(["--stdin-json", "--dry-run"]).write_stdin(fixture("stop.json")).assert().success()
    .stdout("{\"title\":\"Finished\",\"subtitle\":\"demo\",\"body\":\"I updated the README and ran the tests. All 12 tests pass.\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n"); }
#[test] fn dry_run_permission_prompt_fixture() { /* same shape */ .stdout("{\"title\":\"Permission needed\",\"subtitle\":\"demo\",\"body\":\"Claude needs your permission to use Bash\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n"); }
#[test] fn dry_run_subagent_stop_fixture() { .stdout("{\"title\":\"Subagent finished\",\"subtitle\":\"demo\",\"body\":\"Explore\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n"); }
#[test] fn dry_run_unknown_event_fixture() { .stdout("{\"title\":\"Pre tool use\",\"subtitle\":\"demo\",\"body\":\"\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n"); }
#[test] fn dry_run_invalid_json() { bin().args(["--stdin-json", "--dry-run"]).write_stdin("not json").assert().success()
    .stdout("{\"title\":\"Claude Code\",\"subtitle\":\"\",\"body\":\"not json\",\"sound\":\"default\",\"thread_id\":null}\n"); }
#[test] fn dry_run_empty_stdin() { .write_stdin("") ... .stdout(contains("\"body\":\"(empty hook payload)\"")); }
#[test] fn dry_run_plain_message() { bin().args(["--dry-run", "hello"]).assert().success()
    .stdout("{\"title\":\"\",\"subtitle\":\"\",\"body\":\"hello\",\"sound\":\"default\",\"thread_id\":null}\n"); }
#[test] fn dry_run_overrides_in_hook_mode() { bin().args(["--stdin-json", "--dry-run", "-t", "T", "-s", "S", "--sound", "Glass"]).write_stdin(fixture("stop.json")).assert().success()
    .stdout(contains("\"title\":\"T\",\"subtitle\":\"S\"")).stdout(contains("\"sound\":\"Glass\"")); }
#[test] fn no_args_prints_usage_to_stderr_and_exits_1() { bin().assert().code(1).stdout("").stderr(contains("Usage:")).stderr(contains("MESSAGE")); }
#[test] fn message_and_stdin_json_conflict_exits_1() { bin().args(["--stdin-json", "hello"]).assert().code(1).stderr(contains("cannot be used with")); }
#[test] fn timeout_zero_is_usage_error() { bin().args(["--timeout", "0", "hello"]).assert().code(1); }
#[test] fn help_exits_0_on_stdout() { bin().arg("--help").assert().success().stdout(contains("--stdin-json")).stdout(contains("--request-permission")); }
#[test] fn version_exits_0() { bin().arg("--version").assert().success().stdout(contains("claude-hook-notify 0.1.0")); }
#[test] fn posting_outside_bundle_exits_1_with_fix() { bin().arg("hello").assert().code(1).stdout("").stderr(contains("must run from inside ClaudeHookNotify.app")); }
#[test] fn check_outside_bundle_exits_1() { bin().arg("--check").assert().code(1).stdout(contains("bundle id: none")); }
#[test] fn request_permission_outside_bundle_exits_1() { bin().arg("--request-permission").assert().code(1).stderr(contains("must run from inside")); }
```

- [ ] **Step 4: Run the tests to verify they fail**

Run: `cargo test`
Expected: compile errors in `src/cli.rs` tests and `tests/cli.rs` (no `cli` module, binary exits 0 doing nothing).

- [ ] **Step 5: Implement `src/cli.rs`**

`#[derive(Parser)]` with `#[command(name = "claude-hook-notify", version, about = "Post a macOS notification from the command line or a Claude Code hook")]`. Attributes that matter:
- `message`: `#[arg(value_name = "MESSAGE", required_unless_present_any = ["stdin_json", "check", "request_permission"], conflicts_with = "stdin_json")]`
- `title`: `short = 't', long, value_name = "TEXT"`; `subtitle`: `short = 's', long, value_name = "TEXT"`
- `sound`: `long, value_name = "NAME", default_value = "default"`
- `timeout`: `long, value_name = "SECS", default_value_t = 5, value_parser = clap::value_parser!(u64).range(1..)`
- `verbose`: `short = 'v', long`; the four booleans are plain `long` flags.
Doc comments are the help strings from spec 6.2, copied verbatim. `parse_from` wraps `Cli::try_parse_from`; `exit_code_for` is `if err.use_stderr() { 1 } else { 0 }`.

- [ ] **Step 6: Implement dispatch in `src/main.rs`**

`fn main() { std::process::exit(run()) }` and `fn run() -> i32`:
1. `cli::parse_from(std::env::args_os())`; on `Err(e)`: `let _ = e.print(); return cli::exit_code_for(&e);`
2. `verbose::set(cli.verbose)`; `timeout = Duration::from_secs(cli.timeout)`.
3. `--check`: `require_bundle()`; on error print `bundle id: none` to stdout, the error to stderr, return 1. Otherwise print `bundle id: {id}` and `authorization: {label}` to stdout; return 0 if `can_post()` else 1 (a timeout prints the error to stderr and returns 1).
4. `--request-permission`: `require_bundle()` (on `Err(e)` print it to stderr, return 1); status with `timeout`; `can_post` → stderr `already authorized`, 0; `Denied` → stderr `MacError::Denied`, 1; otherwise `request_authorization(Duration::from_secs(120))`: `true` → stderr `permission granted`, 0; `false` → stderr `MacError::NotGranted`, 1; `Err(e)` → stderr, 1.
5. Hook mode stdin: if `cli.stdin_json`, read all of stdin with `std::io::read_to_string`; if stdin `is_terminal()` (`std::io::IsTerminal`), do not read, use `""` and print `stdin is a terminal; expected a hook JSON payload` to stderr (keeps the "never hang" rule). Read errors also give `""`.
6. `let spec = spec::build_spec(&Inputs { .. })`. `--dry-run` → `println!("{}", spec.to_json())`, return 0.
7. `macos::ensure_authorized(timeout).and_then(|_| macos::post(&spec, timeout))`; `Err(e)` → `eprintln!("claude-hook-notify: {e}")`, return 1; else 0.

Every stderr error line is prefixed `claude-hook-notify: `. Remove `#![allow(dead_code)]` from `macos.rs`.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all unit tests (text 10, hook 16, spec 7, cli 8) and 16 integration tests pass; clippy clean.

- [ ] **Step 8: Commit**

```bash
git add src/cli.rs src/main.rs src/macos.rs tests/
git commit -m "feat: CLI parsing, dispatch and integration tests"
```

---

### Task 6: Bundle, install scripts, Makefile, README

**Files:**
- Create: `bundle/Info.plist.template`, `scripts/bundle.sh`, `scripts/install.sh`, `scripts/uninstall.sh`, `Makefile`, `README.md`
- Test: shell verification steps below (no cargo tests)

**Interfaces:**
- Consumes: the release binary `target/release/claude-hook-notify`, version string from `Cargo.toml`.
- Produces: `target/bundle/ClaudeHookNotify.app` (signed), `~/Applications/ClaudeHookNotify.app`, wrapper `~/.local/bin/claude-hook-notify`.

- [ ] **Step 1: Write `bundle/Info.plist.template`**

Keys from spec 7.1 with the values in Global Constraints; `CFBundleVersion` and `CFBundleShortVersionString` are `__VERSION__`; no `CFBundleIconFile` (no icon in v1). `CFBundleName` is `Claude Hook Notify` because Notification Center shows `CFBundleName`, not the display name (observed in the spike log).

- [ ] **Step 2: Write `scripts/bundle.sh`**

`#!/bin/sh`, `set -eu`. Resolve `ROOT` from the script location. `VERSION` via `sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1`. Then: `cargo build --release`; `rm -rf target/bundle/ClaudeHookNotify.app`; `mkdir -p .../Contents/MacOS`; copy the binary; `sed "s/__VERSION__/$VERSION/g"` the template to `Contents/Info.plist`; `plutil -lint` it; `codesign --force --sign - "$APP"`; `codesign --verify --verbose=2 "$APP"`; echo the bundle path.

- [ ] **Step 3: Write `scripts/install.sh`**

`set -eu`. Fail with `run 'make bundle' first` if `target/bundle/ClaudeHookNotify.app` is missing. `DEST=$HOME/Applications/ClaudeHookNotify.app`, `WRAPPER=$HOME/.local/bin/claude-hook-notify`. `mkdir -p` both parents; `rm -rf "$DEST"`; `cp -R`; `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$DEST"`; write the wrapper with `printf '#!/bin/sh\nexec "%s" "$@"\n' "$DEST/Contents/MacOS/claude-hook-notify"`; `chmod 755`. Print: the wrapper path, `Next: $WRAPPER --request-permission`, and the hook JSON from spec 6.4 with `$WRAPPER` substituted for the command.

- [ ] **Step 4: Write `scripts/uninstall.sh`**

Remove the wrapper, `lsregister -u "$DEST"` (ignore failure), `rm -rf "$DEST"`, print what was removed.

- [ ] **Step 5: Write `Makefile`**

Targets `build` (`cargo build`), `test` (`cargo test`), `bundle` (`sh scripts/bundle.sh`), `install` (depends on `bundle`, then `sh scripts/install.sh`), `uninstall`; all `.PHONY`.

- [ ] **Step 6: Write `README.md`**

Sections: what it is (two sentences), requirements (macOS 13+, Rust), install (`make install`, then `claude-hook-notify --request-permission`), hook configuration (the JSON from spec 6.4 with `/Users/<user>` placeholder and the note about matchers), usage examples (`claude-hook-notify "hello"`, `-t`, `--sound Glass`, `--dry-run`, `--check`), exit codes (0 delivered, 1 anything else, never 2), uninstall.

- [ ] **Step 7: Verify the bundle without touching Notification Center**

Run:
```bash
make bundle
plutil -p target/bundle/ClaudeHookNotify.app/Contents/Info.plist | grep -E 'CFBundleIdentifier|CFBundleName|LSUIElement|ShortVersion'
target/bundle/ClaudeHookNotify.app/Contents/MacOS/claude-hook-notify --dry-run hello
```
Expected: `codesign --verify` prints `valid on disk` and `satisfies its Designated Requirement`; the plist shows `CFBundleIdentifier` `com.isegodin.claude-hook-notify`, `CFBundleName` `Claude Hook Notify`, `LSUIElement` `1`, `CFBundleShortVersionString` `0.1.0`; the dry-run prints the plain-message JSON. Do not run `--check` from `target/bundle` (it would register that path with Launch Services).

- [ ] **Step 8: Install and run checklist item 1**

Run: `make install && cat ~/.local/bin/claude-hook-notify && claude-hook-notify --check; echo "exit=$?"`
Expected: the wrapper's second line is `exec "/Users/<user>/Applications/ClaudeHookNotify.app/Contents/MacOS/claude-hook-notify" "$@"`; `--check` prints `bundle id: com.isegodin.claude-hook-notify` and `authorization: not determined`, exit 1.

- [ ] **Step 9: Commit**

```bash
git add bundle/ scripts/ Makefile README.md
git commit -m "feat: app bundle, install scripts and README"
```

---

### Task 7: Manual verification checklist and hook wiring

**Files:**
- Modify: `CLAUDE.md` (status), `~/.claude/settings.json` (only with the user's go-ahead)
- Test: spec section 9 checklist, items 2 to 9. Items 2, 3, 6, 8 and 9 need the user at the screen.

- [ ] **Step 1: Permission (checklist 2)**

Run: `claude-hook-notify --request-permission; echo "exit=$?"`, user clicks Allow.
Expected: `permission granted` on stderr, exit 0; `claude-hook-notify --check` now prints `authorization: authorized`, exit 0. If the dialog answer comes back as not granted (the spike saw this once), the message must tell the user to enable the app in System Settings > Notifications; after they do, `--check` exits 0.

- [ ] **Step 2: Plain message (checklist 3)**

Run: `time claude-hook-notify "hello"`. Expected: banner headed `Claude Hook Notify` with body `hello`, exit 0, under one second (spike: about 20 ms).

- [ ] **Step 3: Hook payloads (checklist 4 and 5)**

Run: `claude-hook-notify --stdin-json < tests/fixtures/stop.json` and `... < tests/fixtures/notification_permission_prompt.json`.
Expected: `Finished / demo / I updated the README...` and `Permission needed / demo / Claude needs your permission to use Bash`, both exit 0, both grouped under the same thread in Notification Center.

- [ ] **Step 4: Sounds (checklist 6)**

Run: `claude-hook-notify --sound none "silent"` then `claude-hook-notify --sound Glass "glass"`. Expected: first silent, second plays Glass, both exit 0.

- [ ] **Step 5: Outside the bundle (checklist 7)**

Run: `target/release/claude-hook-notify "hello"; echo "exit=$?"`. Expected: stderr `claude-hook-notify: claude-hook-notify must run from inside ClaudeHookNotify.app; ...`, exit 1, no notification.

- [ ] **Step 6: Denied state (checklist 8)**

User turns notifications off for Claude Hook Notify in System Settings. Run: `claude-hook-notify "x"; echo "exit=$?"`. Expected: the `Denied` message, exit 1. User turns them back on; `--check` exits 0 again.

- [ ] **Step 7: Rebuild keeps the permission**

Run: `make install && claude-hook-notify --check`. Expected: still `authorized`, exit 0 (spike result 5).

- [ ] **Step 8: Wire the hooks (checklist 9)**

Ask the user before editing `~/.claude/settings.json`. Replace the existing `osascript` `Notification` hook with the `Stop` and `Notification` entries printed by `make install` (absolute wrapper path, `timeout: 10`). Then in a Claude Code session: a finished turn shows `Finished / <project> / <snippet>`; a permission request shows `Permission needed / ...`. Confirm with `claude --debug` or the hook log that the hook exit code was 0.

- [ ] **Step 9: Record status and commit**

Update the `Status` section of `CLAUDE.md` with the date and "manual checklist passed". Commit:

```bash
git add CLAUDE.md
git commit -m "docs: record completed manual verification"
```
