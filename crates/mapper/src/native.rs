//! The Speed Editor keys' native DaVinci Resolve behaviour on the Edit page,
//! rebuilt from Resolve keyboard commands.
//!
//! Source: DaVinci Resolve reference manual, chapter 49 "Using the DaVinci
//! Resolve Speed Editor". Each key has up to four gestures: press, double
//! press (the function printed in brackets on the key), press and hold while
//! turning the wheel, and double press and hold while turning the wheel.
//!
//! Commands are Resolve keyboard command names; the runtime presses them with
//! the user's own Resolve shortcut. Functions Resolve offers no keyboard
//! command for (Close Up, Sync Bin, transition duration, smooth cut, live
//! overwrite, video/audio only…) are left out, as are those the manual marks
//! inactive on the Edit page.

use speed_editor::Key;

/// One step of a native function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// A Resolve keyboard command, pressed with the user's shortcut.
    Command(&'static str),
    /// A plain key, for functions that are a key in Resolve (Esc).
    Key(&'static str),
    /// A Resolve menu item, clicked directly (for functions whose shortcut
    /// cannot be typed on every keyboard).
    Menu(&'static [&'static str]),
}

/// A function driven by the wheel while a key is held: `begin` once when the
/// wheel first moves, `forward`/`reverse` per step, `end` when the key is let go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dial {
    pub begin: &'static [Step],
    pub forward: Step,
    pub reverse: Step,
    pub end: &'static [Step],
    /// Wheel units per forward/reverse step.
    pub units: i32,
    /// Shortest time between two steps; wheel movement in between is dropped.
    pub interval_ms: u64,
    /// When set, one step per touch: after a step the wheel must rest this
    /// long before it can step again.
    pub rest_ms: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NativeKey {
    /// Press. With a dial function, it fires on release if the wheel wasn't used.
    pub press: &'static [Step],
    /// Second press within `DOUBLE_PRESS_MS`.
    pub double: &'static [Step],
    /// Press and hold + wheel.
    pub dial: Option<Dial>,
    /// Double press and hold + wheel.
    pub double_dial: Option<Dial>,
}

/// Two presses closer than this count as a double press.
pub const DOUBLE_PRESS_MS: u64 = 300;

use Step::{Command as C, Key as K, Menu as M};

const NUDGE_FORWARD: Step = C("editNudgeTrimStepNudgeForward");
const NUDGE_REVERSE: Step = C("editNudgeTrimStepNudgeReverse");
/// Clear the selection after a wheel edit; the tool mode is left alone.
const DONE: &[Step] = &[C("editDeselectAll")];

/// TRIM OUT uses the same ripple selection as TRIM IN (see below): Resolve
/// offers no keyboard way to pick only the outgoing side, so turning left
/// ripple-shortens the outgoing clip's end, turning right the incoming
/// clip's start.
const TRIM_OUT: Dial = TRIM_IN;
/// Measured in Resolve 21.1 (tools/capture/resolve_key_lab.py, either side of
/// the cut): select nearest edit + one toggle; then "." shortens the incoming
/// clip's start and "," the outgoing clip's end. Like the real Speed Editor it
/// follows Resolve's current tool: ripple in trim mode (T), a plain trim that
/// leaves a gap in selection mode (A). The tool mode is not changed.
const TRIM_IN: Dial = Dial {
    begin: &[C("editSelectEditPoint"), C("editToggleEditPointType")],
    forward: NUDGE_FORWARD,
    reverse: NUDGE_REVERSE,
    end: DONE,
    units: DIAL_UNITS,
    interval_ms: DIAL_STEP_INTERVAL_MS,
    rest_ms: 0,
};
/// Both sides of the nearest edit selected rolls it, in either tool mode.
const ROLL: Dial = Dial {
    begin: &[C("editSelectEditPoint")],
    forward: NUDGE_FORWARD,
    reverse: NUDGE_REVERSE,
    end: DONE,
    units: DIAL_UNITS,
    interval_ms: DIAL_STEP_INTERVAL_MS,
    rest_ms: 0,
};
/// Trim mode with a clip body selected slips it; slide mode slides it. Only
/// trim mode slips (selection mode would move the clip over its neighbour),
/// so these switch to it and return to selection mode afterwards: Resolve
/// does not tell scripts which tool was active before.
const SLIP: Dial = Dial {
    begin: &[C("editTrim"), C("editSelectItemAtPlayhead")],
    forward: NUDGE_FORWARD,
    reverse: NUDGE_REVERSE,
    end: &[C("editDeselectAll"), C("editPointer")],
    units: DIAL_UNITS,
    interval_ms: DIAL_STEP_INTERVAL_MS,
    rest_ms: 0,
};
const SLIDE: Dial = Dial {
    begin: &[C("editTrim"), C("editSelectItemAtPlayhead"), C("trimToggleSlideMode")],
    forward: NUDGE_FORWARD,
    reverse: NUDGE_REVERSE,
    end: &[C("trimToggleSlideMode"), C("editDeselectAll"), C("editPointer")],
    units: DIAL_UNITS,
    interval_ms: DIAL_STEP_INTERVAL_MS,
    rest_ms: 0,
};
/// The clip at the playhead swaps places with its neighbours, edit to edit,
/// like MOVE reflowing the timeline. A swap takes a deliberate turn.
const MOVE: Dial = Dial {
    begin: &[C("editPointer"), C("editSelectItemAtPlayhead")],
    forward: M(&["Edit", "Swap Clips Towards Right"]),
    reverse: M(&["Edit", "Swap Clips Towards Left"]),
    end: &[C("editDeselectAll")],
    units: DIAL_UNITS * 4,
    interval_ms: SWAP_INTERVAL_MS,
    rest_ms: SWAP_REST_MS,
};
/// Fn + SPLIT: moves the clip at the playhead freely, one frame per step
/// (measured: selection mode, select clip, nudge). Overwrites what it moves
/// onto, like dragging it.
const MOVE_FREE: Dial = Dial {
    begin: &[C("editPointer"), C("editSelectItemAtPlayhead")],
    forward: NUDGE_FORWARD,
    reverse: NUDGE_REVERSE,
    end: DONE,
    units: DIAL_UNITS,
    interval_ms: DIAL_STEP_INTERVAL_MS,
    rest_ms: 0,
};
const AUDIO_LEVEL: Dial = Dial {
    begin: &[],
    forward: C("clipAudioIncreaseAudioLevel1dB"),
    reverse: C("clipAudioDecreaseAudioLevel1dB"),
    end: &[],
    units: DIAL_UNITS,
    interval_ms: DIAL_STEP_INTERVAL_MS,
    rest_ms: 0,
};

const fn press(steps: &'static [Step]) -> NativeKey {
    NativeKey { press: steps, double: &[], dial: None, double_dial: None }
}

/// Native behaviour of a key, if Resolve offers any of it as commands.
pub fn native_key(key: Key) -> Option<NativeKey> {
    Some(match key {
        Key::SmartInsert => NativeKey {
            double: &[C("editInsertOverwriteActionInsert")],
            ..press(&[C("editInsertOverwriteActionInsert")])
        },
        Key::Append => NativeKey {
            double: &[C("editInsertOverwriteActionAppendAtEnd")],
            ..press(&[C("editInsertOverwriteActionAppendAtEnd")])
        },
        Key::RippleOverwrite => press(&[C("editInsertOverwriteActionRippleOverwrite")]),
        Key::PlaceOnTop => press(&[C("editInsertOverwriteActionPlaceOnTop")]),
        Key::In => NativeKey { double: &[C("markResetIn")], ..press(&[C("markIn")]) },
        Key::Out => NativeKey { double: &[C("markResetOut")], ..press(&[C("markOut")]) },
        Key::TrimIn => NativeKey { dial: Some(TRIM_IN), ..NativeKey::default() },
        Key::TrimOut => NativeKey { dial: Some(TRIM_OUT), ..NativeKey::default() },
        Key::Roll => NativeKey { dial: Some(ROLL), double_dial: Some(SLIDE), ..NativeKey::default() },
        Key::SlipSource | Key::SlipDest => NativeKey { dial: Some(SLIP), ..NativeKey::default() },
        Key::Dissolve => press(&[C("editAddVideoTransition")]),
        Key::Escape => NativeKey { double: &[C("editUndo")], ..press(&[K("esc")]) },
        Key::AudioLevel => NativeKey { double: &[C("markMarkerAdd")], dial: Some(AUDIO_LEVEL), ..NativeKey::default() },
        Key::FullView => press(&[C("workspaceViewerModeFullViewer")]),
        Key::Transition => press(&[C("editAddTransition")]),
        // Split: a quick press splits; holding (or double press and hold) with the wheel moves the clip.
        Key::Split => NativeKey {
            press: &[C("editBladeRazor")],
            double: &[],
            dial: Some(MOVE),
            double_dial: Some(MOVE),
        },
        Key::Snap => press(&[C("editSnapping")]),
        // Native RIPL DEL removes the clip under the playhead, selected or not.
        Key::RippleDelete => press(&[C("editSelectItemAtPlayhead"), C("editDelete")]),
        Key::StopPlay => press(&[C("controlPlayToggle")]),
        Key::Source | Key::Timeline => press(&[C("viewToggleSourceTimeline")]),
        Key::Cam1 => press(&[C("editMulticamCutToAngle1")]),
        Key::Cam2 => press(&[C("editMulticamCutToAngle2")]),
        Key::Cam3 => press(&[C("editMulticamCutToAngle3")]),
        Key::Cam4 => press(&[C("editMulticamCutToAngle4")]),
        Key::Cam5 => press(&[C("editMulticamCutToAngle5")]),
        Key::Cam6 => press(&[C("editMulticamCutToAngle6")]),
        Key::Cam7 => press(&[C("editMulticamCutToAngle7")]),
        Key::Cam8 => press(&[C("editMulticamCutToAngle8")]),
        Key::Cam9 => press(&[C("editMulticamCutToAngle9")]),
        _ => return None,
    })
}

/// Extra native gestures while the Fn key is held (keys without their own
/// Fn-layer mapping).
pub fn native_fn_key(key: Key) -> Option<NativeKey> {
    match key {
        Key::Split => Some(NativeKey { dial: Some(MOVE_FREE), ..NativeKey::default() }),
        _ => None,
    }
}

/// Resolve's default shortcuts for the commands above, used until the user's
/// own layout has been read from Resolve.
pub const DEFAULT_SHORTCUTS: &[(&str, &str)] = &[
    ("editInsertOverwriteActionInsert", "F9"),
    ("editInsertOverwriteActionAppendAtEnd", "Shift+F12"),
    ("editInsertOverwriteActionRippleOverwrite", "Shift+F10"),
    ("editInsertOverwriteActionPlaceOnTop", "F12"),
    ("markIn", "I"),
    ("markOut", "O"),
    ("markResetIn", "Alt+I"),
    ("markResetOut", "Alt+O"),
    ("markMarkerAdd", "M"),
    ("editUndo", "Ctrl+Z"),
    ("workspaceViewerModeFullViewer", "Shift+F"),
    ("editAddTransition", "Ctrl+T"),
    ("editAddVideoTransition", "Alt+T"),
    ("editBladeRazor", "Ctrl+B"),
    ("editSnapping", "N"),
    ("editDelete", "Shift+Backspace"),
    ("controlPlayToggle", "Space"),
    ("viewToggleSourceTimeline", "Alt+Q"),
    ("editPointer", "A"),
    ("editTrim", "T"),
    ("editSelectEditPoint", "V"),
    ("editToggleEditPointType", "U"),
    ("editSelectItemAtPlayhead", "Shift+V"),
    ("editDeselectAll", "Ctrl+Shift+A"),
    ("editNudgeTrimStepNudgeForward", "."),
    ("editNudgeTrimStepNudgeReverse", ","),
    ("editNudgeSwapEditForward", "Ctrl+Shift+."),
    ("editNudgeSwapEditReverse", "Ctrl+Shift+,"),
    ("trimToggleSlideMode", "S"),
    ("clipAudioIncreaseAudioLevel1dB", "Ctrl+Alt+="),
    ("clipAudioDecreaseAudioLevel1dB", "Ctrl+Alt+-"),
    ("trimRippleStartToPlayhead", "Ctrl+Shift+["),
    ("trimRippleEndToPlayhead", "Ctrl+Shift+]"),
    ("editNudgeTrimStepTrimStart", "Shift+["),
    ("editNudgeTrimStepTrimEnd", "Shift+]"),
    ("editSelectClipsSelectAllBefore", "Ctrl+Alt+Y"),
    ("editSelectClipsSelectAllAfter", "Alt+Y"),
    ("editMulticamCutToAngle1", "1"),
    ("editMulticamCutToAngle2", "2"),
    ("editMulticamCutToAngle3", "3"),
    ("editMulticamCutToAngle4", "4"),
    ("editMulticamCutToAngle5", "5"),
    ("editMulticamCutToAngle6", "6"),
    ("editMulticamCutToAngle7", "7"),
    ("editMulticamCutToAngle8", "8"),
    ("editMulticamCutToAngle9", "9"),
];

/// Wheel units per dial step for wheel-driven functions (one frame, as JOG).
pub const DIAL_UNITS: i32 = 20_500;
/// Shortest time between two wheel-driven steps. Each trim/roll/slip step makes
/// Resolve rework the timeline; sending them faster than it can follow queues
/// them up, and the edit keeps going after the wheel has stopped. Steps that
/// come too soon are dropped instead.
pub const DIAL_STEP_INTERVAL_MS: u64 = 45;
/// Swapping clips goes through Resolve's menu, which takes about 0.2 s; wheel
/// movement during that time used to swap again, jumping several clips. One
/// swap per this interval moves exactly one clip per deliberate turn.
pub const SWAP_INTERVAL_MS: u64 = 400;
/// A gentle touch of the wheel still swapped 3–4 clips, so swapping is one
/// clip per touch: the wheel must rest this long before the next swap.
pub const SWAP_REST_MS: u64 = 250;
/// In shuttle mode the wheel reports positions (±4,096 per half turn); a
/// position change is scaled so a dial step is about 1/16 of a turn.
const POSITION_SCALE: i32 = 40;

/// Tracks presses and holds of native keys and turns them into outputs.
#[derive(Default)]
pub struct Gestures {
    /// Last press time per key, to detect double presses.
    last_press: Vec<(Key, u64)>,
    /// The key held for a wheel function: key, spec, whether it was a double
    /// press, whether the wheel has moved, accumulated units, last absolute
    /// shuttle position.
    held: Option<HeldDial>,
}

struct HeldDial {
    key: Key,
    spec: NativeKey,
    double: bool,
    started: bool,
    acc: i32,
    last_position: Option<i32>,
    /// When the last step was sent.
    last_step_ms: Option<u64>,
    /// When the wheel last moved, and whether it must rest before the next step.
    last_move_ms: Option<u64>,
    resting: bool,
}

impl Gestures {
    /// A native key went down. Returns the steps to run now.
    pub fn press(&mut self, key: Key, spec: NativeKey, now_ms: u64) -> Vec<Step> {
        let double = self
            .last_press
            .iter()
            .any(|&(k, t)| k == key && now_ms.saturating_sub(t) <= DOUBLE_PRESS_MS);
        self.last_press.retain(|&(k, _)| k != key);
        self.last_press.push((key, now_ms));
        if spec.dial.is_some() || spec.double_dial.is_some() {
            self.held = Some(HeldDial {
                key,
                spec,
                double,
                started: false,
                acc: 0,
                last_position: None,
                last_step_ms: None,
                last_move_ms: None,
                resting: false,
            });
            if double && !spec.double.is_empty() {
                return spec.double.to_vec();
            }
            return Vec::new();
        }
        if double && !spec.double.is_empty() { spec.double.to_vec() } else { spec.press.to_vec() }
    }

    /// A native key came up. Returns the steps to run now.
    pub fn release(&mut self, key: Key) -> Vec<Step> {
        let Some(held) = self.held.take_if(|h| h.key == key) else {
            return Vec::new();
        };
        let dial = if held.double { held.spec.double_dial } else { held.spec.dial };
        match (held.started, dial) {
            (true, Some(d)) => d.end.to_vec(),
            // Held without turning the wheel: an ordinary press (e.g. SPLIT).
            (false, _) if !held.double => held.spec.press.to_vec(),
            _ => Vec::new(),
        }
    }

    /// True while a key with a wheel function is held: the wheel belongs to it.
    pub fn owns_wheel(&self) -> bool {
        self.held.as_ref().is_some_and(|h| {
            if h.double { h.spec.double_dial.is_some() } else { h.spec.dial.is_some() }
        })
    }

    /// A wheel report while a dial key is held. `absolute` means the wheel
    /// reports a position (shuttle mode) rather than movement.
    pub fn wheel(&mut self, value: i32, absolute: bool, now_ms: u64) -> Vec<Step> {
        let Some(held) = self.held.as_mut() else {
            return Vec::new();
        };
        let Some(dial) = (if held.double { held.spec.double_dial } else { held.spec.dial }) else {
            return Vec::new();
        };
        let delta = if absolute {
            let previous = held.last_position.replace(value).unwrap_or(value);
            (value - previous).saturating_mul(POSITION_SCALE)
        } else {
            value
        };
        let mut steps = Vec::new();
        if dial.rest_ms > 0 && delta != 0 {
            let rested = held.last_move_ms.is_none_or(|t| now_ms.saturating_sub(t) >= dial.rest_ms);
            held.last_move_ms = Some(now_ms);
            if held.resting {
                if !rested {
                    return steps;
                }
                held.resting = false;
                held.acc = 0;
            }
        }
        if !held.started && delta != 0 {
            held.started = true;
            steps.extend_from_slice(dial.begin);
        }
        held.acc = held.acc.saturating_add(delta);
        let n = held.acc / dial.units;
        if n == 0 {
            return steps;
        }
        // At most one step per interval; the rest of this turn is dropped so
        // nothing queues up in Resolve.
        held.acc = 0;
        if held.last_step_ms.is_some_and(|t| now_ms.saturating_sub(t) < dial.interval_ms) {
            return steps;
        }
        held.last_step_ms = Some(now_ms);
        held.resting = dial.rest_ms > 0;
        steps.push(if n > 0 { dial.forward } else { dial.reverse });
        steps
    }

    /// Forgets held keys, e.g. after a disconnect.
    pub fn reset(&mut self) {
        self.held = None;
        self.last_press.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(key: Key) -> NativeKey {
        native_key(key).unwrap()
    }

    #[test]
    fn double_press_runs_the_bracket_function() {
        let mut g = Gestures::default();
        assert_eq!(g.press(Key::In, spec(Key::In), 1000), vec![C("markIn")]);
        g.release(Key::In);
        assert_eq!(g.press(Key::In, spec(Key::In), 1200), vec![C("markResetIn")]);
        // Too slow for a double press.
        g.release(Key::In);
        assert_eq!(g.press(Key::In, spec(Key::In), 2000), vec![C("markIn")]);
    }

    #[test]
    fn trim_in_trims_with_the_wheel_while_held() {
        let mut g = Gestures::default();
        assert!(g.press(Key::TrimIn, spec(Key::TrimIn), 1000).is_empty());
        assert!(g.owns_wheel());
        let steps = g.wheel(DIAL_UNITS * 2, false, 1000);
        let n = TRIM_IN.begin.len();
        assert_eq!(&steps[..n], TRIM_IN.begin);
        assert!(!steps.contains(&C("editTrim")) && !steps.contains(&C("editPointer")), "tool mode untouched");
        assert_eq!(steps[n..].to_vec(), vec![NUDGE_FORWARD], "one step at a time, the surplus is dropped");
        assert!(g.wheel(-DIAL_UNITS, false, 1010).is_empty(), "too soon after the last step");
        assert_eq!(g.wheel(-DIAL_UNITS, false, 1000 + DIAL_STEP_INTERVAL_MS), vec![NUDGE_REVERSE]);
        assert_eq!(g.release(Key::TrimIn), DONE.to_vec());
        assert!(!g.owns_wheel());
    }

    #[test]
    fn split_splits_on_a_quick_press_and_moves_when_turned() {
        let mut g = Gestures::default();
        assert!(g.press(Key::Split, spec(Key::Split), 1000).is_empty());
        assert_eq!(g.release(Key::Split), vec![C("editBladeRazor")]);

        let mut g = Gestures::default();
        g.press(Key::Split, spec(Key::Split), 1000);
        let steps = g.wheel(MOVE.units, false, 1000);
        assert_eq!(steps, vec![C("editPointer"), C("editSelectItemAtPlayhead"), M(&["Edit", "Swap Clips Towards Right"])]);
        assert_eq!(g.release(Key::Split), vec![C("editDeselectAll")], "moved, so no split");
    }

    #[test]
    fn swap_moves_one_clip_per_turn() {
        let mut g = Gestures::default();
        g.press(Key::Split, spec(Key::Split), 1000);
        let swaps = |steps: &[Step]| steps.iter().filter(|s| matches!(s, Step::Menu(_))).count();
        assert_eq!(swaps(&g.wheel(MOVE.units, false, 1000)), 1);
        // Movement that piled up while Resolve ran the menu doesn't swap again.
        assert_eq!(swaps(&g.wheel(MOVE.units, false, 1200)), 0);
        // Still turning (no rest): no further swap, however long it goes on.
        for t in (1200..3000).step_by(50) {
            assert_eq!(swaps(&g.wheel(MOVE.units, false, t)), 0, "at {t}");
        }
        // Rested, then touched again: one more clip.
        let t = 3000 + SWAP_REST_MS;
        assert_eq!(swaps(&g.wheel(MOVE.units, false, t)), 1);
    }

    #[test]
    fn stopping_the_wheel_stops_the_edit() {
        let mut g = Gestures::default();
        g.press(Key::TrimOut, spec(Key::TrimOut), 1000);
        // A fast spin: 20 big reports within 100 ms.
        let sent: usize = (0..20)
            .map(|i| g.wheel(DIAL_UNITS * 5, false, 1000 + i * 5).iter().filter(|s| **s == NUDGE_FORWARD).count())
            .sum();
        assert!(sent <= 3, "only what Resolve can follow is sent, got {sent}");
        // Nothing is left over to fire after the wheel stops.
        assert!(g.wheel(0, false, 2000).is_empty());
    }

    #[test]
    fn roll_double_press_and_hold_slides() {
        let mut g = Gestures::default();
        g.press(Key::Roll, spec(Key::Roll), 1000);
        g.release(Key::Roll);
        g.press(Key::Roll, spec(Key::Roll), 1100);
        let steps = g.wheel(DIAL_UNITS, false, 1200);
        assert!(steps.contains(&C("trimToggleSlideMode")));
    }

    #[test]
    fn shuttle_positions_become_movement() {
        let mut g = Gestures::default();
        g.press(Key::AudioLevel, spec(Key::AudioLevel), 1000);
        assert!(g.wheel(1000, true, 1000).is_empty(), "first position is the reference");
        let one_step = DIAL_UNITS / POSITION_SCALE + 1;
        assert_eq!(g.wheel(1000 + one_step, true, 1050), vec![C("clipAudioIncreaseAudioLevel1dB")]);
    }
}
