//! Synthetic keyboard and mouse input on Windows (SendInput).
//!
//! Every virtual key is reference counted, so two held mappings that share a
//! modifier (ctrl+1 and ctrl+2) do not release it under each other, and
//! `release_all` can always lift every key this program pressed.

use std::collections::HashMap;
use std::mem::size_of;
use std::os::windows::process::CommandExt;
use std::process::Command;

use mapper::keyboard::Shortcut;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE,
    MAPVK_VK_TO_VSC, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_WHEEL, MOUSEINPUT, MapVirtualKeyW, SendInput,
};

/// One mouse wheel notch.
const WHEEL_DELTA: i32 = 120;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Default)]
pub struct Input {
    held: HashMap<u16, u32>,
}

impl Input {
    pub fn press(&mut self, shortcut: &Shortcut) {
        for &vk in &shortcut.keys {
            let count = self.held.entry(vk).or_insert(0);
            *count += 1;
            if *count == 1 {
                send_key(vk, false);
            }
        }
    }

    pub fn release(&mut self, shortcut: &Shortcut) {
        for &vk in shortcut.keys.iter().rev() {
            if let Some(count) = self.held.get_mut(&vk) {
                *count -= 1;
                if *count == 0 {
                    self.held.remove(&vk);
                    send_key(vk, true);
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
        for unit in text.encode_utf16() {
            for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
                let ki = KEYBDINPUT { wVk: 0, wScan: unit, dwFlags: flags, time: 0, dwExtraInfo: 0 };
                send(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } });
            }
        }
    }

    pub fn release_all(&mut self) {
        for (vk, _) in self.held.drain() {
            send_key(vk, true);
        }
    }

    /// Positive steps scroll down (vertical) or right (horizontal).
    pub fn scroll(&self, steps: i32, horizontal: bool) {
        let (flags, data) = if horizontal {
            (MOUSEEVENTF_HWHEEL, steps * WHEEL_DELTA)
        } else {
            (MOUSEEVENTF_WHEEL, -steps * WHEEL_DELTA)
        };
        let mi = MOUSEINPUT { dx: 0, dy: 0, mouseData: data as _, dwFlags: flags, time: 0, dwExtraInfo: 0 };
        send(INPUT { r#type: INPUT_MOUSE, Anonymous: INPUT_0 { mi } });
    }
}

impl Drop for Input {
    fn drop(&mut self) {
        self.release_all();
    }
}

/// Opens a program, file or URL the way Explorer would.
pub fn run(target: &str) -> std::io::Result<()> {
    Command::new("cmd")
        .args(["/C", "start", "", target])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map(drop)
}

fn send_key(vk: u16, up: bool) {
    let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    // Sending the scan code too keeps apps that read scan codes working.
    let scan = unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16;
    let ki = KEYBDINPUT { wVk: vk, wScan: scan, dwFlags: flags, time: 0, dwExtraInfo: 0 };
    send(INPUT { r#type: INPUT_KEYBOARD, Anonymous: INPUT_0 { ki } });
}

fn send(input: INPUT) {
    unsafe {
        SendInput(1, &input, size_of::<INPUT>() as i32);
    }
}

/// Keys that live on the extended part of the keyboard.
fn is_extended(vk: u16) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2d | 0x2e | 0x5b | 0xad..=0xb3)
}
