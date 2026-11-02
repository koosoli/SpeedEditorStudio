//! Pass-through to Resolve through an emulated Speed Editor is Windows-only
//! for now: it needs usbip-win2 to plug a virtual USB device in. On macOS the
//! app imitates the native keys with Resolve's shortcuts and menus instead.
//! (A macOS route would be a virtual HID device, which needs a driver
//! entitlement from Apple; see README, "macOS".)

use speed_editor::emulator::Emulator;

/// Serial number the emulated device would use (kept so the device search
/// can skip it on every platform).
pub const SERIAL: &str = "0000000000000000000000000000E001";

pub struct VirtualDevice {
    pub emulator: Emulator,
}

impl VirtualDevice {
    pub fn start() -> Result<VirtualDevice, String> {
        Err("pass-through to Resolve is only available on Windows".into())
    }
}
