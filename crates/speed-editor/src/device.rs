//! Access to a connected Speed Editor through the OS HID stack.
//!
//! Two handle types make the ownership model explicit:
//!
//! * [`Observer`] only reads input reports. It never sends feature or output
//!   reports, so it cannot disturb another program (such as DaVinci Resolve)
//!   that owns the device.
//! * [`Controller`] authenticates and may set LEDs and jog modes. Using it
//!   while Resolve runs would fight Resolve for the device.

use std::ffi::CString;
use std::fmt;
use std::time::{Duration, Instant};

use hidapi::{HidApi, HidDevice, HidError};

use crate::auth;
use crate::keys::JogMode;
use crate::{PID, VID};

#[derive(Debug)]
pub enum Error {
    Hid(HidError),
    /// The handshake returned an unexpected report.
    Handshake { step: &'static str, data: Vec<u8> },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Hid(e) => write!(f, "HID error: {e}"),
            Error::Handshake { step, data } => {
                write!(f, "authentication failed at {step}: got {}", hex(data))
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<HidError> for Error {
    fn from(e: HidError) -> Self {
        Error::Hid(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// One HID interface (top-level collection) of a connected Speed Editor.
#[derive(Clone, Debug)]
pub struct Interface {
    pub path: CString,
    pub interface_number: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub product: Option<String>,
    pub bus: String,
}

/// Lists all HID interfaces exposed by connected Speed Editors.
pub fn interfaces(api: &HidApi) -> Vec<Interface> {
    api.device_list()
        .filter(|d| d.vendor_id() == VID && d.product_id() == PID)
        .map(|d| Interface {
            path: d.path().to_owned(),
            interface_number: d.interface_number(),
            usage_page: d.usage_page(),
            usage: d.usage(),
            product: d.product_string().map(str::to_owned),
            bus: format!("{:?}", d.bus_type()),
        })
        .collect()
}

/// Read-only handle. Never writes to the device.
pub struct Observer {
    dev: HidDevice,
}

impl Observer {
    pub fn open(api: &HidApi, interface: &Interface) -> Result<Observer> {
        Ok(Observer { dev: api.open_path(&interface.path)? })
    }

    /// Reads one input report into `buf`, waiting at most `timeout`.
    /// Returns the report length, or 0 on timeout.
    pub fn read(&self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        Ok(self.dev.read_timeout(buf, timeout.as_millis() as i32)?)
    }
}

/// Authenticated handle that owns the device.
pub struct Controller {
    dev: HidDevice,
    reauth_at: Instant,
}

/// Re-authenticate this long before the device's timeout expires.
const REAUTH_MARGIN: Duration = Duration::from_secs(10);

impl Controller {
    pub fn open(api: &HidApi, interface: &Interface) -> Result<Controller> {
        let dev = api.open_path(&interface.path)?;
        let timeout = authenticate(&dev)?;
        Ok(Controller { dev, reauth_at: reauth_deadline(timeout) })
    }

    /// Repeats the handshake when the previous one is about to expire.
    /// Call this regularly; the device stops reporting once auth lapses.
    pub fn keep_alive(&mut self) -> Result<()> {
        if Instant::now() >= self.reauth_at {
            let timeout = authenticate(&self.dev)?;
            self.reauth_at = reauth_deadline(timeout);
        }
        Ok(())
    }

    pub fn read(&self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        Ok(self.dev.read_timeout(buf, timeout.as_millis() as i32)?)
    }

    /// Sets the key LEDs from a bitfield of `Key::led` bits.
    pub fn set_leds(&self, leds: u32) -> Result<()> {
        let [a, b, c, d] = leds.to_le_bytes();
        self.dev.write(&[2, a, b, c, d])?;
        Ok(())
    }

    /// Sets the JOG/SHTL/SCRL LEDs from a bitfield of `jog_led` bits.
    pub fn set_jog_leds(&self, leds: u8) -> Result<()> {
        self.dev.write(&[4, leds])?;
        Ok(())
    }

    pub fn set_jog_mode(&self, mode: JogMode) -> Result<()> {
        self.dev.write(&[3, mode as u8, 0, 0, 0, 0, 255])?;
        Ok(())
    }
}

fn reauth_deadline(timeout: Duration) -> Instant {
    Instant::now() + timeout.saturating_sub(REAUTH_MARGIN)
}

/// Runs the handshake and returns how long the authentication stays valid.
fn authenticate(dev: &HidDevice) -> Result<Duration> {
    let id = auth::REPORT_ID;

    // Reset the device's auth state machine, then fetch its challenge.
    dev.send_feature_report(&[id, 0, 0, 0, 0, 0, 0, 0, 0, 0])?;
    let data = get_feature(dev, "challenge", 0)?;
    let challenge = u64::from_le_bytes(data[2..10].try_into().unwrap());

    // The device also expects to be challenged. Its answer is not checked.
    dev.send_feature_report(&[id, 1, 0, 0, 0, 0, 0, 0, 0, 0])?;
    get_feature(dev, "device response", 2)?;

    let mut answer = [0u8; 10];
    answer[0] = id;
    answer[1] = 3;
    answer[2..].copy_from_slice(&auth::response(challenge).to_le_bytes());
    dev.send_feature_report(&answer)?;

    let data = get_feature(dev, "status", 4)?;
    let secs = u16::from_le_bytes([data[2], data[3]]);
    Ok(Duration::from_secs(secs.into()))
}

/// Reads the auth feature report and checks its step byte.
fn get_feature(dev: &HidDevice, step: &'static str, expected: u8) -> Result<[u8; 10]> {
    let mut buf = [0u8; 10];
    buf[0] = auth::REPORT_ID;
    let n = dev.get_feature_report(&mut buf)?;
    if n < 10 || buf[0] != auth::REPORT_ID || buf[1] != expected {
        return Err(Error::Handshake { step, data: buf[..n.min(10)].to_vec() });
    }
    Ok(buf)
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
