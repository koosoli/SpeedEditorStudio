//! Synthetic keyboard and mouse input on macOS (CoreGraphics events).
//!
//! Shortcuts hold platform-neutral key codes (Windows virtual-key numbers,
//! see mapper::keyboard). Here they become Mac key codes: modifiers and named
//! keys by a fixed table, characters through the current keyboard layout, so
//! "z" is the Z key on a German keyboard too. Codes with `MAC_KEY` set are Mac
//! key codes already (from the Resolve layout, see resolve_keys).
//!
//! Like on Windows, every key is reference counted, so two held mappings that
//! share a modifier don't release it under each other.
//!
//! Only partly tested on a Mac (see README, "macOS"). Needs
//! the Accessibility permission (System Settings › Privacy & Security).

use std::collections::HashMap;
use std::process::Command;

use mapper::keyboard::Shortcut;

use crate::mac::{self, Owned};

/// Marks a key code that is already a Mac virtual key code.
pub const MAC_KEY: u16 = 0x8000;

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
const VK_LWIN: u16 = 0x5b;

pub struct Input {
    held: HashMap<u16, u32>,
    source: Option<Owned>,
    /// Character → (Mac key code, Shift/Option needed), for the layout in use.
    layout: std::sync::Arc<Vec<(char, u16, u32)>>,
}

impl Default for Input {
    fn default() -> Self {
        Input {
            held: HashMap::new(),
            source: Owned::new(unsafe { mac::CGEventSourceCreate(mac::kCGEventSourceStateHIDSystemState) }),
            layout: mac::layout_table(),
        }
    }
}

impl Input {
    pub fn press(&mut self, shortcut: &Shortcut) {
        for &key in &shortcut.keys {
            let count = self.held.entry(key).or_insert(0);
            *count += 1;
            if *count == 1 {
                self.send_key(key, true);
            }
        }
    }

    pub fn release(&mut self, shortcut: &Shortcut) {
        for &key in shortcut.keys.iter().rev() {
            if let Some(count) = self.held.get_mut(&key) {
                *count -= 1;
                if *count == 0 {
                    self.held.remove(&key);
                    self.send_key(key, false);
                }
            }
        }
    }

    pub fn tap(&mut self, shortcut: &Shortcut) {
        self.press(shortcut);
        self.release(shortcut);
    }

    /// Types text as Unicode characters, independent of the keyboard layout.
    pub fn type_text(&self, text: &str) {
        let source = self.source.as_ref().map_or(std::ptr::null(), |s| s.get());
        for c in text.chars() {
            let mut units = [0u16; 2];
            let units = c.encode_utf16(&mut units);
            for down in [true, false] {
                if let Some(event) = Owned::new(unsafe { mac::CGEventCreateKeyboardEvent(source, 0, down) }) {
                    unsafe {
                        mac::CGEventKeyboardSetUnicodeString(event.get(), units.len(), units.as_ptr());
                        mac::CGEventPost(mac::kCGHIDEventTap, event.get());
                    }
                }
            }
        }
    }

    pub fn release_all(&mut self) {
        let keys: Vec<u16> = self.held.drain().map(|(k, _)| k).collect();
        for key in keys {
            self.send_key(key, false);
        }
    }

    /// Positive steps scroll down (vertical) or right (horizontal).
    pub fn scroll(&self, steps: i32, horizontal: bool) {
        let source = self.source.as_ref().map_or(std::ptr::null(), |s| s.get());
        // Wheel 1 is vertical (positive = up), wheel 2 horizontal (positive = left).
        let event = unsafe {
            if horizontal {
                mac::CGEventCreateScrollWheelEvent(source, mac::kCGScrollEventUnitLine, 2, 0, -steps)
            } else {
                mac::CGEventCreateScrollWheelEvent(source, mac::kCGScrollEventUnitLine, 1, -steps)
            }
        };
        if let Some(event) = Owned::new(event) {
            unsafe { mac::CGEventPost(mac::kCGHIDEventTap, event.get()) };
        }
    }

    /// Modifier flags of the modifiers currently held.
    fn flags(&self) -> u64 {
        let mut flags = 0;
        for &key in self.held.keys() {
            flags |= match key {
                VK_SHIFT => mac::kCGEventFlagMaskShift,
                VK_CONTROL => mac::kCGEventFlagMaskControl,
                VK_MENU => mac::kCGEventFlagMaskAlternate,
                VK_LWIN => mac::kCGEventFlagMaskCommand,
                _ => 0,
            };
        }
        flags
    }

    fn send_key(&self, key: u16, down: bool) {
        if let Some(media) = media_key(key) {
            mac::post_media_key(media, down);
            return;
        }
        let Some((code, extra)) = self.mac_key(key) else { return };
        let source = self.source.as_ref().map_or(std::ptr::null(), |s| s.get());
        if let Some(event) = Owned::new(unsafe { mac::CGEventCreateKeyboardEvent(source, code, down) }) {
            unsafe {
                mac::CGEventSetFlags(event.get(), self.flags() | extra);
                mac::CGEventPost(mac::kCGHIDEventTap, event.get());
            }
        }
    }

    /// The Mac key code for a platform-neutral key code, and the modifier
    /// flags the layout needs for its character (Option+5 types "[" on a
    /// German keyboard).
    fn mac_key(&self, key: u16) -> Option<(u16, u64)> {
        if key & MAC_KEY != 0 {
            return Some((key & !MAC_KEY, 0));
        }
        if let Some(code) = named_key(key) {
            return Some((code, 0));
        }
        let c = char_of(key)?;
        match self.layout.iter().find(|(t, ..)| *t == c) {
            Some(&(_, code, modifiers)) => {
                let mut extra = 0;
                if modifiers & mac::UC_SHIFT != 0 {
                    extra |= mac::kCGEventFlagMaskShift;
                }
                if modifiers & mac::UC_OPTION != 0 {
                    extra |= mac::kCGEventFlagMaskAlternate;
                }
                Some((code, extra))
            }
            None => us_key(c).map(|code| (code, 0)),
        }
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.release_all();
    }
}

/// Opens a program, file or URL the way Finder would. A bare name such as
/// "Calculator" opens the app of that name.
pub fn run(target: &str) -> std::io::Result<()> {
    let mut command = Command::new("open");
    let path = std::path::Path::new(target);
    let is_app_name = !target.contains(['/', ':', '.']) || (target.ends_with(".app") && !path.exists());
    if is_app_name {
        command.arg("-a");
    }
    let mut child = command.arg(target).spawn()?;
    // Reap `open` when it's done, so no zombie process stays behind.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The NX_KEYTYPE_* value of a media key (Windows virtual key codes).
fn media_key(vk: u16) -> Option<u32> {
    Some(match vk {
        0xaf => 0,  // volume up
        0xae => 1,  // volume down
        0xad => 7,  // mute
        0xb3 => 16, // play / pause
        0xb0 => 17, // next track
        0xb1 => 18, // previous track
        _ => return None,
    })
}

/// Mac key codes (kVK_*) of keys that don't depend on the layout.
fn named_key(vk: u16) -> Option<u16> {
    Some(match vk {
        VK_SHIFT => 0x38,
        VK_CONTROL => 0x3b,
        VK_MENU => 0x3a,
        VK_LWIN => 0x37,
        0x0d => 0x24, // return
        0x1b => 0x35, // escape
        0x09 => 0x30, // tab
        0x20 => 0x31, // space
        0x08 => 0x33, // backspace ("delete" on a Mac keyboard)
        0x2e => 0x75, // forward delete
        0x2d => 0x72, // insert / help
        0x24 => 0x73, // home
        0x23 => 0x77, // end
        0x21 => 0x74, // page up
        0x22 => 0x79, // page down
        0x25 => 0x7b, // left
        0x26 => 0x7e, // up
        0x27 => 0x7c, // right
        0x28 => 0x7d, // down
        0x70 => 0x7a, // F1
        0x71 => 0x78,
        0x72 => 0x63,
        0x73 => 0x76,
        0x74 => 0x60,
        0x75 => 0x61,
        0x76 => 0x62,
        0x77 => 0x64,
        0x78 => 0x65,
        0x79 => 0x6d,
        0x7a => 0x67,
        0x7b => 0x6f, // F12
        0x7c => 0x69, // F13
        0x7d => 0x6b,
        0x7e => 0x71,
        0x7f => 0x6a,
        0x80 => 0x40,
        0x81 => 0x4f,
        0x82 => 0x50,
        0x83 => 0x5a, // F20
        0x60 => 0x52, // keypad 0
        0x61 => 0x53,
        0x62 => 0x54,
        0x63 => 0x55,
        0x64 => 0x56,
        0x65 => 0x57,
        0x66 => 0x58,
        0x67 => 0x59,
        0x68 => 0x5b,
        0x69 => 0x5c, // keypad 9
        _ => return None,
    })
}

/// The character a Windows virtual key types on a US keyboard.
fn char_of(vk: u16) -> Option<char> {
    Some(match vk {
        0x41..=0x5a => (b'a' + (vk - 0x41) as u8) as char,
        0x30..=0x39 => (b'0' + (vk - 0x30) as u8) as char,
        0xba => ';',
        0xbb => '=',
        0xbc => ',',
        0xbd => '-',
        0xbe => '.',
        0xbf => '/',
        0xc0 => '`',
        0xdb => '[',
        0xdc => '\\',
        0xdd => ']',
        0xde => '\'',
        _ => return None,
    })
}

/// US key positions, for when the layout table is empty.
fn us_key(c: char) -> Option<u16> {
    const KEYS: &[(char, u16)] = &[
        ('a', 0x00), ('s', 0x01), ('d', 0x02), ('f', 0x03), ('h', 0x04), ('g', 0x05), ('z', 0x06), ('x', 0x07),
        ('c', 0x08), ('v', 0x09), ('b', 0x0b), ('q', 0x0c), ('w', 0x0d), ('e', 0x0e), ('r', 0x0f), ('y', 0x10),
        ('t', 0x11), ('1', 0x12), ('2', 0x13), ('3', 0x14), ('4', 0x15), ('6', 0x16), ('5', 0x17), ('=', 0x18),
        ('9', 0x19), ('7', 0x1a), ('-', 0x1b), ('8', 0x1c), ('0', 0x1d), (']', 0x1e), ('o', 0x1f), ('u', 0x20),
        ('[', 0x21), ('i', 0x22), ('p', 0x23), ('l', 0x25), ('j', 0x26), ('\'', 0x27), ('k', 0x28), (';', 0x29),
        ('\\', 0x2a), (',', 0x2b), ('/', 0x2c), ('n', 0x2d), ('m', 0x2e), ('.', 0x2f), ('`', 0x32),
    ];
    KEYS.iter().find(|(k, _)| *k == c).map(|&(_, code)| code)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Presses the mute key twice (needs the Accessibility permission for the
    /// test runner): `cargo test -p runtime -- --ignored media_keys`.
    #[test]
    #[ignore = "changes the system volume state and needs the Accessibility permission"]
    fn media_keys_reach_the_system() {
        let muted = || {
            let out = Command::new("osascript").args(["-e", "output muted of (get volume settings)"]).output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim() == "true"
        };
        let before = muted();
        let mut input = Input::default();
        let mute = Shortcut::parse("volume_mute").unwrap();
        input.tap(&mute);
        std::thread::sleep(std::time::Duration::from_millis(500));
        let after = muted();
        input.tap(&mute);
        std::thread::sleep(std::time::Duration::from_millis(500));
        assert_ne!(before, after, "mute key had no effect");
        assert_eq!(before, muted());
    }
}
