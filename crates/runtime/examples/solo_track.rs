//! Runs the menu steps of CUT + CAM n (only V n + A n take part in edits) and
//! reports each one, to check them against a running Resolve.
//!
//!   cargo run -p runtime --example solo_track -- 1

use std::time::Instant;

use runtime::menus::Menus;

fn main() {
    let n: u8 = std::env::args().nth(1).and_then(|a| a.parse().ok()).unwrap_or(1);
    let mut menus = Menus::new().expect("UI Automation");
    let auto = |item: &str| vec!["Timeline".to_string(), "Auto Select".to_string(), item.to_string()];
    for path in [
        auto("Auto Deselect All Tracks"),
        auto(&format!("Auto Select/Deselect V{n}")),
        auto(&format!("Auto Select/Deselect A{n}")),
    ] {
        let t = Instant::now();
        let result = menus.invoke(&path);
        println!("{}: {result:?} ({} ms)", path.join(" › "), t.elapsed().as_millis());
    }
}
