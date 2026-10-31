//! se-mapper: runs a profile on the Speed Editor from the command line.
//!
//!   se-mapper [PROFILE.toml] [--seconds N]
//!
//! PROFILE defaults to profiles/example.toml; --seconds stops cleanly after
//! N seconds (for test runs). DaVinci Resolve must be closed.

use std::process::ExitCode;
use std::sync::mpsc;
use std::time::Duration;

use mapper::Profile;
use runtime::{Event, Options, Runtime};

const DEFAULT_PROFILE: &str = "profiles/example.toml";

fn main() -> ExitCode {
    let mut path = DEFAULT_PROFILE.to_string();
    let mut limit = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--seconds" {
            match args.next().and_then(|n| n.parse().ok()) {
                Some(n) => limit = Some(Duration::from_secs(n)),
                None => {
                    eprintln!("--seconds needs a number");
                    return ExitCode::from(2);
                }
            }
        } else {
            path = arg;
        }
    }

    let profile = match Profile::load(&path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("cannot load profile {path}: {e}");
            return ExitCode::FAILURE;
        }
    };

    let (stop_tx, stop_rx) = mpsc::channel();
    let ctrlc_tx = stop_tx.clone();
    if let Err(e) = ctrlc::set_handler(move || {
        let _ = ctrlc_tx.send(());
    }) {
        eprintln!("cannot install Ctrl+C handler: {e}");
        return ExitCode::FAILURE;
    }

    println!("profile {path} loaded. Ctrl+C to quit. DaVinci Resolve must be closed.");
    let options = Options { active: true, yield_to_resolve: false };
    let runtime = Runtime::spawn(profile, options, |event| match event {
        Event::YieldedToResolve => println!("DaVinci Resolve is running: the Speed Editor is left to it"),
        Event::Waiting(why) => println!("waiting for the Speed Editor ({why})"),
        Event::Connected { bus } => println!("connected over {bus}"),
        Event::Battery { charging, level } => {
            println!("battery {level}%{}", if charging { " (charging)" } else { "" })
        }
        Event::Disconnected(why) => println!("disconnected ({why}); waiting to reconnect"),
        Event::Status(text) => println!("{text}"),
        Event::Keys(_) | Event::Leds(_) | Event::Jog { .. } | Event::State { .. } | Event::Stopped => {}
    });

    match limit {
        Some(limit) => {
            let _ = stop_rx.recv_timeout(limit);
        }
        None => {
            let _ = stop_rx.recv();
        }
    }
    runtime.stop();
    println!("stopped; all keys released.");
    ExitCode::SUCCESS
}
