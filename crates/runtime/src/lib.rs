//! Runs a profile on a connected Speed Editor in a background thread.
//!
//! DaVinci Resolve opens the Speed Editor exclusively (see docs/FINDINGS.md),
//! so device and Resolve take turns: with `yield_to_resolve`, the runtime
//! lets go of the device as soon as Resolve starts and takes it back when
//! Resolve quits. It reconnects after unplugging and releases every key it
//! pressed on stop, disconnect, hand-over or profile change.

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("the runtime's input and output layer supports Windows and macOS");

// The parts that talk to the operating system have one file per platform.
// The macOS ones are only partly tested (see README, "macOS").
#[cfg_attr(windows, path = "focus_windows.rs")]
#[cfg_attr(target_os = "macos", path = "focus_macos.rs")]
pub mod focus;
#[cfg(target_os = "macos")]
mod mac;
#[cfg_attr(windows, path = "menus_windows.rs")]
#[cfg_attr(target_os = "macos", path = "menus_macos.rs")]
pub mod menus;
#[cfg_attr(windows, path = "output_windows.rs")]
#[cfg_attr(target_os = "macos", path = "output_macos.rs")]
mod output;
mod pacer;
#[cfg_attr(windows, path = "processes_windows.rs")]
#[cfg_attr(target_os = "macos", path = "processes_macos.rs")]
pub mod processes;
pub mod resolve;
mod resolve_keys;
#[cfg_attr(windows, path = "virtual_device_windows.rs")]
#[cfg_attr(target_os = "macos", path = "virtual_device_macos.rs")]
mod virtual_device;

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hidapi::HidApi;
use mapper::{Engine, Mode, Output, Profile};
use speed_editor::device::{self, Controller};
use speed_editor::emulator::{HostReport, key_report};
use speed_editor::{JogMode, Key, Report};

const POLL: Duration = Duration::from_millis(20);
/// Device poll while wheel steps are being paced against Resolve's playhead.
const PACE_POLL: Duration = Duration::from_millis(2);
const RETRY: Duration = Duration::from_secs(1);
/// How often the user's Resolve keyboard layout is read again.
const LAYOUT_REFRESH: Duration = Duration::from_secs(300);
/// How long to wait for Resolve to apply a selection key.
const SELECT_TRACK_SETTLE: Duration = Duration::from_millis(600);
/// How often to check whether DaVinci Resolve runs.
const RESOLVE_CHECK: Duration = Duration::from_millis(500);

/// The HID context of this run. Windows makes a fresh one each time.
#[cfg(not(target_os = "macos"))]
fn open_hid() -> Result<impl std::ops::DerefMut<Target = HidApi>, String> {
    HidApi::new().map(Box::new).map_err(|e| e.to_string())
}

/// The HID context of this run. On macOS hidapi ties its device manager to the run loop
/// of the thread that initialised it, and every later call crashes once that thread is
/// gone, e.g. after Disconnect and Connect. So there is one context for the whole
/// process, created on the thread that first starts a `Runtime` (the main thread, whose
/// run loop lives as long as the app) and never dropped.
#[cfg(target_os = "macos")]
fn open_hid() -> Result<impl std::ops::DerefMut<Target = HidApi>, String> {
    use std::sync::{Mutex, OnceLock, PoisonError};
    static API: OnceLock<Result<Mutex<HidApi>, String>> = OnceLock::new();
    match API.get_or_init(|| HidApi::new().map(Mutex::new).map_err(|e| e.to_string())) {
        Ok(api) => Ok(api.lock().unwrap_or_else(PoisonError::into_inner)),
        Err(e) => Err(e.clone()),
    }
}

/// Something the runtime wants its owner to know.
#[derive(Clone, Debug)]
pub enum Event {
    /// No device yet; the text says why.
    Waiting(String),
    /// Connected; `bus` is "USB" or "Bluetooth".
    Connected { bus: String },
    /// The key LED bitfield just sent to the device (see `Key::led`).
    Leds(u32),
    /// Wheel movement as reported (`mode` is the raw jog mode).
    Jog { mode: u8, value: i32 },
    /// Battery state, reported by the device from time to time.
    Battery { charging: bool, level: u8 },
    Disconnected(String),
    /// Every key currently held, after each key report.
    Keys(Vec<Key>),
    /// Current mode and page, sent on connect and whenever either changes.
    State { custom: bool, page: usize },
    /// Mode or page change, or a failed action.
    Status(String),
    /// DaVinci Resolve is running, so the device was left (or handed) to it.
    YieldedToResolve,
    Stopped,
}

/// How the runtime behaves.
#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// When false, key presses are only reported as events and no mapping
    /// fires (useful while editing).
    pub active: bool,
    /// Leave the device to DaVinci Resolve while it runs.
    pub yield_to_resolve: bool,
    /// Hand native keys and the wheel to Resolve through an emulated Speed
    /// Editor (needs usbip-win2); otherwise they are imitated with shortcuts.
    pub pass_through: bool,
}

enum Command {
    SetProfile(Profile),
    SetActive(bool),
    SetYield(bool),
    Stop,
}

/// Why a session with the device ended.
enum SessionEnd {
    Stopped,
    Resolve,
    Failed(String),
}

/// Handle to a running runtime. Dropping it stops the runtime.
pub struct Runtime {
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Runtime {
    /// Starts the device loop.
    pub fn spawn(profile: Profile, options: Options, on_event: impl Fn(Event) + Send + 'static) -> Runtime {
        // Only the main thread may read the keyboard layout (see mac::capture_layout);
        // both apps start the runtime from it.
        #[cfg(target_os = "macos")]
        {
            mac::capture_layout();
            let _ = open_hid();
        }
        let (commands, receiver) = mpsc::channel();
        let thread = thread::spawn(move || Loop::new(profile, options, receiver, on_event).run());
        Runtime { commands, thread: Some(thread) }
    }

    pub fn set_profile(&self, profile: Profile) {
        let _ = self.commands.send(Command::SetProfile(profile));
    }

    pub fn set_active(&self, active: bool) {
        let _ = self.commands.send(Command::SetActive(active));
    }

    pub fn set_yield_to_resolve(&self, yield_to_resolve: bool) {
        let _ = self.commands.send(Command::SetYield(yield_to_resolve));
    }

    /// Stops the runtime and waits until every key is released.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for Runtime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct Loop<F> {
    engine: Engine,
    active: bool,
    yield_to_resolve: bool,
    input: output::Input,
    resolve: resolve::ResolveBridge,
    /// The user's Resolve keyboard layout, for keys' native functions.
    keys: resolve_keys::KeyLayout,
    /// Pending `keyboard_layout` request and when the layout was last asked for.
    layout_request: Option<u64>,
    layout_asked: Option<Instant>,
    /// Clicks Resolve menu items (started on first use).
    menus: Option<menus::Menus>,
    /// Paces the wheel's playhead steps so they never queue up in Resolve.
    pacer: pacer::Pacer,
    /// Debug log of reports and outputs, when OPEN_SPEED_EDITOR_LOG names a file.
    log: Option<std::fs::File>,
    /// Pass-through: the emulated Speed Editor Resolve uses, and the last
    /// wheel report (handed on as it is).
    pass_through: bool,
    virtual_device: Option<virtual_device::VirtualDevice>,
    last_jog: [u8; 7],
    /// The native keys last handed to Resolve (key codes).
    native_sent: Vec<u16>,
    commands: Receiver<Command>,
    on_event: F,
    clock: Instant,
    stopping: bool,
    /// Last state reported through `Event::State`.
    reported: Option<(bool, usize)>,
}

impl<F: Fn(Event)> Loop<F> {
    fn new(profile: Profile, options: Options, commands: Receiver<Command>, on_event: F) -> Self {
        Loop {
            engine: Engine::new(profile),
            active: options.active,
            yield_to_resolve: options.yield_to_resolve,
            input: output::Input::default(),
            resolve: resolve::ResolveBridge::default(),
            keys: resolve_keys::KeyLayout::default(),
            layout_request: None,
            layout_asked: None,
            menus: None,
            pacer: pacer::Pacer::default(),
            log: std::env::var_os("OPEN_SPEED_EDITOR_LOG").and_then(|p| std::fs::File::create(p).ok()),
            pass_through: options.pass_through,
            virtual_device: None,
            last_jog: [3, 0, 0, 0, 0, 0, 0xff],
            native_sent: Vec::new(),
            commands,
            on_event,
            clock: Instant::now(),
            stopping: false,
            reported: None,
        }
    }

    fn report_state(&mut self) {
        let state = (self.engine.mode() == Mode::Custom, self.engine.page());
        if self.reported != Some(state) {
            self.reported = Some(state);
            (self.on_event)(Event::State { custom: state.0, page: state.1 });
        }
    }

    fn run(mut self) {
        let mut api = match open_hid() {
            Ok(api) => api,
            Err(e) => {
                (self.on_event)(Event::Waiting(format!("cannot initialise HID: {e}")));
                (self.on_event)(Event::Stopped);
                return;
            }
        };
        let mut last_wait = String::new();
        #[cfg(target_os = "macos")]
        if !mac::accessibility_trusted(true) {
            (self.on_event)(Event::Status(
                "macOS: allow Speed Editor Studio under System Settings › Privacy & Security › Accessibility, \
                 then restart it; until then it can't send keys to Resolve"
                    .into(),
            ));
        }
        if self.pass_through {
            match virtual_device::VirtualDevice::start() {
                Ok(device) => {
                    self.virtual_device = Some(device);
                    (self.on_event)(Event::Status("Resolve pass-through on: native keys and the wheel go to Resolve unchanged".into()));
                }
                Err(e) => (self.on_event)(Event::Status(format!("Resolve pass-through off ({e}); native keys use Resolve's shortcuts"))),
            }
        }
        while !self.stopping {
            if self.yield_to_resolve && processes::is_running(processes::RESOLVE_EXE) {
                if last_wait != "resolve" {
                    (self.on_event)(Event::YieldedToResolve);
                    last_wait = "resolve".into();
                }
                self.idle(RESOLVE_CHECK);
                continue;
            }
            let _ = api.refresh_devices();
            // Prefer the cable when the device is connected both ways.
            let mut interfaces = device::interfaces(&api);
            // Never our own emulated Speed Editor.
            interfaces.retain(|i| i.serial.as_deref() != Some(virtual_device::SERIAL));
            interfaces.sort_by_key(|i| i.bus != "Usb");
            let bus = interfaces.first().map(|i| if i.bus == "Usb" { "USB" } else { "Bluetooth" });
            let controller = interfaces
                .first()
                .ok_or_else(|| "Speed Editor not found".to_string())
                .and_then(|i| Controller::open(&api, i).map_err(|e| e.to_string()));
            let mut controller = match controller {
                Ok(c) => c,
                // The device is there but can't be opened while Resolve runs:
                // Resolve has it.
                Err(_) if !interfaces.is_empty() && processes::is_running(processes::RESOLVE_EXE) => {
                    let e = if self.virtual_device.is_some() {
                        "DaVinci Resolve has the Speed Editor. Quit Resolve and start it again: \
                         the app then takes the Speed Editor and Resolve uses the emulated one."
                    } else {
                        "DaVinci Resolve has the Speed Editor. Quit Resolve, then start it after this app."
                    }
                    .to_string();
                    if e != last_wait {
                        (self.on_event)(Event::Waiting(e.clone()));
                        last_wait = e;
                    }
                    self.idle(RETRY);
                    continue;
                }
                Err(e) => {
                    #[cfg(target_os = "macos")]
                    let e = if !interfaces.is_empty() && !mac::input_monitoring_allowed(true) {
                        format!(
                            "{e} (macOS: allow Speed Editor Studio under System Settings › Privacy & Security › \
                             Input Monitoring, then reconnect; a Bluetooth Speed Editor needs it)"
                        )
                    } else {
                        e
                    };
                    if e != last_wait {
                        (self.on_event)(Event::Waiting(e.clone()));
                        last_wait = e;
                    }
                    self.idle(RETRY);
                    continue;
                }
            };
            last_wait.clear();
            (self.on_event)(Event::Connected { bus: bus.unwrap_or("USB").to_string() });

            let end = self.session(&mut controller);
            let reset = self.engine.device_reset();
            let _ = self.apply(reset, &controller);
            match end {
                // Stopped or handing over: leave the device dark.
                SessionEnd::Stopped | SessionEnd::Resolve => {
                    let _ = controller.set_leds(0);
                    (self.on_event)(Event::Leds(0));
                    let _ = controller.set_jog_leds(0);
                }
                SessionEnd::Failed(e) => (self.on_event)(Event::Disconnected(e)),
            }
            // Closing the device now lets Resolve open it.
            drop(controller);
        }
        self.input.release_all();
        (self.on_event)(Event::Stopped);
    }

    fn session(&mut self, controller: &mut Controller) -> SessionEnd {
        match self.run_session(controller) {
            Ok(end) => end,
            Err(e) => SessionEnd::Failed(e),
        }
    }

    /// Runs until stopped, Resolve starts, or the device fails (Err).
    fn run_session(&mut self, controller: &mut Controller) -> Result<SessionEnd, String> {
        // The engine sends the wheel setup (jog mode and its LEDs) with its
        // first outputs, matching what Resolve would set on DaVinci pages.
        let initial = self.engine.on_keys(&[]);
        self.apply(initial, controller)?;
        self.sync_virtual_device(controller)?;
        self.reported = None;
        self.refresh_layout();
        self.report_state();

        let mut buf = [0u8; 64];
        let mut resolve_checked = Instant::now();
        while !self.handle_commands(controller)? {
            if self.yield_to_resolve && resolve_checked.elapsed() >= RESOLVE_CHECK {
                resolve_checked = Instant::now();
                if processes::is_running(processes::RESOLVE_EXE) {
                    return Ok(SessionEnd::Resolve);
                }
            }
            controller.keep_alive().map_err(|e| e.to_string())?;
            let poll = if self.pacer.busy() { PACE_POLL } else { POLL };
            let n = controller.read(&mut buf, poll).map_err(|e| e.to_string())?;
            if n > 0 {
                let outputs = match Report::parse(&buf[..n]) {
                    Report::Keys(held) => {
                        (self.on_event)(Event::Keys(held.iter().filter_map(|&c| Key::from_code(c)).collect()));
                        self.engine.on_keys(&held)
                    }
                    Report::Jog { mode, value } => {
                        self.debug(format_args!("jog mode {mode} value {value}"));
                        if n >= 7 {
                            self.last_jog.copy_from_slice(&buf[..7]);
                        }
                        (self.on_event)(Event::Jog { mode, value });
                        self.engine.on_jog(value)
                    }
                    Report::Battery { charging, level } => {
                        (self.on_event)(Event::Battery { charging, level });
                        if let Some(device) = &self.virtual_device {
                            device.emulator.send_input(&buf[..n.min(3)]);
                        }
                        Vec::new()
                    }
                    Report::Other(_) => Vec::new(),
                };
                self.apply(outputs, controller)?;
                self.report_state();
            }
            let tick = self.engine.tick(self.clock.elapsed().as_millis() as u64);
            self.apply(tick, controller)?;
            self.pump_pacer();
            self.sync_virtual_device(controller)?;
            for reply in self.resolve.replies() {
                if self.pacer.on_reply(reply.id, reply.ok, &reply.message, Instant::now()) {
                    continue;
                }
                if reply.id.is_some() && reply.id == self.layout_request {
                    self.layout_request = None;
                    // Resolve not running yet: keep the defaults, ask again later.
                    if let Some(Ok(layout)) = reply.ok.then(|| resolve_keys::KeyLayout::from_json(&reply.message)) {
                        let name = layout.preset.clone().unwrap_or_default();
                        self.keys = layout;
                        (self.on_event)(Event::Status(format!("Resolve keyboard layout: {name}")));
                    }
                    continue;
                }
                let mark = if reply.ok { "Resolve" } else { "Resolve error" };
                (self.on_event)(Event::Status(format!("{mark}: {}", reply.message)));
            }
        }
        Ok(SessionEnd::Stopped)
    }

    /// Applies pending commands. Returns true when the loop should stop.
    fn handle_commands(&mut self, controller: &Controller) -> Result<bool, String> {
        loop {
            match self.commands.try_recv() {
                Ok(Command::SetProfile(profile)) => {
                    self.pacer.clear();
                    let released = self.engine.release_all();
                    self.apply(released, controller)?;
                    self.engine = Engine::new(profile);
                    let initial = self.engine.on_keys(&[]);
                    self.apply(initial, controller)?;
                    (self.on_event)(Event::Status(format!("profile updated, page 1: {}", self.engine.page_name())));
                    self.report_state();
                }
                Ok(Command::SetActive(active)) => {
                    if !active {
                        let released = self.engine.release_all();
                        self.apply(released, controller)?;
                    }
                    self.active = active;
                }
                Ok(Command::SetYield(on)) => self.yield_to_resolve = on,
                Ok(Command::Stop) | Err(TryRecvError::Disconnected) => {
                    self.stopping = true;
                    return Ok(true);
                }
                Err(TryRecvError::Empty) => return Ok(false),
            }
        }
    }

    /// Waits without a device, still answering commands.
    fn idle(&mut self, total: Duration) {
        let start = Instant::now();
        while start.elapsed() < total && !self.stopping {
            match self.commands.recv_timeout(POLL) {
                Ok(Command::SetProfile(profile)) => self.engine = Engine::new(profile),
                Ok(Command::SetActive(active)) => self.active = active,
                Ok(Command::SetYield(on)) => self.yield_to_resolve = on,
                Ok(Command::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => self.stopping = true,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }

    /// Presses the user's Resolve shortcut for a keyboard command, after making
    /// sure Resolve has keyboard focus.
    fn press_resolve_command(&mut self, command: &str) -> Result<(), String> {
        self.refresh_layout();
        match focus::ensure_resolve_in_front() {
            focus::Focus::Fine => {}
            focus::Focus::Restored(title) => {
                (self.on_event)(Event::Status(format!("brought Resolve back to the front (focus was on \"{title}\")")));
            }
            focus::Focus::Failed(title) => {
                (self.on_event)(Event::Status(format!("could not bring Resolve to the front (focus is on \"{title}\")")));
            }
        }
        let shortcut = self.keys.shortcut(command)?;
        self.input.tap(&shortcut);
        Ok(())
    }

    /// Lets only track n take part in Resolve's edits — its video track, its
    /// audio track or both — (n = 0: all tracks) through Resolve's Timeline ›
    /// Auto Select menu, which has absolute "all on / all off" items, so no
    /// state needs remembering.
    fn solo_track(&mut self, n: u8, video: bool, audio: bool) {
        let auto = |item: &str| vec!["Timeline".to_string(), "Auto Select".to_string(), item.to_string()];
        let mut steps = Vec::new();
        if n == 0 {
            steps.push(auto("Auto Select All Tracks"));
        } else {
            steps.push(auto("Auto Deselect All Tracks"));
            if video {
                steps.push(auto(&format!("Auto Select/Deselect V{n}")));
            }
            if audio {
                steps.push(auto(&format!("Auto Select/Deselect A{n}")));
            }
        }
        for path in steps {
            if let Err(e) = self.run_menu(&path) {
                (self.on_event)(Event::Status(format!("Resolve: {e}")));
                return;
            }
        }
        let text = match (n, video, audio) {
            (0, ..) => "all tracks take part in edits again".to_string(),
            (_, true, false) => format!("only V{n} takes part in edits now"),
            (_, false, true) => format!("only A{n} takes part in edits now"),
            _ => format!("only V{n} + A{n} take part in edits now"),
        };
        (self.on_event)(Event::Status(text));
    }

    /// Selects the clip under the playhead on track n — its video with the
    /// linked sound, only its video, or only its audio (docs/FINDINGS.md,
    /// "Selecting a clip on a track"). "Select at playhead" takes the clip
    /// under the playhead on the topmost auto-selected track, so when that
    /// isn't track n already, track n becomes the auto-selected one first (a
    /// Resolve menu, like CUT + CAM). Stepping the selection to other tracks
    /// is not used: it goes by the selected clip's start, not the playhead.
    fn select_track(&mut self, n: u8, video: bool, audio: bool) {
        let wanted = format!("{}{n}", if video { 'V' } else { 'A' });
        let mut unlinked = false;
        let result = self.select_at_playhead_on(n, video, audio, &wanted, &mut unlinked);
        if unlinked {
            // Always switch linked selection back on: it is a project-wide
            // setting the user expects to stay as it was.
            let _ = self.press_resolve_command("editLinkedSelection");
            let after = self.read_selection().map(|(t, _)| t).unwrap_or_default();
            thread::sleep(Duration::from_millis(250));
            let later = self.read_selection().map(|(t, _)| t).unwrap_or_default();
            self.debug(format_args!("select {wanted}: linked selection back on, selection {after:?} then {later:?}"));
        }
        let text = match result {
            Ok(()) => format!("selected the clip on {wanted}"),
            Err(e) => {
                self.debug(format_args!("select {wanted} failed: {e}"));
                e
            }
        };
        (self.on_event)(Event::Status(text));
    }

    fn select_at_playhead_on(
        &mut self,
        n: u8,
        video: bool,
        audio: bool,
        wanted: &str,
        unlinked: &mut bool,
    ) -> Result<(), String> {
        let resolve_err = |e: String| format!("Resolve: {e}");
        let unreadable = || "Resolve: could not read the selection (is scripting set to Local?)".to_string();
        let only = !(video && audio);
        let fits = |t: &[String]| t.iter().any(|x| x == wanted) && (!only || t.len() == 1);
        let linked = |t: &[String]| t.iter().any(|x| x.starts_with('V')) && t.iter().any(|x| x.starts_with('A'));

        // Quick path: the target may already be the topmost auto-selected track.
        let mut tracks = self.reselect_at_playhead().ok_or_else(unreadable)?;
        if fits(&tracks) && !(!only && self.linked_part_unselected()) {
            return Ok(());
        }
        if only && linked(&tracks) {
            // Linked selection is on: switch it off and select again.
            self.debug(format_args!("select {wanted}: {tracks:?} is linked, switching linked selection off"));
            self.press_resolve_command("editLinkedSelection").map_err(resolve_err)?;
            *unlinked = true;
            tracks = self.reselect_at_playhead().ok_or_else(unreadable)?;
            if fits(&tracks) {
                return Ok(());
            }
        }
        if !fits(&tracks) {
            // "Select at playhead" takes the topmost auto-selected track: make
            // that track n (or only its video / its audio) and select again.
            self.debug(format_args!("select {wanted}: got {tracks:?}, auto-selecting track {n}"));
            self.solo_track(n, video, audio);
            tracks = self.reselect_at_playhead().ok_or_else(unreadable)?;
            if fits(&tracks) && !(!only && self.linked_part_unselected()) {
                return Ok(());
            }
        }
        if !only && tracks.iter().any(|x| x == wanted) && self.linked_part_unselected() {
            // Video and sound together, but linked selection is off: switch
            // it on and select again.
            self.debug(format_args!("select {wanted}: linked part not selected, switching linked selection on"));
            self.press_resolve_command("editLinkedSelection").map_err(resolve_err)?;
            tracks = self.reselect_at_playhead().ok_or_else(unreadable)?;
            if fits(&tracks) {
                return Ok(());
            }
        }
        self.debug(format_args!("select {wanted}: ended with {tracks:?}"));
        Err(format!("no clip under the playhead on {wanted}"))
    }

    /// Deselects everything and selects at the playhead; returns the tracks
    /// selected under the playhead once Resolve shows the new selection.
    fn reselect_at_playhead(&mut self) -> Option<Vec<String>> {
        let before = self.read_selection()?.0;
        self.press_resolve_command("editDeselectAll").ok()?;
        self.press_resolve_command("editSelectItemAtPlayhead").ok()?;
        self.wait_for_selection(&before).map(|(t, _)| t)
    }

    /// Whether a selected clip has a linked part that isn't selected, i.e.
    /// Resolve's linked selection is off.
    fn linked_part_unselected(&mut self) -> bool {
        let Ok(reply) = self.resolve.request("selection", &Default::default(), Duration::from_secs(2)) else {
            return false;
        };
        serde_json::from_str::<serde_json::Value>(&reply)
            .ok()
            .and_then(|v| v["unselected_links"].as_array().map(|a| !a.is_empty()))
            .unwrap_or(false)
    }

    /// The tracks selected under the playhead, and the number of video tracks.
    /// Resolve sometimes refuses the query right after a selection change;
    /// it is asked again a few times.
    fn read_selection(&mut self) -> Option<(Vec<String>, usize)> {
        let mut reply = Err(String::new());
        for attempt in 0..4 {
            reply = self.resolve.request("selection", &Default::default(), Duration::from_secs(2));
            match &reply {
                Ok(_) => break,
                Err(e) => {
                    self.debug(format_args!("reading the selection failed (attempt {}): {e}", attempt + 1));
                    thread::sleep(Duration::from_millis(80));
                }
            }
        }
        let value: serde_json::Value = serde_json::from_str(&reply.ok()?).ok()?;
        let tracks = value["tracks"].as_array()?.iter().filter_map(|t| t.as_str().map(str::to_owned)).collect();
        Some((tracks, value["video_tracks"].as_u64().unwrap_or(0) as usize))
    }

    /// Asks the helper for the selected tracks until they differ from
    /// `previous` or a short time has passed (Resolve applies keys
    /// asynchronously).
    fn wait_for_selection(&mut self, previous: &[String]) -> Option<(Vec<String>, usize)> {
        let start = Instant::now();
        loop {
            let (tracks, video) = self.read_selection()?;
            // An empty selection is usually a moment between "deselect" and
            // "select" taking effect, not the result.
            let changed = tracks != previous && !tracks.is_empty();
            if changed || start.elapsed() >= SELECT_TRACK_SETTLE {
                self.debug(format_args!("selection: {tracks:?}"));
                return Some((tracks, video));
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    /// Clicks a Resolve menu item, starting the menu engine on first use.
    fn run_menu(&mut self, path: &[String]) -> Result<(), String> {
        if self.menus.is_none() {
            self.menus = Some(menus::Menus::new()?);
        }
        let menus = self.menus.as_mut().unwrap();
        menus.invoke(path)?;
        // Give the keyboard back to the timeline if the menu bar kept it.
        let esc = mapper::keyboard::Shortcut::parse("esc").expect("valid shortcut");
        for _ in 0..2 {
            if !menus.menu_has_focus() {
                break;
            }
            self.input.tap(&esc);
            std::thread::sleep(Duration::from_millis(15));
        }
        Ok(())
    }

    /// Pass-through: follows whether the emulated Speed Editor is attached,
    /// and applies the LEDs and wheel mode Resolve set on it.
    fn sync_virtual_device(&mut self, controller: &Controller) -> Result<(), String> {
        let Some(device) = &self.virtual_device else { return Ok(()) };
        let attached = device.emulator.attached();
        let reports = device.emulator.host_reports();
        for event in device.emulator.events() {
            self.debug(format_args!("emulated Speed Editor: {event:?}"));
        }
        let mut outputs = Vec::new();
        if attached != self.engine.passthrough() {
            outputs.extend(self.engine.set_passthrough(attached));
        }
        for report in reports {
            self.debug(format_args!("Resolve set {report:?}"));
            outputs.extend(match report {
                HostReport::KeyLeds(leds) => self.engine.set_resolve_leds(leds),
                HostReport::JogMode(mode) => self.engine.set_resolve_jog_mode(mode),
                HostReport::JogLeds(leds) => self.engine.set_resolve_jog_leds(leds),
                HostReport::Other(_) => Vec::new(),
            });
        }
        self.apply(outputs, controller)
    }

    /// Sends the wheel steps Resolve is ready for and asks for its playhead.
    fn pump_pacer(&mut self) {
        if !self.active {
            self.pacer.clear();
            return;
        }
        let now = Instant::now();
        let (taps, query) = self.pacer.pump(now);
        for tap in &taps {
            self.debug(format_args!("paced {tap:?}"));
            self.input.tap(tap);
        }
        if query {
            match self.resolve.send("playhead", &Default::default()) {
                Ok(id) => self.pacer.queried(id, now),
                // No helper: let the steps go straight through.
                Err(_) => self.pacer.fail(now),
            }
        }
    }

    /// Asks Resolve for the user's keyboard layout, at most every few minutes.
    fn refresh_layout(&mut self) {
        let due = self.layout_asked.is_none_or(|t| t.elapsed() >= LAYOUT_REFRESH);
        if self.layout_request.is_none() && due {
            self.layout_asked = Some(Instant::now());
            self.layout_request = self.resolve.send("keyboard_layout", &Default::default()).ok();
        }
    }

    fn debug(&mut self, what: std::fmt::Arguments) {
        if let Some(log) = &mut self.log {
            use std::io::Write;
            let _ = writeln!(log, "{:>9} {what}", self.clock.elapsed().as_millis());
        }
    }

    /// SOURCE opens the Source Tape on the Cut page. On the Edit page Resolve
    /// only switches to the source viewer, so the app also switches that
    /// viewer between Source Clip and Source Tape, as on the Cut page.
    fn source_on_edit_page(&mut self) {
        match self.resolve.request("current_page", &Default::default(), Duration::from_secs(1)) {
            Ok(page) if page.trim() == "edit" => {
                if let Err(e) = self.press_resolve_command("viewViewerSourceModeToggle") {
                    (self.on_event)(Event::Status(format!("Resolve: {e}")));
                }
            }
            Ok(_) => {}
            Err(e) => self.debug(format_args!("reading Resolve's page failed: {e}")),
        }
    }

    fn apply(&mut self, outputs: Vec<Output>, controller: &Controller) -> Result<(), String> {
        for o in outputs {
            self.debug(format_args!("{}{o:?}", if self.active { "" } else { "(inactive) " }));
            match o {
                // LEDs and releases always go through, so nothing stays stuck
                // when the runtime is switched to inactive.
                Output::Leds(leds) => {
                    controller.set_leds(leds).map_err(|e| e.to_string())?;
                    (self.on_event)(Event::Leds(leds));
                }
                Output::JogMode(mode) => {
                    let mode = JogMode::from_raw(mode).unwrap_or(JogMode::Relative);
                    controller.set_jog_mode(mode).map_err(|e| e.to_string())?;
                }
                Output::JogLeds(leds) => controller.set_jog_leds(leds).map_err(|e| e.to_string())?,
                Output::KeysUp(s) => self.input.release(&s),
                // Letting go of native keys always reaches Resolve.
                Output::NativeKeys(codes) if codes.is_empty() || self.active => {
                    if let Some(device) = &self.virtual_device {
                        device.emulator.send_input(&key_report(&codes));
                    }
                    let source = Key::Source.code();
                    let source_pressed = codes.contains(&source) && !self.native_sent.contains(&source);
                    self.native_sent = codes;
                    if source_pressed {
                        self.source_on_edit_page();
                    }
                }
                Output::NativeKeys(_) => {}
                Output::Status(text) => (self.on_event)(Event::Status(text)),
                _ if !self.active => {}
                Output::KeysDown(s) => self.input.press(&s),
                Output::Tap(s) => self.input.tap(&s),
                Output::NativeJog => {
                    if let Some(device) = &self.virtual_device {
                        device.emulator.send_input(&self.last_jog);
                    }
                }
                Output::PlayheadSteps { steps, forward, reverse, frames } => {
                    self.pacer.add(steps, forward, reverse, frames, Instant::now());
                    self.pump_pacer();
                }
                Output::Type(text) => self.input.type_text(&text),
                Output::Resolve { command, params } => {
                    if let Err(e) = self.resolve.send(&command, &params) {
                        (self.on_event)(Event::Status(format!("Resolve error: {e}")));
                    }
                }
                Output::ResolveKey(command) => {
                    if let Err(e) = self.press_resolve_command(&command) {
                        (self.on_event)(Event::Status(format!("Resolve: {e}")));
                    }
                }
                Output::SoloTrack { track, video, audio } => self.solo_track(track, video, audio),
                Output::SelectTrack { track, video, audio } => self.select_track(track, video, audio),
                Output::ResolveMenu(path) => {
                    if let Err(e) = self.run_menu(&path) {
                        (self.on_event)(Event::Status(format!("Resolve: {e}")));
                    }
                }
                Output::Scroll { steps, horizontal } => self.input.scroll(steps, horizontal),
                Output::Run(target) => {
                    if let Err(e) = output::run(&target) {
                        (self.on_event)(Event::Status(format!("cannot run {target:?}: {e}")));
                    }
                }
            }
        }
        Ok(())
    }
}
