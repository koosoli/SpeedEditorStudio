//! se-probe: diagnostic tool for the Speed Editor protocol.
//!
//!   se-probe list                 show the device's HID interfaces
//!   se-probe descriptor           print each interface's HID report descriptor (works while Resolve runs)
//!   se-probe watch   [options]    read-only: never sends anything to the device
//!   se-probe control [options]    authenticate and own the device (Resolve closed!)
//!
//! Options:
//!   --raw            also print raw report bytes
//!   --log FILE       append every report as a JSON line to FILE
//!   --seconds N      stop after N seconds (default: until Ctrl+C)

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use hidapi::HidApi;
use speed_editor::device::{self, Controller, Interface, Observer, hex};
use speed_editor::{JogMode, Key, KeyTracker, Report, jog_led, key_name};

const POLL: Duration = Duration::from_millis(20);

struct Options {
    raw: bool,
    log: Option<File>,
    seconds: Option<u64>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        eprintln!("usage: se-probe <list|descriptor|watch|control> [--raw] [--log FILE] [--seconds N]");
        return ExitCode::from(2);
    };
    let options = match parse_options(&args[1..]) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("cannot initialise HID: {e}");
            return ExitCode::FAILURE;
        }
    };

    let result = match command {
        "list" => list(&api),
        "descriptor" => descriptor(&api),
        "watch" => watch(&api, options),
        "control" => control(&api, options),
        other => Err(format!("unknown command {other:?}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options { raw: false, log: None, seconds: None };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--raw" => options.raw = true,
            "--log" => {
                let path = it.next().ok_or("--log needs a file name")?;
                let file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .map_err(|e| format!("cannot open {path}: {e}"))?;
                options.log = Some(file);
            }
            "--seconds" => {
                let n = it.next().ok_or("--seconds needs a number")?;
                options.seconds = Some(n.parse().map_err(|_| format!("bad number {n:?}"))?);
            }
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    Ok(options)
}

fn list(api: &HidApi) -> Result<(), String> {
    let interfaces = device::interfaces(api);
    if interfaces.is_empty() {
        println!("No Speed Editor HID interface found.");
    }
    for i in &interfaces {
        println!(
            "interface {}  usage {:04x}:{:04x}  bus {}  {}\n  path {}",
            i.interface_number,
            i.usage_page,
            i.usage,
            i.bus,
            i.product.as_deref().unwrap_or("?"),
            i.path.to_string_lossy(),
        );
    }
    Ok(())
}

/// Prints the HID report descriptor of every interface. On Windows, hidapi
/// rebuilds it from the parsed data, so it describes the same reports but may
/// not match the device's bytes exactly.
fn descriptor(api: &HidApi) -> Result<(), String> {
    for i in find_interfaces(api)? {
        let device = api.open_path(&i.path).map_err(|e| format!("cannot open {}: {e}", i.path.to_string_lossy()))?;
        let mut buf = [0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
        let n = device.get_report_descriptor(&mut buf).map_err(|e| e.to_string())?;
        println!("interface {} usage {:04x}:{:04x}, {n} bytes
{}", i.interface_number, i.usage_page, i.usage, hex(&buf[..n]));
    }
    Ok(())
}

fn find_interfaces(api: &HidApi) -> Result<Vec<Interface>, String> {
    let interfaces = device::interfaces(api);
    if interfaces.is_empty() {
        return Err("no Speed Editor HID interface found (is it plugged in?)".into());
    }
    Ok(interfaces)
}

/// Stops on Ctrl+C or when the optional time limit is reached.
struct StopSignal {
    flag: Arc<AtomicBool>,
    deadline: Option<Instant>,
}

impl StopSignal {
    fn new(seconds: Option<u64>) -> StopSignal {
        let flag = Arc::new(AtomicBool::new(false));
        let handler_flag = flag.clone();
        // Failing to install the handler only loses graceful Ctrl+C.
        let _ = ctrlc::set_handler(move || handler_flag.store(true, Ordering::SeqCst));
        StopSignal {
            flag,
            deadline: seconds.map(|s| Instant::now() + Duration::from_secs(s)),
        }
    }

    fn stopped(&self) -> bool {
        self.flag.load(Ordering::SeqCst) || self.deadline.is_some_and(|d| Instant::now() >= d)
    }
}

/// Prints and logs decoded reports.
struct Printer {
    start: Instant,
    options: Options,
    keys: KeyTracker,
    reports: usize,
}

impl Printer {
    fn new(options: Options) -> Printer {
        Printer { start: Instant::now(), options, keys: KeyTracker::default(), reports: 0 }
    }

    /// Prints one report and returns it decoded.
    fn report(&mut self, interface: i32, buf: &[u8]) -> Report {
        self.reports += 1;
        let t = self.start.elapsed().as_secs_f64();
        let report = Report::parse(buf);
        let text = match &report {
            Report::Keys(held) => {
                let changes = self.keys.update(held);
                let mut parts: Vec<String> = changes.pressed.iter().map(|&c| format!("+{}", key_name(c))).collect();
                parts.extend(changes.released.iter().map(|&c| format!("-{}", key_name(c))));
                let held: Vec<String> = held.iter().map(|&c| key_name(c)).collect();
                format!("keys  {:<24} held [{}]", parts.join(" "), held.join(", "))
            }
            Report::Jog { mode, value } => format!("jog   mode {mode}  value {value:+}"),
            Report::Battery { charging, level } => format!("batt  {level}%{}", if *charging { " charging" } else { "" }),
            Report::Other(_) => format!("other id {:#04x} len {}", buf.first().copied().unwrap_or(0), buf.len()),
        };
        let raw = if self.options.raw { format!("   [{}]", hex(buf)) } else { String::new() };
        println!("{t:9.3}s  if{interface}  {text}{raw}");
        let _ = io::stdout().flush();

        if let Some(log) = &mut self.options.log {
            let _ = writeln!(
                log,
                "{{\"t\":{t:.6},\"interface\":{interface},\"raw\":\"{}\",\"decoded\":\"{}\"}}",
                hex(buf),
                text.replace('\\', "\\\\").replace('"', "\\\""),
            );
        }
        report
    }

    fn summary(&self) {
        println!(
            "\n{} reports in {:.1}s. Keys still held at exit: [{}]",
            self.reports,
            self.start.elapsed().as_secs_f64(),
            self.keys.held().iter().map(|&c| key_name(c)).collect::<Vec<_>>().join(", "),
        );
    }
}

fn watch(api: &HidApi, options: Options) -> Result<(), String> {
    let interfaces = find_interfaces(api)?;
    let mut observers = Vec::new();
    for i in &interfaces {
        match Observer::open(api, i) {
            Ok(o) => {
                println!("watching interface {} (usage {:04x}:{:04x}) read-only", i.interface_number, i.usage_page, i.usage);
                observers.push((i.interface_number, o));
            }
            Err(e) => println!("cannot open interface {}: {e}", i.interface_number),
        }
    }
    if observers.is_empty() {
        return Err("no interface could be opened".into());
    }
    println!("Read-only mode: nothing is sent to the device. Press keys and turn the wheel; Ctrl+C to stop.\n");

    let stop = StopSignal::new(options.seconds);
    let mut printer = Printer::new(options);
    let mut buf = [0u8; 64];
    let mut last_hint = Instant::now();
    while !stop.stopped() {
        for (number, observer) in &observers {
            let n = observer.read(&mut buf, POLL).map_err(|e| e.to_string())?;
            if n > 0 {
                printer.report(*number, &buf[..n]);
            }
        }
        if printer.reports == 0 && last_hint.elapsed() > Duration::from_secs(10) {
            println!("  ... no reports yet. Without an authenticated owner (e.g. Resolve) the device stays silent.");
            last_hint = Instant::now();
        }
    }
    printer.summary();
    Ok(())
}

fn control(api: &HidApi, options: Options) -> Result<(), String> {
    let interfaces = find_interfaces(api)?;
    let interface = &interfaces[0];
    let mut controller = Controller::open(api, interface).map_err(|e| e.to_string())?;
    println!("authenticated on interface {}. LEDs follow held keys; JOG/SHTL/SCRL switch wheel mode. Ctrl+C to stop.\n", interface.interface_number);

    let set_wheel = |c: &Controller, key: Key, mode: JogMode| -> Result<(), String> {
        c.set_jog_mode(mode).map_err(|e| e.to_string())?;
        c.set_jog_leds(jog_led(key).unwrap_or(0)).map_err(|e| e.to_string())
    };
    set_wheel(&controller, Key::Jog, JogMode::Relative)?;

    let stop = StopSignal::new(options.seconds);
    let mut printer = Printer::new(options);
    let mut buf = [0u8; 64];
    let result = loop {
        if stop.stopped() {
            break Ok(());
        }
        if let Err(e) = controller.keep_alive() {
            break Err(e.to_string());
        }
        let n = match controller.read(&mut buf, POLL) {
            Ok(n) => n,
            Err(e) => break Err(e.to_string()),
        };
        if n == 0 {
            continue;
        }
        if let Report::Keys(held) = printer.report(interface.interface_number, &buf[..n]) {
            let keys: Vec<Key> = held.iter().filter_map(|&c| Key::from_code(c)).collect();
            let leds = keys.iter().filter_map(|k| k.led()).fold(0, |a, b| a | b);
            let mut step = controller.set_leds(leds).map_err(|e| e.to_string());
            for (key, mode) in [(Key::Jog, JogMode::Relative), (Key::Shuttle, JogMode::AbsoluteDeadZero), (Key::Scroll, JogMode::Relative2)] {
                if step.is_ok() && keys.contains(&key) {
                    step = set_wheel(&controller, key, mode);
                }
            }
            if let Err(e) = step {
                break Err(e);
            }
        }
    };

    // Leave the device dark so it looks untouched for the next owner.
    let _ = controller.set_leds(0);
    let _ = controller.set_jog_leds(0);
    printer.summary();
    result
}
