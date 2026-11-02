//! The emulated Speed Editor that hands native keys and the wheel to Resolve
//! (pass-through, docs/FINDINGS.md "Emulated Speed Editor").
//!
//! The emulator (speed_editor::emulator) serves the device over USB/IP;
//! usbip-win2's `usbip.exe` plugs it into Windows and unplugs it again when
//! this is dropped. Without usbip-win2 installed, `start` fails and the app
//! imitates the native keys with Resolve's shortcuts as before.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use speed_editor::emulator::{BUS_ID, Config, DEFAULT_PORT, Emulator};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Serial number of the emulated device, so the app never connects to it as
/// if it were the real one.
pub const SERIAL: &str = "0000000000000000000000000000E001";

pub struct VirtualDevice {
    pub emulator: Emulator,
    usbip: PathBuf,
    /// usbip-win2 port the device is attached to.
    port: Option<u32>,
}

impl VirtualDevice {
    /// Starts the emulator and plugs it in.
    pub fn start() -> Result<VirtualDevice, String> {
        let usbip = locate_usbip().ok_or("usbip-win2 is not installed")?;
        // A device left attached by an earlier run would point at a dead
        // emulator: unplug it first.
        for port in attached_ports(&usbip) {
            let _ = run(&usbip, &["detach", "-p", &port.to_string()]);
        }
        let config = Config { serial: SERIAL.into(), ..Config::default() };
        let emulator = Emulator::start(config, DEFAULT_PORT)
            .map_err(|e| format!("cannot serve the emulated Speed Editor on port {DEFAULT_PORT}: {e}"))?;
        let output = run(&usbip, &["attach", "-r", "127.0.0.1", "-b", BUS_ID])?;
        let port = parse_port(&output);
        if port.is_none() {
            return Err(format!("usbip could not attach the emulated Speed Editor: {}", output.trim()));
        }
        Ok(VirtualDevice { emulator, usbip, port })
    }
}

impl Drop for VirtualDevice {
    fn drop(&mut self) {
        if let Some(port) = self.port.take() {
            let _ = run(&self.usbip, &["detach", "-p", &port.to_string()]);
        }
    }
}

fn locate_usbip() -> Option<PathBuf> {
    ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|base| PathBuf::from(base).join("USBip").join("usbip.exe"))
        .find(|p| p.is_file())
}

fn run(usbip: &PathBuf, args: &[&str]) -> Result<String, String> {
    let output = Command::new(usbip)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("cannot run usbip: {e}"))?;
    Ok(String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr))
}

/// "successfully attached to port 1" → 1
fn parse_port(output: &str) -> Option<u32> {
    let rest = output.split("attached to port").nth(1)?;
    rest.trim().split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Ports with our emulated device attached (`usbip port` lists
/// "Port 01: …" followed by "-> usbip://127.0.0.1:3240/1-1").
fn attached_ports(usbip: &PathBuf) -> Vec<u32> {
    let Ok(output) = run(usbip, &["port"]) else { return Vec::new() };
    let ours = format!("usbip://127.0.0.1:{DEFAULT_PORT}/{BUS_ID}");
    let mut ports = Vec::new();
    let mut current = None;
    for line in output.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Port ") {
            current = rest.split(':').next().and_then(|n| n.trim().parse::<u32>().ok());
        } else if line.contains(&ours) {
            ports.extend(current);
        }
    }
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_usbip_output() {
        assert_eq!(parse_port("successfully attached to port 1\n"), Some(1));
        assert_eq!(parse_port("error: something"), None);
    }
}
