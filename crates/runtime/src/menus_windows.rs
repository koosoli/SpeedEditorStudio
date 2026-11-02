//! Runs DaVinci Resolve menu items through Windows UI Automation, so functions
//! work without keyboard shortcuts (docs/FINDINGS.md, "Resolve's menus through
//! Windows UI Automation").
//!
//! Resolve's Qt menus only expose their items while open, so a menu path such
//! as ["Timeline", "Auto Select", "Auto Select All Tracks"] is opened item by
//! item, the last one is invoked, and the menu is closed again (about 0.1 s).

use std::time::Duration;

use uiautomation::patterns::{UIExpandCollapsePattern, UIInvokePattern, UITogglePattern};
use uiautomation::types::{ControlType, ToggleState};
use uiautomation::{UIAutomation, UIElement};

use crate::processes;

/// How long to wait for an opened menu's items to appear.
const ITEM_WAIT: Duration = Duration::from_millis(800);
const MAIN_WINDOW_CLASS: &str = "UiMainWindowImp";

pub struct Menus {
    automation: UIAutomation,
    window: Option<UIElement>,
}

impl Menus {
    pub fn new() -> Result<Menus, String> {
        let automation = UIAutomation::new().map_err(|e| format!("UI Automation unavailable: {e}"))?;
        Ok(Menus { automation, window: None })
    }

    /// Invokes the menu item at `path` (top menu first).
    pub fn invoke(&mut self, path: &[String]) -> Result<(), String> {
        let item = self.open(path)?;
        let result = item
            .get_pattern::<UIInvokePattern>()
            .and_then(|p| p.invoke())
            .map_err(|e| format!("cannot run menu item \"{}\": {e}", path.join(" › ")));
        self.close(path);
        result
    }

    /// Checks that the menu item at `path` exists, without clicking it.
    pub fn check(&mut self, path: &[String]) -> Result<(), String> {
        self.open(path)?;
        self.close(path);
        Ok(())
    }

    /// Names of the items in the submenu at `path`, without clicking any.
    pub fn items(&mut self, path: &[String]) -> Result<Vec<String>, String> {
        let menu = self.open(path)?;
        let listed = expand(&menu)
            .map_err(|e| format!("cannot open menu \"{}\": {e}", path.join(" › ")))
            .and_then(|_| {
                std::thread::sleep(Duration::from_millis(200));
                self.automation
                    .create_matcher()
                    .from_ref(&menu)
                    .control_type(ControlType::MenuItem)
                    .timeout(ITEM_WAIT.as_millis() as u64)
                    .find_all()
                    .map_err(|e| e.to_string())
            })
            .map(|items| items.iter().filter_map(|i| i.get_name().ok()).collect());
        self.close(path);
        listed
    }

    /// Whether the checkable menu item at `path` is checked.
    pub fn is_checked(&mut self, path: &[String]) -> Result<bool, String> {
        let item = self.open(path)?;
        let state = item.get_pattern::<UITogglePattern>().and_then(|p| p.get_toggle_state());
        self.close(path);
        match state {
            Ok(ToggleState::On) => Ok(true),
            Ok(_) => Ok(false),
            Err(e) => Err(format!("menu item \"{}\" has no check mark: {e}", path.join(" › "))),
        }
    }

    /// Whether keyboard focus sits in Resolve's menus. After a menu item runs,
    /// Resolve's menu bar can keep the focus, and the next key press then only
    /// leaves the menu instead of reaching the timeline.
    pub fn menu_has_focus(&mut self) -> bool {
        let Ok(window) = self.window() else { return false };
        let Ok(focused) = self.automation.get_focused_element() else { return false };
        let same_app = focused.get_process_id().ok() == window.get_process_id().ok();
        let in_menu = matches!(
            focused.get_control_type(),
            Ok(ControlType::MenuItem | ControlType::MenuBar | ControlType::Menu)
        );
        same_app && in_menu
    }

    /// Opens every menu on the way and returns the final item.
    fn open(&mut self, path: &[String]) -> Result<UIElement, String> {
        let (first, rest) = path.split_first().ok_or("empty menu path")?;
        let window = self.window()?;
        let mut current = self.find(&window, first).inspect_err(|_| self.window = None)?;
        for name in rest {
            expand(&current).map_err(|e| {
                self.close(path);
                format!("cannot open menu \"{}\": {e}", current.get_name().unwrap_or_default())
            })?;
            current = self.find(&current, name).inspect_err(|_| self.close(path))?;
        }
        Ok(current)
    }

    /// Closes the top menu of `path`, ignoring errors (it may be closed already).
    fn close(&mut self, path: &[String]) {
        let Some(first) = path.first() else { return };
        if let Ok(window) = self.window() {
            if let Ok(top) = self.find_now(&window, first) {
                if let Ok(p) = top.get_pattern::<UIExpandCollapsePattern>() {
                    let _ = p.collapse();
                }
            }
        }
    }

    fn find(&self, parent: &UIElement, name: &str) -> Result<UIElement, String> {
        self.automation
            .create_matcher()
            .from_ref(parent)
            .control_type(ControlType::MenuItem)
            .name(name)
            .timeout(ITEM_WAIT.as_millis() as u64)
            .find_first()
            .map_err(|_| format!("menu item \"{name}\" not found (greyed out, or another Resolve page?)"))
    }

    fn find_now(&self, parent: &UIElement, name: &str) -> Result<UIElement, String> {
        self.automation
            .create_matcher()
            .from_ref(parent)
            .control_type(ControlType::MenuItem)
            .name(name)
            .timeout(0)
            .find_first()
            .map_err(|e| e.to_string())
    }

    /// Resolve's main window (not the shadow windows of open menus).
    fn window(&mut self) -> Result<UIElement, String> {
        if let Some(w) = &self.window {
            if w.get_classname().is_ok() {
                return Ok(w.clone());
            }
        }
        let pid = processes::find_pid(processes::RESOLVE_EXE).ok_or("DaVinci Resolve is not running")?;
        let root = self.automation.get_root_element().map_err(|e| e.to_string())?;
        let window = self
            .automation
            .create_matcher()
            .from(root)
            .depth(2)
            .classname(MAIN_WINDOW_CLASS)
            .filter_fn(Box::new(move |e: &UIElement| Ok(e.get_process_id()? == pid)))
            .timeout(0)
            .find_first()
            .map_err(|_| "Resolve's main window not found".to_string())?;
        self.window = Some(window.clone());
        Ok(window)
    }
}

fn expand(item: &UIElement) -> uiautomation::Result<()> {
    item.get_pattern::<UIExpandCollapsePattern>()?.expand()
}
