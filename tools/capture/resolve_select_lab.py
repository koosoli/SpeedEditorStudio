"""Measures how Resolve's keyboard selects clips on particular tracks.

Run with Resolve's own Python (it loads fusionscript):

    "<Resolve>\\ResolvePython\\ResolvePython.exe" tools/capture/resolve_select_lab.py <fusionscript.dll>

Brings Resolve to the front, presses selection shortcuts (Shift+V, Ctrl+Up,
Ctrl+Down, Ctrl+Shift+A = the defaults in the user's layout) and reads which
timeline items are selected after each one through scripting. Nothing is
edited; the playhead and selection are restored at the end.
"""

import ctypes
import ctypes.wintypes as w
import importlib.machinery
import importlib.util
import json
import sys
import time

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32")

VK = {"shift": 0x10, "ctrl": 0x11, "up": 0x26, "down": 0x28, "v": 0x56, "a": 0x41, "l": 0x4C}
EXTENDED = {0x26, 0x28}


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", w.WORD), ("wScan", w.WORD), ("dwFlags", w.DWORD), ("time", w.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("ki", KEYBDINPUT), ("pad", ctypes.c_byte * 32)]
    _anonymous_ = ("u",)
    _fields_ = [("type", w.DWORD), ("u", _U)]


def key(vk, up):
    flags = (0x2 if up else 0) | (0x1 if vk in EXTENDED else 0)
    scan = user32.MapVirtualKeyW(vk, 0)
    i = INPUT(type=1, ki=KEYBDINPUT(vk, scan, flags, 0, 0))
    user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(i))


def press(combo):
    keys = [VK[k] for k in combo.split("+")]
    for k in keys:
        key(k, False)
    for k in reversed(keys):
        key(k, True)
    time.sleep(0.35)


def resolve_window():
    found = []

    @ctypes.WINFUNCTYPE(w.BOOL, w.HWND, w.LPARAM)
    def visit(hwnd, _):
        buf = ctypes.create_unicode_buffer(256)
        user32.GetClassNameW(hwnd, buf, 256)
        if buf.value == "UiMainWindowImp" and user32.IsWindowVisible(hwnd):
            found.append(hwnd)
        return True

    user32.EnumWindows(visit, 0)
    return found[0] if found else None


def same_process(a, b):
    pa, pb = w.DWORD(), w.DWORD()
    user32.GetWindowThreadProcessId(a, ctypes.byref(pa))
    user32.GetWindowThreadProcessId(b, ctypes.byref(pb))
    return pa.value == pb.value


def resolve_in_front(hwnd):
    return same_process(user32.GetForegroundWindow(), hwnd)


def bring_to_front(hwnd):
    """Brings Resolve to the front and checks it worked (Windows refuses
    focus changes under some conditions; a lone Alt press lifts that)."""
    for _ in range(3):
        fg = user32.GetForegroundWindow()
        if resolve_in_front(hwnd):
            return True
        key(0x12, False)
        key(0x12, True)
        a = user32.GetWindowThreadProcessId(fg, None)
        b = kernel32.GetCurrentThreadId()
        user32.AttachThreadInput(b, a, True)
        user32.ShowWindow(hwnd, 9)  # SW_RESTORE
        user32.SetForegroundWindow(hwnd)
        user32.BringWindowToTop(hwnd)
        user32.AttachThreadInput(b, a, False)
        time.sleep(0.4)
    return resolve_in_front(hwnd)


def load(dll):
    loader = importlib.machinery.ExtensionFileLoader("fusionscript", dll)
    spec = importlib.util.spec_from_file_location("fusionscript", dll, loader=loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def main():
    import os
    os.add_dll_directory(os.path.dirname(sys.argv[1]))
    resolve = load(sys.argv[1]).scriptapp("Resolve")
    tl = resolve.GetProjectManager().GetCurrentProject().GetCurrentTimeline()

    def selected():
        items = tl.GetSelectedClips() or []
        items = list(items.values()) if isinstance(items, dict) else list(items)
        return sorted(f"{t[0][0].upper()}{t[1]}" for t in (i.GetTrackTypeAndIndex() for i in items))

    start_tc = tl.GetCurrentTimecode()
    log = {"playhead_before": start_tc}
    hwnd = resolve_window()
    # Windows may refuse to bring Resolve forward from a background process:
    # then wait for the user to click into Resolve's timeline.
    deadline = time.time() + 60
    while not resolve_in_front(hwnd) and not bring_to_front(hwnd) and time.time() < deadline:
        time.sleep(0.3)
    time.sleep(0.8)
    if not resolve_in_front(hwnd):
        print(json.dumps({"error": "Resolve could not be brought to the front; nothing was pressed"}))
        return
    tl.SetCurrentTimecode("00:00:03:10")
    time.sleep(0.5)
    press("ctrl+shift+a")
    log["after deselect"] = selected()
    steps = ["shift+v", "ctrl+up", "ctrl+up", "ctrl+down", "ctrl+down", "ctrl+down"]
    if "--unlinked" in sys.argv:
        # Linked selection off (Ctrl+Shift+L), step around, then back on.
        steps = ["ctrl+shift+l", "shift+v", "ctrl+down", "ctrl+down", "ctrl+down", "ctrl+up", "ctrl+up",
                 "ctrl+up", "ctrl+up", "ctrl+shift+l", "ctrl+shift+a", "shift+v"]
    for step in steps:
        if not resolve_in_front(hwnd):
            log["aborted"] = f"Resolve lost the focus before {step}"
            break
        press(step)
        log.setdefault("steps", []).append([step, selected()])
    press("ctrl+shift+a")
    tl.SetCurrentTimecode(start_tc)
    log["restored"] = {"selection": selected(), "playhead": tl.GetCurrentTimecode()}
    print(json.dumps(log))


if __name__ == "__main__":
    main()
