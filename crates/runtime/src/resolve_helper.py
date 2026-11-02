"""Bridge between Speed Editor Studio and DaVinci Resolve's scripting API.

Started by crates/runtime/src/resolve.rs with Resolve's bundled Python.
argv[1] is the path of Resolve's fusionscript.dll.

Reads one JSON request per line on stdin:
    {"id": 1, "command": "add_marker", "params": {"color": "Red", ...}}
and answers one JSON line per request on stdout:
    {"id": 1, "ok": true, "message": "Red marker added"}

Command ids and their settings are listed in crates/mapper/src/resolve.rs.
"""

import importlib.machinery
import importlib.util
import json
import os
import re
import sys
import time


class Fail(Exception):
    """A command could not run; the message is shown to the user."""


def load_fusionscript(path):
    # fusionscript.dll needs other DLLs from Resolve's folder, which Python
    # 3.8+ no longer searches by itself.
    if hasattr(os, "add_dll_directory"):
        os.add_dll_directory(os.path.dirname(path))
    loader = importlib.machinery.ExtensionFileLoader("fusionscript", path)
    spec = importlib.util.spec_from_loader("fusionscript", loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


# ---------------------------------------------------------------- timecode

def frame_rate(timeline):
    rate_text = str(timeline.GetSetting("timelineFrameRate"))
    match = re.match(r"[\d.]+", rate_text)
    rate = float(match.group(0)) if match else 24.0
    drop = str(timeline.GetSetting("timelineDropFrameTimecode")) in ("1", "True", "true") or "DF" in rate_text
    return round(rate), drop


def tc_to_frames(tc, nominal, drop):
    h, m, s, f = (int(p) for p in re.split(r"[:;.]", tc.strip()))
    frames = ((h * 3600 + m * 60 + s) * nominal) + f
    if drop:
        dropped = 2 * (nominal // 30)
        minutes = h * 60 + m
        frames -= dropped * (minutes - minutes // 10)
    return frames


def frames_to_tc(frames, nominal, drop):
    if drop:
        dropped = 2 * (nominal // 30)
        per_10_min = nominal * 600 - dropped * 9
        per_min = nominal * 60 - dropped
        tens, rest = divmod(frames, per_10_min)
        frames += dropped * 9 * tens
        if rest > dropped:
            frames += dropped * ((rest - dropped) // per_min)
    f = frames % nominal
    s = (frames // nominal) % 60
    m = (frames // (nominal * 60)) % 60
    h = frames // (nominal * 3600)
    return f"{h:02d}:{m:02d}:{s:02d}{';' if drop else ':'}{f:02d}"


# ---------------------------------------------------------------- bridge

class Bridge:
    def __init__(self, lib_path):
        self.dvr = load_fusionscript(lib_path)
        self.app = None

    def resolve(self):
        if self.app is not None:
            try:
                self.app.GetVersionString()
            except Exception:
                self.app = None
        if self.app is None:
            self.app = self.dvr.scriptapp("Resolve")
        if self.app is None:
            raise Fail("Resolve isn't reachable. Is it running, with Preferences > System > General > "
                       "External scripting using set to Local?")
        return self.app

    def project(self):
        project = self.resolve().GetProjectManager().GetCurrentProject()
        if not project:
            raise Fail("No project is open in Resolve")
        return project

    def timeline(self):
        timeline = self.project().GetCurrentTimeline()
        if not timeline:
            raise Fail("No timeline is open in Resolve")
        return timeline

    def selected(self, timeline):
        items = timeline.GetSelectedClips() or []
        if not items:
            raise Fail("Select clips on the timeline first")
        return list(items.values()) if isinstance(items, dict) else list(items)

    def current_item(self, timeline):
        item = timeline.GetCurrentVideoItem()
        if not item:
            raise Fail("No clip under the playhead")
        return item

    def playhead(self, timeline):
        nominal, drop = frame_rate(timeline)
        return tc_to_frames(timeline.GetCurrentTimecode(), nominal, drop), nominal, drop

    def seek(self, timeline, frame):
        _, nominal, drop = self.playhead(timeline)
        frame = max(timeline.GetStartFrame(), min(frame, timeline.GetEndFrame() - 1))
        if not timeline.SetCurrentTimecode(frames_to_tc(frame, nominal, drop)):
            raise Fail("Resolve did not move the playhead (try the Edit or Cut page)")

    # ---------------------------------------------------------- commands

    def cmd_selection(self, p):
        # Tracks of the selected timeline items, e.g. ["A1", "V1"], and the
        # number of video tracks, for stepping the selection to a track.
        # Only items under the playhead count; others are listed apart.
        timeline = self.timeline()
        frame, _, _ = self.playhead(timeline)
        items = timeline.GetSelectedClips() or []
        items = list(items.values()) if isinstance(items, dict) else list(items)
        name = lambda i: "{}{}".format(i.GetTrackTypeAndIndex()[0][0].upper(), i.GetTrackTypeAndIndex()[1])
        under = sorted({name(i) for i in items if i.GetStart() <= frame < i.GetEnd()})
        elsewhere = sorted({name(i) for i in items if not (i.GetStart() <= frame < i.GetEnd())})
        # Linked parts of selected clips that are not selected themselves:
        # then Resolve's linked selection is off.
        selected_ids = {i.GetUniqueId() for i in items}
        unselected_links = sorted({
            name(l) for i in items for l in (i.GetLinkedItems() or []) if l.GetUniqueId() not in selected_ids
        })
        return json.dumps({"tracks": under, "elsewhere": elsewhere, "unselected_links": unselected_links,
                           "video_tracks": timeline.GetTrackCount("video")})

    def cmd_playhead(self, p):
        # The playhead as a frame number, so the app can pace wheel steps.
        frame, _, _ = self.playhead(self.timeline())
        return str(frame)

    def cmd_ping(self, p):
        return "Connected to DaVinci Resolve " + str(self.resolve().GetVersionString())

    def cmd_page(self, p):
        check(self.resolve().OpenPage(p["page"]), "open the page")
        return f"{p['page'].capitalize()} page"

    def cmd_go_start(self, p):
        tl = self.timeline()
        self.seek(tl, tl.GetStartFrame())
        return "Timeline start"

    def cmd_go_end(self, p):
        tl = self.timeline()
        self.seek(tl, tl.GetEndFrame() - 1)
        return "Timeline end"

    def cmd_nudge(self, p):
        tl = self.timeline()
        current, _, _ = self.playhead(tl)
        self.seek(tl, current + int(p["frames"]))
        return f"Moved {p['frames']} frames"

    def jump_marker(self, forward):
        tl = self.timeline()
        current, _, _ = self.playhead(tl)
        start = tl.GetStartFrame()
        marks = sorted(start + int(round(float(f))) for f in (tl.GetMarkers() or {}))
        targets = [m for m in marks if m > current] if forward else [m for m in marks if m < current]
        if not targets:
            raise Fail("No more markers in that direction")
        self.seek(tl, targets[0] if forward else targets[-1])
        return "Next marker" if forward else "Previous marker"

    def cmd_next_marker(self, p):
        return self.jump_marker(True)

    def cmd_prev_marker(self, p):
        return self.jump_marker(False)

    def cmd_add_marker(self, p):
        tl = self.timeline()
        current, _, _ = self.playhead(tl)
        ok = tl.AddMarker(current - tl.GetStartFrame(), p["color"], p["name"], p["note"], 1)
        check(ok, "add the marker (is there already one at this frame?)")
        return f"{p['color']} marker added"

    def cmd_clip_color(self, p):
        for item in self.selected(self.timeline()):
            item.SetClipColor(p["color"])
        return f"Clips coloured {p['color']}"

    def cmd_clear_clip_color(self, p):
        for item in self.selected(self.timeline()):
            item.ClearClipColor()
        return "Clip colour cleared"

    def cmd_toggle_clips(self, p):
        items = self.selected(self.timeline())
        enable = not all(item.GetClipEnabled() for item in items)
        for item in items:
            item.SetClipEnabled(enable)
        return "Clips enabled" if enable else "Clips disabled"

    def cmd_ripple_delete_clips(self, p):
        tl = self.timeline()
        check(tl.DeleteClips(self.selected(tl), True), "delete the clips")
        return "Clips ripple deleted"

    def cmd_link_clips(self, p):
        tl = self.timeline()
        check(tl.SetClipsLinked(self.selected(tl), True), "link the clips")
        return "Clips linked"

    def cmd_unlink_clips(self, p):
        tl = self.timeline()
        check(tl.SetClipsLinked(self.selected(tl), False), "unlink the clips")
        return "Clips unlinked"

    def cmd_compound_clip(self, p):
        tl = self.timeline()
        check(tl.CreateCompoundClip(self.selected(tl)), "make a compound clip")
        return "Compound clip created"

    def cmd_insert_title(self, p):
        check(self.timeline().InsertTitleIntoTimeline(p["name"]), f"insert the title {p['name']!r}")
        return f"Title {p['name']} inserted"

    def cmd_grab_still(self, p):
        check(self.timeline().GrabStill(), "grab a still (open the Color page)")
        return "Still grabbed"

    def cmd_grab_all_stills(self, p):
        check(self.timeline().GrabAllStills(2), "grab stills")
        return "Stills grabbed"

    def cmd_reset_grades(self, p):
        graph = self.current_item(self.timeline()).GetNodeGraph()
        check(graph and graph.ResetAllGrades(), "reset the grade")
        return "Grade reset"

    def cmd_stabilize(self, p):
        check(self.current_item(self.timeline()).Stabilize(), "stabilize the clip")
        return "Clip stabilized"

    def cmd_smart_reframe(self, p):
        check(self.current_item(self.timeline()).SmartReframe(), "run Smart Reframe")
        return "Smart Reframe done"

    def cmd_save_project(self, p):
        check(self.resolve().GetProjectManager().SaveProject(), "save the project")
        return "Project saved"

    def step_timeline(self, step):
        project = self.project()
        count = project.GetTimelineCount()
        current = project.GetCurrentTimeline()
        ids = [project.GetTimelineByIndex(i).GetUniqueId() for i in range(1, count + 1)]
        index = ids.index(current.GetUniqueId()) if current and current.GetUniqueId() in ids else 0
        target = project.GetTimelineByIndex((index + step) % count + 1)
        check(project.SetCurrentTimeline(target), "switch timeline")
        return "Timeline: " + target.GetName()

    def cmd_next_timeline(self, p):
        return self.step_timeline(1)

    def cmd_prev_timeline(self, p):
        return self.step_timeline(-1)

    def cmd_duplicate_timeline(self, p):
        tl = self.timeline()
        copy = tl.DuplicateTimeline(tl.GetName() + " copy")
        check(copy, "duplicate the timeline")
        return "Duplicated as " + copy.GetName()

    def cmd_detect_scene_cuts(self, p):
        check(self.timeline().DetectSceneCuts(), "detect scene cuts")
        return "Scene cuts detected"

    def cmd_add_render_job(self, p):
        project = self.project()
        if p["preset"].strip():
            check(project.LoadRenderPreset(p["preset"].strip()), f"load the render preset {p['preset']!r}")
        check(project.AddRenderJob(), "add the render job")
        return "Added to the render queue"

    def cmd_start_render(self, p):
        check(self.project().StartRendering(), "start rendering")
        return "Rendering started"

    def cmd_export_still(self, p):
        tl = self.timeline()
        folder = p["folder"].strip() or os.path.join(os.path.expanduser("~"), "Pictures")
        os.makedirs(folder, exist_ok=True)
        name = re.sub(r'[\\/:*?"<>|]', "_", f"{tl.GetName()} {tl.GetCurrentTimecode()}")
        path = os.path.join(folder, name + ".png")
        check(self.project().ExportCurrentFrameAsStill(path), "export the frame")
        return "Saved " + path

    def cmd_keyboard_preset(self, p):
        check(self.resolve().LoadKeyboardPreset(p["name"]), f"load the keyboard preset {p['name']!r}")
        return "Keyboard layout: " + p["name"]

    def cmd_layout_preset(self, p):
        check(self.resolve().LoadLayoutPreset(p["name"]), f"load the layout preset {p['name']!r}")
        return "UI layout: " + p["name"]

    def cmd_delete_marker(self, p):
        tl = self.timeline()
        current, _, _ = self.playhead(tl)
        check(tl.DeleteMarkerAtFrame(current - tl.GetStartFrame()), "find a marker at the playhead")
        return "Marker deleted"

    def cmd_delete_markers(self, p):
        check(self.timeline().DeleteMarkersByColor(p["color"]), "delete the markers")
        return f"{p['color']} markers deleted"

    def cmd_clear_in_out(self, p):
        check(self.timeline().ClearMarkInOut("all"), "clear In and Out")
        return "In and Out cleared"

    def cmd_add_flag(self, p):
        for item in self.selected(self.timeline()):
            item.AddFlag(p["color"])
        return f"{p['color']} flag added"

    def cmd_clear_flags(self, p):
        for item in self.selected(self.timeline()):
            item.ClearFlags(p["color"])
        return "Flags cleared"

    def cmd_set_speed(self, p):
        options = {"Percentage": float(p["percent"]), "RippleTimeline": p["ripple"] == "Yes"}
        for item in self.selected(self.timeline()):
            check(item.SetSpeed(options), "change the clip speed")
        return f"Speed {p['percent']}%"

    def cmd_set_fades(self, p):
        fades = {"FadeIn": int(p["fade_in"]), "FadeOut": int(p["fade_out"])}
        for item in self.selected(self.timeline()):
            check(item.SetFades(fades), "set the fades")
        return "Fades set"

    def cmd_add_transition(self, p):
        options = {"type": p["type"], "position": p["position"]}
        if p["frames"].strip():
            options["duration"] = int(p["frames"])
        for item in self.selected(self.timeline()):
            check(item.AddTransition(options), f"add {p['type']!r}")
        return p["type"] + " added"

    def cmd_fusion_clip(self, p):
        tl = self.timeline()
        check(tl.CreateFusionClip(self.selected(tl)), "make a Fusion clip")
        return "Fusion clip created"

    def cmd_insert_generator(self, p):
        check(self.timeline().InsertGeneratorIntoTimeline(p["name"]), f"insert the generator {p['name']!r}")
        return f"Generator {p['name']} inserted"

    def cmd_normalize_audio(self, p):
        tl = self.timeline()
        check(tl.NormalizeAudioLevel(self.selected(tl)), "normalize the audio")
        return "Audio normalized"

    def cmd_toggle_track(self, p):
        tl = self.timeline()
        kind, number = p["track"], int(p["number"])
        enable = not tl.GetIsTrackEnabled(kind, number)
        check(tl.SetTrackEnable(kind, number, enable), f"switch {kind} track {number}")
        return f"{kind.capitalize()} track {number} {'on' if enable else 'off'}"

    def cmd_toggle_track_lock(self, p):
        tl = self.timeline()
        kind, number = p["track"], int(p["number"])
        lock = not tl.GetIsTrackLocked(kind, number)
        check(tl.SetTrackLock(kind, number, lock), f"lock {kind} track {number}")
        return f"{kind.capitalize()} track {number} {'locked' if lock else 'unlocked'}"

    def cmd_add_version(self, p):
        name = p["name"].strip() or time.strftime("Version %H:%M:%S")
        check(self.current_item(self.timeline()).AddVersion(name, 0), "add a colour version")
        return "Version added: " + name

    def cmd_magic_mask(self, p):
        check(self.current_item(self.timeline()).CreateMagicMask(p["direction"]), "create the Magic Mask")
        return "Magic Mask created"

    def cmd_stop_render(self, p):
        self.project().StopRendering()
        return "Rendering stopped"

    def cmd_subtitles_from_audio(self, p):
        check(self.timeline().CreateSubtitlesFromAudio(), "create subtitles")
        return "Subtitles created"

    def cmd_track_counts(self, p):
        tl = self.timeline()
        return json.dumps({"video": tl.GetTrackCount("video"), "audio": tl.GetTrackCount("audio")})

    def cmd_keyboard_layout(self, p):
        """Returns the current keyboard layout as JSON: {command: [shortcuts]}.

        Exported layouts hold only their changes on top of an imported base
        layout, so the base is exported and merged first."""
        resolve = self.resolve()
        name = resolve.GetCurrentKeyboardPreset()
        commands, seen = {}, set()

        def load(preset):
            if preset in seen:
                return
            seen.add(preset)
            path = os.path.join(os.environ.get("TEMP", "."), f"ose-keys-{len(seen)}.txt")
            check(resolve.ExportKeyboardPreset(preset, path), f"export the keyboard layout {preset!r}")
            with open(path, encoding="utf-8", errors="replace") as f:
                lines = f.read().replace("\r\n", "\n").replace("\r", "\n").split("\n")
            os.remove(path)
            own = {}
            for line in lines:
                text = line.strip()
                if not text or text.startswith("#") and ":=" not in text:
                    continue
                match = re.match(r'import\s+"([^"]+)"', text)
                if match:
                    load(match.group(1))
                    continue
                if ":=" in text:
                    command, keys = text.split(":=", 1)
                    own[command.strip()] = [k.strip() for k in keys.split(" | ") if k.strip()]
            commands.update(own)

        load(name)
        return json.dumps({"preset": name, "commands": commands})

    def run(self, command, params):
        handler = getattr(self, "cmd_" + command, None)
        if handler is None:
            raise Fail(f"Unknown command {command!r}")
        return handler(params)


def check(result, what):
    if not result:
        raise Fail("Resolve could not " + what)


def reply(request_id, ok, message):
    sys.stdout.write(json.dumps({"id": request_id, "ok": ok, "message": message}) + "\n")
    sys.stdout.flush()


def main():
    bridge, load_error = None, None
    try:
        bridge = Bridge(sys.argv[1])
    except Exception as e:  # keep answering so the app can show the reason
        load_error = f"Cannot load Resolve's scripting library: {e}"

    for line in sys.stdin:
        if not line.strip():
            continue
        request_id = None
        try:
            request = json.loads(line)
            request_id = request.get("id")
            if bridge is None:
                raise Fail(load_error)
            reply(request_id, True, bridge.run(request["command"], request.get("params") or {}))
        except Fail as e:
            reply(request_id, False, str(e))
        except Exception as e:
            if bridge is not None:
                bridge.app = None
            reply(request_id, False, f"{type(e).__name__}: {e}")


if __name__ == "__main__":
    main()
