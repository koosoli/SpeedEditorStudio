//! The mapping engine: turns device events into outputs.
//!
//! Behaviour:
//! * The page keys (CUT, DIS, SMTH CUT in the example profile) select a page
//!   when released. Acting on release means pressing the toggle chord never
//!   flips pages on the way.
//! * Pressing all toggle chord keys together switches between custom mode
//!   (pages active) and normal mode (no mappings). Custom mode always starts
//!   on the first page.
//! * LEDs: in custom mode the active page's key blinks. Switching to normal
//!   mode flashes all page keys briefly to confirm; after that the page keys
//!   behave like Resolve's transition keys: the last one pressed stays lit.
//! * A mapped shortcut is held down exactly as long as its Speed Editor key,
//!   and is released by the key that pressed it even if the page changed.
//! * Modifier layers: while a modifier key is held (the `fn_key`, e.g. CUT,
//!   or one of the `modifier_keys`, e.g. DIS and SMTH CUT), other keys use
//!   that modifier's layer on the current page. A quick tap of a modifier
//!   on its own still selects its page; after a long press or use as a
//!   modifier the page stays as it is.
//! * A key with a long-press action waits: released within `HOLD_MS` it fires
//!   its press action (shortcuts are tapped), held longer it fires the
//!   long-press action instead.
//! * Pass-through (`set_passthrough`): with an emulated Speed Editor attached
//!   to Resolve, keys doing their native job and the DaVinci wheel are handed
//!   to Resolve unchanged (`NativeKeys`, `NativeJog`) instead of being
//!   imitated with shortcuts. Resolve's LEDs and wheel mode come back through
//!   `set_resolve_*`; the app keeps its own LEDs (page keys, Fn, solo CAM).

use std::collections::BTreeMap;

use speed_editor::Key;

use crate::davinci_wheel::{DaVinciWheel, DaVinciWheelConfig, WheelMode};
use crate::keyboard::Shortcut;
use crate::native::{Gestures, Step, native_fn_key, native_key};
use crate::profile::{Action, Mapping, Profile, Wheel};

/// How long a key must be held to count as a long press.
pub const HOLD_MS: u64 = 500;

/// LED blink period and how long the LED stays lit in each period.
const BLINK_PERIOD_MS: u64 = 1000;
const BLINK_ON_MS: u64 = 600;

/// Confirmation when switching to normal mode: three quick flashes.
const FLASH_MS: u64 = 600;
const FLASH_HALF_PERIOD_MS: u64 = 100;

/// Upper bound on wheel steps acted on per report, so a burst or a stale
/// counter can never fire hundreds of shortcuts.
const MAX_WHEEL_STEPS: i32 = 30;

/// Which part of a track a track choice (Fn + CAM n) takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TrackScope {
    #[default]
    Both,
    Video,
    Audio,
}

impl TrackScope {
    /// (video, audio)
    fn parts(self) -> (bool, bool) {
        match self {
            TrackScope::Both => (true, true),
            TrackScope::Video => (true, false),
            TrackScope::Audio => (false, true),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Pages and mappings are active.
    Custom,
    /// Mappings are off; the keys keep their normal meaning.
    Normal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Output {
    KeysDown(Shortcut),
    KeysUp(Shortcut),
    /// Press and release once.
    Tap(Shortcut),
    Run(String),
    /// Type this text.
    Type(String),
    /// Run a DaVinci Resolve scripting command.
    Resolve { command: String, params: BTreeMap<String, String> },
    /// Press the shortcut the user's Resolve layout has for this Resolve
    /// keyboard command (a key's native function, see `native`).
    ResolveKey(String),
    /// Limit Resolve's edits to track n (0 = all tracks): its video track,
    /// its audio track, or both.
    SoloTrack { track: u8, video: bool, audio: bool },
    /// Select the clip under the playhead on track n: its video with the
    /// linked sound, or only the video, or only the audio.
    SelectTrack { track: u8, video: bool, audio: bool },
    /// Click a Resolve menu item (path from the top menu).
    ResolveMenu(Vec<String>),
    /// Wheel steps, positive meaning the device's positive jog direction.
    Scroll { steps: i32, horizontal: bool },
    /// Move Resolve's playhead by `steps` presses of `forward` (negative:
    /// `reverse`), each moving it `frames` frames. The runtime paces these
    /// against Resolve's real playhead so they never queue up.
    PlayheadSteps { steps: i32, forward: Shortcut, reverse: Shortcut, frames: i32 },
    /// New LED bitfield for the key LEDs.
    Leds(u32),
    /// Set the device's jog mode (raw `JogMode` value, as Resolve sends it).
    JogMode(u8),
    /// New bitfield for the JOG/SHTL/SCRL LEDs.
    JogLeds(u8),
    /// Pass-through: the keys Resolve's emulated Speed Editor now shows as
    /// held (key codes).
    NativeKeys(Vec<u16>),
    /// Pass-through: hand the wheel report just received to Resolve.
    NativeJog,
    /// Human-readable state change, for the log or a status display.
    Status(String),
}

pub struct Engine {
    profile: Profile,
    mode: Mode,
    page: usize,
    held: Vec<Key>,
    /// Shortcuts currently held down, with the key that pressed them.
    active: Vec<(Key, Shortcut)>,
    /// Keys with a long-press action, waiting to see how long they're held:
    /// key, time pressed, and the mapping as it was on the page then.
    pending: Vec<(Key, u64, Mapping)>,
    /// Set when the toggle chord fired; cleared once all its keys are up.
    chord_latched: bool,
    /// Modifiers held in custom mode, in the order they went down: key, when,
    /// and whether another key was pressed while it was held.
    modifiers_held: Vec<(Key, u64, bool)>,
    wheel_acc: i32,
    skip_next_jog: bool,
    blink_on: bool,
    sent_leds: Option<u32>,
    /// Time of the latest `tick`, used to time the confirmation flash.
    now_ms: u64,
    /// When the normal-mode confirmation flash started.
    flash_start: Option<u64>,
    /// Page key lit in normal mode: the transition last selected.
    normal_selected: usize,
    /// State of the Resolve-style wheel, used on pages with `Wheel::Davinci`.
    davinci: DaVinciWheel,
    /// Jog mode and jog LEDs last sent to the device.
    sent_wheel: Option<(u8, u8)>,
    /// Press / double press / hold-and-turn handling of native DaVinci keys.
    gestures: Gestures,
    /// Track pair chosen with a `SoloTrack` action (0 = all tracks); its CAM
    /// key stays lit.
    solo_track: u8,
    /// VIDEO ONLY / AUDIO ONLY toggled on a DaVinci page: track choices
    /// (CUT + CAM, DIS + CAM) take only that part of the track until it is
    /// toggled off again.
    track_scope: TrackScope,
    /// Fn + CLOSE UP: Resolve's Trim tool (TRIM IN/OUT ripple the timeline)
    /// instead of the Selection tool (they leave a gap); CLOSE UP lights.
    /// Resolve doesn't report its tool, so this is what was last chosen here.
    trim_tool: bool,
    /// Pass-through to an emulated Speed Editor in Resolve.
    passthrough: bool,
    /// Keys held as native keys (pass-through), and what was last sent.
    native_held: Vec<Key>,
    sent_native: Vec<Key>,
    /// What Resolve last set on the emulated device: key LEDs, wheel mode,
    /// wheel LEDs.
    resolve_leds: u32,
    resolve_jog_mode: Option<u8>,
    resolve_jog_leds: Option<u8>,
}

impl Engine {
    pub fn new(profile: Profile) -> Engine {
        Engine {
            profile,
            mode: Mode::Custom,
            page: 0,
            held: Vec::new(),
            active: Vec::new(),
            pending: Vec::new(),
            chord_latched: false,
            modifiers_held: Vec::new(),
            wheel_acc: 0,
            skip_next_jog: true,
            blink_on: true,
            sent_leds: None,
            now_ms: 0,
            flash_start: None,
            normal_selected: 0,
            davinci: DaVinciWheel::default(),
            sent_wheel: None,
            gestures: Gestures::default(),
            solo_track: 0,
            track_scope: TrackScope::Both,
            trim_tool: false,
            passthrough: false,
            native_held: Vec::new(),
            sent_native: Vec::new(),
            resolve_leds: 0,
            resolve_jog_mode: None,
            resolve_jog_leds: None,
        }
    }

    /// Turns pass-through to Resolve's emulated Speed Editor on or off.
    pub fn set_passthrough(&mut self, on: bool) -> Vec<Output> {
        let mut out = Vec::new();
        if on == self.passthrough {
            return out;
        }
        out.extend(self.release_davinci());
        self.passthrough = on;
        self.native_held.clear();
        self.push_native(&mut out);
        if !on {
            self.resolve_jog_mode = None;
            self.resolve_jog_leds = None;
            self.resolve_leds = 0;
        }
        self.sent_leds = None;
        self.sent_wheel = None;
        self.push_leds(&mut out);
        self.push_wheel(&mut out);
        out
    }

    pub fn passthrough(&self) -> bool {
        self.passthrough
    }

    /// Resolve set the key LEDs of its emulated Speed Editor.
    pub fn set_resolve_leds(&mut self, leds: u32) -> Vec<Output> {
        self.resolve_leds = leds;
        let mut out = Vec::new();
        self.push_leds(&mut out);
        out
    }

    /// Resolve set the wheel mode of its emulated Speed Editor.
    pub fn set_resolve_jog_mode(&mut self, mode: u8) -> Vec<Output> {
        self.resolve_jog_mode = Some(mode);
        let mut out = Vec::new();
        self.push_wheel(&mut out);
        out
    }

    /// Resolve set the JOG/SHTL/SCRL LEDs of its emulated Speed Editor.
    pub fn set_resolve_jog_leds(&mut self, leds: u8) -> Vec<Output> {
        self.resolve_jog_leds = Some(leds);
        let mut out = Vec::new();
        self.push_wheel(&mut out);
        out
    }

    /// True when the wheel goes straight to Resolve (pass-through with the
    /// DaVinci wheel).
    fn wheel_native(&self) -> bool {
        self.passthrough && self.davinci_config().is_some()
    }

    /// A key doing its native job: handed to Resolve in pass-through,
    /// imitated with Resolve's shortcuts otherwise.
    fn native(&mut self, key: Key, out: &mut Vec<Output>) {
        if self.passthrough {
            if !self.native_held.contains(&key) {
                self.native_held.push(key);
            }
        } else {
            self.native_press(key, out);
        }
    }

    fn push_native(&mut self, out: &mut Vec<Output>) {
        if self.sent_native != self.native_held {
            self.sent_native = self.native_held.clone();
            out.push(Output::NativeKeys(self.native_held.iter().map(|k| k.code()).collect()));
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// Index of the active page (also the remembered page in normal mode).
    pub fn page(&self) -> usize {
        self.page
    }

    pub fn page_name(&self) -> &str {
        &self.profile.pages[self.page].name
    }

    /// Handles a key report listing every key currently held.
    pub fn on_keys(&mut self, codes: &[u16]) -> Vec<Output> {
        let now: Vec<Key> = codes.iter().filter_map(|&c| Key::from_code(c)).collect();
        let pressed: Vec<Key> = now.iter().copied().filter(|k| !self.held.contains(k)).collect();
        let released: Vec<Key> = self.held.iter().copied().filter(|k| !now.contains(k)).collect();
        self.held = now;
        let mut out = Vec::new();

        if !pressed.is_empty() {
            // A key pressed while a modifier is held uses that modifier; a
            // modifier pressed that way counts as used too, so neither
            // selects its page when released.
            let others_held = !self.modifiers_held.is_empty();
            for held in &mut self.modifiers_held {
                held.2 = true;
            }
            if self.mode == Mode::Custom {
                for &key in &pressed {
                    if self.profile.modifiers.contains(&key) {
                        self.modifiers_held.push((key, self.now_ms, others_held));
                    }
                }
            }
        }

        let chord = &self.profile.toggle_chord;
        if !self.chord_latched && chord.iter().all(|k| self.held.contains(k)) {
            self.chord_latched = true;
            self.toggle_mode(&mut out);
        }

        for key in released {
            self.native_held.retain(|&k| k != key);
            out.extend(self.gestures.release(key).into_iter().map(step_output));
            if let Some(index) = self.profile.page_keys.iter().position(|&k| k == key) {
                if self.chord_latched {
                    continue;
                }
                if let Some(i) = self.modifiers_held.iter().position(|m| m.0 == key) {
                    // Only a quick tap on its own selects the page; after a
                    // long press or use as a modifier the page stays as it is.
                    let (_, since, used) = self.modifiers_held.remove(i);
                    let held_long = self.now_ms.saturating_sub(since) >= HOLD_MS;
                    if self.mode == Mode::Custom && (used || held_long) {
                        continue;
                    }
                }
                match self.mode {
                    Mode::Custom if index != self.page => {
                        out.extend(self.release_davinci());
                        self.page = index;
                        self.wheel_acc = 0;
                        out.push(Output::Status(format!("page {}: {}", index + 1, self.page_name())));
                    }
                    Mode::Custom => {}
                    Mode::Normal => {
                        self.normal_selected = index;
                        self.flash_start = None;
                        // The transition keys also do their native job,
                        // pressed on release so the toggle chord stays apart.
                        if self.passthrough {
                            let mut tapped = self.native_held.clone();
                            tapped.push(key);
                            out.push(Output::NativeKeys(tapped.iter().map(|k| k.code()).collect()));
                            self.sent_native = tapped;
                        } else if let Some(spec) = native_key(key) {
                            out.extend(spec.press.iter().map(|s| step_output(*s)));
                        }
                    }
                }
            } else if let Some(i) = self.pending.iter().position(|(k, ..)| *k == key) {
                // Released before it became a long press: a short press.
                let (_, _, mapping) = self.pending.remove(i);
                if let Some(action) = &mapping.press {
                    self.fire_once(action, &mut out);
                }
            } else if let Some(i) = self.active.iter().position(|(k, _)| *k == key) {
                let (_, shortcut) = self.active.remove(i);
                out.push(Output::KeysUp(shortcut));
            }
        }

        if self.chord_latched && !self.profile.toggle_chord.iter().any(|k| self.held.contains(k)) {
            self.chord_latched = false;
        }

        {
            let modifier = self.active_modifier();
            let fn_held = modifier.is_some();
            for key in pressed {
                // With Resolve's wheel (DaVinci pages and normal mode),
                // JOG/SHTL/SCRL switch its mode.
                if let (false, Some(config)) = (fn_held, self.davinci_config()) {
                    let mode = match key {
                        Key::Jog => Some(WheelMode::Jog),
                        Key::Shuttle => Some(WheelMode::Shuttle),
                        Key::Scroll => Some(WheelMode::Scroll),
                        _ => None,
                    };
                    if let Some(mode) = mode {
                        if self.passthrough {
                            // Resolve switches the mode and tells us.
                            self.native(key, &mut out);
                        } else {
                            out.extend(self.davinci.set_mode(mode, &config));
                            self.skip_next_jog = true;
                        }
                        continue;
                    }
                }
                if self.mode == Mode::Normal {
                    // Normal mode: every key does its native Resolve function.
                    if !self.profile.page_keys.contains(&key) {
                        self.native(key, &mut out);
                    }
                    continue;
                }
                let page = &self.profile.pages[self.page];
                let mapping = match modifier {
                    Some(m) => page.layer(m).and_then(|layer| layer.get(&key)).cloned(),
                    None => page.keys.get(&key).cloned(),
                };
                let Some(mapping) = mapping else {
                    // Uncustomised keys on a DaVinci page keep their native
                    // function; with the Fn key held, a few get an extra one.
                    if page.davinci_defaults {
                        match modifier {
                            None if matches!(key, Key::VideoOnly | Key::AudioOnly) => self.arm_track_scope(key),
                            None => self.native(key, &mut out),
                            Some(m) if Some(m) == self.profile.fn_key => match key {
                                Key::VideoOnly | Key::AudioOnly => self.arm_track_scope(key),
                                Key::CloseUp => self.toggle_trim_tool(&mut out),
                                _ => self.native_fn_press(key, &mut out),
                            },
                            Some(_) => {}
                        }
                    }
                    continue;
                };
                if mapping.hold.is_some() {
                    self.pending.push((key, self.now_ms, mapping));
                } else if let Some(action) = &mapping.press {
                    self.fire_held(key, action, &mut out);
                }
            }
        }

        // Released keys leave the emulated device too (and a normal-mode
        // transition tap above is let go here).
        self.push_native(&mut out);
        self.push_leds(&mut out);
        self.push_wheel(&mut out);
        out
    }

    /// Handles a wheel report (relative movement, or the shuttle position
    /// while Resolve's wheel is in SHTL).
    pub fn on_jog(&mut self, value: i32) -> Vec<Output> {
        if std::mem::take(&mut self.skip_next_jog) {
            return Vec::new();
        }
        // A held TRIM IN, ROLL, AUDIO LEVEL… takes the wheel.
        if self.gestures.owns_wheel() {
            let absolute = self.davinci_config().is_some() && self.davinci.mode() == WheelMode::Shuttle;
            return self.gestures.wheel(value, absolute, self.now_ms).into_iter().map(step_output).collect();
        }
        if self.wheel_native() {
            return vec![Output::NativeJog];
        }
        if let Some(config) = self.davinci_config() {
            return self.davinci.on_report(value, self.now_ms, &config);
        }
        if self.mode != Mode::Custom {
            return Vec::new();
        }
        let Some(wheel) = &self.profile.pages[self.page].wheel else {
            return Vec::new();
        };
        let step = self.profile.wheel_step;
        self.wheel_acc = self.wheel_acc.saturating_add(value);
        let steps = self.wheel_acc / step;
        self.wheel_acc -= steps * step;
        let steps = steps.clamp(-MAX_WHEEL_STEPS, MAX_WHEEL_STEPS);
        if steps == 0 {
            return Vec::new();
        }
        match wheel {
            Wheel::Keys { left, right } => {
                let shortcut = if steps > 0 { right } else { left };
                (0..steps.abs()).map(|_| Output::Tap(shortcut.clone())).collect()
            }
            Wheel::Scroll { horizontal } => vec![Output::Scroll { steps, horizontal: *horizontal }],
            Wheel::Davinci(_) => Vec::new(),
        }
    }

    /// The current page's Resolve-style wheel settings, in custom mode.
    /// Runs a key's native press gesture, if it has a native function.
    fn native_press(&mut self, key: Key, out: &mut Vec<Output>) {
        if let Some(spec) = native_key(key) {
            out.extend(self.gestures.press(key, spec, self.now_ms).into_iter().map(step_output));
        }
    }

    /// VIDEO ONLY / AUDIO ONLY: track choices take only video or only audio
    /// from now on. Pressing the same key again goes back to both.
    fn arm_track_scope(&mut self, key: Key) {
        let scope = if key == Key::VideoOnly { TrackScope::Video } else { TrackScope::Audio };
        self.track_scope = if self.track_scope == scope { TrackScope::Both } else { scope };
    }

    /// Runs a key's extra native gesture for while Fn is held (CUT + SPLIT;
    /// the Fn key itself zooms the timeline with the wheel).
    fn native_fn_press(&mut self, key: Key, out: &mut Vec<Output>) {
        if Some(key) == self.profile.fn_key {
            let spec = crate::native::NativeKey { dial: Some(crate::native::ZOOM), ..Default::default() };
            out.extend(self.gestures.press(key, spec, self.now_ms).into_iter().map(step_output));
            return;
        }
        if let Some(spec) = native_fn_key(key) {
            out.extend(self.gestures.press(key, spec, self.now_ms).into_iter().map(step_output));
        }
    }

    /// The Resolve-style wheel in use: the page's in custom mode; in normal
    /// mode the first DaVinci page's, or the measured defaults.
    fn davinci_config(&self) -> Option<DaVinciWheelConfig> {
        let of = |p: &crate::profile::Page| match &p.wheel {
            Some(Wheel::Davinci(config)) => Some(config.clone()),
            _ => None,
        };
        match self.mode {
            Mode::Custom => of(&self.profile.pages[self.page]),
            Mode::Normal => Some(self.profile.pages.iter().find_map(of).unwrap_or_default()),
        }
    }

    /// Stops Resolve playback started by the shuttle, before leaving its page.
    fn release_davinci(&mut self) -> Vec<Output> {
        match self.davinci_config() {
            Some(config) => self.davinci.release(&config),
            None => Vec::new(),
        }
    }

    /// Jog mode and jog LEDs the device should have now: Resolve's setup on a
    /// page with its wheel, plain relative movement and no LEDs otherwise.
    fn desired_wheel(&self) -> (u8, u8) {
        if self.wheel_native() {
            // Whatever Resolve set on its emulated device.
            let (mode, leds) = self.davinci.setup();
            (self.resolve_jog_mode.unwrap_or(mode), self.resolve_jog_leds.unwrap_or(leds))
        } else if self.davinci_config().is_some() {
            self.davinci.setup()
        } else {
            (0, 0)
        }
    }

    fn push_wheel(&mut self, out: &mut Vec<Output>) {
        let wanted = self.desired_wheel();
        if self.sent_wheel != Some(wanted) {
            if self.sent_wheel.map(|(mode, _)| mode) != Some(wanted.0) {
                out.push(Output::JogMode(wanted.0));
                self.skip_next_jog = true;
            }
            out.push(Output::JogLeds(wanted.1));
            self.sent_wheel = Some(wanted);
        }
    }

    /// Advances the blink timer. Call regularly with a monotonic clock.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Output> {
        self.now_ms = now_ms;
        self.blink_on = now_ms % BLINK_PERIOD_MS < BLINK_ON_MS;
        if self.flash_start.is_some_and(|start| now_ms.saturating_sub(start) >= FLASH_MS) {
            self.flash_start = None;
        }
        let mut out = Vec::new();
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|(_, since, _)| now_ms.saturating_sub(*since) >= HOLD_MS);
        self.pending = waiting;
        for (key, _, mapping) in due {
            if let Some(action) = &mapping.hold {
                self.fire_held(key, action, &mut out);
            }
        }
        if let (false, Some(config)) = (self.wheel_native(), self.davinci_config()) {
            out.extend(self.davinci.tick(now_ms, &config));
        }
        self.push_leds(&mut out);
        self.push_wheel(&mut out);
        out
    }

    /// Releases every held shortcut, e.g. on exit or device disconnect.
    pub fn release_all(&mut self) -> Vec<Output> {
        self.pending.clear();
        self.active.drain(..).map(|(_, s)| Output::KeysUp(s)).collect()
    }

    /// Fires an action that lasts while `key` is held: shortcuts go down now
    /// and up when the key is released.
    fn fire_held(&mut self, key: Key, action: &Action, out: &mut Vec<Output>) {
        match action {
            Action::Keys(shortcut) => {
                out.push(Output::KeysDown(shortcut.clone()));
                self.active.push((key, shortcut.clone()));
            }
            other => self.fire_once(other, out),
        }
    }

    /// Fires an action instantly: shortcuts are tapped.
    fn fire_once(&mut self, action: &Action, out: &mut Vec<Output>) {
        if let Action::SoloTrack(n) = action {
            self.solo_track = *n;
        }
        let scope = self.track_scope;
        out.push(match action {
            Action::Keys(shortcut) => Output::Tap(shortcut.clone()),
            Action::Run(target) => Output::Run(target.clone()),
            Action::Text(text) => Output::Type(text.clone()),
            Action::Resolve { command, params } => {
                Output::Resolve { command: command.clone(), params: params.clone() }
            }
            Action::ResolveKey(command) => Output::ResolveKey(command.clone()),
            Action::SoloTrack(n) => {
                let (video, audio) = scope.parts();
                Output::SoloTrack { track: *n, video, audio }
            }
            Action::SelectTrack(n) => {
                let (video, audio) = scope.parts();
                Output::SelectTrack { track: *n, video, audio }
            }
            Action::ResolveMenu(path) => Output::ResolveMenu(path.clone()),
        });
    }

    /// Forgets device-side state after a reconnect: held keys, the wheel
    /// counter and which LEDs were last sent.
    pub fn device_reset(&mut self) -> Vec<Output> {
        let mut out = self.release_davinci();
        out.extend(self.release_all());
        self.held.clear();
        self.chord_latched = false;
        self.modifiers_held.clear();
        self.wheel_acc = 0;
        self.skip_next_jog = true;
        self.sent_leds = None;
        self.sent_wheel = None;
        self.gestures.reset();
        self.native_held.clear();
        self.push_native(&mut out);
        out
    }

    fn toggle_mode(&mut self, out: &mut Vec<Output>) {
        match self.mode {
            Mode::Custom => {
                out.extend(self.release_davinci());
                out.extend(self.release_all());
                self.mode = Mode::Normal;
                self.flash_start = Some(self.now_ms);
                out.push(Output::Status("normal mode: native DaVinci keys and wheel".into()));
            }
            Mode::Normal => {
                out.extend(self.release_davinci());
                self.mode = Mode::Custom;
                self.page = 0;
                self.wheel_acc = 0;
                out.push(Output::Status(format!("custom mode, page 1: {}", self.page_name())));
            }
        }
    }

    /// LED of the CAM key whose track pair is chosen, if any.
    fn solo_led(&self) -> u32 {
        const CAMS: [Key; 8] = [Key::Cam1, Key::Cam2, Key::Cam3, Key::Cam4, Key::Cam5, Key::Cam6, Key::Cam7, Key::Cam8];
        match self.solo_track {
            n @ 1..=8 => CAMS[n as usize - 1].led().unwrap_or(0),
            _ => 0,
        }
    }

    /// The modifier in effect in custom mode: the first one pressed of those
    /// still held. Other keys then use its layer on the active page.
    fn active_modifier(&self) -> Option<Key> {
        if self.mode != Mode::Custom {
            return None;
        }
        self.modifiers_held.iter().map(|m| m.0).find(|k| self.held.contains(k))
    }

    fn fn_held(&self) -> bool {
        self.active_modifier().is_some()
    }

    fn desired_leds(&self) -> u32 {
        let led = |k: &Key| k.led().unwrap_or(0);
        let keys = &self.profile.page_keys;
        match (self.mode, self.flash_start) {
            // While a modifier is held, it and the page key light steadily; an
            // armed VIDEO ONLY / AUDIO ONLY flashes.
            (Mode::Custom, _) if self.fn_held() => {
                led(&keys[self.page]) | self.active_modifier().as_ref().map_or(0, led) | self.solo_led() | self.scope_led()
            }
            (Mode::Custom, _) if self.blink_on => led(&keys[self.page]) | self.solo_led() | self.scope_led(),
            (Mode::Custom, _) => self.solo_led() | self.scope_led(),
            (Mode::Normal, Some(start)) => {
                let lit = (self.now_ms.saturating_sub(start) / FLASH_HALF_PERIOD_MS) % 2 == 0;
                if lit { keys.iter().map(led).fold(0, |a, b| a | b) } else { 0 }
            }
            (Mode::Normal, None) => led(&keys[self.normal_selected]),
        }
    }

    /// LEDs of the app's DaVinci-page toggles: VIDEO ONLY / AUDIO ONLY, and
    /// CLOSE UP for the Trim tool.
    fn scope_led(&self) -> u32 {
        let scope = match self.track_scope {
            TrackScope::Both => 0,
            TrackScope::Video => Key::VideoOnly.led().unwrap_or(0),
            TrackScope::Audio => Key::AudioOnly.led().unwrap_or(0),
        };
        let trim = if self.trim_tool { Key::CloseUp.led().unwrap_or(0) } else { 0 };
        scope | trim
    }

    /// Fn + CLOSE UP: switches Resolve between the Trim tool (trims ripple)
    /// and the Selection tool (trims leave a gap).
    fn toggle_trim_tool(&mut self, out: &mut Vec<Output>) {
        self.trim_tool = !self.trim_tool;
        let (command, text) = if self.trim_tool {
            ("editTrim", "Trim tool: TRIM IN/OUT ripple the timeline")
        } else {
            ("editPointer", "Selection tool: TRIM IN/OUT leave a gap")
        };
        out.push(Output::ResolveKey(command.into()));
        out.push(Output::Status(text.into()));
    }

    /// LEDs the app controls in pass-through; Resolve sets the rest.
    fn app_led_mask(&self) -> u32 {
        let led = |k: &Key| k.led().unwrap_or(0);
        let page_keys = self.profile.page_keys.iter().map(led).fold(0, |a, b| a | b);
        match self.mode {
            Mode::Normal if self.flash_start.is_some() => page_keys,
            Mode::Normal => 0,
            Mode::Custom if self.profile.pages[self.page].davinci_defaults => {
                // VIDEO ONLY / AUDIO ONLY are the app's toggles on DaVinci pages.
                let scope = led(&Key::VideoOnly) | led(&Key::AudioOnly) | led(&Key::CloseUp);
                page_keys | self.profile.fn_key.as_ref().map_or(0, led) | self.solo_led() | scope
            }
            // A page of the app's own: all LEDs are the app's.
            Mode::Custom => u32::MAX,
        }
    }

    #[cfg(test)]
    fn desired_leds_for_test(&mut self) -> u32 {
        let mut out = Vec::new();
        self.sent_leds = None;
        self.push_leds(&mut out);
        self.sent_leds.unwrap()
    }

    fn push_leds(&mut self, out: &mut Vec<Output>) {
        let mut leds = self.desired_leds();
        if self.passthrough {
            let mask = self.app_led_mask();
            leds = (self.resolve_leds & !mask) | (leds & mask);
        }
        if self.sent_leds != Some(leds) {
            self.sent_leds = Some(leds);
            out.push(Output::Leds(leds));
        }
    }
}

/// The output that performs one native step.
fn step_output(step: Step) -> Output {
    match step {
        Step::Command(command) => Output::ResolveKey(command.into()),
        Step::Key(key) => Output::Tap(Shortcut::parse(key).expect("native step keys are valid")),
        Step::Menu(path) => Output::ResolveMenu(path.iter().map(|p| p.to_string()).collect()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve", "smooth_cut"]
        toggle_chord = ["cut", "smooth_cut"]

        [[pages]]
        name = "Main"
        wheel = { keys = { left = "left", right = "right" } }
        keys.cam1 = { keys = "ctrl+c" }

        [[pages]]
        name = "Second"
        keys.cam1 = { keys = "f13" }

        [[pages]]
        name = "Third"
        wheel = { scroll = {} }
    "#;

    fn engine() -> Engine {
        let mut e = Engine::new(Profile::from_toml(PROFILE).unwrap());
        e.on_keys(&[]);
        e
    }

    const CUT: u16 = 0x0f;
    const DIS: u16 = 0x10;
    const SMTH: u16 = 0x11;
    const CAM1: u16 = 0x33;

    fn shortcut(s: &str) -> Shortcut {
        Shortcut::parse(s).unwrap()
    }

    fn has(out: &[Output], wanted: &Output) -> bool {
        out.contains(wanted)
    }

    #[test]
    fn starts_on_first_page_with_its_led_lit() {
        let mut e = Engine::new(Profile::from_toml(PROFILE).unwrap());
        assert_eq!(e.page_name(), "Main");
        assert!(has(&e.on_keys(&[]), &Output::Leds(Key::Cut.led().unwrap())));
    }

    #[test]
    fn mapped_key_is_held_while_key_is_held() {
        let mut e = engine();
        assert!(has(&e.on_keys(&[CAM1]), &Output::KeysDown(shortcut("ctrl+c"))));
        assert!(has(&e.on_keys(&[]), &Output::KeysUp(shortcut("ctrl+c"))));
    }

    #[test]
    fn page_switches_on_release() {
        let mut e = engine();
        e.on_keys(&[DIS]);
        assert_eq!(e.page_name(), "Main");
        e.on_keys(&[]);
        assert_eq!(e.page_name(), "Second");
        assert!(has(&e.on_keys(&[CAM1]), &Output::KeysDown(shortcut("f13"))));
    }

    #[test]
    fn shortcut_is_released_by_its_key_after_page_change() {
        let mut e = engine();
        e.on_keys(&[CAM1]);
        e.on_keys(&[CAM1, DIS]);
        e.on_keys(&[CAM1]);
        assert_eq!(e.page_name(), "Second");
        assert!(has(&e.on_keys(&[]), &Output::KeysUp(shortcut("ctrl+c"))));
    }

    #[test]
    fn chord_toggles_mode_without_switching_page() {
        let mut e = engine();
        e.on_keys(&[DIS]);
        e.on_keys(&[]);
        e.on_keys(&[CUT]);
        e.on_keys(&[CUT, SMTH]);
        assert_eq!(e.mode(), Mode::Normal);
        e.on_keys(&[SMTH]);
        e.on_keys(&[]);
        assert_eq!(e.mode(), Mode::Normal);
        assert!(e.on_keys(&[CAM1]).iter().all(|o| !matches!(o, Output::KeysDown(_))));
        e.on_keys(&[]);

        // Back to custom: always on the first page.
        e.on_keys(&[CUT, SMTH]);
        e.on_keys(&[]);
        assert_eq!(e.mode(), Mode::Custom);
        assert_eq!(e.page_name(), "Main");
    }

    #[test]
    fn normal_mode_confirms_with_flash_then_lights_selected_transition() {
        let all = Key::Cut.led().unwrap() | Key::Dissolve.led().unwrap() | Key::SmoothCut.led().unwrap();
        let mut e = engine();
        e.tick(5000);
        assert!(has(&e.on_keys(&[CUT, SMTH]), &Output::Leds(all)));
        assert_eq!(e.tick(5150), vec![Output::Leds(0)]);
        assert_eq!(e.tick(5250), vec![Output::Leds(all)]);

        // Flash over: releasing the chord keeps CUT selected, steadily lit.
        e.on_keys(&[]);
        assert_eq!(e.tick(5650), vec![Output::Leds(Key::Cut.led().unwrap())]);
        assert!(e.tick(6400).is_empty(), "normal mode does not blink");

        // DIS selects the dissolve transition: only its light is on, and it
        // adds Resolve's transition like the native key.
        e.on_keys(&[DIS]);
        let out = e.on_keys(&[]);
        assert!(has(&out, &Output::Leds(Key::Dissolve.led().unwrap())));
        assert!(has(&out, &Output::ResolveKey("editAddVideoTransition".into())));
        assert_eq!(e.mode(), Mode::Normal);
    }

    #[test]
    fn pressing_a_transition_key_ends_the_flash_early() {
        let mut e = engine();
        e.on_keys(&[CUT, SMTH]);
        e.on_keys(&[]);
        e.on_keys(&[SMTH]);
        assert_eq!(e.on_keys(&[]), vec![Output::Leds(Key::SmoothCut.led().unwrap())]);
    }

    #[test]
    fn custom_mode_blinks_page_led() {
        let mut e = engine();
        assert_eq!(e.tick(700), vec![Output::Leds(0)]);
        assert_eq!(e.tick(1100), vec![Output::Leds(Key::Cut.led().unwrap())]);
        assert!(e.tick(1200).is_empty());
    }

    #[test]
    fn toggling_off_releases_held_shortcuts() {
        let mut e = engine();
        e.on_keys(&[CAM1]);
        let out = e.on_keys(&[CAM1, CUT, SMTH]);
        assert!(has(&out, &Output::KeysUp(shortcut("ctrl+c"))));
    }

    #[test]
    fn wheel_taps_per_step_and_ignores_first_report() {
        let mut e = engine();
        assert!(e.on_jog(104065).is_empty(), "first report after connect is skipped");
        assert!(e.on_jog(200).is_empty());
        let out = e.on_jog(-920);
        assert_eq!(out, vec![Output::Tap(shortcut("left")), Output::Tap(shortcut("left"))]);
    }

    #[test]
    fn wheel_burst_is_capped() {
        let mut e = engine();
        e.on_jog(0);
        assert_eq!(e.on_jog(360 * 1000).len(), MAX_WHEEL_STEPS as usize);
    }

    const HOLD_PROFILE: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve"]
        toggle_chord = ["cut", "dissolve"]

        [[pages]]
        name = "Main"
        keys.cam1 = { keys = "f13", hold = { keys = "f14" } }
        keys.cam2 = { hold = { text = "hello" } }

        [[pages]]
        name = "Two"
    "#;

    #[test]
    fn short_press_taps_press_action() {
        let mut e = Engine::new(Profile::from_toml(HOLD_PROFILE).unwrap());
        e.tick(1000);
        assert!(e.on_keys(&[CAM1]).iter().all(|o| !matches!(o, Output::KeysDown(_) | Output::Tap(_))));
        e.tick(1200);
        assert!(has(&e.on_keys(&[]), &Output::Tap(shortcut("f13"))));
    }

    #[test]
    fn long_press_holds_hold_action_until_release() {
        let mut e = Engine::new(Profile::from_toml(HOLD_PROFILE).unwrap());
        e.tick(1000);
        e.on_keys(&[CAM1]);
        assert!(has(&e.tick(1000 + HOLD_MS), &Output::KeysDown(shortcut("f14"))));
        let out = e.on_keys(&[]);
        assert!(has(&out, &Output::KeysUp(shortcut("f14"))));
        assert!(!has(&out, &Output::Tap(shortcut("f13"))));
    }

    #[test]
    fn hold_only_key_does_nothing_on_short_press() {
        let mut e = Engine::new(Profile::from_toml(HOLD_PROFILE).unwrap());
        e.tick(1000);
        e.on_keys(&[0x34]);
        assert!(e.on_keys(&[]).iter().all(|o| matches!(o, Output::Leds(_))));
        e.on_keys(&[0x34]);
        assert!(has(&e.tick(2000), &Output::Type("hello".into())));
    }

    const FN_PROFILE: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve", "smooth_cut"]
        toggle_chord = ["cut", "smooth_cut"]
        fn_key = "cut"

        [[pages]]
        name = "DaVinci"
        keys.trim_in = { keys = "f13" }
        fn_keys.in = { keys = "ctrl+shift+[" }

        [[pages]]
        name = "Two"
        keys.in = { keys = "f14" }
        fn_keys.in = { keys = "f15" }

        [[pages]]
        name = "Three"
    "#;
    const IN: u16 = 0x07;
    const TRIM_IN: u16 = 0x09;

    #[test]
    fn fn_layer_used_while_page_key_held() {
        let mut e = Engine::new(Profile::from_toml(FN_PROFILE).unwrap());
        e.on_keys(&[]);
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, IN]), &Output::KeysDown(shortcut("ctrl+shift+["))));
        assert!(has(&e.on_keys(&[CUT]), &Output::KeysUp(shortcut("ctrl+shift+["))));
        // Keys without an Fn mapping do nothing while Fn is held.
        assert!(e.on_keys(&[CUT, TRIM_IN]).iter().all(|o| matches!(o, Output::Leds(_))));
        e.on_keys(&[]);
        assert_eq!(e.page_name(), "DaVinci");
        // Without Fn, IN does nothing on this page and TRIM IN its own action.
        assert!(has(&e.on_keys(&[TRIM_IN]), &Output::KeysDown(shortcut("f13"))));
    }

    #[test]
    fn fn_key_works_on_other_pages_and_keeps_the_page() {
        let mut e = Engine::new(Profile::from_toml(FN_PROFILE).unwrap());
        e.on_keys(&[DIS]);
        e.on_keys(&[]);
        assert_eq!(e.page_name(), "Two");
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, IN]), &Output::KeysDown(shortcut("f15"))));
        e.on_keys(&[CUT]);
        e.on_keys(&[]);
        assert_eq!(e.page_name(), "Two", "using CUT as Fn does not jump to page 1");
    }

    #[test]
    fn quick_tap_of_fn_key_selects_its_page_but_long_press_does_not() {
        let mut e = Engine::new(Profile::from_toml(FN_PROFILE).unwrap());
        e.on_keys(&[DIS]);
        e.on_keys(&[]);
        e.tick(1000);
        e.on_keys(&[CUT]);
        e.tick(1000 + HOLD_MS);
        e.on_keys(&[]);
        assert_eq!(e.page_name(), "Two", "long press alone keeps the page");
        e.on_keys(&[CUT]);
        e.tick(1000 + HOLD_MS + 100);
        e.on_keys(&[]);
        assert_eq!(e.page_name(), "DaVinci", "quick tap selects page 1");
    }

    #[test]
    fn fn_held_lights_fn_and_page_leds_steadily() {
        let mut e = Engine::new(Profile::from_toml(FN_PROFILE).unwrap());
        e.on_keys(&[DIS]);
        e.on_keys(&[]);
        let both = Key::Cut.led().unwrap() | Key::Dissolve.led().unwrap();
        assert!(has(&e.on_keys(&[CUT]), &Output::Leds(both)));
        assert_eq!(e.tick(700), Vec::<Output>::new(), "no blink-off while Fn is held");
    }

    #[test]
    fn normal_mode_has_no_fn_layer() {
        let mut e = Engine::new(Profile::from_toml(FN_PROFILE).unwrap());
        e.on_keys(&[CUT, SMTH]);
        e.on_keys(&[]);
        e.on_keys(&[CUT]);
        assert!(e.on_keys(&[CUT, IN]).iter().all(|o| !matches!(o, Output::KeysDown(_))));
    }

    const DAVINCI_PROFILE: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve"]
        toggle_chord = ["cut", "dissolve"]

        [[pages]]
        name = "DaVinci"
        wheel = { davinci = {} }

        [[pages]]
        name = "Other"
        wheel = { scroll = {} }
    "#;

    #[test]
    fn davinci_wheel_is_set_up_like_resolve() {
        let mut e = Engine::new(Profile::from_toml(DAVINCI_PROFILE).unwrap());
        let out = e.on_keys(&[]);
        assert!(has(&out, &Output::JogMode(2)) && has(&out, &Output::JogLeds(1)), "JOG on connect");

        // SHTL switches the device to mode 3 and lights SHTL; the key does nothing else.
        let out = e.on_keys(&[Key::Shuttle.code()]);
        assert!(has(&out, &Output::JogMode(3)) && has(&out, &Output::JogLeds(2)));
        e.on_keys(&[]);
        e.on_jog(0); // first report after a mode change is skipped
        let out = e.on_jog(4096);
        assert!(has(&out, &Output::Tap(shortcut("l"))), "full shuttle plays forward");

        // Leaving the page stops playback and returns the wheel to plain mode.
        e.on_keys(&[DIS]);
        let out = e.on_keys(&[]);
        assert!(has(&out, &Output::Tap(shortcut("k"))));
        assert!(has(&out, &Output::JogMode(0)) && has(&out, &Output::JogLeds(0)));
    }

    #[test]
    fn native_keys_on_davinci_page_and_in_normal_mode() {
        let text = DAVINCI_PROFILE.replace("wheel = { davinci = {} }", "davinci_defaults = true\n        wheel = { davinci = {} }\n        keys.out = { keys = \"f13\" }");
        let mut e = Engine::new(Profile::from_toml(&text).unwrap());
        e.on_keys(&[]);
        // Uncustomised IN does its native job; customised OUT does the mapping.
        assert!(has(&e.on_keys(&[Key::In.code()]), &Output::ResolveKey("markIn".into())));
        e.on_keys(&[]);
        assert!(has(&e.on_keys(&[Key::Out.code()]), &Output::KeysDown(shortcut("f13"))));
        e.on_keys(&[]);

        // Normal mode: OUT is native again and the DaVinci wheel stays on.
        e.on_keys(&[CUT, DIS]);
        e.on_keys(&[]);
        assert_eq!(e.mode(), Mode::Normal);
        assert!(has(&e.on_keys(&[Key::Out.code()]), &Output::ResolveKey("markOut".into())));
        e.on_keys(&[]);
        let out = e.on_keys(&[Key::Shuttle.code()]);
        assert!(has(&out, &Output::JogMode(3)), "wheel modes work in normal mode");
    }

    const SOLO_PROFILE: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve"]
        toggle_chord = ["cut", "dissolve"]
        fn_key = "cut"

        [[pages]]
        name = "DaVinci"
        fn_keys.cam2 = { solo_track = 2 }
        fn_keys.cam9 = { solo_track = 0 }

        [[pages]]
        name = "Other"
    "#;

    #[test]
    fn solo_track_lights_its_cam_key() {
        let mut e = Engine::new(Profile::from_toml(SOLO_PROFILE).unwrap());
        e.on_keys(&[]);
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, Key::Cam2.code()]), &Output::SoloTrack { track: 2, video: true, audio: true }));
        e.on_keys(&[]);
        let cam2 = Key::Cam2.led().unwrap();
        assert_eq!(e.tick(700), vec![Output::Leds(cam2)], "page LED blinks off, CAM 2 stays lit");
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, Key::Cam9.code()]), &Output::SoloTrack { track: 0, video: true, audio: true }));
        e.on_keys(&[]);
        assert_eq!(e.solo_led(), 0, "all tracks again");
    }

    #[test]
    fn fn_split_moves_the_clip_frame_by_frame() {
        let text = DAVINCI_PROFILE
            .replace("toggle_chord = [\"cut\", \"dissolve\"]", "toggle_chord = [\"cut\", \"dissolve\"]
        fn_key = \"cut\"")
            .replace("wheel = { davinci = {} }", "davinci_defaults = true
        wheel = { davinci = {} }");
        let mut e = Engine::new(Profile::from_toml(&text).unwrap());
        e.on_keys(&[]);
        e.tick(1000);
        e.on_jog(0); // the report right after the wheel mode is set is skipped
        e.on_keys(&[CUT]);
        e.on_keys(&[CUT, Key::Split.code()]);
        let out = e.on_jog(crate::native::DIAL_UNITS);
        assert!(has(&out, &Output::ResolveKey("editSelectItemAtPlayhead".into())));
        assert!(has(&out, &Output::ResolveKey("editNudgeTrimStepNudgeForward".into())), "moves, not swaps");
        assert!(!out.iter().any(|o| matches!(o, Output::ResolveMenu(_))), "no swap menu");
    }

    fn passthrough_engine() -> Engine {
        let text = DAVINCI_PROFILE
            .replace("toggle_chord = [\"cut\", \"dissolve\"]", "toggle_chord = [\"cut\", \"dissolve\"]\n        fn_key = \"cut\"")
            .replace("wheel = { davinci = {} }", "davinci_defaults = true\n        wheel = { davinci = {} }\n        keys.out = { keys = \"f13\" }");
        let mut e = Engine::new(Profile::from_toml(&text).unwrap());
        e.on_keys(&[]);
        e.tick(1000);
        e.set_passthrough(true);
        e.on_jog(0); // the report right after a wheel mode change is skipped
        e
    }

    #[test]
    fn passthrough_hands_native_keys_and_wheel_to_resolve() {
        let mut e = passthrough_engine();
        let out = e.on_keys(&[Key::In.code()]);
        assert!(has(&out, &Output::NativeKeys(vec![Key::In.code()])));
        assert!(!out.iter().any(|o| matches!(o, Output::ResolveKey(_))), "not imitated with shortcuts");
        assert_eq!(e.on_jog(20_000), vec![Output::NativeJog]);
        assert!(has(&e.on_keys(&[]), &Output::NativeKeys(vec![])));
        // A customised key still does the user's mapping.
        let out = e.on_keys(&[Key::Out.code()]);
        assert!(has(&out, &Output::KeysDown(shortcut("f13"))));
        assert!(!out.iter().any(|o| matches!(o, Output::NativeKeys(_))));
    }

    #[test]
    fn passthrough_keeps_the_fn_layer_in_the_app() {
        let mut e = passthrough_engine();
        e.on_keys(&[CUT]);
        let out = e.on_keys(&[CUT, Key::Split.code()]);
        assert!(!out.iter().any(|o| matches!(o, Output::NativeKeys(_))), "CUT and CUT + SPLIT are the app's");
        let out = e.on_jog(crate::native::DIAL_UNITS);
        assert!(has(&out, &Output::ResolveKey("editSelectItemAtPlayhead".into())), "CUT + SPLIT + wheel moves the clip");
    }

    #[test]
    fn passthrough_leds_and_wheel_mode_follow_resolve() {
        let mut e = passthrough_engine();
        // JOG/SHTL/SCRL go to Resolve, which sets the mode on its device.
        assert!(has(&e.on_keys(&[Key::Shuttle.code()]), &Output::NativeKeys(vec![Key::Shuttle.code()])));
        assert!(has(&e.set_resolve_jog_mode(3), &Output::JogMode(3)));
        assert!(has(&e.set_resolve_jog_leds(2), &Output::JogLeds(2)));
        // Resolve's key LEDs show, the app keeps the page keys.
        let in_led = Key::Cam1.led().unwrap(); // a key Resolve lights (multicam angle)
        let cut_led = Key::Cut.led().unwrap();
        let out = e.set_resolve_leds(in_led | cut_led);
        let leds = out.iter().find_map(|o| if let Output::Leds(l) = o { Some(*l) } else { None }).unwrap();
        assert_eq!(leds & in_led, in_led);
        e.tick(1000 + BLINK_ON_MS); // blink off: the app's CUT LED is dark despite Resolve
        let leds = e.desired_leds_for_test();
        assert_eq!(leds & cut_led, 0);
    }

    #[test]
    fn passthrough_normal_mode_taps_transition_keys_on_release() {
        let mut e = passthrough_engine();
        e.on_keys(&[CUT, DIS]);
        e.on_keys(&[]);
        assert_eq!(e.mode(), Mode::Normal);
        e.on_keys(&[DIS]);
        let out = e.on_keys(&[]);
        let i = out.iter().position(|o| *o == Output::NativeKeys(vec![DIS])).expect("DIS pressed on Resolve's device");
        assert!(out[i + 1..].contains(&Output::NativeKeys(vec![])), "and let go");
    }

    const MODIFIER_PROFILE: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve", "smooth_cut"]
        toggle_chord = ["cut", "smooth_cut"]
        fn_key = "cut"
        modifier_keys = ["dissolve", "smooth_cut"]

        [[pages]]
        name = "One"
        fn_keys.cam1 = { keys = "f13" }
        layers.dissolve.cam1 = { keys = "f14" }
        layers.smooth_cut.cam1 = { keys = "f15" }

        [[pages]]
        name = "Two"

        [[pages]]
        name = "Three"
    "#;

    #[test]
    fn each_modifier_has_its_own_layer() {
        let mut e = Engine::new(Profile::from_toml(MODIFIER_PROFILE).unwrap());
        e.on_keys(&[]);
        e.tick(1000);
        for (modifier, wanted) in [(CUT, "f13"), (DIS, "f14"), (SMTH, "f15")] {
            e.on_keys(&[modifier]);
            let out = e.on_keys(&[modifier, CAM1]);
            assert!(has(&out, &Output::KeysDown(shortcut(wanted))), "modifier {modifier:#x}");
            e.on_keys(&[modifier]);
            e.on_keys(&[]);
            assert_eq!(e.page(), 0, "using a modifier doesn't switch pages");
        }
    }

    #[test]
    fn a_quick_tap_of_a_modifier_still_selects_its_page() {
        let mut e = Engine::new(Profile::from_toml(MODIFIER_PROFILE).unwrap());
        e.on_keys(&[]);
        e.tick(1000);
        e.on_keys(&[DIS]);
        e.on_keys(&[]);
        assert_eq!(e.page(), 1);
        // DIS pressed while CUT is held is used, so it doesn't switch pages.
        e.on_keys(&[CUT]);
        e.on_keys(&[CUT, DIS]);
        e.on_keys(&[CUT]);
        e.on_keys(&[]);
        assert_eq!(e.page(), 1);
    }

    #[test]
    fn modifier_layers_survive_saving() {
        let doc = crate::profile::ProfileDoc::from_toml(MODIFIER_PROFILE).unwrap();
        let again = crate::profile::ProfileDoc::from_toml(&doc.to_toml()).unwrap();
        assert_eq!(doc, again);
        assert!(doc.to_toml().contains("[pages.layers.dissolve]"));
    }

    fn davinci_fn_engine() -> Engine {
        let text = DAVINCI_PROFILE
            .replace("toggle_chord = [\"cut\", \"dissolve\"]", "toggle_chord = [\"cut\", \"dissolve\"]\n        fn_key = \"cut\"")
            .replace("wheel = { davinci = {} }", "davinci_defaults = true\n        wheel = { davinci = {} }\n        fn_keys.cam1 = { solo_track = 1 }");
        let mut e = Engine::new(Profile::from_toml(&text).unwrap());
        e.on_keys(&[]);
        e.tick(1000);
        e.on_jog(0); // the report right after the wheel mode is set is skipped
        e
    }

    #[test]
    fn fn_key_and_wheel_zoom_the_timeline() {
        let mut e = davinci_fn_engine();
        e.on_keys(&[CUT]);
        let out = e.on_jog(crate::native::DIAL_UNITS * 2);
        assert!(has(&out, &Output::ResolveKey("viewZoomSubZoomIn".into())));
        e.tick(1100);
        let out = e.on_jog(-crate::native::DIAL_UNITS * 2);
        assert!(has(&out, &Output::ResolveKey("viewZoomSubZoomOut".into())));
        // Letting go of CUT after zooming doesn't switch pages.
        e.on_keys(&[]);
        assert_eq!(e.page(), 0);
    }

    #[test]
    fn video_only_or_audio_only_narrows_the_next_track_choice() {
        let mut e = davinci_fn_engine();
        // A plain press of AUDIO ONLY toggles it on; it lights and isn't sent on.
        let out = e.on_keys(&[Key::AudioOnly.code()]);
        assert!(!out.iter().any(|o| matches!(o, Output::ResolveKey(_) | Output::NativeKeys(_))));
        e.on_keys(&[]);
        assert_ne!(e.desired_leds_for_test() & Key::AudioOnly.led().unwrap(), 0);
        for _ in 0..2 {
            e.on_keys(&[CUT]);
            let out = e.on_keys(&[CUT, CAM1]);
            assert!(has(&out, &Output::SoloTrack { track: 1, video: false, audio: true }), "it lasts");
            e.on_keys(&[]);
        }
        // VIDEO ONLY switches over; pressing it again goes back to both.
        e.on_keys(&[Key::VideoOnly.code()]);
        e.on_keys(&[]);
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, CAM1]), &Output::SoloTrack { track: 1, video: true, audio: false }));
        e.on_keys(&[]);
        e.on_keys(&[Key::VideoOnly.code()]);
        e.on_keys(&[]);
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, CAM1]), &Output::SoloTrack { track: 1, video: true, audio: true }));
    }

    #[test]
    fn fn_close_up_toggles_the_trim_tool() {
        let mut e = davinci_fn_engine();
        e.on_keys(&[CUT]);
        let out = e.on_keys(&[CUT, Key::CloseUp.code()]);
        assert!(has(&out, &Output::ResolveKey("editTrim".into())));
        e.on_keys(&[]);
        assert_ne!(e.desired_leds_for_test() & Key::CloseUp.led().unwrap(), 0, "CLOSE UP lights");
        e.on_keys(&[CUT]);
        assert!(has(&e.on_keys(&[CUT, Key::CloseUp.code()]), &Output::ResolveKey("editPointer".into())));
        e.on_keys(&[]);
        assert_eq!(e.desired_leds_for_test() & Key::CloseUp.led().unwrap(), 0);
        // CLOSE UP on its own keeps its native function.
        let out = e.on_keys(&[Key::CloseUp.code()]);
        assert!(!has(&out, &Output::ResolveKey("editTrim".into())));
    }

    #[test]
    fn device_reset_releases_and_resends_leds() {
        let mut e = engine();
        e.on_keys(&[CAM1]);
        assert_eq!(e.device_reset(), vec![Output::KeysUp(shortcut("ctrl+c"))]);
        assert!(!e.on_keys(&[]).is_empty(), "LEDs are sent again after reconnect");
    }
}
