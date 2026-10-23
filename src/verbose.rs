//! Process-wide `--verbose` switch. Diagnostics go to stderr only when on.

use std::fmt::Display;
use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE: AtomicBool = AtomicBool::new(false);

pub fn set(on: bool) {
    VERBOSE.store(on, Ordering::Relaxed);
}

pub fn log(msg: impl Display) {
    if VERBOSE.load(Ordering::Relaxed) {
        eprintln!("[claude-hook-notify] {msg}");
    }
}
