//! A DaVinci Resolve wheel that behaves like Resolve's own Speed Editor
//! support, driven through Resolve's playback and step shortcuts.
//!
//! Everything here copies measurements of Resolve 21.1 driving a real Speed
//! Editor over USB (docs/FINDINGS.md):
//! * JOG and SCRL put the device in jog mode 2, SHTL in mode 3; Resolve sends
//!   these with parameter 255 and lights the matching mode key.
//! * JOG: Resolve moves one frame per about 20,500 summed report units and
//!   adds no acceleration of its own (the device's mode 2 does that).
//!   Fractions of a frame are dropped when the wheel pauses.
//! * SCRL: about 30 times JOG's rate, i.e. about one second per 20,000 units,
//!   sent as Resolve's one-second step.
//! * SHTL: speed ≈ 26 × (position / 4096)^2.2 times real time. Resolve's
//!   keyboard only plays at 1×, 2×, 4×…, so the nearest of those is used, and
//!   slower speeds are played as timed single-frame steps.

use crate::engine::Output;
use crate::keyboard::Shortcut;

/// Jog modes understood by the device (see speed_editor::JogMode).
const DEVICE_MODE_RELATIVE_2: u8 = 2;
const DEVICE_MODE_ABSOLUTE_DEADZERO: u8 = 3;

/// Jog LED bits (output report 4).
const LED_JOG: u8 = 1 << 0;
const LED_SHUTTLE: u8 = 1 << 1;
const LED_SCROLL: u8 = 1 << 2;

/// Without a report for this long, a partial jog frame is dropped.
const JOG_PAUSE_MS: u64 = 150;
/// Most steps acted on per report, so a burst never floods Resolve.
const MAX_STEPS: i32 = 60;

/// Shuttle: speed in multiples of real time at full deflection, and the curve.
const SHUTTLE_FULL_SPEED: f64 = 26.0;
const SHUTTLE_POWER: f64 = 2.2;
const SHUTTLE_FULL_POSITION: f64 = 4096.0;
/// Below this speed the shuttle stops; below `SHUTTLE_PLAY_FROM` it steps.
const SHUTTLE_STOP_BELOW: f64 = 0.1;
const SHUTTLE_PLAY_FROM: f64 = 0.75;
/// Highest J/L speed level used: 2^5 = 32×.
const SHUTTLE_MAX_LEVEL: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelMode {
    Jog,
    Shuttle,
    Scroll,
}

/// Settings of the DaVinci wheel, with Resolve's default shortcuts.
#[derive(Clone, Debug, PartialEq)]
pub struct DaVinciWheelConfig {
    pub step_forward: Shortcut,
    pub step_reverse: Shortcut,
    pub second_forward: Shortcut,
    pub second_reverse: Shortcut,
    pub play_forward: Shortcut,
    pub play_reverse: Shortcut,
    pub stop: Shortcut,
    /// Summed JOG report units per frame.
    pub jog_units: i32,
    /// Summed SCRL report units per one-second step.
    pub scroll_units: i32,
    /// Timeline frame rate, used to time slow shuttle steps.
    pub fps: f64,
}

impl Default for DaVinciWheelConfig {
    fn default() -> Self {
        let s = |text| Shortcut::parse(text).unwrap();
        DaVinciWheelConfig {
            step_forward: s("right"),
            step_reverse: s("left"),
            second_forward: s("shift+right"),
            second_reverse: s("shift+left"),
            play_forward: s("l"),
            play_reverse: s("j"),
            stop: s("k"),
            jog_units: 20_500,
            scroll_units: 20_000,
            fps: 30.0,
        }
    }
}

/// What the shuttle is doing in Resolve right now.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shuttle {
    Stopped,
    /// Timed frame steps at this many frames per second (sign = direction).
    Stepping(f64),
    /// Playing via J/L, pressed `presses` times (sign = direction).
    Playing(i32),
}

pub struct DaVinciWheel {
    mode: WheelMode,
    acc: i32,
    last_report_ms: Option<u64>,
    shuttle: Shuttle,
    step_acc: f64,
    last_tick_ms: Option<u64>,
}

impl Default for DaVinciWheel {
    fn default() -> Self {
        DaVinciWheel {
            mode: WheelMode::Jog,
            acc: 0,
            last_report_ms: None,
            shuttle: Shuttle::Stopped,
            step_acc: 0.0,
            last_tick_ms: None,
        }
    }
}

impl DaVinciWheel {
    pub fn mode(&self) -> WheelMode {
        self.mode
    }

    /// Device jog mode and jog LEDs for the current wheel mode, as Resolve sets them.
    pub fn setup(&self) -> (u8, u8) {
        match self.mode {
            WheelMode::Jog => (DEVICE_MODE_RELATIVE_2, LED_JOG),
            WheelMode::Shuttle => (DEVICE_MODE_ABSOLUTE_DEADZERO, LED_SHUTTLE),
            WheelMode::Scroll => (DEVICE_MODE_RELATIVE_2, LED_SCROLL),
        }
    }

    /// Switches mode (JOG/SHTL/SCRL keys). Stops any shuttle playback.
    pub fn set_mode(&mut self, mode: WheelMode, config: &DaVinciWheelConfig) -> Vec<Output> {
        let out = self.release(config);
        self.mode = mode;
        out
    }

    /// Stops playback and forgets partial movement, e.g. when leaving the page.
    pub fn release(&mut self, config: &DaVinciWheelConfig) -> Vec<Output> {
        self.acc = 0;
        self.last_report_ms = None;
        self.step_acc = 0.0;
        let was_playing = matches!(self.shuttle, Shuttle::Playing(_));
        self.shuttle = Shuttle::Stopped;
        if was_playing { vec![Output::Tap(config.stop.clone())] } else { Vec::new() }
    }

    /// Handles one wheel report.
    pub fn on_report(&mut self, value: i32, now_ms: u64, config: &DaVinciWheelConfig) -> Vec<Output> {
        match self.mode {
            WheelMode::Jog => {
                self.relative(value, now_ms, config.jog_units, &config.step_forward, &config.step_reverse, 1)
            }
            WheelMode::Scroll => {
                let frames = config.fps.round().max(1.0) as i32;
                self.relative(value, now_ms, config.scroll_units, &config.second_forward, &config.second_reverse, frames)
            }
            WheelMode::Shuttle => self.shuttle_to(value, now_ms, config),
        }
    }

    /// Advances slow shuttle stepping. Call regularly.
    pub fn tick(&mut self, now_ms: u64, config: &DaVinciWheelConfig) -> Vec<Output> {
        let dt = self.last_tick_ms.map_or(0.0, |t| now_ms.saturating_sub(t) as f64 / 1000.0);
        self.last_tick_ms = Some(now_ms);
        let Shuttle::Stepping(rate) = self.shuttle else {
            return Vec::new();
        };
        self.step_acc += rate * dt;
        let steps = self.step_acc.trunc() as i32;
        self.step_acc -= steps as f64;
        let key = if steps > 0 { &config.step_forward } else { &config.step_reverse };
        (0..steps.abs().min(MAX_STEPS)).map(|_| Output::Tap(key.clone())).collect()
    }

    fn relative(
        &mut self,
        value: i32,
        now_ms: u64,
        units: i32,
        forward: &Shortcut,
        reverse: &Shortcut,
        frames: i32,
    ) -> Vec<Output> {
        if self.last_report_ms.is_some_and(|t| now_ms.saturating_sub(t) > JOG_PAUSE_MS) {
            self.acc = 0;
        }
        self.last_report_ms = Some(now_ms);
        self.acc = self.acc.saturating_add(value);
        let steps = self.acc / units;
        self.acc -= steps * units;
        if steps == 0 {
            return Vec::new();
        }
        vec![Output::PlayheadSteps {
            steps: steps.clamp(-MAX_STEPS, MAX_STEPS),
            forward: forward.clone(),
            reverse: reverse.clone(),
            frames,
        }]
    }

    fn shuttle_to(&mut self, position: i32, now_ms: u64, config: &DaVinciWheelConfig) -> Vec<Output> {
        let fraction = (position.unsigned_abs() as f64 / SHUTTLE_FULL_POSITION).min(1.0);
        let speed = SHUTTLE_FULL_SPEED * fraction.powf(SHUTTLE_POWER);
        let sign = if position < 0 { -1 } else { 1 };
        let target = if speed < SHUTTLE_STOP_BELOW {
            Shuttle::Stopped
        } else if speed < SHUTTLE_PLAY_FROM {
            Shuttle::Stepping(sign as f64 * speed * config.fps)
        } else {
            let level = speed.log2().round().clamp(0.0, SHUTTLE_MAX_LEVEL as f64) as i32;
            Shuttle::Playing(sign * (level + 1))
        };
        self.last_tick_ms.get_or_insert(now_ms);
        let mut out = Vec::new();
        match (self.shuttle, target) {
            (Shuttle::Playing(a), Shuttle::Playing(b)) if a == b => {}
            (_, Shuttle::Playing(presses)) => {
                out.push(Output::Tap(config.stop.clone()));
                let key = if presses > 0 { &config.play_forward } else { &config.play_reverse };
                out.extend((0..presses.abs()).map(|_| Output::Tap(key.clone())));
            }
            (Shuttle::Playing(_), _) => out.push(Output::Tap(config.stop.clone())),
            _ => {}
        }
        if !matches!(target, Shuttle::Stepping(_)) {
            self.step_acc = 0.0;
        }
        self.shuttle = target;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Presses of `key`, as taps or playhead steps.
    fn taps(out: &[Output], key: &str) -> usize {
        let k = Shortcut::parse(key).unwrap();
        out.iter()
            .map(|o| match o {
                Output::Tap(s) if *s == k => 1,
                Output::PlayheadSteps { steps, forward, reverse, .. } => {
                    let s = if *steps > 0 { forward } else { reverse };
                    if *s == k { steps.unsigned_abs() as usize } else { 0 }
                }
                _ => 0,
            })
            .sum()
    }

    #[test]
    fn modes_match_resolve() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        assert_eq!(w.setup(), (2, 1));
        w.set_mode(WheelMode::Shuttle, &c);
        assert_eq!(w.setup(), (3, 2));
        w.set_mode(WheelMode::Scroll, &c);
        assert_eq!(w.setup(), (2, 4));
    }

    #[test]
    fn jog_steps_one_frame_per_units() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        let mut frames = 0;
        // 4,811,002 units in fast reports gave 234 frames natively.
        for i in 0..296 {
            frames += taps(&w.on_report(16_253, 1000 + i * 20, &c), "right");
        }
        assert!((232..=236).contains(&frames), "got {frames}");
    }

    #[test]
    fn jog_drops_partial_frame_after_pause() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        assert!(w.on_report(15_000, 1000, &c).is_empty());
        // After a pause the 15,000 is forgotten, so 10,000 more is still not a frame.
        assert!(w.on_report(10_000, 1500, &c).is_empty());
        assert_eq!(taps(&w.on_report(10_600, 1510, &c), "right"), 1);
    }

    #[test]
    fn scroll_uses_one_second_steps() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        w.set_mode(WheelMode::Scroll, &c);
        assert_eq!(taps(&w.on_report(-40_000, 1000, &c), "shift+left"), 2);
    }

    #[test]
    fn shuttle_plays_and_stops() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        w.set_mode(WheelMode::Shuttle, &c);
        // 1900/4096 deflection ≈ 4.8×: nearest level 4× = three presses of L.
        let out = w.on_report(1900, 1000, &c);
        assert_eq!((taps(&out, "k"), taps(&out, "l")), (1, 3));
        // Same level again: nothing to do.
        assert!(w.on_report(1950, 1050, &c).is_empty());
        // Back to the centre: stop.
        assert_eq!(taps(&w.on_report(0, 1100, &c), "k"), 1);
    }

    #[test]
    fn slow_shuttle_steps_frames_over_time() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        w.set_mode(WheelMode::Shuttle, &c);
        // Speed 26 × (0.25)^2.2 ≈ 1.2× would play; a smaller deflection steps.
        let pos = (4096.0 * (0.5f64 / 26.0).powf(1.0 / 2.2)) as i32; // ≈ 0.5×
        assert!(w.on_report(pos, 1000, &c).is_empty());
        let stepped: usize = (1..=10).map(|i| taps(&w.tick(1000 + i * 100, &c), "right")).sum();
        assert!((14..=16).contains(&stepped), "0.5× at 30 fps for 1 s ≈ 15 steps, got {stepped}");
    }

    #[test]
    fn leaving_shuttle_stops_playback() {
        let mut w = DaVinciWheel::default();
        let c = DaVinciWheelConfig::default();
        w.set_mode(WheelMode::Shuttle, &c);
        w.on_report(4096, 1000, &c);
        assert_eq!(taps(&w.set_mode(WheelMode::Jog, &c), "k"), 1);
    }
}
