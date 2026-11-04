//! Keeps DaVinci Resolve in front while the Speed Editor drives it (macOS).
//!
//! Resolve functions are sent as keystrokes, which go to the app that has
//! keyboard focus, so before sending one the runtime makes Resolve the
//! frontmost app through the Accessibility API.
//!
//! Only partly tested on a Mac (see README, "macOS").

use crate::mac::{self, Owned};
use crate::processes;

/// What `ensure_resolve_in_front` had to do.
pub enum Focus {
    /// Resolve already had focus, or isn't running (nothing to do).
    Fine,
    /// Another app had focus; Resolve was brought back. Holds its name.
    Restored(String),
    /// Another app had focus and macOS refused to switch (Accessibility
    /// permission missing?).
    Failed(String),
}

/// Makes Resolve the frontmost app if another app has focus.
pub fn ensure_resolve_in_front() -> Focus {
    let Some(pid) = processes::find_pid(processes::RESOLVE_EXE) else {
        return Focus::Fine;
    };
    let focused = mac::focused_application();
    if focused.as_ref().and_then(mac::ax_pid) == Some(pid as i32) {
        return Focus::Fine;
    }
    let name = focused.as_ref().and_then(mac::ax_title).unwrap_or_else(|| "another app".into());
    let Some(resolve) = Owned::new(unsafe { mac::AXUIElementCreateApplication(pid as i32) }) else {
        return Focus::Failed(name);
    };
    if !mac::ax_set_true(&resolve, "AXFrontmost") {
        return Focus::Failed(name);
    }
    std::thread::sleep(std::time::Duration::from_millis(80));
    if mac::focused_application().as_ref().and_then(mac::ax_pid) == Some(pid as i32) {
        Focus::Restored(name)
    } else {
        Focus::Failed(name)
    }
}
