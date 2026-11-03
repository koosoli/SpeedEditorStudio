//! Runs DaVinci Resolve scripting commands through a helper process.
//!
//! The helper (resolve_helper.py) runs with the Python that ships with
//! Resolve and loads Resolve's fusionscript.dll. It stays alive between
//! commands, so a key press costs one line of JSON each way. It starts on the
//! first command and restarts once if it has died.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const HELPER: &str = include_str!("resolve_helper.py");
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Debug, Deserialize)]
pub struct Reply {
    pub id: Option<u64>,
    pub ok: bool,
    pub message: String,
}

#[derive(Serialize)]
struct Request<'a> {
    id: u64,
    command: &'a str,
    params: &'a BTreeMap<String, String>,
}

pub struct ResolveBridge {
    process: Option<(Child, ChildStdin)>,
    replies_tx: Sender<Reply>,
    replies: Receiver<Reply>,
    next_id: u64,
}

impl Default for ResolveBridge {
    fn default() -> Self {
        let (replies_tx, replies) = mpsc::channel();
        ResolveBridge { process: None, replies_tx, replies, next_id: 1 }
    }
}

impl ResolveBridge {
    /// Queues a command; its reply arrives later through `replies`.
    pub fn send(&mut self, command: &str, params: &BTreeMap<String, String>) -> Result<u64, String> {
        let id = self.next_id;
        self.next_id += 1;
        let line = serde_json::to_string(&Request { id, command, params }).map_err(|e| e.to_string())? + "\n";
        for attempt in 0..2 {
            if self.process.is_none() {
                self.process = Some(spawn(self.replies_tx.clone())?);
            }
            let (_, stdin) = self.process.as_mut().unwrap();
            if stdin.write_all(line.as_bytes()).and_then(|_| stdin.flush()).is_ok() {
                return Ok(id);
            }
            // The helper died: start a fresh one and try once more.
            self.stop();
            if attempt == 1 {
                break;
            }
        }
        Err("the Resolve helper stopped responding".into())
    }

    /// Replies received so far.
    pub fn replies(&self) -> Vec<Reply> {
        self.replies.try_iter().collect()
    }

    /// Sends a command and waits for its reply.
    pub fn request(&mut self, command: &str, params: &BTreeMap<String, String>, timeout: Duration) -> Result<String, String> {
        let id = self.send(command, params)?;
        let deadline = Instant::now() + timeout;
        let mut others = Vec::new();
        let mut answer = Err("Resolve did not answer in time".to_string());
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match self.replies.recv_timeout(left) {
                Ok(r) if r.id == Some(id) => {
                    answer = if r.ok { Ok(r.message) } else { Err(r.message) };
                    break;
                }
                Ok(other) => others.push(other),
                Err(_) => break,
            }
        }
        // Keep replies to other requests for whoever is waiting for them.
        for reply in others {
            let _ = self.replies_tx.send(reply);
        }
        answer
    }

    fn stop(&mut self) {
        if let Some((mut child, stdin)) = self.process.take() {
            drop(stdin); // the helper exits at end of input
            if child.wait_timeout_ms(500).is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

impl Drop for ResolveBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

trait WaitTimeout {
    fn wait_timeout_ms(&mut self, ms: u64) -> Option<()>;
}

impl WaitTimeout for Child {
    fn wait_timeout_ms(&mut self, ms: u64) -> Option<()> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.try_wait() {
                return Some(());
            }
            thread::sleep(Duration::from_millis(20));
        }
        None
    }
}

fn spawn(replies: Sender<Reply>) -> Result<(Child, ChildStdin), String> {
    let (python, library) = locate()?;
    let script = std::env::temp_dir().join("open-speed-editor-resolve-helper.py");
    std::fs::write(&script, HELPER).map_err(|e| format!("cannot write the Resolve helper: {e}"))?;

    let mut command = Command::new(&python);
    command.arg("-u").arg(&script).arg(&library).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    let mut child = command
        .spawn()
        .map_err(|e| format!("cannot start {}: {e}", python.display()))?;
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let reply = serde_json::from_str(&line)
                .unwrap_or(Reply { id: None, ok: false, message: format!("unexpected helper output: {line}") });
            if replies.send(reply).is_err() {
                break;
            }
        }
    });
    Ok((child, stdin))
}

/// Finds Resolve's bundled Python and its scripting library.
#[cfg(windows)]
fn locate() -> Result<(PathBuf, PathBuf), String> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os("RESOLVE_INSTALL_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    for base in ["ProgramFiles", "ProgramW6432"].iter().filter_map(std::env::var_os) {
        dirs.push(PathBuf::from(base).join("Blackmagic Design").join("DaVinci Resolve"));
    }
    for drive in 'C'..='Z' {
        dirs.push(PathBuf::from(format!("{drive}:\\Program Files\\Blackmagic Design\\DaVinci Resolve")));
    }
    let dir = dirs
        .into_iter()
        .find(|d| d.join("fusionscript.dll").is_file())
        .ok_or("DaVinci Resolve's install folder was not found")?;
    let bundled = dir.join("ResolvePython").join("ResolvePython.exe");
    let python = if bundled.is_file() { bundled } else { PathBuf::from("python") };
    Ok((python, dir.join("fusionscript.dll")))
}

/// Finds a Python 3 and Resolve's scripting library (fusionscript.so) on
/// macOS. Resolve's scripting needs a Python 3 installed on the Mac; one
/// bundled with Resolve is used if present.
#[cfg(target_os = "macos")]
fn locate() -> Result<(PathBuf, PathBuf), String> {
    let mut apps = Vec::new();
    if let Some(dir) = std::env::var_os("RESOLVE_INSTALL_DIR") {
        apps.push(PathBuf::from(dir));
    }
    apps.push(PathBuf::from("/Applications/DaVinci Resolve/DaVinci Resolve.app"));
    apps.push(PathBuf::from("/Applications/DaVinci Resolve Studio/DaVinci Resolve.app"));
    let app = apps
        .into_iter()
        .find(|a| a.join("Contents/Libraries/Fusion/fusionscript.so").is_file())
        .ok_or("DaVinci Resolve was not found in /Applications")?;
    let library = app.join("Contents/Libraries/Fusion/fusionscript.so");
    let bundled = ["Contents/Libraries/ResolvePython/bin/python3", "Contents/MacOS/ResolvePython"]
        .iter()
        .map(|p| app.join(p))
        .find(|p| p.is_file());
    // A GUI app doesn't get the shell's PATH, so the usual places are tried by
    // hand. The system Python (Xcode Command Line Tools) comes first: it is the
    // version Resolve's scripting library is known to load in.
    let python = bundled
        .or_else(|| {
            ["/usr/bin/python3", "/opt/homebrew/bin/python3", "/usr/local/bin/python3"]
                .iter()
                .map(PathBuf::from)
                .find(|p| p.is_file())
        })
        .ok_or("Python 3 was not found (install the Xcode Command Line Tools: xcode-select --install)")?;
    Ok((python, library))
}
