//! se-emu: runs the emulated Speed Editor (speed_editor::emulator) on its own,
//! for testing without the app.
//!
//!   se-emu [--port 3240] [--serial HEX32]
//!
//! Attach it with `usbip attach -r 127.0.0.1 -b 1-1`. Input reports can be
//! typed on stdin as hex (e.g. `04 07` = IN down, `04` = all keys up) or sent
//! as lines to 127.0.0.1:<port + 1>. Everything the host sends is printed.

use std::io::{self, BufRead};
use std::net::TcpListener;
use std::process::ExitCode;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use speed_editor::emulator::{BUS_ID, Config, DEFAULT_PORT, Emulator};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("se-emu: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let (mut port, mut config) = (DEFAULT_PORT, Config::default());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--port" => port = args.next().and_then(|p| p.parse().ok()).ok_or("--port needs a number")?,
            "--serial" => config.serial = args.next().ok_or("--serial needs 32 hex digits")?,
            other => return Err(format!("unknown option {other:?}")),
        }
    }
    let emu = Arc::new(Emulator::start(config, port).map_err(|e| format!("cannot listen on port {port}: {e}"))?);
    println!("se-emu: emulated Speed Editor on 127.0.0.1:{port}, bus id {BUS_ID}");
    println!("attach it with:  usbip attach -r 127.0.0.1 -b {BUS_ID}");

    let feed = emu.clone();
    let feed_listener = TcpListener::bind(("127.0.0.1", port + 1)).map_err(|e| format!("cannot listen on port {}: {e}", port + 1))?;
    thread::spawn(move || {
        for stream in feed_listener.incoming().map_while(Result::ok) {
            for line in io::BufReader::new(stream).lines().map_while(Result::ok) {
                send(&feed, &line);
            }
        }
    });
    let input = emu.clone();
    thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            send(&input, &line);
        }
    });

    loop {
        for event in emu.events() {
            println!("{event:?}");
        }
        for report in emu.host_reports() {
            println!("host sent {report:?}");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn send(emu: &Emulator, line: &str) {
    let digits: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes: Option<Vec<u8>> = (digits.len() % 2 == 0)
        .then(|| (0..digits.len()).step_by(2).map(|i| u8::from_str_radix(&digits[i..i + 2], 16).ok()).collect())
        .flatten();
    match bytes {
        Some(mut report) if !report.is_empty() => {
            // Key reports are always 13 bytes.
            if report[0] == 4 {
                report.resize(13, 0);
            }
            println!("sending input report {report:02x?}");
            emu.send_input(&report);
        }
        _ => eprintln!("type an input report as hex, e.g. 04 07"),
    }
}
