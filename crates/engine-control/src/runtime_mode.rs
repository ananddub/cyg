//! Whether this process was started as a local development session.
//!
//! `cyg run -d` says the operator wants to see their game on their own
//! machine, right now, without going and reading a config file first. That is
//! a property of how the process was launched, not of the config on disk, so it
//! lives here as one process-wide flag rather than being threaded through every
//! layer that needs to ask.
//!
//! The flag only ever *grants* the local affordances (an unauthenticated
//! console). It never relaxes anything reachable from another host — the boot
//! guard is separate and still refuses a public bind with placeholder secrets.

use std::sync::atomic::{AtomicBool, Ordering};

/// The env var that carries the flag across crate boundaries.
///
/// `engine-api` must answer "am I in dev?" on every console request but does
/// not depend on `engine-control`, so the flag is published in the environment
/// once at startup. Reading it is a cheap lookup of an already-set variable.
pub const DEV_ENV: &str = "CYG_DEV_MODE";

static DEV: AtomicBool = AtomicBool::new(false);

/// Mark this process as a local development session. Called once, at startup.
pub fn enter_dev() {
    DEV.store(true, Ordering::Relaxed);
    // SAFETY: called from `main` before any other thread exists.
    unsafe { std::env::set_var(DEV_ENV, "1") };
}

/// Whether `-d` / `--dev` was passed.
pub fn is_dev() -> bool {
    DEV.load(Ordering::Relaxed) || std::env::var(DEV_ENV).as_deref() == Ok("1")
}
