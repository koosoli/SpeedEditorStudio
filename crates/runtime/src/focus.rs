//! Keeps DaVinci Resolve in front while the Speed Editor drives it.
//!
//! Resolve functions are sent as keystrokes, which go to whichever window has
//! keyboard focus, so before sending one the runtime makes sure Resolve's main
//! window is the foreground window.

use windows_sys::Win32::Foundation::{HWND, LPARAM};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GW_OWNER, GetForegroundWindow, GetWindow, GetWindowTextLengthW, GetWindowTextW,
    GetWindowThreadProcessId, IsIconic, IsWindowVisible, SW_RESTORE, SetForegroundWindow, ShowWindow,
};

use crate::processes;

/// What `ensure_resolve_in_front` had to do.
pub enum Focus {
    /// Resolve already had focus, or isn't running (nothing to do).
    Fine,
    /// Another window had focus; Resolve was brought back. Holds its title.
    Restored(String),
    /// Another window had focus and Windows refused to switch.
    Failed(String),
}

/// Brings Resolve's main window to the front if another window has focus.
pub fn ensure_resolve_in_front() -> Focus {
    let Some(pid) = processes::find_pid(processes::RESOLVE_EXE) else {
        return Focus::Fine;
    };
    unsafe {
        let foreground = GetForegroundWindow();
        if window_pid(foreground) == pid {
            return Focus::Fine;
        }
        let Some(target) = main_window(pid) else {
            return Focus::Fine;
        };
        let title = window_title(foreground);
        // Windows only lets the foreground thread hand focus over, so join
        // its input queue for the switch.
        let fg_thread = GetWindowThreadProcessId(foreground, std::ptr::null_mut());
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, 1) != 0;
        if IsIconic(target) != 0 {
            ShowWindow(target, SW_RESTORE);
        }
        BringWindowToTop(target);
        let ok = SetForegroundWindow(target) != 0;
        if attached {
            AttachThreadInput(me, fg_thread, 0);
        }
        if ok && window_pid(GetForegroundWindow()) == pid { Focus::Restored(title) } else { Focus::Failed(title) }
    }
}

unsafe fn window_pid(hwnd: HWND) -> u32 {
    let mut pid = 0;
    unsafe { GetWindowThreadProcessId(hwnd, &mut pid) };
    pid
}

unsafe fn window_title(hwnd: HWND) -> String {
    unsafe {
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return "(untitled window)".into();
        }
        let mut buf = vec![0u16; len as usize + 1];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

/// The largest visible, unowned top-level window of the process.
unsafe fn main_window(pid: u32) -> Option<HWND> {
    struct Search {
        pid: u32,
        found: Option<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, data: LPARAM) -> i32 {
        unsafe {
            let search = &mut *(data as *mut Search);
            if window_pid(hwnd) == search.pid
                && IsWindowVisible(hwnd) != 0
                && GetWindow(hwnd, GW_OWNER).is_null()
                && GetWindowTextLengthW(hwnd) > 0
            {
                search.found = Some(hwnd);
                return 0;
            }
            1
        }
    }
    let mut search = Search { pid, found: None };
    unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
    search.found
}
