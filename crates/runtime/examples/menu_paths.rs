//! Checks that every Resolve menu item the app uses exists, without clicking
//! any of them.
//!
//!   cargo run -p runtime --example menu_paths

use runtime::menus::Menus;

const PATHS: &[&str] = &[
    "Trim > Ripple > Start to Playhead",
    "Trim > Ripple > End to Playhead",
    "Trim > Trim Start",
    "Trim > Trim End",
    "Timeline > Select Clips > Backward on All Tracks",
    "Timeline > Select Clips > Forward on All Tracks",
    "Edit > Swap Clips Towards Left",
    "Edit > Swap Clips Towards Right",
    "Timeline > Auto Select > Auto Deselect All Tracks",
    "Timeline > Auto Select > Auto Select All Tracks",
    "Timeline > Auto Select > Auto Select/Deselect V1",
    "Timeline > Auto Select > Auto Select/Deselect A8",
];

fn main() {
    let mut menus = Menus::new().expect("UI Automation");
    for p in PATHS {
        let path: Vec<String> = p.split('>').map(|s| s.trim().to_string()).collect();
        let t = std::time::Instant::now();
        match menus.check(&path) {
            Ok(()) => println!("ok    {p} ({} ms)", t.elapsed().as_millis()),
            Err(e) => println!("FAIL  {p}: {e}"),
        }
    }
}
