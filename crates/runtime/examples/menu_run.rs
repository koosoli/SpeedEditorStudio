//! Runs one Resolve menu item, e.g.:
//!
//!   cargo run -p runtime --example menu_run -- "Timeline > Auto Select > Auto Select All Tracks"

use runtime::menus::Menus;

fn main() {
    let arg = std::env::args().nth(1).expect("menu path, e.g. \"Edit > Undo\"");
    let path: Vec<String> = arg.split('>').map(|p| p.trim().to_string()).collect();
    let result = Menus::new().expect("UI Automation").invoke(&path);
    println!("{}: {result:?}", path.join(" › "));
}
