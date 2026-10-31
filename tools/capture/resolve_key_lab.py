"""Finds which Resolve keystroke sequences perform which trim edits.

    ResolvePython.exe -u resolve_key_lab.py <fusionscript.dll> <out.json>

Needs DaVinci Resolve on the Edit page with a timeline holding at least
three clips on V1 (use a throwaway project). For each candidate sequence it
puts the playhead a few frames after the first cut on V1, types the keys into
Resolve, reads where the clips' edges moved through the scripting API, and
undoes until the timeline is back to its starting state.

Result per sequence: how the first two clips' start/end and the third clip's
start changed, e.g. a ripple trim of the second clip's In point shortens
clip 2 and shifts clip 3; a roll moves the cut without shifting clip 3.
"""

import ctypes
import importlib.machinery
import importlib.util
import json
import os
import sys
import time
from ctypes import wintypes

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)

INPUT_KEYBOARD = 1
KEYEVENTF_KEYUP = 0x0002
VK = {"ctrl": 0x11, "shift": 0x10, "alt": 0x12, "esc": 0x1B, "left": 0x25, "right": 0x27, "up": 0x26, "down": 0x28}


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wintypes.WORD), ("wScan", wintypes.WORD), ("dwFlags", wintypes.DWORD),
                ("time", wintypes.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("ki", KEYBDINPUT), ("pad", ctypes.c_byte * 32)]
    _anonymous_ = ("u",)
    _fields_ = [("type", wintypes.DWORD), ("u", _U)]


def send_vk(vk, up):
    scan = user32.MapVirtualKeyW(vk, 0)
    i = INPUT(type=INPUT_KEYBOARD)
    i.ki = KEYBDINPUT(vk, scan, KEYEVENTF_KEYUP if up else 0, 0, 0)
    user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(INPUT))


def press(combo):
    """Presses a combination like "ctrl+shift+." or "u" on the active layout."""
    parts = combo.split("+") if combo != "+" else ["+"]
    keys = []
    for part in parts:
        lower = part.lower()
        if lower in VK:
            keys.append(VK[lower])
        elif lower[0] == "f" and lower[1:].isdigit() and 1 <= int(lower[1:]) <= 24:
            keys.append(0x6F + int(lower[1:]))
        else:
            scan = user32.VkKeyScanW(ord(lower))
            vk, state = scan & 0xFF, (scan >> 8) & 0xFF
            for bit, mod in ((1, VK["shift"]), (2, VK["ctrl"]), (4, VK["alt"])):
                if state & bit and mod not in keys:
                    keys.append(mod)
            keys.append(vk)
    for k in keys:
        send_vk(k, False)
    for k in reversed(keys):
        send_vk(k, True)
    time.sleep(0.12)


def load_fusionscript(path):
    os.add_dll_directory(os.path.dirname(path))
    loader = importlib.machinery.ExtensionFileLoader("fusionscript", path)
    spec = importlib.util.spec_from_loader("fusionscript", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def focus_resolve():
    pid_target = None
    import subprocess
    out = subprocess.run(["tasklist", "/FI", "IMAGENAME eq Resolve.exe", "/FO", "CSV", "/NH"],
                         capture_output=True, text=True).stdout
    for line in out.splitlines():
        cells = [c.strip('"') for c in line.split(",")]
        if len(cells) > 1 and cells[0].lower() == "resolve.exe":
            pid_target = int(cells[1])
    found = []

    @ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
    def visit(hwnd, _):
        pid = wintypes.DWORD()
        user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
        if pid.value == pid_target and user32.IsWindowVisible(hwnd) and not user32.GetWindow(hwnd, 4) \
                and user32.GetWindowTextLengthW(hwnd) > 0:
            found.append(hwnd)
            return False
        return True

    user32.EnumWindows(visit, 0)
    if not found:
        sys.exit("Resolve window not found")
    hwnd = found[0]
    fg = user32.GetForegroundWindow()
    fg_thread = user32.GetWindowThreadProcessId(fg, None)
    me = kernel32.GetCurrentThreadId()
    user32.AttachThreadInput(me, fg_thread, True)
    user32.BringWindowToTop(hwnd)
    user32.SetForegroundWindow(hwnd)
    user32.AttachThreadInput(me, fg_thread, False)
    time.sleep(0.4)


def main():
    lib, out_path = sys.argv[1], sys.argv[2]
    resolve = load_fusionscript(lib).scriptapp("Resolve")
    resolve.OpenPage("edit")
    tl = resolve.GetProjectManager().GetCurrentProject().GetCurrentTimeline()
    fps = round(float(str(tl.GetSetting("timelineFrameRate")).split()[0]))

    def items():
        # Position plus source offset, so a slip (content moves, position not) shows too.
        return [(it.GetStart(), it.GetEnd(), it.GetLeftOffset()) for it in tl.GetItemListInTrack("video", 1)]

    def tc(frame):
        f = frame % fps; s = frame // fps % 60; m = frame // (fps * 60) % 60; h = frame // (fps * 3600)
        return f"{h:02d}:{m:02d}:{s:02d}:{f:02d}"

    start = items()
    if len(start) < 3:
        sys.exit("need at least three clips on V1")
    cut = start[0][1]  # first cut on V1

    round2 = len(sys.argv) > 3 and sys.argv[3] == "2"
    # (playhead offset from the cut, keys)
    candidates2 = {
        "before cut: T V U . .": (-3, ["t", "v", "u", ".", "."]),
        "before cut: T V U U . .": (-3, ["t", "v", "u", "u", ".", "."]),
        "after cut: up left T V U . .": (3, ["up", "left", "t", "v", "u", ".", "."]),
        "after cut: T V U , ,": (3, ["t", "v", "u", ",", ","]),
        "slip: T shift+V . .": (3, ["t", "shift+v", ".", "."]),
        "move: A shift+V ctrl+shift+.": (3, ["a", "shift+v", "ctrl+shift+."]),
        "move: A shift+V ctrl+shift+:": (3, ["a", "shift+v", "ctrl+shift+:"]),
        "move: A shift+V ctrl+.": (3, ["a", "shift+v", "ctrl+."]),
        "move: A shift+V . .": (3, ["a", "shift+v", ".", "."]),
    }
    candidates = {
        "T V . .": ["t", "v", ".", "."],
        "T V U . .": ["t", "v", "u", ".", "."],
        "T V U U . .": ["t", "v", "u", "u", ".", "."],
        "T V U U U . .": ["t", "v", "u", "u", "u", ".", "."],
        "A V . .": ["a", "v", ".", "."],
        "A V U . .": ["a", "v", "u", ".", "."],
        "A V U U . .": ["a", "v", "u", "u", ".", "."],
        "T shift+V . . (slip/slide test)": ["t", "shift+v", ".", "."],
        "T shift+V S . . (slide)": ["t", "shift+v", "s", ".", ".", "s"],
        "A shift+V ctrl+shift+. (swap)": ["a", "shift+v", "ctrl+shift+."],
    }
    results = {"start": start, "cut": cut, "fps": fps, "tests": {}}
    focus_resolve()
    tests = candidates2.items() if round2 else ((n, (3, k)) for n, k in candidates.items())
    for name, (offset, keys) in tests:
        tl.SetCurrentTimecode(tc(cut + offset))
        time.sleep(0.6)
        focus_resolve()
        press("ctrl+shift+a")
        for k in keys:
            press(k)
        time.sleep(0.6)
        after = items()
        results["tests"][name] = {"after": after, "changed": after != start}
        print(name, "->", "changed" if after != start else "no change", flush=True)
        # Back to the start: selection mode, deselect, undo until restored.
        press("a")
        press("ctrl+shift+a")
        for _ in range(6):
            if items() == start:
                break
            press("ctrl+z")
            time.sleep(0.4)
        results["tests"][name]["restored"] = items() == start
        if items() != start:
            print("could not restore the timeline, stopping", flush=True)
            break
    with open(out_path, "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    main()
