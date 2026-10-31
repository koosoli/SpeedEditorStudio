//! Checks that the runtime can bring DaVinci Resolve to the front, the way it
//! does before sending a Resolve function.
//!
//!   cargo run -p runtime --example focus_check

use runtime::focus::{Focus, ensure_resolve_in_front};

fn main() {
    match ensure_resolve_in_front() {
        Focus::Fine => println!("Resolve already in front (or not running)"),
        Focus::Restored(title) => println!("brought Resolve to the front; focus had been on \"{title}\""),
        Focus::Failed(title) => println!("FAILED to bring Resolve to the front; focus stays on \"{title}\""),
    }
}
