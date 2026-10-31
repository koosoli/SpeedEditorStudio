"""Tries different ways of sending Alt + function key to Resolve.

    ResolvePython.exe -u resolve_altkey_lab.py <fusionscript.dll> <out.json> <frame> <ripple key>

Alt+F1 toggles V1's Auto Track Selector. Each method sends it; a probe ripple
at <frame> (undone) shows whether V1 dropped out. If it did, the same method
sends it again to switch V1 back.
"""

import ctypes
import json
import sys
import time

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0])
from resolve_key_lab import INPUT, INPUT_KEYBOARD, KEYBDINPUT, focus_resolve, load_fusionscript, press, user32  # noqa: E402

KEYEVENTF_EXTENDEDKEY = 0x0001
KEYEVENTF_KEYUP = 0x0002
KEYEVENTF_SCANCODE = 0x0008
VK_MENU, VK_LMENU, VK_F1 = 0x12, 0xA4, 0x70
SC_ALT, SC_F1 = 0x38, 0x3B


def send(vk, scan, flags):
    i = INPUT(type=INPUT_KEYBOARD)
    i.ki = KEYBDINPUT(vk, scan, flags, 0, 0)
    user32.SendInput(1, ctypes.byref(i), ctypes.sizeof(INPUT))


def vk_lmenu(hold):
    send(VK_LMENU, SC_ALT, 0)
    send(VK_F1, SC_F1, 0)
    time.sleep(hold)
    send(VK_F1, SC_F1, KEYEVENTF_KEYUP)
    send(VK_LMENU, SC_ALT, KEYEVENTF_KEYUP)


def scancodes(hold):
    send(0, SC_ALT, KEYEVENTF_SCANCODE)
    send(0, SC_F1, KEYEVENTF_SCANCODE)
    time.sleep(hold)
    send(0, SC_F1, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)
    send(0, SC_ALT, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP)


def slow_vk(hold):
    send(VK_MENU, SC_ALT, 0)
    time.sleep(0.05)
    send(VK_F1, SC_F1, 0)
    time.sleep(hold)
    send(VK_F1, SC_F1, KEYEVENTF_KEYUP)
    time.sleep(0.05)
    send(VK_MENU, SC_ALT, KEYEVENTF_KEYUP)


METHODS = {
    "left Alt key code (VK_LMENU)": lambda: vk_lmenu(0.03),
    "hardware scan codes only": lambda: scancodes(0.03),
    "slow, with pauses (VK_MENU)": lambda: slow_vk(0.08),
    "hardware scan codes, slow": lambda: scancodes(0.12),
}


def main():
    lib, out_path, frame, ripple_key = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
    resolve = load_fusionscript(lib).scriptapp("Resolve")
    tl = resolve.GetProjectManager().GetCurrentProject().GetCurrentTimeline()
    fps = round(float(str(tl.GetSetting("timelineFrameRate")).split()[0]))

    def snapshot():
        return {f"{k[0].upper()}{i}": [(it.GetStart(), it.GetEnd()) for it in (tl.GetItemListInTrack(k, i) or [])]
                for k in ("video", "audio") for i in range(1, tl.GetTrackCount(k) + 1)}

    def tc(f):
        return f"{f // (fps * 3600):02d}:{f // (fps * 60) % 60:02d}:{f // fps % 60:02d}:{f % fps:02d}"

    start = snapshot()

    def probe():
        tl.SetCurrentTimecode(tc(frame))
        time.sleep(0.5)
        focus_resolve()
        press("ctrl+shift+a")
        press(ripple_key)
        time.sleep(0.5)
        after = snapshot()
        changed = sorted(t for t in start if start[t] != after.get(t))
        for _ in range(6):
            if snapshot() == start:
                break
            press("ctrl+z")
            time.sleep(0.4)
        if snapshot() != start:
            raise SystemExit("could not undo the probe ripple - stopping")
        return changed

    baseline = probe()
    print("baseline:", baseline, flush=True)
    results = {"baseline": baseline, "methods": {}}
    for name, method in METHODS.items():
        focus_resolve()
        method()
        time.sleep(0.3)
        got = probe()
        works = "V1" in baseline and "V1" not in got
        print(f"{name}: {'WORKS' if works else 'no effect'} {got}", flush=True)
        results["methods"][name] = {"works": works, "affected": got}
        if works:
            focus_resolve()
            method()
            time.sleep(0.3)
            restored = probe() == baseline
            results["methods"][name]["restored"] = restored
            if not restored:
                print("could not switch V1 back - stopping", flush=True)
                break
    with open(out_path, "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    main()
