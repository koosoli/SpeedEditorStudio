"""Finds which tracks "Ripple Start to Playhead" affects in Resolve, under
different Auto Track Selector and sync lock settings.

    ResolvePython.exe -u resolve_ripple_lab.py <fusionscript.dll> <out.json> <frame> <ripple key>

Puts the playhead at <frame>, toggles track settings with Resolve's own
shortcuts, presses <ripple key> (the user's shortcut for Ripple Start to
Playhead), records how every track's clips changed, then undoes and toggles
the settings back. Track switches are toggles that Resolve cannot report, so
each test flips them and flips them back again.
"""

import json
import sys
import time

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0])
from resolve_key_lab import focus_resolve, load_fusionscript, press  # noqa: E402


def main():
    lib, out_path, frame, ripple_key = sys.argv[1], sys.argv[2], int(sys.argv[3]), sys.argv[4]
    resolve = load_fusionscript(lib).scriptapp("Resolve")
    tl = resolve.GetProjectManager().GetCurrentProject().GetCurrentTimeline()
    fps = round(float(str(tl.GetSetting("timelineFrameRate")).split()[0]))

    def snapshot():
        snap = {}
        for kind in ("video", "audio"):
            for i in range(1, tl.GetTrackCount(kind) + 1):
                snap[f"{kind[0].upper()}{i}"] = [(it.GetStart(), it.GetEnd()) for it in (tl.GetItemListInTrack(kind, i) or [])]
        return snap

    def tc(f):
        return f"{f // (fps * 3600):02d}:{f // (fps * 60) % 60:02d}:{f // fps % 60:02d}:{f % fps:02d}"

    def diff(before, after):
        changes = {}
        for track, clips in before.items():
            now = after.get(track, [])
            if clips != now:
                trimmed = [f"{a}->{b}" for a, b in zip(clips, now) if a != b][:3]
                changes[track] = {"clips": f"{len(clips)}->{len(now)}", "first changes": trimmed}
        return changes

    tests = {
        "all as they are": [],
        "Auto Track Selector V2 toggled": ["alt+f2"],
        "Auto Track Selector V2, A1, A3 toggled": ["alt+f2", "alt+ctrl+f1", "alt+ctrl+f3"],
        "sync lock V2 toggled": ["ctrl+f2"],
        "sync lock V2, A1, A3 toggled": ["ctrl+f2", "ctrl+shift+f1", "ctrl+shift+f3"],
    }
    start = snapshot()
    results = {"frame": frame, "tests": {}}
    for name, toggles in tests.items():
        tl.SetCurrentTimecode(tc(frame))
        time.sleep(0.6)
        focus_resolve()
        press("ctrl+shift+a")
        for t in toggles:
            press(t)
        press(ripple_key)
        time.sleep(0.6)
        changes = diff(start, snapshot())
        print(f"{name}: {sorted(changes) or 'nothing changed'}", flush=True)
        results["tests"][name] = changes
        for _ in range(6):
            if snapshot() == start:
                break
            press("ctrl+z")
            time.sleep(0.4)
        for t in toggles:
            press(t)
        if snapshot() != start:
            print("could not restore the timeline, stopping", flush=True)
            results["tests"][name]["restored"] = False
            break
    with open(out_path, "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    main()
