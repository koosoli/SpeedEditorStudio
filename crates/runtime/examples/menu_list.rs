//! Lists the items of a Resolve submenu, without clicking anything:
//!
//!   cargo run -p runtime --example menu_list -- "Timeline > Sync Lock"

use runtime::menus::Menus;

fn main() {
    let arg = std::env::args().nth(1).expect("submenu path, e.g. \"Timeline > Sync Lock\"");
    let path: Vec<String> = arg.split('>').map(|p| p.trim().to_string()).collect();
    match Menus::new().expect("UI Automation").items(&path) {
        Ok(items) => items.iter().for_each(|i| println!("{i}")),
        Err(e) => println!("{e}"),
    }
}
