//! Parsing of shortcut strings such as `ctrl+shift+m` into virtual key codes.
//!
//! Codes are Windows virtual-key codes. They are plain numbers, so parsing
//! stays platform independent; only the app's output layer sends them.

/// A key combination, in press order (modifiers first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub keys: Vec<u16>,
}

impl Shortcut {
    pub fn parse(text: &str) -> Result<Shortcut, String> {
        let keys = text
            .split('+')
            .map(|name| {
                let name = name.trim().to_ascii_lowercase();
                vk(&name).ok_or_else(|| format!("unknown key {name:?} in shortcut {text:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        if keys.is_empty() {
            return Err("empty shortcut".into());
        }
        Ok(Shortcut { keys })
    }
}

fn vk(name: &str) -> Option<u16> {
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        let c = bytes[0];
        return match c {
            b'a'..=b'z' => Some((c - b'a' + 0x41) as u16),
            b'0'..=b'9' => Some((c - b'0' + 0x30) as u16),
            b'-' => Some(0xbd),
            b'=' => Some(0xbb),
            b',' => Some(0xbc),
            b'.' => Some(0xbe),
            b'/' => Some(0xbf),
            b';' => Some(0xba),
            b'\'' => Some(0xde),
            b'[' => Some(0xdb),
            b']' => Some(0xdd),
            b'\\' => Some(0xdc),
            b'`' => Some(0xc0),
            _ => None,
        };
    }
    if let Some(n) = name.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        return (1..=24).contains(&n).then(|| 0x70 + n - 1);
    }
    if let Some(n) = name.strip_prefix("num").and_then(|n| n.parse::<u16>().ok()) {
        return (n <= 9).then(|| 0x60 + n);
    }
    Some(match name {
        "ctrl" | "control" => 0x11,
        "shift" => 0x10,
        "alt" | "option" | "opt" => 0x12,
        // The Windows key; on macOS the Command key (⌘).
        "win" | "super" | "cmd" | "command" | "meta" => 0x5b,
        "enter" | "return" => 0x0d,
        "esc" | "escape" => 0x1b,
        "tab" => 0x09,
        "space" => 0x20,
        "backspace" => 0x08,
        "delete" | "del" => 0x2e,
        "insert" | "ins" => 0x2d,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "page_up" => 0x21,
        "pagedown" | "page_down" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "plus" => 0xbb,
        "minus" => 0xbd,
        "printscreen" => 0x2c,
        "volume_mute" => 0xad,
        "volume_down" => 0xae,
        "volume_up" => 0xaf,
        "media_next" => 0xb0,
        "media_previous" | "media_prev" => 0xb1,
        "media_stop" => 0xb2,
        "media_play_pause" => 0xb3,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_combinations() {
        assert_eq!(Shortcut::parse("Ctrl+Shift+M").unwrap().keys, vec![0x11, 0x10, 0x4d]);
        assert_eq!(Shortcut::parse("f13").unwrap().keys, vec![0x7c]);
        assert_eq!(Shortcut::parse("media_play_pause").unwrap().keys, vec![0xb3]);
    }

    #[test]
    fn rejects_unknown_keys() {
        assert!(Shortcut::parse("ctrl+banana").is_err());
        assert!(Shortcut::parse("f25").is_err());
    }
}
