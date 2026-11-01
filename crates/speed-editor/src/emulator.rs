//! An emulated Speed Editor, served over USB/IP.
//!
//! With usbip-win2 installed, `usbip attach -r 127.0.0.1 -b 1-1` plugs it into
//! Windows, which then sees a Speed Editor with the real device's USB and HID
//! descriptors. DaVinci Resolve accepts it like the real one and runs its own
//! native functions for the key presses and wheel reports sent through it
//! (docs/FINDINGS.md, "Emulated Speed Editor").
//!
//! The emulator answers what Resolve asks a real device when adding it: the
//! firmware version (feature 1), the serial number (feature 8), the battery
//! (input 7) and the authentication (feature 6). Reports Resolve sends — key
//! LEDs, wheel mode, wheel LEDs — are collected for the caller to apply to the
//! real device.

use std::collections::VecDeque;
use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::{PID, VID, auth};

/// The bus id the emulated device is exported under.
pub const BUS_ID: &str = "1-1";
/// The USB/IP port usbip-win2 connects to by default.
pub const DEFAULT_PORT: u16 = 3240;

/// Device descriptor, copied from the real device (USBPcap).
const DEVICE_DESCRIPTOR: [u8; 18] = [
    0x12, 0x01, 0x00, 0x02, 0xef, 0x02, 0x01, 0x40, 0xdb, 0x1e, 0x0e, 0xda, 0x02, 0x01, 0x01, 0x02, 0x03, 0x01,
];
/// Configuration descriptor with its interfaces, copied from the real device:
/// a vendor interface, a DFU interface and the HID interface (endpoints 0x81
/// in, 0x01 out, 64 bytes).
const CONFIG_DESCRIPTOR: [u8; 86] = [
    0x09, 0x02, 0x56, 0x00, 0x03, 0x01, 0x00, 0x80, 0x32, // configuration
    0x08, 0x0b, 0x00, 0x02, 0xff, 0x03, 0x00, 0x02, // interface association
    0x09, 0x04, 0x00, 0x00, 0x00, 0xff, 0x03, 0x00, 0x00, // interface 0: vendor
    0x0a, 0x21, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // its class descriptor
    0x09, 0x04, 0x01, 0x00, 0x00, 0xfe, 0x01, 0x01, 0x00, // interface 1: DFU
    0x09, 0x21, 0x01, 0xe8, 0x03, 0x00, 0x02, 0x01, 0x01, // DFU functional descriptor
    0x09, 0x04, 0x02, 0x00, 0x02, 0x03, 0x00, 0x00, 0x00, // interface 2: HID
    0x09, 0x21, 0x10, 0x01, 0x00, 0x01, 0x22, 0x34, 0x01, // HID descriptor, report descriptor 308 bytes
    0x07, 0x05, 0x81, 0x03, 0x40, 0x00, 0x0a, // endpoint 0x81 interrupt in
    0x07, 0x05, 0x01, 0x03, 0x40, 0x00, 0x0a, // endpoint 0x01 interrupt out
];
/// The device's own HID report descriptor (308 bytes, captured while Windows
/// set the device up). The one Windows rebuilds from parsed data drops
/// feature reports 1 and 8, and Resolve then refuses the device.
const REPORT_DESCRIPTOR: [u8; 308] = [
    0x06, 0x00, 0xff, 0x09, 0x00, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x01, 0x05, 0x06, 0x15, 0x00, 0x26,
    0xff, 0x00, 0x09, 0x2a, 0x75, 0x08, 0x95, 0x04, 0xb1, 0x81, 0x09, 0x2d, 0x09, 0x2e, 0x09, 0x2f, 0x75, 0x08,
    0x95, 0x03, 0xb1, 0x81, 0xc0, 0x09, 0x08, 0xa1, 0x01, 0x85, 0x08, 0x06, 0x07, 0xff, 0x15, 0x00, 0x26, 0xff,
    0x00, 0x09, 0x00, 0x75, 0x08, 0x95, 0x20, 0xb1, 0x81, 0xc0, 0x09, 0x02, 0xa1, 0x01, 0x85, 0x03, 0x06, 0x01,
    0xff, 0x15, 0x00, 0x26, 0xff, 0x00, 0x09, 0x00, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, 0x09, 0x01, 0x75, 0x08,
    0x95, 0x04, 0x81, 0x02, 0x09, 0x02, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, 0x09, 0x00, 0x75, 0x08, 0x95, 0x01,
    0x91, 0x02, 0x09, 0x01, 0x75, 0x08, 0x95, 0x04, 0x91, 0x02, 0x09, 0x02, 0x75, 0x08, 0x95, 0x01, 0x91, 0x02,
    0xc0, 0x09, 0x03, 0xa1, 0x01, 0x85, 0x04, 0x06, 0x02, 0xff, 0x15, 0x00, 0x26, 0xff, 0x00, 0x19, 0x00, 0x29,
    0xff, 0x75, 0x10, 0x95, 0x06, 0x81, 0x00, 0x15, 0x00, 0x25, 0x01, 0x19, 0x01, 0x29, 0x08, 0x75, 0x01, 0x95,
    0x08, 0x91, 0x02, 0xc0, 0x09, 0x06, 0xa1, 0x01, 0x85, 0x02, 0x06, 0x05, 0xff, 0x15, 0x00, 0x25, 0x01, 0x19,
    0x01, 0x29, 0x20, 0x75, 0x01, 0x95, 0x20, 0x91, 0x02, 0xc0, 0x09, 0x04, 0xa1, 0x01, 0x85, 0x05, 0x06, 0x03,
    0xff, 0x15, 0x00, 0x26, 0xff, 0x00, 0x09, 0x00, 0x75, 0x08, 0x95, 0x01, 0xb1, 0x82, 0x09, 0x01, 0x75, 0x08,
    0x95, 0x04, 0xb1, 0x82, 0x09, 0x02, 0x75, 0x08, 0x95, 0x02, 0xb1, 0x82, 0xc0, 0x09, 0x05, 0xa1, 0x01, 0x85,
    0x06, 0x06, 0x04, 0xff, 0x15, 0x00, 0x26, 0xff, 0x00, 0x09, 0x00, 0x75, 0x08, 0x95, 0x01, 0xb1, 0x82, 0x09,
    0x01, 0x75, 0x08, 0x95, 0x08, 0xb1, 0x82, 0xc0, 0x09, 0x07, 0xa1, 0x01, 0x85, 0x07, 0x06, 0x06, 0xff, 0x15,
    0x00, 0x26, 0xff, 0x00, 0x09, 0x00, 0x75, 0x08, 0x95, 0x01, 0x81, 0x02, 0x09, 0x01, 0x75, 0x08, 0x95, 0x01,
    0x81, 0x02, 0x09, 0x00, 0x75, 0x08, 0x95, 0x01, 0xb1, 0x02, 0x09, 0x01, 0x75, 0x08, 0x95, 0x01, 0xb1, 0x02,
    0xc0, 0xc0,
];
const HID_INTERFACE: u16 = 2;
const MANUFACTURER: &str = "Blackmagic Design";
const PRODUCT: &str = "DaVinci Resolve Speed Editor";

/// Feature report 1 as the real device answers it: software version, then
/// firmware 1.5.2.
pub const DEFAULT_VERSION: [u8; 8] = [0x01, 0x4d, 0x02, 0x25, 0x7b, 0x01, 0x05, 0x02];
/// Input report 7 before the real battery is known: charging, 100 %.
const DEFAULT_BATTERY: [u8; 3] = [0x07, 0x01, 0x64];
/// Input reports waiting for the host beyond this are dropped (oldest first).
const MAX_QUEUED: usize = 64;

const USBIP_VERSION: u16 = 0x0111;
const OP_REQ_DEVLIST: u16 = 0x8005;
const OP_REP_DEVLIST: u16 = 0x0005;
const OP_REQ_IMPORT: u16 = 0x8003;
const OP_REP_IMPORT: u16 = 0x0003;
const CMD_SUBMIT: u32 = 1;
const CMD_UNLINK: u32 = 2;
const RET_SUBMIT: u32 = 3;
const RET_UNLINK: u32 = 4;
const EPIPE: i32 = -32;
const ECONNRESET: i32 = -104;

/// What the emulated device presents.
#[derive(Clone, Debug)]
pub struct Config {
    /// USB serial number; also returned as feature report 8. Resolve expects
    /// 32 hex digits, like the real device's.
    pub serial: String,
    /// Feature report 1 (firmware version).
    pub version: [u8; 8],
}

impl Default for Config {
    fn default() -> Self {
        Config { serial: "0000000000000000000000000000E001".into(), version: DEFAULT_VERSION }
    }
}

/// A report the host (Resolve) sent to the emulated device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostReport {
    /// Report 2: key LED bitfield.
    KeyLeds(u32),
    /// Report 3: wheel mode (raw `JogMode`).
    JogMode(u8),
    /// Report 4: JOG/SHTL/SCRL LEDs.
    JogLeds(u8),
    Other(Vec<u8>),
}

/// Something that happened, for logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The host attached (`true`) or the connection closed (`false`).
    Attached(bool),
    /// The host finished the handshake; `answer_right` tells whether its answer
    /// to our challenge was correct.
    Authenticated { answer_right: bool },
    Note(String),
}

pub struct Emulator {
    shared: Arc<Shared>,
    port: u16,
}

struct Shared {
    config: Config,
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// The attached connection; every write goes through it under this lock,
    /// so replies and input reports never interleave.
    link: Option<TcpStream>,
    /// Interrupt-in requests waiting for a report: seqnum and buffer length.
    pending_in: VecDeque<(u32, usize)>,
    /// Input reports waiting for a request.
    queued: VecDeque<Vec<u8>>,
    battery: Option<[u8; 3]>,
    /// What the next feature report 6 read returns, and our last challenge.
    auth_reply: [u8; 10],
    challenge: u64,
    challenges_sent: u64,
    host_reports: Vec<HostReport>,
    events: Vec<Event>,
}

impl Emulator {
    /// Starts serving on 127.0.0.1:`port` (0 = any free port).
    pub fn start(config: Config, port: u16) -> io::Result<Emulator> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        let shared = Arc::new(Shared { config, state: Mutex::new(State::default()) });
        let serving = shared.clone();
        thread::spawn(move || {
            for stream in listener.incoming().map_while(Result::ok) {
                let shared = serving.clone();
                thread::spawn(move || {
                    let _ = serve(stream, &shared);
                    let mut state = shared.state.lock().unwrap();
                    if state.link.take().is_some() {
                        state.pending_in.clear();
                        state.queued.clear();
                        state.events.push(Event::Attached(false));
                    }
                });
            }
        });
        Ok(Emulator { shared, port })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Whether a host has the device attached.
    pub fn attached(&self) -> bool {
        self.shared.state.lock().unwrap().link.is_some()
    }

    /// Sends an input report (keys, wheel, battery) to the host. Dropped when
    /// no host is attached, so nothing stale arrives later.
    pub fn send_input(&self, report: &[u8]) {
        let mut state = self.shared.state.lock().unwrap();
        if report.first() == Some(&7) && report.len() >= 3 {
            state.battery = Some([report[0], report[1], report[2]]);
        }
        if state.link.is_none() {
            return;
        }
        if state.queued.len() >= MAX_QUEUED {
            state.queued.pop_front();
        }
        state.queued.push_back(report.to_vec());
        flush_input(&mut state);
    }

    /// Reports the host sent since the last call.
    pub fn host_reports(&self) -> Vec<HostReport> {
        std::mem::take(&mut self.shared.state.lock().unwrap().host_reports)
    }

    /// Events since the last call.
    pub fn events(&self) -> Vec<Event> {
        std::mem::take(&mut self.shared.state.lock().unwrap().events)
    }
}

/// Answers waiting interrupt-in requests with queued reports.
fn flush_input(state: &mut State) {
    while !state.pending_in.is_empty() && !state.queued.is_empty() {
        let (seqnum, max) = state.pending_in.pop_front().unwrap();
        let mut report = state.queued.pop_front().unwrap();
        report.truncate(max);
        if let Some(link) = state.link.as_mut() {
            let _ = link.write_all(&ret_submit(seqnum, 0, &report));
        }
    }
}

fn reply(shared: &Shared, bytes: &[u8]) -> io::Result<()> {
    let mut state = shared.state.lock().unwrap();
    match state.link.as_mut() {
        Some(link) => link.write_all(bytes),
        None => Err(io::Error::new(io::ErrorKind::NotConnected, "not attached")),
    }
}

fn serve(mut stream: TcpStream, shared: &Shared) -> io::Result<()> {
    stream.set_nodelay(true).ok();
    let mut head = [0u8; 8];
    stream.read_exact(&mut head)?;
    match u16::from_be_bytes([head[2], head[3]]) {
        OP_REQ_DEVLIST => {
            let mut out = op_header(OP_REP_DEVLIST);
            out.extend_from_slice(&1u32.to_be_bytes());
            out.extend(device_info());
            // Interfaces: class, subclass, protocol, padding.
            out.extend_from_slice(&[0xff, 0x03, 0x00, 0, 0xfe, 0x01, 0x01, 0, 0x03, 0x00, 0x00, 0]);
            stream.write_all(&out)
        }
        OP_REQ_IMPORT => {
            let mut bus_id = [0u8; 32];
            stream.read_exact(&mut bus_id)?;
            let wanted = String::from_utf8_lossy(&bus_id).trim_end_matches('\0').to_string();
            let mut out = op_header(OP_REP_IMPORT);
            if wanted != BUS_ID || shared.state.lock().unwrap().link.is_some() {
                out[4..8].copy_from_slice(&1u32.to_be_bytes());
                return stream.write_all(&out);
            }
            out.extend(device_info());
            stream.write_all(&out)?;
            {
                let mut state = shared.state.lock().unwrap();
                state.link = Some(stream.try_clone()?);
                state.events.push(Event::Attached(true));
            }
            urbs(stream, shared)
        }
        _ => Ok(()),
    }
}

fn op_header(code: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&USBIP_VERSION.to_be_bytes());
    out.extend_from_slice(&code.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes());
    out
}

/// The USB/IP device record (312 bytes).
fn device_info() -> Vec<u8> {
    let mut out = vec![0u8; 256];
    let path = b"/sys/devices/speed-editor-emulator/1-1";
    out[..path.len()].copy_from_slice(path);
    let mut bus = [0u8; 32];
    bus[..BUS_ID.len()].copy_from_slice(BUS_ID.as_bytes());
    out.extend_from_slice(&bus);
    out.extend_from_slice(&1u32.to_be_bytes()); // bus number
    out.extend_from_slice(&1u32.to_be_bytes()); // device number
    out.extend_from_slice(&2u32.to_be_bytes()); // full speed
    out.extend_from_slice(&VID.to_be_bytes());
    out.extend_from_slice(&PID.to_be_bytes());
    out.extend_from_slice(&0x0102u16.to_be_bytes()); // bcdDevice
    out.extend_from_slice(&[0xef, 0x02, 0x01, 0x01, 0x01, 0x03]);
    out
}

/// URB phase: requests from the host's USB stack.
fn urbs(mut stream: TcpStream, shared: &Shared) -> io::Result<()> {
    loop {
        let mut head = [0u8; 48];
        stream.read_exact(&mut head)?;
        let word = |i: usize| u32::from_be_bytes(head[i..i + 4].try_into().unwrap());
        let (command, seqnum, direction, ep) = (word(0), word(4), word(12), word(16));
        match command {
            CMD_SUBMIT => {
                let length = word(24) as usize;
                let setup: [u8; 8] = head[40..48].try_into().unwrap();
                let mut data = vec![0u8; if direction == 0 { length } else { 0 }];
                stream.read_exact(&mut data)?;
                let result = match ep {
                    0 => control(shared, setup, &data),
                    1 if direction == 1 => {
                        let mut state = shared.state.lock().unwrap();
                        state.pending_in.push_back((seqnum, length));
                        flush_input(&mut state);
                        continue;
                    }
                    1 => {
                        host_report(shared, &data);
                        Ok(Vec::new())
                    }
                    _ => Err(EPIPE),
                };
                let packet = match result {
                    Ok(bytes) => ret_submit(seqnum, 0, &bytes),
                    Err(status) => ret_submit(seqnum, status, &[]),
                };
                reply(shared, &packet)?;
            }
            CMD_UNLINK => {
                let target = word(20);
                let status = {
                    let mut state = shared.state.lock().unwrap();
                    let before = state.pending_in.len();
                    state.pending_in.retain(|&(s, _)| s != target);
                    if state.pending_in.len() < before { ECONNRESET } else { 0 }
                };
                let mut out = Vec::with_capacity(48);
                for v in [RET_UNLINK, seqnum, 0, 0, 0] {
                    out.extend_from_slice(&v.to_be_bytes());
                }
                out.extend_from_slice(&status.to_be_bytes());
                out.resize(48, 0);
                reply(shared, &out)?;
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "unexpected USB/IP command")),
        }
    }
}

fn ret_submit(seqnum: u32, status: i32, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(48 + data.len());
    for v in [RET_SUBMIT, seqnum, 0, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&status.to_be_bytes());
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0u8; 12]); // start frame, number of packets, error count
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(data);
    out
}

/// Answers a control request on endpoint 0.
fn control(shared: &Shared, setup: [u8; 8], data: &[u8]) -> Result<Vec<u8>, i32> {
    let (request_type, request) = (setup[0], setup[1]);
    let value = u16::from_le_bytes([setup[2], setup[3]]);
    let index = u16::from_le_bytes([setup[4], setup[5]]);
    let length = u16::from_le_bytes([setup[6], setup[7]]) as usize;
    let cut = |mut v: Vec<u8>| {
        v.truncate(length);
        Ok(v)
    };
    let (kind, id) = (value >> 8, value as u8);
    match (request_type, request) {
        // GET_DESCRIPTOR (device)
        (0x80, 0x06) => match kind {
            1 => cut(DEVICE_DESCRIPTOR.to_vec()),
            2 => cut(CONFIG_DESCRIPTOR.to_vec()),
            3 => cut(string_descriptor(&shared.config, id).ok_or(EPIPE)?),
            _ => Err(EPIPE), // device qualifier, BOS, …: a full-speed USB 2.0 device has none
        },
        // GET_DESCRIPTOR (interface): the HID report descriptor
        (0x81, 0x06) if kind == 0x22 && index == HID_INTERFACE => cut(REPORT_DESCRIPTOR.to_vec()),
        // GET_STATUS
        (0x80..=0x82, 0x00) => cut(vec![0, 0]),
        // SET_ADDRESS, SET_CONFIGURATION, SET_INTERFACE, CLEAR_FEATURE, HID SET_IDLE
        (0x00, 0x05) | (0x00, 0x09) | (0x01, 0x0b) | (0x00..=0x02, 0x01) | (0x21, 0x0a) => Ok(Vec::new()),
        // HID GET_REPORT
        (0xa1, 0x01) => match (kind, id) {
            (3, auth::REPORT_ID) => cut(shared.state.lock().unwrap().auth_reply.to_vec()),
            (3, 1) => cut(shared.config.version.to_vec()),
            (3, 8) => {
                let mut report = vec![8u8];
                let mut serial = shared.config.serial.as_bytes().to_vec();
                serial.resize(32, 0);
                report.extend(serial);
                cut(report)
            }
            (1, 7) => cut(shared.state.lock().unwrap().battery.unwrap_or(DEFAULT_BATTERY).to_vec()),
            _ => Err(EPIPE),
        },
        // HID SET_REPORT
        (0x21, 0x09) => {
            if kind == 3 && id == auth::REPORT_ID && data.len() >= 10 {
                authenticate(shared, data);
            } else {
                host_report(shared, data);
            }
            Ok(Vec::new())
        }
        _ => Err(EPIPE),
    }
}

fn host_report(shared: &Shared, data: &[u8]) {
    let report = match data {
        [2, a, b, c, d, ..] => HostReport::KeyLeds(u32::from_le_bytes([*a, *b, *c, *d])),
        [3, mode, ..] => HostReport::JogMode(*mode),
        [4, leds, ..] => HostReport::JogLeds(*leds),
        other => HostReport::Other(other.to_vec()),
    };
    shared.state.lock().unwrap().host_reports.push(report);
}

/// The device side of the handshake: the host resets it, reads our challenge,
/// challenges us, then answers our challenge.
///
/// The real device's answer to the host's challenge uses a calculation not
/// known yet (FINDINGS.md); Resolve retries a few times without it and then
/// accepts the device anyway.
fn authenticate(shared: &Shared, data: &[u8]) {
    let mut state = shared.state.lock().unwrap();
    let mut reply = [0u8; 10];
    reply[0] = auth::REPORT_ID;
    match data[1] {
        0 => {
            state.challenges_sent += 1;
            state.challenge = challenge(state.challenges_sent);
            reply[2..].copy_from_slice(&state.challenge.to_le_bytes());
        }
        1 => reply[1] = 2,
        3 => {
            let answer = u64::from_le_bytes(data[2..10].try_into().unwrap());
            let answer_right = answer == auth::response(state.challenge);
            state.events.push(Event::Authenticated { answer_right });
            reply[1] = 4;
            reply[2..4].copy_from_slice(&600u16.to_le_bytes()); // seconds, as the real device
        }
        other => state.events.push(Event::Note(format!("unexpected handshake step {other}"))),
    }
    state.auth_reply = reply;
}

/// A different challenge each time (not secret: anything is accepted).
fn challenge(n: u64) -> u64 {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    (nanos ^ n.wrapping_mul(0x9e37_79b9_7f4a_7c15)).rotate_left(17)
}

fn string_descriptor(config: &Config, index: u8) -> Option<Vec<u8>> {
    let text = match index {
        0 => return Some(vec![4, 3, 0x09, 0x04]), // English (US)
        1 => MANUFACTURER,
        2 => PRODUCT,
        3 => config.serial.as_str(),
        _ => return None,
    };
    let mut out = vec![0, 3];
    for unit in text.encode_utf16() {
        out.extend_from_slice(&unit.to_le_bytes());
    }
    out[0] = out.len() as u8;
    Some(out)
}

/// Builds the key report (report 4) for these held keys, as the device does:
/// up to six key codes, little-endian.
pub fn key_report(codes: &[u16]) -> [u8; 13] {
    let mut report = [0u8; 13];
    report[0] = 4;
    for (i, code) in codes.iter().take(6).enumerate() {
        report[1 + 2 * i..3 + 2 * i].copy_from_slice(&code.to_le_bytes());
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rx(s: &mut TcpStream, n: usize) -> Vec<u8> {
        let mut b = vec![0u8; n];
        s.read_exact(&mut b).unwrap();
        b
    }

    fn submit(s: &mut TcpStream, seq: u32, direction: u32, ep: u32, length: u32, setup: [u8; 8], data: &[u8]) {
        let mut out = Vec::new();
        for v in [CMD_SUBMIT, seq, 0x10001, direction, ep, 0, length, 0, 0, 0] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        out.extend_from_slice(&setup);
        out.extend_from_slice(data);
        s.write_all(&out).unwrap();
    }

    /// Returns status and data of a RET_SUBMIT.
    fn ret(s: &mut TcpStream) -> (i32, Vec<u8>) {
        let h = rx(s, 48);
        let status = i32::from_be_bytes(h[20..24].try_into().unwrap());
        let len = i32::from_be_bytes(h[24..28].try_into().unwrap()).max(0) as usize;
        (status, rx(s, len))
    }

    fn setup(rt: u8, rq: u8, value: u16, index: u16, length: u16) -> [u8; 8] {
        let mut s = [rt, rq, 0, 0, 0, 0, 0, 0];
        s[2..4].copy_from_slice(&value.to_le_bytes());
        s[4..6].copy_from_slice(&index.to_le_bytes());
        s[6..8].copy_from_slice(&length.to_le_bytes());
        s
    }

    #[test]
    fn behaves_like_the_device_resolve_accepts() {
        let emu = Emulator::start(Config::default(), 0).unwrap();
        let mut s = TcpStream::connect(("127.0.0.1", emu.port())).unwrap();
        let mut import = op_header(OP_REQ_IMPORT);
        import.extend_from_slice(&{
            let mut b = [0u8; 32];
            b[..3].copy_from_slice(b"1-1");
            b
        });
        s.write_all(&import).unwrap();
        let head = rx(&mut s, 8);
        assert_eq!(&head[4..8], &[0, 0, 0, 0], "import accepted");
        rx(&mut s, 312);

        submit(&mut s, 1, 1, 0, 400, setup(0x81, 6, 0x2200, 2, 400), &[]);
        assert_eq!(ret(&mut s).1.len(), 308, "the device's own report descriptor");
        submit(&mut s, 2, 1, 0, 8, setup(0xa1, 1, 0x0301, 2, 8), &[]);
        assert_eq!(ret(&mut s).1, DEFAULT_VERSION.to_vec(), "firmware version");
        submit(&mut s, 3, 1, 0, 33, setup(0xa1, 1, 0x0308, 2, 33), &[]);
        let serial = ret(&mut s).1;
        assert_eq!(&serial[1..33], Config::default().serial.as_bytes(), "serial number");

        // LEDs and wheel mode reach the caller.
        submit(&mut s, 4, 0, 0, 5, setup(0x21, 9, 0x0202, 2, 5), &[2, 0x20, 0, 0, 0]);
        ret(&mut s);
        submit(&mut s, 5, 0, 0, 7, setup(0x21, 9, 0x0203, 2, 7), &[3, 3, 0, 0, 0, 0, 0xff]);
        ret(&mut s);
        assert_eq!(emu.host_reports(), vec![HostReport::KeyLeds(0x20), HostReport::JogMode(3)]);

        // A key report waits for the host's interrupt-in request.
        emu.send_input(&key_report(&[0x07]));
        submit(&mut s, 6, 1, 1, 64, [0; 8], &[]);
        assert_eq!(ret(&mut s).1, key_report(&[0x07]).to_vec());
        assert!(emu.attached());
    }

    #[test]
    fn key_report_layout() {
        let r = key_report(&[0x07, 0x3c]);
        assert_eq!(&r[..5], &[4, 0x07, 0, 0x3c, 0]);
    }
}
