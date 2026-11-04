//! The user's DaVinci Resolve keyboard layout: which shortcut runs which
//! Resolve command, and how to type it on the current keyboard (Windows or
//! macOS; the macOS part is untested, see README).
//!
//! Resolve writes shortcuts by the character on the key ("Ctrl+Shift+[",
//! "#"), so single characters are converted with the active keyboard layout
//! (a German keyboard types "[" as AltGr+8).

use std::collections::HashMap;

use mapper::keyboard::Shortcut;
use mapper::native::DEFAULT_SHORTCUTS;
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW;

const VK_SHIFT: u16 = 0x10;
const VK_CONTROL: u16 = 0x11;
const VK_MENU: u16 = 0x12;
/// The Windows key; on macOS the Command key.
#[cfg(not(windows))]
const VK_LWIN: u16 = 0x5b;
/// Resolve's macOS layouts write Cmd where the Windows ones write Ctrl.
#[cfg(windows)]
const VK_CMD: u16 = VK_CONTROL;
#[cfg(not(windows))]
const VK_CMD: u16 = VK_LWIN;

pub struct KeyLayout {
    commands: HashMap<String, Vec<String>>,
    /// Name of the Resolve layout loaded, or None for Resolve's defaults.
    pub preset: Option<String>,
}

impl Default for KeyLayout {
    fn default() -> Self {
        let commands = DEFAULT_SHORTCUTS.iter().map(|(c, k)| (c.to_string(), vec![default_spec(k)])).collect();
        KeyLayout { commands, preset: None }
    }
}

/// Resolve's default shortcuts are listed as on Windows; on a Mac its
/// defaults use Cmd where those use Ctrl.
fn default_spec(spec: &str) -> String {
    if cfg!(target_os = "macos") { spec.replace("Ctrl+", "Cmd+") } else { spec.to_string() }
}

impl KeyLayout {
    /// Reads the helper's `keyboard_layout` reply.
    pub fn from_json(text: &str) -> Result<KeyLayout, String> {
        #[derive(serde::Deserialize)]
        struct Reply {
            preset: String,
            commands: HashMap<String, Vec<String>>,
        }
        let reply: Reply = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(KeyLayout { commands: reply.commands, preset: Some(reply.preset) })
    }

    /// The keys to press for a Resolve command: the first of its shortcuts
    /// this keyboard can type. Err explains why there is none.
    pub fn shortcut(&self, command: &str) -> Result<Shortcut, String> {
        let specs = self.commands.get(command).filter(|s| !s.is_empty()).ok_or_else(|| {
            format!("{command} has no shortcut in your Resolve keyboard layout")
        })?;
        // Prefer a shortcut whose key needs no extra modifier on this
        // keyboard layout: Shift + punctuation doesn't reach Resolve
        // (docs/FINDINGS.md), so "Ctrl++" beats "Ctrl+=" on a German keyboard.
        let parsed: Vec<(Shortcut, bool)> = specs.iter().filter_map(|spec| parse_spec_extra(spec)).collect();
        parsed
            .iter()
            .find(|(_, extra)| !extra)
            .or(parsed.first())
            .map(|(shortcut, _)| shortcut.clone())
            .ok_or_else(|| format!("cannot type {command}'s shortcut {:?} on this keyboard", specs[0]))
    }
}

/// Converts a Resolve shortcut such as "Ctrl+Shift+[" or "Shift+F12".
#[cfg(test)]
fn parse_spec(spec: &str) -> Option<Shortcut> {
    parse_spec_extra(spec).map(|(shortcut, _)| shortcut)
}

/// Like `parse_spec`, also telling whether typing the key's character on
/// this keyboard layout needs a modifier the shortcut doesn't name.
fn parse_spec_extra(spec: &str) -> Option<(Shortcut, bool)> {
    // The key itself may be "+", written "Ctrl++".
    let (mods, key) = match spec.strip_suffix("++") {
        Some(rest) => (rest, "+"),
        None => match spec.rfind('+') {
            Some(i) if i + 1 < spec.len() => (&spec[..i], &spec[i + 1..]),
            _ => ("", spec),
        },
    };
    let mut keys = Vec::new();
    let mut extra = false;
    for m in mods.split('+').filter(|m| !m.is_empty()) {
        keys.push(match m.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => VK_CONTROL,
            "cmd" | "command" => VK_CMD,
            "shift" => VK_SHIFT,
            "alt" | "option" | "opt" => VK_MENU,
            _ => return None,
        });
    }
    match named_key(key) {
        Some(vk) => keys.push(vk),
        None => {
            // A single character: ask the active keyboard layout how to type it.
            let mut chars = key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None;
            };
            // Resolve writes letter keys in capitals ("I") but means the plain key.
            let c = c.to_lowercase().next().unwrap_or(c);
            let mut units = [0u16; 2];
            if c.encode_utf16(&mut units).len() != 1 {
                return None;
            }
            let (vk, needs) = char_key(units[0])?;
            for modifier in needs {
                if !keys.contains(&modifier) {
                    keys.push(modifier);
                    extra = true;
                }
            }
            keys.push(vk);
        }
    }
    Some((Shortcut { keys }, extra))
}

/// The key that types `unit` on the active keyboard layout, and the
/// modifiers it needs (a German keyboard types "[" as AltGr+8).
#[cfg(windows)]
fn char_key(unit: u16) -> Option<(u16, Vec<u16>)> {
    let scan = unsafe { VkKeyScanW(unit) };
    if scan == -1 {
        return None;
    }
    let (vk, state) = ((scan & 0xff) as u16, (scan >> 8) & 0xff);
    let needs = [(1, VK_SHIFT), (2, VK_CONTROL), (4, VK_MENU)]
        .into_iter()
        .filter(|(bit, _)| state & bit != 0)
        .map(|(_, m)| m)
        .collect();
    Some((vk, needs))
}

/// macOS: looked up in the current keyboard layout; the result is a Mac key
/// code, marked with `MAC_KEY` for the output layer.
#[cfg(target_os = "macos")]
fn char_key(unit: u16) -> Option<(u16, Vec<u16>)> {
    let c = char::from_u32(unit as u32)?;
    let table = crate::mac::layout_table();
    let &(_, code, modifiers) = table.iter().find(|(t, ..)| *t == c)?;
    let mut needs = Vec::new();
    if modifiers & crate::mac::UC_SHIFT != 0 {
        needs.push(VK_SHIFT);
    }
    if modifiers & crate::mac::UC_OPTION != 0 {
        needs.push(VK_MENU);
    }
    Some((crate::output::MAC_KEY | code, needs))
}

fn named_key(name: &str) -> Option<u16> {
    let lower = name.to_ascii_lowercase();
    if let Some(n) = lower.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&n).then(|| 0x70 + n - 1);
    }
    Some(match lower.as_str() {
        "space" => 0x20,
        "del" | "delete" => 0x2e,
        "backspace" => 0x08,
        "ins" | "insert" => 0x2d,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "pgup" | "pageup" => 0x21,
        "pgdown" | "pgdn" | "pagedown" => 0x22,
        "home" => 0x24,
        "end" => 0x23,
        "tab" => 0x09,
        "return" | "enter" => 0x0d,
        "esc" | "escape" => 0x1b,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_and_modified_keys() {
        assert_eq!(parse_spec("Shift+F12").unwrap().keys, vec![VK_SHIFT, 0x7b]);
        assert_eq!(parse_spec("Space").unwrap().keys, vec![0x20]);
        assert_eq!(parse_spec("Ctrl+Shift+Left").unwrap().keys, vec![VK_CONTROL, VK_SHIFT, 0x25]);
    }

    #[cfg(windows)]
    #[test]
    fn letters_use_the_keyboard_layout() {
        // Letters sit on the same virtual keys in every Latin layout.
        assert_eq!(parse_spec("I").unwrap().keys, vec![0x49]);
        assert_eq!(parse_spec("Ctrl+B").unwrap().keys, vec![VK_CONTROL, 0x42]);
    }

    #[test]
    fn layout_lookup_reports_unbound_commands() {
        let layout = KeyLayout::from_json(r#"{"preset":"X","commands":{"markIn":["F9"],"editMulticamCutToAngle1":[]}}"#).unwrap();
        assert!(layout.shortcut("markIn").is_ok());
        assert!(layout.shortcut("editMulticamCutToAngle1").unwrap_err().contains("no shortcut"));
    }
}
