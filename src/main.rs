//! claude-hook-notify: post a macOS notification from the command line or a
//! Claude Code hook. `main` stays thin: parse, build the spec, dispatch.
//! Every path exits 0 or 1, never 2 (spec section 6.2).

mod cli;
mod hook;
mod macos;
mod spec;
mod text;
mod verbose;

use std::io::IsTerminal;
use std::time::Duration;

use macos::{AuthStatus, MacError};

/// How long `--request-permission` waits for the user to answer the dialog.
const PERMISSION_DIALOG_WAIT: Duration = Duration::from_secs(120);

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let cli = match cli::parse_from(std::env::args_os()) {
        Ok(cli) => cli,
        Err(e) => {
            let _ = e.print();
            return cli::exit_code_for(&e);
        }
    };
    verbose::set(cli.verbose);
    let timeout = Duration::from_secs(cli.timeout);

    if cli.check {
        return cmd_check(timeout);
    }
    if cli.request_permission {
        return cmd_request_permission(timeout);
    }

    let stdin = if cli.stdin_json {
        Some(read_hook_stdin())
    } else {
        None
    };
    let spec = spec::build_spec(&spec::Inputs {
        message: cli.message.as_deref(),
        title: cli.title.as_deref(),
        subtitle: cli.subtitle.as_deref(),
        sound: &cli.sound,
        stdin_json: stdin.as_deref(),
    });
    if cli.dry_run {
        println!("{}", spec.to_json());
        return 0;
    }

    match macos::ensure_authorized(timeout).and_then(|()| macos::post(&spec, timeout)) {
        Ok(()) => 0,
        Err(e) => fail(&e),
    }
}

fn fail(e: &MacError) -> i32 {
    eprintln!("claude-hook-notify: {e}");
    1
}

fn cmd_check(timeout: Duration) -> i32 {
    let id = match macos::require_bundle() {
        Ok(id) => id,
        Err(e) => {
            println!("bundle id: none");
            return fail(&e);
        }
    };
    println!("bundle id: {id}");
    match macos::authorization_status(timeout) {
        Ok(status) => {
            println!("authorization: {}", status.label());
            if status.can_post() { 0 } else { 1 }
        }
        Err(e) => fail(&e),
    }
}

fn cmd_request_permission(timeout: Duration) -> i32 {
    if let Err(e) = macos::require_bundle() {
        return fail(&e);
    }
    let status = match macos::authorization_status(timeout) {
        Ok(status) => status,
        Err(e) => return fail(&e),
    };
    if status.can_post() {
        eprintln!("claude-hook-notify: already authorized");
        return 0;
    }
    if status == AuthStatus::Denied {
        return fail(&MacError::Denied);
    }
    match macos::request_authorization(PERMISSION_DIALOG_WAIT) {
        Ok(true) => {
            eprintln!("claude-hook-notify: permission granted");
            0
        }
        Ok(false) => fail(&MacError::NotGranted),
        Err(e) => fail(&e),
    }
}

/// Reads the whole hook payload. A terminal on stdin means nobody is piping a
/// payload; rather than block until Ctrl-D, treat it as empty and say so.
fn read_hook_stdin() -> String {
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        eprintln!("claude-hook-notify: stdin is a terminal; expected a hook JSON payload");
        return String::new();
    }
    std::io::read_to_string(stdin).unwrap_or_default()
}
