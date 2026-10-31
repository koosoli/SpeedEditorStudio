//! Runs a profile on a connected Speed Editor in a background thread.
//!
//! DaVinci Resolve opens the Speed Editor exclusively (see docs/FINDINGS.md),
//! so device and Resolve take turns: with `yield_to_resolve`, the runtime
//! lets go of the device as soon as Resolve starts and takes it back when
//! Resolve quits. It reconnects after unplugging and releases every key it
//! pressed on stop, disconnect, hand-over or profile change.

#[cfg(not(windows))]
compile_error!("the runtime's input output layer currently supports Windows only");

pub mod focus;
pub mod menus;
mod output;
mod pacer;
pub mod processes;
pub mod resolve;
mod resolve_keys;

use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use hidapi::HidApi;
use mapper::{Engine, Mode, Output, Profile};
use speed_editor::device::{self, Controller};
use speed_editor::{JogMode, Key, Report};

const POLL: Duration = Duration::from_millis(20);
/// Device poll while wheel steps are being paced against Resolve's playhead.
const PACE_POLL: Duration = Duration::from_millis(2);
const RETRY: Duration = Duration::from_secs(1);
/// How often the user's Resolve keyboard layout is read again.
const LAYOUT_REFRESH: Duration = Duration::from_secs(300);
/// How often to check whether DaVinci Resolve runs.
const RESOLVE_CHECK: Duration = Duration::from_millis(500);

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
        let mut api = match HidApi::new() {
            Ok(api) => api,
            Err(e) => {
                (self.on_event)(Event::Waiting(format!("cannot initialise HID: {e}")));
                (self.on_event)(Event::Stopped);
                return;
            }
        };
        let mut last_wait = String::new();
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
            interfaces.sort_by_key(|i| i.bus != "Usb");
            let bus = interfaces.first().map(|i| if i.bus == "Usb" { "USB" } else { "Bluetooth" });
            let controller = interfaces
                .first()
                .ok_or_else(|| "Speed Editor not found".to_string())
                .and_then(|i| Controller::open(&api, i).map_err(|e| e.to_string()));
            let mut controller = match controller {
                Ok(c) => c,
                Err(e) => {
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
                        (self.on_event)(Event::Jog { mode, value });
                        self.engine.on_jog(value)
                    }
                    Report::Battery { charging, level } => {
                        (self.on_event)(Event::Battery { charging, level });
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
            for reply in self.resolve.replies() {
                if self.pacer.on_reply(reply.id, reply.ok, &reply.message, Instant::now()) {
                    continue;
                }
                if reply.id.is_some() && reply.id == self.layout_request {
                    self.layout_request = None;
                    match reply.ok.then(|| resolve_keys::KeyLayout::from_json(&reply.message)) {
                        Some(Ok(layout)) => {
                            let name = layout.preset.clone().unwrap_or_default();
                            self.keys = layout;
                            (self.on_event)(Event::Status(format!("Resolve keyboard layout: {name}")));
                        }
                        // Resolve not running yet: keep the defaults, ask again later.
                        _ => {}
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

    /// Lets only video track n and audio track n take part in Resolve's edits
    /// (n = 0: all tracks) through Resolve's Timeline › Auto Select menu, which
    /// has absolute "all on / all off" items, so no state needs remembering.
    fn solo_track(&mut self, n: u8) {
        let auto = |item: &str| vec!["Timeline".to_string(), "Auto Select".to_string(), item.to_string()];
        let steps = if n == 0 {
            vec![auto("Auto Select All Tracks")]
        } else {
            vec![
                auto("Auto Deselect All Tracks"),
                auto(&format!("Auto Select/Deselect V{n}")),
                auto(&format!("Auto Select/Deselect A{n}")),
            ]
        };
        for path in steps {
            if let Err(e) = self.run_menu(&path) {
                (self.on_event)(Event::Status(format!("Resolve: {e}")));
                return;
            }
        }
        let text = if n == 0 {
            "all tracks take part in edits again".to_string()
        } else {
            format!("only V{n} + A{n} take part in edits now")
        };
        (self.on_event)(Event::Status(text));
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
                Output::Status(text) => (self.on_event)(Event::Status(text)),
                _ if !self.active => {}
                Output::KeysDown(s) => self.input.press(&s),
                Output::Tap(s) => self.input.tap(&s),
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
                Output::SoloTrack(n) => self.solo_track(n),
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
