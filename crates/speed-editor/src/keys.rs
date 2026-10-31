//! Key codes, LED bits and jog modes.
//!
//! Key codes and LED bit positions follow Sylvain Munaut's protocol notes in
//! `bmd.py` (Apache-2.0). LED bits address output report 2; the three jog
//! mode keys have their own LEDs on output report 4 (see [`jog_led`]).

macro_rules! keys {
    ($($variant:ident = $code:literal, $id:literal, $label:literal, $led:expr;)*) => {
        /// A physical key, identified by the code the device reports.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        #[repr(u16)]
        pub enum Key {
            $($variant = $code,)*
        }

        impl Key {
            pub const ALL: &'static [Key] = &[$(Key::$variant,)*];

            pub fn from_code(code: u16) -> Option<Key> {
                match code {
                    $($code => Some(Key::$variant),)*
                    _ => None,
                }
            }

            pub fn code(self) -> u16 {
                self as u16
            }

            /// Stable identifier for configuration files and logs.
            pub fn id(self) -> &'static str {
                match self {
                    $(Key::$variant => $id,)*
                }
            }

            pub fn from_id(id: &str) -> Option<Key> {
                match id {
                    $($id => Some(Key::$variant),)*
                    _ => None,
                }
            }

            /// Text printed on the key cap.
            pub fn label(self) -> &'static str {
                match self {
                    $(Key::$variant => $label,)*
                }
            }

            /// Bit in the LED bitfield (output report 2), if the key has an LED.
            pub fn led(self) -> Option<u32> {
                match self {
                    $(Key::$variant => $led,)*
                }
            }
        }
    };
}

const fn bit(n: u32) -> Option<u32> {
    Some(1 << n)
}

keys! {
    SmartInsert        = 0x01, "smart_insert",        "SMART INSRT",  None;
    Append             = 0x02, "append",              "APPND",        None;
    RippleOverwrite    = 0x03, "ripple_overwrite",    "RIPL O/WR",    None;
    CloseUp            = 0x04, "close_up",            "CLOSE UP",     bit(0);
    PlaceOnTop         = 0x05, "place_on_top",        "PLACE ON TOP", None;
    SourceOverwrite    = 0x06, "source_overwrite",    "SRC O/WR",     None;
    In                 = 0x07, "in",                  "IN",           None;
    Out                = 0x08, "out",                 "OUT",          None;
    TrimIn             = 0x09, "trim_in",             "TRIM IN",      None;
    TrimOut            = 0x0a, "trim_out",            "TRIM OUT",     None;
    Roll               = 0x0b, "roll",                "ROLL",         None;
    SlipSource         = 0x0c, "slip_source",         "SLIP SRC",     None;
    SlipDest           = 0x0d, "slip_dest",           "SLIP DEST",    None;
    TransitionDuration = 0x0e, "transition_duration", "TRANS DUR",    None;
    Cut                = 0x0f, "cut",                 "CUT",          bit(1);
    Dissolve           = 0x10, "dissolve",            "DIS",          bit(2);
    SmoothCut          = 0x11, "smooth_cut",          "SMTH CUT",     bit(3);
    Source             = 0x1a, "source",              "SOURCE",       None;
    Timeline           = 0x1b, "timeline",            "TIMELINE",     None;
    Shuttle            = 0x1c, "shuttle",             "SHTL",         None;
    Jog                = 0x1d, "jog",                 "JOG",          None;
    Scroll             = 0x1e, "scroll",              "SCRL",         None;
    SyncBin            = 0x1f, "sync_bin",            "SYNC BIN",     None;
    Transition         = 0x22, "transition",          "TRANS",        bit(4);
    VideoOnly          = 0x25, "video_only",          "VIDEO ONLY",   bit(13);
    AudioOnly          = 0x26, "audio_only",          "AUDIO ONLY",   bit(17);
    RippleDelete       = 0x2b, "ripple_delete",       "RIPL DEL",     None;
    AudioLevel         = 0x2c, "audio_level",         "AUDIO LEVEL",  None;
    FullView           = 0x2d, "full_view",           "FULL VIEW",    None;
    Snap               = 0x2e, "snap",                "SNAP",         bit(5);
    Split              = 0x2f, "split",               "SPLIT",        None;
    LiveOverwrite      = 0x30, "live_overwrite",      "LIVE O/WR",    bit(9);
    Escape             = 0x31, "escape",              "ESC",          None;
    Cam1               = 0x33, "cam1",                "CAM1",         bit(14);
    Cam2               = 0x34, "cam2",                "CAM2",         bit(15);
    Cam3               = 0x35, "cam3",                "CAM3",         bit(16);
    Cam4               = 0x36, "cam4",                "CAM4",         bit(10);
    Cam5               = 0x37, "cam5",                "CAM5",         bit(11);
    Cam6               = 0x38, "cam6",                "CAM6",         bit(12);
    Cam7               = 0x39, "cam7",                "CAM7",         bit(6);
    Cam8               = 0x3a, "cam8",                "CAM8",         bit(7);
    Cam9               = 0x3b, "cam9",                "CAM9",         bit(8);
    StopPlay           = 0x3c, "stop_play",           "STOP/PLAY",    None;
}

/// Bit in the jog LED bitfield (output report 4) for the jog mode keys.
pub fn jog_led(key: Key) -> Option<u8> {
    match key {
        Key::Jog => Some(1 << 0),
        Key::Shuttle => Some(1 << 1),
        Key::Scroll => Some(1 << 2),
        _ => None,
    }
}

/// Wheel reporting mode, set with output report 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum JogMode {
    /// Relative movement since the previous report.
    Relative = 0,
    /// Position relative to where the mode was set, about -4096..4096 per half turn.
    AbsoluteContinuous = 1,
    /// Appears to behave like `Relative`.
    Relative2 = 2,
    /// Like `AbsoluteContinuous` with a small dead band around zero.
    AbsoluteDeadZero = 3,
}

impl JogMode {
    pub fn from_raw(raw: u8) -> Option<JogMode> {
        match raw {
            0 => Some(JogMode::Relative),
            1 => Some(JogMode::AbsoluteContinuous),
            2 => Some(JogMode::Relative2),
            3 => Some(JogMode::AbsoluteDeadZero),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_ids_round_trip() {
        for &key in Key::ALL {
            assert_eq!(Key::from_code(key.code()), Some(key));
            assert_eq!(Key::from_id(key.id()), Some(key));
        }
    }

    #[test]
    fn led_bits_are_unique() {
        let mut seen = 0u32;
        for bit in Key::ALL.iter().filter_map(|k| k.led()) {
            assert_eq!(seen & bit, 0);
            seen |= bit;
        }
        assert_eq!(seen, (1 << 18) - 1);
    }
}
