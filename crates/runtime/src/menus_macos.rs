//! Runs DaVinci Resolve menu items through the macOS Accessibility API, so
//! functions work without keyboard shortcuts (the Windows version uses UI
//! Automation, see menus_windows.rs).
//!
//! The menu bar is AXMenuBar › AXMenuBarItem › AXMenu › AXMenuItem (› AXMenu
//! › AXMenuItem for submenus); an item is clicked with AXPress, without
//! opening the menu on screen.
//!
//! Only partly tested on a Mac (see README, "macOS"). Needs
//! the Accessibility permission (System Settings › Privacy & Security).

use crate::mac::{self, Owned};
use crate::processes;

pub struct Menus;

impl Menus {
    pub fn new() -> Result<Menus, String> {
        if mac::accessibility_trusted(false) {
            Ok(Menus)
        } else {
            Err("Speed Editor Studio needs the Accessibility permission (System Settings › Privacy & Security › Accessibility)".into())
        }
    }

    /// Clicks the menu item at `path` (top menu first).
    pub fn invoke(&mut self, path: &[String]) -> Result<(), String> {
        let item = self.find(path)?;
        mac::ax_press(&item).map_err(|e| format!("cannot run menu item \"{}\" (error {e})", path.join(" › ")))
    }

    /// Checks that the menu item at `path` exists, without clicking it.
    pub fn check(&mut self, path: &[String]) -> Result<(), String> {
        self.find(path).map(drop)
    }

    /// Whether the checkable menu item at `path` is checked.
    pub fn is_checked(&mut self, path: &[String]) -> Result<bool, String> {
        let item = self.find(path)?;
        let mark = mac::ax_attribute(&item, "AXMenuItemMarkChar").and_then(|m| mac::string_of(m.get()));
        Ok(mark.is_some_and(|m| !m.is_empty()))
    }

    /// Names of the items in the submenu at `path`, without clicking any.
    pub fn items(&mut self, path: &[String]) -> Result<Vec<String>, String> {
        let menu = self.find(path)?;
        Ok(submenu_items(&menu).iter().filter_map(mac::ax_title).collect())
    }

    /// Menus don't keep the keyboard on macOS: AXPress doesn't open them.
    pub fn menu_has_focus(&mut self) -> bool {
        false
    }

    /// The menu item at `path`.
    fn find(&self, path: &[String]) -> Result<Owned, String> {
        let (first, rest) = path.split_first().ok_or("empty menu path")?;
        let pid = processes::find_pid(processes::RESOLVE_EXE).ok_or("DaVinci Resolve is not running")?;
        let app = Owned::new(unsafe { mac::AXUIElementCreateApplication(pid as i32) }).ok_or("Resolve not reachable")?;
        let bar = mac::ax_attribute(&app, "AXMenuBar").ok_or("Resolve's menu bar not found (Accessibility permission?)")?;
        let mut current = by_title(&mac::ax_children(&bar), first)?;
        for name in rest {
            current = by_title(&submenu_items(&current), name)?;
        }
        Ok(current)
    }
}

/// The items of the menu a menu bar item or menu item opens.
fn submenu_items(item: &Owned) -> Vec<Owned> {
    mac::ax_children(item).first().map(mac::ax_children).unwrap_or_default()
}

fn by_title(items: &[Owned], name: &str) -> Result<Owned, String> {
    items
        .iter()
        .find(|i| mac::ax_title(i).as_deref() == Some(name))
        .cloned()
        .ok_or_else(|| format!("menu item \"{name}\" not found (greyed out, or another Resolve page?)"))
}
