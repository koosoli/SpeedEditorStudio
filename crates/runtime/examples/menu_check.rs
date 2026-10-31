//! Checks the menu engine against a running Resolve: reads Timeline › Snapping,
//! toggles it twice (ending as it started) and times each call.
//!
//!   cargo run -p runtime --example menu_check

use std::time::Instant;

use runtime::menus::Menus;

fn main() {
    let mut menus = Menus::new().expect("UI Automation");
    let snapping: Vec<String> = ["Timeline", "Snapping"].map(String::from).to_vec();
    let report = |menus: &mut Menus, what: &str| {
        let t = Instant::now();
        let state = menus.is_checked(&snapping);
        println!("{what}: {state:?} ({} ms)", t.elapsed().as_millis());
    };
    report(&mut menus, "snapping at start");
    for round in 1..=2 {
        let t = Instant::now();
        let result = menus.invoke(&snapping);
        println!("toggle {round}: {result:?} ({} ms)", t.elapsed().as_millis());
        std::thread::sleep(std::time::Duration::from_millis(300));
        report(&mut menus, "snapping now");
    }
}
