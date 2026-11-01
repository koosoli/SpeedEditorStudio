// Physical layout of the Speed Editor, ids matching crates/speed-editor/src/keys.rs.
// Each block is a grid where a normal key spans 2 columns.
// Entry: [id, label, span, { led, alt (secondary label printed on the key), tone }]
"use strict";

const LED = { led: true };

const KEY_LAYOUT = {
  left: [
    [["smart_insert", "SMART INSRT", 2, { alt: "CLIP" }], ["append", "APPND", 2, { alt: "CLIP" }], ["ripple_overwrite", "RIPL O/WR", 2]],
    [["close_up", "CLOSE UP", 2, { led: true, alt: "YPOS" }], ["place_on_top", "PLACE ON TOP", 2], ["source_overwrite", "SRC O/WR", 2]],
    "gap",
    [["in", "IN", 3, { alt: "CLR", tone: "gray" }], ["out", "OUT", 3, { alt: "CLR", tone: "gray" }]],
    [["trim_in", "TRIM IN", 2], ["trim_out", "TRIM OUT", 2], ["roll", "ROLL", 2, { alt: "SLIDE" }]],
    [["slip_source", "SLIP SRC", 2], ["slip_dest", "SLIP DEST", 2], ["transition_duration", "TRANS DUR", 2, { alt: "SET" }]],
    [["cut", "CUT", 2, LED], ["dissolve", "DIS", 2, LED], ["smooth_cut", "SMTH CUT", 2, LED]],
  ],
  middle: [
    [["escape", "ESC", 2, { alt: "UNDO" }], ["sync_bin", "SYNC BIN", 2], ["audio_level", "AUDIO LEVEL", 2, { alt: "MARK" }], ["full_view", "FULL VIEW", 2, { alt: "RVW", tone: "red" }]],
    [["transition", "TRANS", 2, { led: true, alt: "TITLE" }], ["split", "SPLIT", 2, { alt: "MOVE" }], ["snap", "SNAP", 2, { led: true, alt: "=" }], ["ripple_delete", "RIPL DEL", 2]],
    "gap",
    [["cam7", "CAM 7", 2, LED], ["cam8", "CAM 8", 2, LED], ["cam9", "CAM 9", 2, LED], ["live_overwrite", "LIVE O/WR", 2, { led: true, alt: "RND" }]],
    [["cam4", "CAM 4", 2, LED], ["cam5", "CAM 5", 2, LED], ["cam6", "CAM 6", 2, LED], ["video_only", "VIDEO ONLY", 2, LED]],
    [["cam1", "CAM 1", 2, LED], ["cam2", "CAM 2", 2, LED], ["cam3", "CAM 3", 2, LED], ["audio_only", "AUDIO ONLY", 2, LED]],
    [["stop_play", "STOP / PLAY", 8]],
  ],
  right: [
    [["source", "SOURCE", 3], ["timeline", "TIMELINE", 3]],
    [["shuttle", "SHTL", 2, { led: true, tone: "gray" }], ["jog", "JOG", 2, { led: true, tone: "gray" }], ["scroll", "SCRL", 2, { led: true, tone: "gray" }]],
  ],
};

// Bit of each key's LED in the device's LED bitfield (crates/speed-editor/src/keys.rs).
const LED_BITS = {
  close_up: 0, cut: 1, dissolve: 2, smooth_cut: 3, transition: 4, snap: 5, cam7: 6, cam8: 7, cam9: 8,
  live_overwrite: 9, cam4: 10, cam5: 11, cam6: 12, video_only: 13, cam1: 14, cam2: 15, cam3: 16, audio_only: 17,
};

// Resolve edit functions offered as actions: pressed with the user's own
// Resolve shortcut (Resolve keyboard command names).
const RESOLVE_KEY_COMMANDS = [
  ["trimRippleStartToPlayhead", "Ripple start to playhead", "Trim"],
  ["trimRippleEndToPlayhead", "Ripple end to playhead", "Trim"],
  ["editNudgeTrimStepTrimStart", "Trim start", "Trim"],
  ["editNudgeTrimStepTrimEnd", "Trim end", "Trim"],
  ["editNudgeTrimStepExtendEdit", "Extend edit", "Trim"],
  ["editSelectEditPoint", "Select nearest edit point", "Trim"],
  ["editToggleEditPointType", "Toggle edit point side", "Trim"],
  ["editNudgeTrimStepNudgeForward", "Nudge one frame right", "Trim"],
  ["editNudgeTrimStepNudgeReverse", "Nudge one frame left", "Trim"],
  ["editTrim", "Trim edit mode", "Tools"],
  ["editPointer", "Selection mode", "Tools"],
  ["editBlade", "Blade edit mode", "Tools"],
  ["editToggleDynamicTrimMode", "Dynamic trim mode", "Tools"],
  ["editSelectClipsSelectAllBefore", "Select clips backward (all tracks)", "Select"],
  ["editSelectClipsSelectAllAfter", "Select clips forward (all tracks)", "Select"],
  ["editSelectClipsSelectTrackBefore", "Select clips backward (track)", "Select"],
  ["editSelectClipsSelectTrackAfter", "Select clips forward (track)", "Select"],
  ["editSelectItemAtPlayhead", "Select clip at playhead", "Select"],
  ["editDeselectAll", "Deselect all", "Select"],
  ["editBladeRazor", "Split at playhead", "Edit"],
  ["editDelete", "Ripple delete", "Edit"],
  ["editRippleCut", "Ripple cut", "Edit"],
  ["editUndo", "Undo", "Edit"],
  ["editRedo", "Redo", "Edit"],
  ["editInsertOverwriteActionInsert", "Insert", "Edit"],
  ["editInsertOverwriteActionOverwrite", "Overwrite", "Edit"],
  ["editInsertOverwriteActionReplace", "Replace", "Edit"],
  ["editInsertOverwriteActionPlaceOnTop", "Place on top", "Edit"],
  ["editInsertOverwriteActionRippleOverwrite", "Ripple overwrite", "Edit"],
  ["editInsertOverwriteActionFitToFill", "Fit to fill", "Edit"],
  ["editInsertOverwriteActionAppendAtEnd", "Append at end", "Edit"],
  ["editAddTransition", "Add transition", "Edit"],
  ["editSnapping", "Snapping on/off", "Edit"],
  ["editLinkedSelection", "Linked selection on/off", "Edit"],
  ["markIn", "Mark in", "Marks"],
  ["markOut", "Mark out", "Marks"],
  ["markClip", "Mark clip", "Marks"],
  ["markResetInOut", "Clear in and out", "Marks"],
  ["markMarkerAdd", "Add marker", "Marks"],
  ["controlMarkersNext", "Next marker", "Playback"],
  ["controlMarkersPrev", "Previous marker", "Playback"],
  ["controlPlayToggle", "Play / stop", "Playback"],
  ["controlPlayAroundToPlayAroundCurrentSelection", "Play around selection", "Playback"],
  ["controlFirstFrame", "Go to first frame", "Playback"],
  ["controlLastFrame", "Go to last frame", "Playback"],
  ["clipChangeClipSpeed", "Change clip speed", "Clip"],
  ["clipFreezeFrame", "Freeze frame", "Clip"],
  ["clipRetimeControls", "Retime controls", "Clip"],
  ["clipAudioIncreaseAudioLevel1dB", "Audio level +1 dB", "Clip"],
  ["clipAudioDecreaseAudioLevel1dB", "Audio level −1 dB", "Clip"],
  ["viewToggleSourceTimeline", "Source / timeline viewer", "View"],
  ["workspaceViewerModeFullViewer", "Full view", "View"],
  ["workspaceViewerModeCinemaViewer", "Cinema viewer", "View"],
  ["viewZoomSubZoomToFit", "Zoom timeline to fit", "View"],
];

// Resolve menu items offered as actions: clicked directly, no shortcut needed.
const RESOLVE_MENU_ITEMS = [
  ["Trim > Ripple > Start to Playhead", "Ripple start to playhead", "Trim"],
  ["Trim > Ripple > End to Playhead", "Ripple end to playhead", "Trim"],
  ["Trim > Roll > Start to Playhead", "Roll start to playhead", "Trim"],
  ["Trim > Roll > End to Playhead", "Roll end to playhead", "Trim"],
  ["Trim > Trim Start", "Trim start", "Trim"],
  ["Trim > Trim End", "Trim end", "Trim"],
  ["Trim > Extend Edit", "Extend edit", "Trim"],
  ["Trim > Fade In to Playhead", "Fade in to playhead", "Trim"],
  ["Trim > Fade Out to Playhead", "Fade out to playhead", "Trim"],
  ["Trim > Normal Edit Mode", "Selection mode", "Tools"],
  ["Trim > Trim Mode", "Trim mode", "Tools"],
  ["Trim > Dynamic Trim Mode", "Dynamic trim mode", "Tools"],
  ["Trim > Blade Edit Mode", "Blade mode", "Tools"],
  ["Edit > Swap Clips Towards Left", "Swap clip left", "Edit"],
  ["Edit > Swap Clips Towards Right", "Swap clip right", "Edit"],
  ["Edit > Ripple Delete", "Ripple delete", "Edit"],
  ["Edit > Delete Gaps", "Delete gaps", "Edit"],
  ["Edit > Insert Gap", "Insert gap", "Edit"],
  ["Edit > Duplicate Selection", "Duplicate selection", "Edit"],
  ["Timeline > Select Clips > Backward on All Tracks", "Select clips backward (all tracks)", "Select"],
  ["Timeline > Select Clips > Forward on All Tracks", "Select clips forward (all tracks)", "Select"],
  ["Timeline > Select Clips > Backward on Track", "Select clips backward (track)", "Select"],
  ["Timeline > Select Clips > Forward on Track", "Select clips forward (track)", "Select"],
  ["Timeline > Split Clips", "Split clips", "Timeline"],
  ["Timeline > Join Clips", "Join clips", "Timeline"],
  ["Timeline > Snapping", "Snapping on/off", "Timeline"],
  ["Timeline > Linked Selection", "Linked selection on/off", "Timeline"],
  ["Timeline > Auto Select > Auto Select All Tracks", "Auto select all tracks", "Tracks"],
  ["Timeline > Auto Select > Auto Deselect All Tracks", "Auto deselect all tracks", "Tracks"],
  ["Timeline > Sync Lock > Enable/Disable Sync Lock for All Tracks", "Sync lock all tracks on/off", "Tracks"],
];

// Keys whose native Resolve function the app can reproduce (crates/mapper/src/native.rs).
const NATIVE_KEYS = new Set([
  "smart_insert", "append", "ripple_overwrite", "place_on_top", "in", "out", "trim_in", "trim_out", "roll",
  "slip_source", "slip_dest", "escape", "audio_level", "full_view", "transition", "split", "snap",
  "ripple_delete", "stop_play", "source", "timeline", "dissolve",
  "cam1", "cam2", "cam3", "cam4", "cam5", "cam6", "cam7", "cam8", "cam9",
]);

const KEY_LABELS = {};
const KEY_ALT = {};
for (const block of Object.values(KEY_LAYOUT)) {
  for (const row of block) {
    if (row === "gap") continue;
    for (const [id, label, , opts] of row) {
      KEY_LABELS[id] = label;
      if (opts && opts.alt) KEY_ALT[id] = opts.alt;
    }
  }
}

// Ready-made actions for the palette: { keys }, { run } or { text }.
const PALETTE = [
  { group: "Windows", items: [
    ["Copy", { keys: "ctrl+c" }], ["Paste", { keys: "ctrl+v" }], ["Cut", { keys: "ctrl+x" }],
    ["Undo", { keys: "ctrl+z" }], ["Redo", { keys: "ctrl+y" }], ["Save", { keys: "ctrl+s" }],
    ["Select all", { keys: "ctrl+a" }], ["Find", { keys: "ctrl+f" }], ["Switch app", { keys: "alt+tab" }],
    ["Task view", { keys: "win+tab" }], ["Show desktop", { keys: "win+d" }], ["Snip screen", { keys: "win+shift+s" }],
    ["Fullscreen", { keys: "f11" }], ["New tab", { keys: "ctrl+t" }], ["Close tab", { keys: "ctrl+w" }],
    ["Reopen tab", { keys: "ctrl+shift+t" }], ["Next tab", { keys: "ctrl+tab" }], ["Prev tab", { keys: "ctrl+shift+tab" }],
  ]},
  { group: "Media", items: [
    ["Play / Pause", { keys: "media_play_pause" }], ["Next track", { keys: "media_next" }],
    ["Previous track", { keys: "media_previous" }], ["Stop", { keys: "media_stop" }],
    ["Volume up", { keys: "volume_up" }], ["Volume down", { keys: "volume_down" }], ["Mute", { keys: "volume_mute" }],
  ]},
  { group: "F13–F24", items: Array.from({ length: 12 }, (_, i) => [`F${13 + i}`, { keys: `f${13 + i}` }]) },
  { group: "Navigation", items: [
    ["Left", { keys: "left" }], ["Right", { keys: "right" }], ["Up", { keys: "up" }], ["Down", { keys: "down" }],
    ["Home", { keys: "home" }], ["End", { keys: "end" }], ["Page up", { keys: "pageup" }], ["Page down", { keys: "pagedown" }],
    ["Enter", { keys: "enter" }], ["Escape", { keys: "esc" }], ["Space", { keys: "space" }], ["Tab", { keys: "tab" }],
    ["Delete", { keys: "delete" }], ["Backspace", { keys: "backspace" }],
  ]},
  { group: "Programs", items: [
    ["Calculator", { run: "calc.exe" }], ["Notepad", { run: "notepad.exe" }], ["File Explorer", { run: "explorer.exe" }],
    ["Task Manager", { run: "taskmgr.exe" }], ["Settings", { run: "ms-settings:" }],
  ]},
  { group: "Text", items: [
    ["Today's date", { text: new Date().toISOString().slice(0, 10) }], ["Thumbs up", { text: "👍" }],
    ["Arrow", { text: "→" }], ["Line break", { keys: "shift+enter" }],
  ]},
];

const KEY_NAMES = {
  ctrl: "Ctrl", shift: "Shift", alt: "Alt", win: "Win", esc: "Esc", enter: "Enter", space: "Space",
  tab: "Tab", backspace: "Backspace", delete: "Del", insert: "Ins", home: "Home", end: "End",
  pageup: "PgUp", pagedown: "PgDn", left: "←", right: "→", up: "↑", down: "↓",
  media_play_pause: "Play/Pause", media_next: "Next track", media_previous: "Prev track", media_stop: "Stop",
  volume_up: "Vol +", volume_down: "Vol −", volume_mute: "Mute", printscreen: "PrtSc",
};

function prettyShortcut(text) {
  return text.split("+").map(part => {
    const p = part.trim().toLowerCase();
    if (KEY_NAMES[p]) return KEY_NAMES[p];
    if (/^f\d+$/.test(p)) return p.toUpperCase();
    return p.length === 1 ? p.toUpperCase() : p;
  }).join(" + ");
}

// Which action an object holds: "keys", "run", "text", "resolve" or null.
function actionKind(a) {
  if (!a) return null;
  return ["keys", "run", "text", "resolve", "resolve_key", "solo_track", "select_track", "resolve_menu"].find(k => a[k] !== undefined && a[k] !== null) || null;
}

// DaVinci Resolve commands, loaded from the app (crates/mapper/src/resolve.rs).
let RESOLVE_COMMANDS = [];

function describeAction(a) {
  const kind = actionKind(a);
  if (!kind) return "";
  if (kind === "resolve_menu") {
    const m = RESOLVE_MENU_ITEMS.find(m => m[0] === a.resolve_menu);
    return m ? m[1] : a.resolve_menu.split(">").pop().trim();
  }
  if (kind === "solo_track") return a.solo_track ? `Only track ${a.solo_track}` : "All tracks";
  if (kind === "select_track") return `Select clip on track ${a.select_track}`;
  if (kind === "resolve_key") {
    const c = RESOLVE_KEY_COMMANDS.find(c => c[0] === a.resolve_key);
    return c ? c[1] : a.resolve_key;
  }
  if (kind === "resolve") {
    const c = RESOLVE_COMMANDS.find(c => c.id === a.resolve);
    if (!c) return a.resolve || "Resolve…";
    const first = c.params[0];
    const value = first && ((a.params && a.params[first.key]) || first.default);
    return value && first.options.length ? `${c.name}: ${value}` : c.name;
  }
  const preset = PALETTE.flatMap(g => g.items).find(([, p]) => p[kind] !== undefined && p[kind] === a[kind]);
  if (preset) return preset[0];
  if (kind === "keys") return a.keys ? prettyShortcut(a.keys) : "…";
  if (kind === "run") return "Open " + (a.run.split(/[\\/]/).pop() || "…");
  return "Type “" + a.text + "”";
}

// KeyboardEvent.code → shortcut key name understood by mapper/keyboard.rs.
function codeToKey(code) {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
  if (/^Digit\d$/.test(code)) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code.toLowerCase();
  if (/^Numpad\d$/.test(code)) return "num" + code.slice(6);
  const map = {
    Enter: "enter", NumpadEnter: "enter", Escape: "esc", Tab: "tab", Space: "space", Backspace: "backspace",
    Delete: "delete", Insert: "insert", Home: "home", End: "end", PageUp: "pageup", PageDown: "pagedown",
    ArrowLeft: "left", ArrowRight: "right", ArrowUp: "up", ArrowDown: "down", Minus: "-", Equal: "=",
    Comma: ",", Period: ".", Slash: "/", Semicolon: ";", Quote: "'", BracketLeft: "[", BracketRight: "]",
    Backslash: "\\", Backquote: "`", PrintScreen: "printscreen",
    MediaPlayPause: "media_play_pause", MediaTrackNext: "media_next", MediaTrackPrevious: "media_previous",
    MediaStop: "media_stop", AudioVolumeUp: "volume_up", AudioVolumeDown: "volume_down", AudioVolumeMute: "volume_mute",
  };
  return map[code] || null;
}

const MODIFIER_CODES = {
  ControlLeft: "ctrl", ControlRight: "ctrl", ShiftLeft: "shift", ShiftRight: "shift",
  AltLeft: "alt", AltRight: "alt", MetaLeft: "win", MetaRight: "win",
};
