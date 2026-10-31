"""Logs DaVinci Resolve's playhead position while the Speed Editor is used.

    ResolvePython.exe -u log_playhead.py <fusionscript.dll> <out.jsonl> [seconds]

Writes one JSON line per change: {"t": unix time, "frame": absolute frame},
plus a "page" line whenever Resolve switches page. Runs until the time is up
or a file named <out.jsonl>.stop appears. The capture tool's timestamps use
the same clock, so both logs can be lined up (docs/FINDINGS.md).
"""

import importlib.machinery
import importlib.util
import json
import os
import sys
import time


def load_fusionscript(path):
    os.add_dll_directory(os.path.dirname(path))
    loader = importlib.machinery.ExtensionFileLoader("fusionscript", path)
    spec = importlib.util.spec_from_loader("fusionscript", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


def main():
    lib, out_path = sys.argv[1], sys.argv[2]
    seconds = float(sys.argv[3]) if len(sys.argv) > 3 else 900
    resolve = load_fusionscript(lib).scriptapp("Resolve")
    if resolve is None:
        sys.exit("Resolve is not reachable")
    timeline = resolve.GetProjectManager().GetCurrentProject().GetCurrentTimeline()
    nominal = round(float(str(timeline.GetSetting("timelineFrameRate")).split()[0]))

    def frames(tc):
        h, m, s, f = (int(x) for x in tc.replace(";", ":").split(":"))
        return ((h * 3600 + m * 60 + s) * nominal) + f

    stop_file = out_path + ".stop"
    end = time.time() + seconds
    last_frame = last_page = None
    with open(out_path, "w", encoding="utf-8") as out:
        out.write(json.dumps({"t": time.time(), "fps": nominal}) + "\n")
        while time.time() < end and not os.path.exists(stop_file):
            frame = frames(timeline.GetCurrentTimecode())
            if frame != last_frame:
                out.write(json.dumps({"t": time.time(), "frame": frame}) + "\n")
                out.flush()
                last_frame = frame
            page = resolve.GetCurrentPage()
            if page != last_page:
                out.write(json.dumps({"t": time.time(), "page": page}) + "\n")
                last_page = page
            time.sleep(0.002)


if __name__ == "__main__":
    main()
