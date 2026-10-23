//! End-to-end tests through the real binary. `--dry-run` makes no macOS
//! calls, and the bundle guard is what a bare `target/debug` binary hits, so
//! every test here is deterministic outside the app bundle.

use assert_cmd::Command;
use predicates::str::contains;
use predicates::str::is_match;

fn bin() -> Command {
    Command::cargo_bin("claude-hook-notify").unwrap()
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[test]
fn dry_run_stop_fixture() {
    bin().args(["--stdin-json", "--dry-run"]).write_stdin(fixture("stop.json")).assert().success()
        .stdout("{\"title\":\"Finished\",\"subtitle\":\"demo\",\"body\":\"I updated the README and ran the tests. All 12 tests pass.\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n");
}

#[test]
fn dry_run_permission_prompt_fixture() {
    bin().args(["--stdin-json", "--dry-run"]).write_stdin(fixture("notification_permission_prompt.json")).assert().success()
        .stdout("{\"title\":\"Permission needed\",\"subtitle\":\"demo\",\"body\":\"Claude needs your permission to use Bash\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n");
}

#[test]
fn dry_run_subagent_stop_fixture() {
    bin().args(["--stdin-json", "--dry-run"]).write_stdin(fixture("subagent_stop.json")).assert().success()
        .stdout("{\"title\":\"Subagent finished\",\"subtitle\":\"demo\",\"body\":\"Explore\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n");
}

#[test]
fn dry_run_unknown_event_fixture() {
    bin().args(["--stdin-json", "--dry-run"]).write_stdin(fixture("unknown_event.json")).assert().success()
        .stdout("{\"title\":\"Pre tool use\",\"subtitle\":\"demo\",\"body\":\"\",\"sound\":\"default\",\"thread_id\":\"abc123\"}\n");
}

#[test]
fn dry_run_invalid_json() {
    bin().args(["--stdin-json", "--dry-run"]).write_stdin("not json").assert().success()
        .stdout("{\"title\":\"Claude Code\",\"subtitle\":\"\",\"body\":\"not json\",\"sound\":\"default\",\"thread_id\":null}\n");
}

#[test]
fn dry_run_empty_stdin() {
    bin()
        .args(["--stdin-json", "--dry-run"])
        .write_stdin("")
        .assert()
        .success()
        .stdout(contains("\"body\":\"(empty hook payload)\""));
}

#[test]
fn dry_run_plain_message() {
    bin().args(["--dry-run", "hello"]).assert().success()
        .stdout("{\"title\":\"\",\"subtitle\":\"\",\"body\":\"hello\",\"sound\":\"default\",\"thread_id\":null}\n");
}

#[test]
fn dry_run_overrides_in_hook_mode() {
    bin()
        .args([
            "--stdin-json",
            "--dry-run",
            "-t",
            "T",
            "-s",
            "S",
            "--sound",
            "Glass",
        ])
        .write_stdin(fixture("stop.json"))
        .assert()
        .success()
        .stdout(contains("\"title\":\"T\",\"subtitle\":\"S\""))
        .stdout(contains("\"sound\":\"Glass\""));
}

#[test]
fn no_args_prints_usage_to_stderr_and_exits_1() {
    bin()
        .assert()
        .code(1)
        .stdout("")
        .stderr(contains("Usage:"))
        .stderr(contains("MESSAGE"));
}

#[test]
fn message_and_stdin_json_conflict_exits_1() {
    bin()
        .args(["--stdin-json", "hello"])
        .assert()
        .code(1)
        .stderr(contains("cannot be used with"));
}

#[test]
fn timeout_zero_is_usage_error() {
    bin().args(["--timeout", "0", "hello"]).assert().code(1);
}

#[test]
fn help_exits_0_on_stdout() {
    bin()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("--stdin-json"))
        .stdout(contains("--request-permission"));
}

#[test]
fn version_exits_0() {
    bin()
        .arg("--version")
        .assert()
        .success()
        .stdout(is_match("claude-hook-notify\\s\\d+.\\d+.\\d+").unwrap());
}

#[test]
fn posting_outside_bundle_exits_1_with_fix() {
    bin()
        .arg("hello")
        .assert()
        .code(1)
        .stdout("")
        .stderr(contains("must run from inside ClaudeHookNotify.app"));
}

#[test]
fn check_outside_bundle_exits_1() {
    bin()
        .arg("--check")
        .assert()
        .code(1)
        .stdout(contains("bundle id: none"));
}

#[test]
fn request_permission_outside_bundle_exits_1() {
    bin()
        .arg("--request-permission")
        .assert()
        .code(1)
        .stderr(contains("must run from inside"));
}
