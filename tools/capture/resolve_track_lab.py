"""Checks whether Resolve's Auto Track Selector can be set reliably from the
keyboard, so a Speed Editor key can make the next edits affect one track pair.

    ResolvePython.exe -u resolve_track_lab.py <fusionscript.dll> <out.json> <frame> <ripple key>

The selector state cannot be read through scripting, so it is probed: press
the ripple key at <frame>, see which tracks changed, undo. Each test applies
some toggles, probes, and the timeline is put back until the probe again shows
every track it showed at the start.
"""

import json
import sys
import time

sys.path.insert(0, __file__.rsplit("\\", 1)[0].rsplit("/", 1)[0])
from resolve_key_lab import focus_resolve, load_fusionscript, press  # noqa: E402

ALL = "alt+f10"  # timelineAutoSelectToggleAudioVideoAll


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
        """Tracks a playhead ripple changes right now (then undone)."""
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

    def toggles(keys):
        focus_resolve()
        for k in keys:
            press(k)
        time.sleep(0.3)

    baseline = probe()
    print("baseline:", baseline, flush=True)
    tests = [
        ("all toggled once", [ALL]),
        ("all toggled twice", [ALL, ALL]),
        ("all once, then V1 + A1", [ALL, "alt+f1", "alt+ctrl+f1"]),
        ("all once, then V2", [ALL, "alt+f2"]),
        ("A3 toggled, then all once (mixed state)", ["alt+ctrl+f3", ALL]),
    ]
    results = {"baseline": baseline, "tests": {}}
    for name, keys in tests:
        toggles(keys)
        got = probe()
        print(f"{name}: {got}", flush=True)
        results["tests"][name] = got
        # Put the selectors back: undo per-track toggles, then fix the overall state.
        per_track = [k for k in keys if k != ALL]
        toggles(list(reversed(per_track)))
        for _ in range(3):
            now = probe()
            if now == baseline:
                break
            toggles([ALL])
        results["tests"][name + " / restored"] = probe() == baseline
        if not results["tests"][name + " / restored"]:
            print("could not restore the track selectors - stopping", flush=True)
            break
    with open(out_path, "w") as f:
        json.dump(results, f, indent=1)


if __name__ == "__main__":
    main()
