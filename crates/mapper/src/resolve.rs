//! Catalog of DaVinci Resolve commands that run through Resolve's scripting
//! API instead of keyboard shortcuts, so they work whatever the user's
//! Resolve keyboard layout is. The helper in crates/runtime implements each
//! `id`; the editor builds its action list and settings from this catalog.

use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Param {
    pub key: &'static str,
    pub label: &'static str,
    /// Allowed values; empty means free text.
    pub options: &'static [&'static str],
    pub default: &'static str,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Command {
    pub id: &'static str,
    pub name: &'static str,
    pub group: &'static str,
    pub help: &'static str,
    pub params: &'static [Param],
}

const PAGES: &[&str] = &["media", "cut", "edit", "fusion", "color", "fairlight", "deliver"];
const MARKER_COLORS: &[&str] = &[
    "Blue", "Cyan", "Green", "Yellow", "Red", "Pink", "Purple", "Fuchsia", "Rose", "Lavender", "Sky", "Mint",
    "Lemon", "Sand", "Cocoa", "Cream",
];
const CLIP_COLORS: &[&str] = &[
    "Orange", "Apricot", "Yellow", "Lime", "Olive", "Green", "Teal", "Navy", "Blue", "Purple", "Violet", "Pink",
    "Tan", "Beige", "Brown", "Chocolate",
];

const fn cmd(id: &'static str, name: &'static str, group: &'static str, help: &'static str) -> Command {
    Command { id, name, group, help, params: &[] }
}

const fn with(c: Command, params: &'static [Param]) -> Command {
    Command { params, ..c }
}

const fn choice(key: &'static str, label: &'static str, options: &'static [&'static str], default: &'static str) -> Param {
    Param { key, label, options, default }
}

const fn text(key: &'static str, label: &'static str, default: &'static str) -> Param {
    Param { key, label, options: &[], default }
}

pub const COMMANDS: &[Command] = &[
    with(cmd("page", "Open page", "Pages", "Switches Resolve to a page."), &[choice("page", "Page", PAGES, "edit")]),
    cmd("go_start", "Go to timeline start", "Playhead", "Moves the playhead to the first frame of the timeline."),
    cmd("go_end", "Go to timeline end", "Playhead", "Moves the playhead to the last frame of the timeline."),
    with(
        cmd("nudge", "Move playhead", "Playhead", "Moves the playhead by a number of frames (negative = back)."),
        &[text("frames", "Frames", "10")],
    ),
    cmd("next_marker", "Next marker", "Playhead", "Jumps to the next timeline marker."),
    cmd("prev_marker", "Previous marker", "Playhead", "Jumps to the previous timeline marker."),
    with(
        cmd("add_marker", "Add marker", "Markers", "Adds a timeline marker at the playhead."),
        &[choice("color", "Colour", MARKER_COLORS, "Blue"), text("name", "Name", ""), text("note", "Note", "")],
    ),
    with(
        cmd("clip_color", "Set clip colour", "Clips", "Colours the selected timeline clips."),
        &[choice("color", "Colour", CLIP_COLORS, "Orange")],
    ),
    cmd("clear_clip_color", "Clear clip colour", "Clips", "Removes the colour of the selected clips."),
    cmd("toggle_clips", "Enable / disable clips", "Clips", "Toggles the selected clips on or off."),
    cmd("ripple_delete_clips", "Ripple delete clips", "Clips", "Deletes the selected clips and closes the gap."),
    cmd("link_clips", "Link clips", "Clips", "Links the selected clips."),
    cmd("unlink_clips", "Unlink clips", "Clips", "Unlinks the selected clips."),
    cmd("compound_clip", "Make compound clip", "Clips", "Turns the selected clips into a compound clip."),
    with(
        cmd("insert_title", "Insert title", "Clips", "Inserts a title at the playhead."),
        &[text("name", "Title name", "Text+")],
    ),
    cmd("grab_still", "Grab still", "Color", "Grabs a still of the current clip into the gallery."),
    cmd("grab_all_stills", "Grab stills of all clips", "Color", "Grabs the middle frame of every clip."),
    cmd("reset_grades", "Reset grade", "Color", "Resets all grades of the current clip."),
    cmd("stabilize", "Stabilize clip", "Color", "Runs stabilization on the current clip."),
    cmd("smart_reframe", "Smart Reframe", "Color", "Runs Smart Reframe on the current clip."),
    cmd("save_project", "Save project", "Project", "Saves the current project."),
    cmd("next_timeline", "Next timeline", "Project", "Opens the next timeline of the project."),
    cmd("prev_timeline", "Previous timeline", "Project", "Opens the previous timeline of the project."),
    cmd("duplicate_timeline", "Duplicate timeline", "Project", "Duplicates the current timeline."),
    cmd("detect_scene_cuts", "Detect scene cuts", "Project", "Cuts the timeline at detected scene changes."),
    with(
        cmd("add_render_job", "Add to render queue", "Render", "Adds the timeline to the render queue."),
        &[text("preset", "Render preset (empty = current settings)", "")],
    ),
    cmd("start_render", "Start rendering", "Render", "Renders every job in the queue."),
    with(
        cmd("export_still", "Export frame as image", "Render", "Saves the frame at the playhead as a PNG."),
        &[text("folder", "Folder (empty = Pictures)", "")],
    ),
    with(
        cmd("keyboard_preset", "Load keyboard layout", "Project", "Switches Resolve to a saved keyboard preset."),
        &[text("name", "Preset name", "")],
    ),
    with(
        cmd("layout_preset", "Load UI layout", "Project", "Switches Resolve to a saved UI layout preset."),
        &[text("name", "Layout preset name", "")],
    ),
    cmd("delete_marker", "Delete marker at playhead", "Markers", "Removes the timeline marker under the playhead."),
    with(
        cmd("delete_markers", "Delete markers by colour", "Markers", "Removes all timeline markers of one colour."),
        &[choice("color", "Colour", MARKER_COLORS_ALL, "All")],
    ),
    cmd("clear_in_out", "Clear In and Out", "Markers", "Clears the timeline's In and Out marks."),
    with(
        cmd("add_flag", "Add flag", "Clips", "Flags the selected clips."),
        &[choice("color", "Colour", MARKER_COLORS, "Blue")],
    ),
    with(
        cmd("clear_flags", "Clear flags", "Clips", "Removes flags from the selected clips."),
        &[choice("color", "Colour", MARKER_COLORS_ALL, "All")],
    ),
    with(
        cmd("set_speed", "Set clip speed", "Clips", "Changes the speed of the selected clips (100 = normal)."),
        &[text("percent", "Speed %", "50"), choice("ripple", "Ripple timeline", YES_NO, "No")],
    ),
    with(
        cmd("set_fades", "Add fades", "Clips", "Sets fade in and fade out on the selected clips."),
        &[text("fade_in", "Fade in (frames)", "12"), text("fade_out", "Fade out (frames)", "12")],
    ),
    with(
        cmd("add_transition", "Add transition", "Clips", "Adds a transition to the selected clips."),
        &[
            text("type", "Transition", "Cross Dissolve"),
            choice("position", "Where", &["end", "start"], "end"),
            text("frames", "Length in frames (empty = default)", ""),
        ],
    ),
    cmd("fusion_clip", "Make Fusion clip", "Clips", "Turns the selected clips into a Fusion clip."),
    with(
        cmd("insert_generator", "Insert generator", "Clips", "Inserts a generator at the playhead."),
        &[text("name", "Generator name", "Solid Color")],
    ),
    cmd("normalize_audio", "Normalize audio", "Clips", "Normalizes the audio level of the selected clips."),
    with(
        cmd("toggle_track", "Enable / disable track", "Tracks", "Switches a track on or off."),
        &[choice("track", "Track type", TRACK_TYPES, "video"), text("number", "Track number", "1")],
    ),
    with(
        cmd("toggle_track_lock", "Lock / unlock track", "Tracks", "Locks or unlocks a track."),
        &[choice("track", "Track type", TRACK_TYPES, "video"), text("number", "Track number", "1")],
    ),
    with(
        cmd("add_version", "Add colour version", "Color", "Adds a local colour version to the current clip."),
        &[text("name", "Version name (empty = automatic)", "")],
    ),
    with(
        cmd("magic_mask", "Magic Mask", "Color", "Creates a Magic Mask on the current clip."),
        &[choice("direction", "Track", &["BI", "F", "B"], "BI")],
    ),
    cmd("stop_render", "Stop rendering", "Render", "Stops the render in progress."),
    cmd("subtitles_from_audio", "Create subtitles from audio", "Project", "Transcribes the timeline into subtitles."),
];

const MARKER_COLORS_ALL: &[&str] = &[
    "All", "Blue", "Cyan", "Green", "Yellow", "Red", "Pink", "Purple", "Fuchsia", "Rose", "Lavender", "Sky", "Mint",
    "Lemon", "Sand", "Cocoa", "Cream",
];
const TRACK_TYPES: &[&str] = &["video", "audio", "subtitle"];
const YES_NO: &[&str] = &["No", "Yes"];

pub fn find(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|c| c.id == id)
}

/// Checks a command's parameters and fills in defaults.
pub fn resolve_params(id: &str, given: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>, String> {
    let command = find(id).ok_or_else(|| format!("unknown Resolve command {id:?}"))?;
    if let Some(unknown) = given.keys().find(|k| !command.params.iter().any(|p| p.key == k.as_str())) {
        return Err(format!("Resolve command {id:?} has no setting {unknown:?}"));
    }
    let mut params = BTreeMap::new();
    for p in command.params {
        let value = given.get(p.key).cloned().unwrap_or_else(|| p.default.to_string());
        if !p.options.is_empty() && !p.options.contains(&value.as_str()) {
            return Err(format!("Resolve command {id:?}: {value:?} is not a valid {}", p.label.to_lowercase()));
        }
        params.insert(p.key.to_string(), value);
    }
    let whole = |key: &str, allow_empty: bool| {
        let v = params[key].trim();
        (allow_empty && v.is_empty()) || v.parse::<i64>().is_ok()
    };
    let numeric: &[(&str, &str, bool)] = match id {
        "nudge" => &[("frames", "frames", false)],
        "set_fades" => &[("fade_in", "fade in", false), ("fade_out", "fade out", false)],
        "add_transition" => &[("frames", "length", true)],
        "toggle_track" | "toggle_track_lock" => &[("number", "track number", false)],
        _ => &[],
    };
    if let Some((_, label, _)) = numeric.iter().find(|(key, _, empty)| !whole(key, *empty)) {
        return Err(format!("{}: {label} must be a whole number", command.name));
    }
    if id == "set_speed" && params["percent"].trim().parse::<f64>().is_err() {
        return Err("Set clip speed: the speed must be a number".into());
    }
    Ok(params)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique() {
        for (i, c) in COMMANDS.iter().enumerate() {
            assert!(COMMANDS[..i].iter().all(|o| o.id != c.id), "duplicate {}", c.id);
        }
    }

    #[test]
    fn fills_defaults_and_checks_options() {
        let p = resolve_params("add_marker", &BTreeMap::new()).unwrap();
        assert_eq!(p["color"], "Blue");
        let bad = BTreeMap::from([("color".to_string(), "Plaid".to_string())]);
        assert!(resolve_params("add_marker", &bad).is_err());
        assert!(resolve_params("no_such_thing", &BTreeMap::new()).is_err());
    }
}
