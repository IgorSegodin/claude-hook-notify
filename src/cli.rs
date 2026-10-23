//! Command line surface (spec section 6.2). clap's usage-error exit code of 2
//! is remapped to 1 so a misconfigured hook can never block Claude Code.

use std::ffi::OsString;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "claude-hook-notify",
    version,
    about = "Post a macOS notification from the command line or a Claude Code hook"
)]
pub struct Cli {
    /// Notification body. Required unless --stdin-json, --check or
    /// --request-permission is given.
    #[arg(
        value_name = "MESSAGE",
        required_unless_present_any = ["stdin_json", "check", "request_permission"],
        conflicts_with = "stdin_json"
    )]
    pub message: Option<String>,

    /// Title line. In hook mode overrides the derived title.
    #[arg(short = 't', long, value_name = "TEXT")]
    pub title: Option<String>,

    /// Subtitle line. In hook mode overrides the derived one.
    #[arg(short = 's', long, value_name = "TEXT")]
    pub subtitle: Option<String>,

    /// "default" (system default sound), "none", or the name of a system
    /// sound such as "Glass".
    #[arg(long, value_name = "NAME", default_value = "default")]
    pub sound: String,

    /// Hook mode: read one Claude Code hook JSON object from stdin and derive
    /// title/subtitle/body from it.
    #[arg(long)]
    pub stdin_json: bool,

    /// Print the notification that would be posted, as one JSON object on
    /// stdout, and exit 0. Makes no macOS calls, so it works outside the bundle.
    #[arg(long)]
    pub dry_run: bool,

    /// Print bundle id and authorization status to stdout. Exit 0 if the tool
    /// can post, 1 otherwise.
    #[arg(long)]
    pub check: bool,

    /// Trigger the macOS permission dialog and wait for the answer (up to
    /// 120 s). First-time setup.
    #[arg(long)]
    pub request_permission: bool,

    /// Max wait for delivery confirmation, in seconds.
    #[arg(
        long,
        value_name = "SECS",
        default_value_t = 5,
        value_parser = clap::value_parser!(u64).range(1..)
    )]
    pub timeout: u64,

    /// Diagnostics on stderr.
    #[arg(short = 'v', long)]
    pub verbose: bool,
}

/// Parses `args` (the first item is the program name) without exiting.
pub fn parse_from<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args)
}

/// 0 for `--help` / `--version` (clap prints those to stdout), 1 for every
/// usage error. Never 2.
pub fn exit_code_for(err: &clap::Error) -> i32 {
    if err.use_stderr() { 1 } else { 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Cli, clap::Error> {
        parse_from(std::iter::once("claude-hook-notify").chain(args.iter().copied()))
    }

    #[test]
    fn message_only_parses_with_defaults() {
        let c = p(&["hello"]).unwrap();
        assert_eq!(c.message.as_deref(), Some("hello"));
        assert_eq!(c.sound, "default");
        assert_eq!(c.timeout, 5);
        assert!(!c.stdin_json && !c.dry_run && !c.check && !c.request_permission && !c.verbose);
    }

    #[test]
    fn no_args_is_usage_error_exit_1() {
        let e = p(&[]).unwrap_err();
        assert!(e.use_stderr());
        assert_eq!(exit_code_for(&e), 1);
    }

    #[test]
    fn help_and_version_exit_0() {
        for a in ["--help", "--version"] {
            let e = p(&[a]).unwrap_err();
            assert_eq!(exit_code_for(&e), 0, "{a}");
        }
    }

    #[test]
    fn message_conflicts_with_stdin_json() {
        assert_eq!(
            exit_code_for(&p(&["--stdin-json", "hello"]).unwrap_err()),
            1
        );
    }

    #[test]
    fn flags_that_do_not_need_a_message() {
        for a in ["--stdin-json", "--check", "--request-permission"] {
            assert!(p(&[a]).is_ok(), "{a}");
        }
    }

    #[test]
    fn dry_run_alone_still_needs_a_message() {
        assert_eq!(exit_code_for(&p(&["--dry-run"]).unwrap_err()), 1);
    }

    #[test]
    fn timeout_zero_is_usage_error() {
        assert_eq!(exit_code_for(&p(&["--timeout", "0", "hi"]).unwrap_err()), 1);
        assert_eq!(exit_code_for(&p(&["--timeout", "x", "hi"]).unwrap_err()), 1);
    }

    #[test]
    fn short_flags_parse() {
        let c = p(&[
            "-t",
            "T",
            "-s",
            "S",
            "-v",
            "--sound",
            "Glass",
            "--timeout",
            "9",
            "m",
        ])
        .unwrap();
        assert_eq!(
            (
                c.title.as_deref(),
                c.subtitle.as_deref(),
                c.sound.as_str(),
                c.timeout,
                c.verbose
            ),
            (Some("T"), Some("S"), "Glass", 9, true)
        );
    }
}
