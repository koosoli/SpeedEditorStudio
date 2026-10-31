//! Decoding of input reports sent by the device.

use crate::keys::Key;

/// A decoded input report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Report {
    /// Report 4: codes of all keys currently held, in device order. Sent
    /// whenever the set of held keys changes; the device does no auto-repeat.
    Keys(Vec<u16>),
    /// Report 3: wheel movement. `mode` is the raw jog mode (see `JogMode`).
    Jog { mode: u8, value: i32 },
    /// Report 7: battery state (Bluetooth models).
    Battery { charging: bool, level: u8 },
    /// Anything not understood, kept raw for diagnostics.
    Other(Vec<u8>),
}

impl Report {
    pub fn parse(buf: &[u8]) -> Report {
        match buf {
            [4, rest @ ..] if rest.len() >= 12 => Report::Keys(
                rest[..12]
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .filter(|&code| code != 0)
                    .collect(),
            ),
            [3, mode, a, b, c, d, ..] => Report::Jog {
                mode: *mode,
                value: i32::from_le_bytes([*a, *b, *c, *d]),
            },
            [7, charging, level, ..] => Report::Battery {
                charging: *charging != 0,
                level: *level,
            },
            _ => Report::Other(buf.to_vec()),
        }
    }
}

/// Turns successive "keys held" reports into press and release edges.
#[derive(Default, Debug)]
pub struct KeyTracker {
    held: Vec<u16>,
}

/// Key changes between two key reports. Codes the crate does not know are
/// kept, so unknown keys still show up in diagnostics.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct KeyChanges {
    pub pressed: Vec<u16>,
    pub released: Vec<u16>,
}

impl KeyTracker {
    pub fn update(&mut self, held: &[u16]) -> KeyChanges {
        let changes = KeyChanges {
            pressed: held.iter().copied().filter(|c| !self.held.contains(c)).collect(),
            released: self.held.iter().copied().filter(|c| !held.contains(c)).collect(),
        };
        self.held = held.to_vec();
        changes
    }

    pub fn held(&self) -> &[u16] {
        &self.held
    }

    /// Forget all held keys, e.g. after a disconnect. Returns the keys that
    /// were held so callers can release anything bound to them.
    pub fn reset(&mut self) -> Vec<u16> {
        std::mem::take(&mut self.held)
    }
}

/// Human-readable name for a key code, falling back to hex for unknown codes.
pub fn key_name(code: u16) -> String {
    match Key::from_code(code) {
        Some(key) => key.label().to_string(),
        None => format!("0x{code:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_report() {
        let buf = [4, 0x33, 0, 0x1d, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(Report::parse(&buf), Report::Keys(vec![0x33, 0x1d]));
    }

    #[test]
    fn parses_negative_jog() {
        let buf = [3, 0, 0xfd, 0xff, 0xff, 0xff, 0];
        assert_eq!(Report::parse(&buf), Report::Jog { mode: 0, value: -3 });
    }

    #[test]
    fn short_key_report_is_other() {
        assert_eq!(Report::parse(&[4, 1, 0]), Report::Other(vec![4, 1, 0]));
    }

    #[test]
    fn tracks_press_and_release_edges() {
        let mut t = KeyTracker::default();
        assert_eq!(t.update(&[0x33]).pressed, vec![0x33]);
        let c = t.update(&[0x33, 0x34]);
        assert_eq!((c.pressed, c.released), (vec![0x34], vec![]));
        let c = t.update(&[0x34]);
        assert_eq!((c.pressed, c.released), (vec![], vec![0x33]));
        assert_eq!(t.reset(), vec![0x34]);
    }
}
