//! Protocol and device access for the Blackmagic DaVinci Resolve Speed Editor.
//!
//! The protocol modules (`keys`, `report`, `auth`) have no dependencies and
//! never touch hardware. Device access lives in `device` and is split into a
//! read-only `Observer` (never sends anything to the device) and an
//! authenticated `Controller` (owns the device, may set LEDs and jog modes).

pub mod auth;
pub mod keys;
pub mod report;

#[cfg(feature = "device")]
pub mod device;

pub use keys::{JogMode, Key, jog_led};
pub use report::{KeyChanges, KeyTracker, Report, key_name};

/// Blackmagic Design USB vendor ID.
pub const VID: u16 = 0x1edb;
/// Speed Editor USB product ID.
pub const PID: u16 = 0xda0e;
