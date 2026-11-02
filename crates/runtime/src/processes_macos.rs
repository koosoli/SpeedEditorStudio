//! Checks whether a program is running, by process name (macOS).
//!
//! UNTESTED: written without a Mac to run it on (see README, "macOS").

use std::process::Command;

/// DaVinci Resolve's main executable (DaVinci Resolve.app/Contents/MacOS/Resolve).
pub const RESOLVE_EXE: &str = "Resolve";

/// True if a process with exactly this name runs.
pub fn is_running(name: &str) -> bool {
    find_pid(name).is_some()
}

/// Process id of a running process with exactly this name.
pub fn find_pid(name: &str) -> Option<u32> {
    let output = Command::new("pgrep").args(["-x", name]).output().ok()?;
    String::from_utf8_lossy(&output.stdout).lines().next()?.trim().parse().ok()
}
