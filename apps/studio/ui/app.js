// Speed Editor Studio UI. Edits a ProfileDoc (see crates/mapper/src/profile.rs)
// and talks to the Rust side through Tauri commands and "runtime" events.
"use strict";

const $ = id => document.getElementById(id);
const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
};

const S = {
  doc: null,
  savedJson: "",
  path: "",
  page: 0,
  selected: null,        // key id, "wheel", or null for page settings
  learn: false,
  connected: false,
  active: false,
  deviceState: null,     // { custom, page } reported by the device
  leds: 0,               // key LED bitfield last sent to the device
  held: new Set(),
  error: null,
  layer: "base",         // "base" or a modifier key id: which layer of the page is edited
  view: "picture",       // "picture" (device diagram) or "list" (all mappings)
  paletteGroup: "All",
  assignTo: "press",     // palette clicks set the "press" or the "hold" action
  search: "",
  log: [],
};

// ---------------------------------------------------------------- backend

const api = window.__TAURI__
  ? { invoke: window.__TAURI__.core.invoke, listen: window.__TAURI__.event.listen }
  : mockApi();

function mockApi() {
  // Lets the UI open in a plain browser for design work; nothing is saved.
  const doc = {
    version: 1, page_keys: ["cut", "dissolve", "smooth_cut"], toggle_chord: ["cut", "smooth_cut"], fn_key: "cut", modifier_keys: ["dissolve", "smooth_cut"],
    pages: [
      { name: "DaVinci", davinci_defaults: true, wheel: { davinci: {} }, keys: {}, fn_keys: {
        in: { resolve_menu: "Trim > Ripple > Start to Playhead", label: "Ripple start to playhead" }, out: { resolve_menu: "Trim > Ripple > End to Playhead", label: "Ripple end to playhead" },
        trim_in: { resolve_key: "editNudgeTrimStepTrimStart", label: "Trim start" }, trim_out: { resolve_key: "editNudgeTrimStepTrimEnd", label: "Trim end" },
        slip_source: { resolve_key: "editSelectClipsSelectAllBefore", label: "Select clips backward" }, slip_dest: { resolve_key: "editSelectClipsSelectAllAfter", label: "Select clips forward" },
        cam2: { solo_track: 2, label: "Only track 2" }, cam9: { solo_track: 0, label: "All tracks" } } },
      { name: "Hotkeys", wheel: { keys: { left: "left", right: "right" } }, keys: {
        cam1: { keys: "f13" }, cam2: { keys: "f14" }, escape: { keys: "ctrl+z", hold: { keys: "ctrl+y" } } } },
      { name: "Media", wheel: { keys: { left: "volume_down", right: "volume_up" } }, keys: {
        stop_play: { keys: "media_play_pause" }, cam1: { run: "calc.exe" } } },
    ],
  };
  return {
    invoke: async (cmd) => {
      if (cmd === "load_profile") return { path: "(browser preview - not saved)", doc: structuredClone(doc) };
      if (cmd === "example_profile") return structuredClone(doc);
      if (cmd === "resolve_catalog") return [
        { id: "page", name: "Open page", group: "Pages", help: "Switches Resolve to a page.",
          params: [{ key: "page", label: "Page", options: ["media", "cut", "edit", "fusion", "color", "fairlight", "deliver"], default: "edit" }] },
        { id: "add_marker", name: "Add marker", group: "Markers", help: "Adds a timeline marker at the playhead.",
          params: [{ key: "color", label: "Colour", options: ["Blue", "Red", "Green", "Yellow"], default: "Blue" },
                   { key: "name", label: "Name", options: [], default: "" }] },
        { id: "save_project", name: "Save project", group: "Project", help: "Saves the current project.", params: [] },
      ];
      if (cmd === "connect") throw "Browser preview: no device access";
      return null;
    },
    listen: async () => {},
  };
}

// ---------------------------------------------------------------- helpers

const page = () => S.doc.pages[S.page];
const isPageKey = id => S.doc.page_keys.includes(id);
const keyLabel = id => KEY_LABELS[id] || id;

// Keys that pick a layer when held with another key: the Fn key (CUT) first,
// then the other modifier keys (DIS, SMTH CUT).
const modifiers = () => [S.doc.fn_key, ...(S.doc.modifier_keys || [])].filter(Boolean);

// The layer shown: a modifier held on the device, else the one chosen above.
function activeLayer() {
  const held = modifiers().find(m => S.held.has(m));
  if (held) return held;
  return modifiers().includes(S.layer) ? S.layer : "base";
}
const fnLayer = () => activeLayer() !== "base";
const layerPrefix = l => (l === "base" ? "" : keyLabel(l) + " + ");

// Mappings of one layer of a page. With `create`, makes sure it exists.
function layerKeys(p, l, create) {
  if (l === "base") return p.keys;
  if (l === S.doc.fn_key) {
    if (create && !p.fn_keys) p.fn_keys = {};
    return p.fn_keys || {};
  }
  if (create) {
    p.layers = p.layers || {};
    p.layers[l] = p.layers[l] || {};
  }
  return (p.layers && p.layers[l]) || {};
}

// Mappings of the layer being edited.
const layer = create => layerKeys(page(), activeLayer(), create);
const dirty = () => JSON.stringify(S.doc) !== S.savedJson;

function log(text) {
  const time = new Date().toLocaleTimeString();
  S.log.unshift(`${time}  ${text}`);
  S.log.length = Math.min(S.log.length, 4);
  $("log").replaceChildren(...S.log.map(l => el("div", null, l)));
}

async function changed() {
  try {
    await api.invoke("validate_profile", { doc: S.doc });
    S.error = null;
  } catch (e) {
    S.error = String(e);
  }
  render();
}

// A key mapping is { keys | run | text, label?, hold?: { keys | run | text } }.
const pressOf = m => {
  const kind = actionKind(m);
  return kind ? { [kind]: m[kind] } : null;
};

function updateMapping(keyId, edit) {
  if (!keyId || keyId === "wheel" || isPageKey(keyId)) return;
  const m = structuredClone(layer()[keyId] || {});
  edit(m);
  for (const k of Object.keys(m)) if (m[k] === undefined || m[k] === null) delete m[k];
  if (m.label !== undefined && !m.label.trim()) delete m.label;
  if (Object.keys(m).length) layer(true)[keyId] = m;
  else delete layer()[keyId];
  changed();
}

function setPress(keyId, action) {
  updateMapping(keyId, m => {
    delete m.keys; delete m.run; delete m.text; delete m.resolve; delete m.resolve_key; delete m.solo_track; delete m.select_track; delete m.resolve_menu; delete m.params;
    if (action) Object.assign(m, action);
  });
}

// The press part of a mapping, including a Resolve command's settings.
function pressAction(m) {
  const p = pressOf(m);
  if (p && p.resolve !== undefined && m.params) p.params = m.params;
  return p;
}

// Palette groups: the Resolve commands first, then the built-in presets.
function paletteGroups() {
  const resolve = RESOLVE_COMMANDS.map(c => [c.name, { resolve: c.id }]);
  const functions = RESOLVE_KEY_COMMANDS.map(([id, name]) => [name, { resolve_key: id }]);
  for (const [path, name] of RESOLVE_MENU_ITEMS) functions.push([name, { resolve_menu: path }]);
  for (let n = 1; n <= 8; n++) functions.push([`Only track ${n}`, { solo_track: n }]);
  functions.push(["All tracks", { solo_track: 0 }]);
  for (let n = 1; n <= 8; n++) functions.push([`Select clip on track ${n}`, { select_track: n }]);
  const groups = [{ group: "DaVinci functions", items: functions }, ...PALETTE];
  return resolve.length ? [{ group: "DaVinci scripts", items: resolve }, ...groups] : groups;
}

function setHold(keyId, action) {
  updateMapping(keyId, m => { m.hold = action || undefined; });
}

const assign = (keyId, action) => (S.assignTo === "hold" ? setHold : setPress)(keyId, action);
const defaultName = () => (page().davinci_defaults && !fnLayer() ? "DaVinci" : "Nothing");

// ---------------------------------------------------------------- rendering

function render() {
  renderTopbar();
  renderTabs();
  renderDevice();
  renderInspector();
  renderPalette();
}

function renderTopbar() {
  $("profilePath").textContent = S.path;
  const save = $("saveBtn");
  save.disabled = !dirty() || !!S.error;
  $("saveLabel").textContent = dirty() ? "Save •" : "Save";
  $("revertBtn").disabled = !dirty();
  $("connectToggle").checked = S.connected;
  $("activeToggle").checked = S.active;
  $("activeToggle").disabled = !S.connected;
  const hint = $("hint");
  const normal = S.deviceState && !S.deviceState.custom;
  hint.className = "hint" + (S.error ? " error" : normal ? " normal" : "");
  hint.textContent = S.error
    ? "⚠ " + S.error
    : normal
      ? `Normal mode on the device: your mappings are off and CUT, DIS and SMTH CUT work as transition keys. Press ${chordText()} together to switch the mappings back on.`
    : fnLayer() && !modifiers().some(m => S.held.has(m))
      ? `Editing the ${keyLabel(activeLayer())} + key layer · choose “Keys” above the picture to go back`
      : page().davinci_defaults
        ? `Plain keys keep their DaVinci function · dimmed keys have none until you map them · ${chordText()}: mappings off/on`
        : `${chordText()} together: mappings off/on`;
}

function chordText() {
  return S.doc.toggle_chord.map(k => KEY_LABELS[k] || k).join(" + ");
}

function renderTabs() {
  const tabs = S.doc.pages.map((p, i) => {
    const b = el("button", "tab" + (i === S.page ? " active" : ""));
    const live = S.connected && S.deviceState && S.deviceState.custom && S.deviceState.page === i;
    b.append(el("span", "tab-key", KEY_LABELS[S.doc.page_keys[i]] || S.doc.page_keys[i]));
    b.append(el("span", "tab-name", p.name));
    if (i === 0) b.append(el("span", "badge", "start"));
    if (live) b.append(el("span", "badge live", "on device"));
    b.onclick = () => { S.page = i; S.selected = null; render(); };
    return b;
  });
  const views = el("div", "segmented layer-switch view-switch");
  for (const [k, text] of [["picture", "Picture of the device"], ["list", "All mappings"]]) {
    const b = el("button", "icon-seg" + (S.view === k ? " on" : ""));
    b.append(icon(k));
    b.title = text;
    b.setAttribute("aria-label", text);
    b.onclick = () => { S.view = k; render(); };
    views.append(b);
  }
  tabs.push(el("div", "spacer"), views);
  if (modifiers().length && S.view === "picture") {
    const seg = el("div", "segmented layer-switch");
    for (const k of ["base", ...modifiers()]) {
      // Keys: a keyboard icon. A layer: the layer icon and the held key's name.
      const b = el("button", "icon-seg" + (k === "base" ? "" : " fn") + (activeLayer() === k ? " on" : ""));
      b.append(icon(k === "base" ? "keys" : "layer"));
      if (k !== "base") b.append(el("span", null, keyLabel(k)));
      b.title = k === "base" ? "Keys" : `${keyLabel(k)} + key: hold ${keyLabel(k)} and press another key`;
      b.setAttribute("aria-label", b.title);
      b.onclick = () => { S.layer = k; render(); };
      seg.append(b);
    }
    tabs.push(seg);
  }
  $("pageTabs").replaceChildren(...tabs);
}

function renderDevice() {
  const list = S.view === "list";
  $("listView").hidden = !list;
  $("device").hidden = list;
  if (list) return renderList();
  for (const [block, rows] of Object.entries(KEY_LAYOUT)) {
    const host = $("block" + block[0].toUpperCase() + block.slice(1));
    const wheel = host.querySelector(".wheel-wrap");
    const nodes = [];
    for (const row of rows) {
      if (row === "gap") { nodes.push(el("div", "row-gap")); continue; }
      for (const [id, label, span, opts] of row) nodes.push(keyButton(id, label, span, opts || {}));
    }
    if (wheel) nodes.push(wheel);
    host.replaceChildren(...nodes);
  }

  // Purple only while a function key is held on the device; a layer picked
  // above the picture for editing keeps the normal colours.
  $("device").classList.toggle("fn", modifiers().some(m => S.held.has(m)));
  const w = page().wheel;
  $("wheelLabel").textContent = !w ? "Wheel: nothing"
    : w.davinci ? "DaVinci: JOG · SHTL · SCRL"
    : w.scroll ? (w.scroll.horizontal ? "Scroll sideways" : "Scroll")
    : `${describeAction({ keys: w.keys.left })}  ⟲ ⟳  ${describeAction({ keys: w.keys.right })}`;
  const wheelBtn = $("wheel");
  wheelBtn.classList.toggle("selected", S.selected === "wheel");
  wheelBtn.classList.toggle("mapped", !!w);
  wheelBtn.onclick = () => select("wheel");
  fitDevice();
}

// Scales the device picture to the space it has, so small windows never cut
// it off and large ones use the room (up to 125 %).
function fitDevice() {
  const area = document.querySelector(".device-area");
  const device = $("device");
  if (device.hidden || !area.clientWidth) return;
  device.style.zoom = 1;
  const style = getComputedStyle(area);
  const padX = parseFloat(style.paddingLeft) + parseFloat(style.paddingRight);
  const padY = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
  const hint = $("hint");
  const hintH = hint.offsetHeight + parseFloat(style.rowGap || 0);
  const w = (area.clientWidth - padX) / device.offsetWidth;
  const h = (area.clientHeight - padY - hintH) / device.offsetHeight;
  device.style.zoom = Math.max(0.45, Math.min(1.25, w, h));
}

function keyButton(id, label, span, opts) {
  const b = el("button", "key" + (opts.tone ? " tone-" + opts.tone : ""));
  b.style.gridColumn = `span ${span}`;
  b.dataset.key = id;
  const m = layer()[id];
  const press = pressAction(m);
  const pageIndex = S.doc.page_keys.indexOf(id);
  if (pageIndex >= 0) {
    b.classList.add("page-key");
    if (pageIndex === S.page) b.classList.add("current-page");
  } else if (press || (m && m.hold)) {
    b.classList.add("mapped");
  }
  if (S.selected === id) b.classList.add("selected");
  if (S.held.has(id)) b.classList.add("held");
  if (opts.led) b.append(el("span", "led" + (ledOn(id) ? " on" : "")));
  b.append(el("span", "cap", label));
  if (opts.alt) b.append(el("span", "alt", `(${opts.alt})`));
  const isModifier = modifiers().includes(id);
  if (isModifier) b.classList.add("fn-key");
  if (pageIndex >= 0) {
    const fnNote = isModifier && activeLayer() === id ? "held" : `Page ${pageIndex + 1}`;
    b.append(el("span", "act", fnNote));
  } else {
    const wheelMode = page().wheel && page().wheel.davinci && !fnLayer() && ["jog", "shuttle", "scroll"].includes(id);
    const text = wheelMode ? "Wheel mode" : (m && m.label) || describeAction(press);
    // Keys doing their native job show nothing extra (see the legend below
    // the device); keys without a Resolve command are dimmed.
    if (!text && defaultName() === "DaVinci" && !NATIVE_KEYS.has(id)) b.classList.add("no-native");
    b.append(el("span", "act", text));
    if (m && m.hold) b.append(el("span", "hold", "hold: " + describeAction(m.hold)));
  }
  b.onclick = () => select(id);
  b.ondragover = e => {
    if (pageIndex < 0) { e.preventDefault(); b.classList.add("drop"); }
  };
  b.ondragleave = () => b.classList.remove("drop");
  b.ondrop = e => {
    e.preventDefault();
    b.classList.remove("drop");
    const action = JSON.parse(e.dataTransfer.getData("application/json"));
    S.selected = id;
    assign(id, action);
  };
  return b;
}

// ---------------------------------------------------------------- mappings list

// Keys in physical order, for listing and pickers.
const KEY_ORDER = Object.values(KEY_LAYOUT).flat().filter(r => r !== "gap").flat().map(([id]) => id);

// Every mapping of every page as rows: page, layer, key, press/long press.
function allMappings() {
  const rows = [];
  S.doc.pages.forEach((p, pageIndex) => {
    for (const layer of ["base", ...modifiers()]) {
      const keys = layerKeys(p, layer);
      for (const id of KEY_ORDER.filter(k => keys[k])) {
        const m = keys[id];
        const trigger = layerPrefix(layer) + KEY_LABELS[id];
        const press = pressAction(m);
        if (press) rows.push({ pageIndex, layer, id, part: "press", trigger, action: press, label: m.label });
        if (m.hold) rows.push({ pageIndex, layer, id, part: "hold", trigger: trigger + " (long press)", action: m.hold, label: press ? null : m.label });
      }
    }
    if (p.wheel) {
      const w = p.wheel;
      const text = w.davinci ? "DaVinci wheel (JOG · SHTL · SCRL)"
        : w.scroll ? (w.scroll.horizontal ? "Scroll sideways" : "Scroll")
        : w.keys ? `${describeAction({ keys: w.keys.left })} / ${describeAction({ keys: w.keys.right })}`
        : "Wheel";
      rows.push({ pageIndex, layer: "base", id: "wheel", part: "wheel", trigger: "Jog wheel", text });
    }
  });
  return rows;
}

function renderList() {
  const host = $("listView");
  const nodes = [];
  const rows = allMappings();
  S.doc.pages.forEach((p, i) => {
    const pageRows = rows.filter(r => r.pageIndex === i);
    const section = el("div", "list-page");
    section.append(el("h3", null, `Page ${i + 1} · ${KEY_LABELS[S.doc.page_keys[i]]} · ${p.name}`));
    section.append(addMappingForm(i));
    if (!pageRows.length) section.append(el("p", "info", i === 0 ? "Nothing customised: every key keeps its DaVinci function." : "Nothing mapped on this page yet."));
    for (const r of pageRows) {
      const row = el("div", "list-row");
      const selected = S.page === i && S.selected === r.id && (r.id === "wheel" || activeLayer() === r.layer);
      if (selected) row.classList.add("selected");
      row.append(el("span", "trigger" + (r.layer !== "base" ? " fn" : ""), r.trigger));
      const what = r.text || describeAction(r.action);
      row.append(el("span", "what", what));
      row.append(el("span", "row-label", r.label && r.label !== what ? r.label : ""));
      const del = el("button", "ghost small", "Remove");
      del.title = "Remove this mapping";
      del.onclick = e => {
        e.stopPropagation();
        S.page = i;
        S.layer = r.layer;
        if (r.part === "wheel") { page().wheel = null; changed(); }
        else if (r.part === "hold") setHold(r.id, null);
        else setPress(r.id, null);
      };
      row.append(del);
      row.onclick = () => { S.page = i; S.layer = r.layer; S.selected = r.id; render(); };
      section.append(row);
    }
    nodes.push(section);
  });
  host.replaceChildren(...nodes);
}

// "Add" row of one page: pick Fn or not and the key; the editor on the right
// opens for it.
function addMappingForm(pageIndex) {
  const form = el("div", "list-add");
  form.append(el("span", "field-label", "Add or change"));
  const layerSel = el("select");
  const layers = [["base", "Key"]];
  layers.push(...modifiers().map(m => [m, `${keyLabel(m)} + key`]));
  for (const [v, t] of layers) {
    const o = el("option", null, t);
    o.value = v;
    layerSel.append(o);
  }
  const keySel = el("select");
  const placeholder = el("option", null, "Choose a key…");
  placeholder.value = "";
  keySel.append(placeholder);
  for (const id of KEY_ORDER.filter(k => !isPageKey(k))) {
    const o = el("option", null, KEY_LABELS[id] + (KEY_ALT[id] ? ` (${KEY_ALT[id]})` : ""));
    o.value = id;
    keySel.append(o);
  }
  const go = el("button", "primary small", "Edit");
  go.disabled = true;
  keySel.onchange = () => { go.disabled = !keySel.value; };
  go.onclick = () => {
    S.page = pageIndex;
    S.layer = layerSel.value;
    S.selected = keySel.value;
    log(`Editing ${layerPrefix(layerSel.value)}${KEY_LABELS[keySel.value]} on page ${pageIndex + 1}: choose an action on the right.`);
    render();
  };
  form.append(layerSel, keySel, go);
  return form;
}

function select(id) {
  S.selected = S.selected === id ? null : id;
  render();
}

// ---------------------------------------------------------------- inspector

function renderInspector() {
  const box = $("inspector");
  if (!S.selected) box.replaceChildren(...pageSettings());
  else if (S.selected === "wheel") box.replaceChildren(...wheelEditor());
  else if (isPageKey(S.selected)) box.replaceChildren(...pageKeyInfo(S.selected));
  else box.replaceChildren(...keyEditor(S.selected));
  // Explanations stay hidden until "?" is on, so the panel stays calm.
  box.classList.toggle("help", !!S.help);
  const help = el("button", "ghost help-toggle", "?");
  help.title = S.help ? "Hide explanations" : "Show explanations";
  help.onclick = () => { S.help = !S.help; savePrefs(); renderInspector(); };
  box.prepend(help);
}

function heading(title, sub) {
  const h = el("div", "insp-head");
  h.append(el("h2", null, title));
  if (sub) h.append(el("div", "sub", sub));
  return h;
}

function field(label, control, note) {
  const f = el("label", "field");
  f.append(el("span", "field-label", label), control);
  if (note) f.append(el("span", "note", note));
  return f;
}

function pageSettings() {
  const name = el("input");
  name.value = page().name;
  name.onchange = () => { page().name = name.value; changed(); };
  const pageKey = KEY_LABELS[S.doc.page_keys[S.page]];
  const count = Object.keys(page().keys).length;
  const pill = (n, text) => {
    const p = el("span", "pill");
    p.append(el("b", null, String(n)), document.createTextNode(" " + text));
    return p;
  };
  const counts = el("div", "pills");
  counts.append(pill(count, "keys"), ...modifiers().map(m => pill(Object.keys(layerKeys(page(), m)).length, `${keyLabel(m)} + key`)));
  const mappings = el("div", "section");
  mappings.append(el("div", "field-label", "Mappings on this page"), counts,
    el("p", "info", `Click a key on the picture, or press it on the device while this window is in front. Hold ${modifiers().map(keyLabel).join(", ") || "a modifier"} and press another key for the combinations; pick the layer above the picture to edit them. A quick tap still opens their page.`));
  const foot = el("div", "panel-foot");
  foot.append(resetButton());
  return [
    heading(`Page ${S.page + 1}`, `${pageKey} key`),
    field("Name", name),
    mappings,
    davinciDefaultsSwitch(),
    el("p", "info", `The active page's key blinks. ${chordText()} together switches to normal mode: every key and the wheel do their native DaVinci job (the three lights flash, then work as transition keys). Press them again to come back.`),
    resolveSetup(),
    foot,
  ];
}

function davinciDefaultsSwitch() {
  const box = el("label", "switch");
  const input = el("input");
  input.type = "checkbox";
  input.checked = !!page().davinci_defaults;
  input.onchange = () => {
    if (input.checked) page().davinci_defaults = true;
    else delete page().davinci_defaults;
    changed();
  };
  box.append(input, el("span", "track"), el("span", null, "Keep DaVinci functions on unmapped keys"));
  const wrap = el("div", "section");
  wrap.append(box, el("p", "info",
    "Marked “DaVinci” on the picture. Keys marked “—” have no Resolve keyboard command, so they do nothing until you map them. "
    + "The app types each function with the shortcut your own Resolve keyboard layout uses."));
  return wrap;
}

function resolveSetup() {
  const box = el("div", "section");
  box.append(el("div", "field-label", "DaVinci Resolve"));
  box.append(el("p", "info", "Resolve actions run Resolve's own functions directly, so they work whatever your Resolve keyboard shortcuts are. Resolve must be open, with Preferences › System › General › “External scripting using” set to Local."));
  const b = el("button", "ghost small", "Test connection to Resolve");
  b.onclick = async () => {
    b.disabled = true;
    b.textContent = "Testing…";
    try {
      log(await api.invoke("resolve_test"));
    } catch (e) {
      log("Resolve error: " + e);
    }
    b.disabled = false;
    b.textContent = "Test connection to Resolve";
  };
  box.append(b);
  return box;
}

function resetButton() {
  const b = el("button", "ghost small", "Load example profile");
  b.onclick = async () => {
    if (!confirm("Replace all pages with the example profile? You can still Revert before saving.")) return;
    S.doc = await api.invoke("example_profile");
    S.page = 0;
    S.selected = null;
    changed();
  };
  return b;
}

function pageKeyInfo(id) {
  const index = S.doc.page_keys.indexOf(id);
  const inChord = S.doc.toggle_chord.includes(id);
  return [
    heading(KEY_LABELS[id], `Page key ${index + 1}`),
    el("p", "info", `Releasing ${KEY_LABELS[id]} opens page ${index + 1} “${S.doc.pages[index].name}”. Page keys can't have their own action.`),
    id === S.doc.fn_key
      ? el("p", "info", `Held, ${KEY_LABELS[id]} is the Fn key on every page: other keys then do their action in the “${KEY_LABELS[id]} + key” layer. On a DaVinci page, ${KEY_LABELS[id]} + wheel zooms the timeline, and ${KEY_LABELS[id]} + CLOSE UP switches Resolve between the Trim tool (TRIM IN/OUT ripple; CLOSE UP lights) and the Selection tool (they leave a gap). VIDEO ONLY or AUDIO ONLY pressed on its own (it lights up; press again to switch off) makes track choices such as CUT + CAM and DIS + CAM take only the video or only the audio. With mappings off it is the normal transition.`)
      : modifierSwitch(id),
    inChord ? el("p", "info", `It's part of the on/off combination: ${chordText()}.`) : el("span"),
  ];
}

// Makes a page key a modifier too: held with another key, that key uses the
// page key's own layer ("DIS + key").
function modifierSwitch(id) {
  const box = el("label", "switch");
  const input = el("input");
  input.type = "checkbox";
  input.checked = (S.doc.modifier_keys || []).includes(id);
  input.onchange = () => {
    const list = (S.doc.modifier_keys || []).filter(k => k !== id);
    if (input.checked) list.push(id);
    S.doc.modifier_keys = S.doc.page_keys.filter(k => list.includes(k));
    if (!S.doc.modifier_keys.length) delete S.doc.modifier_keys;
    changed();
  };
  box.append(input, el("span", "track"), el("span", null, `Use ${KEY_LABELS[id]} as a modifier key`));
  const wrap = el("div", "section");
  wrap.append(box, el("p", "info",
    `Held together with another key, ${KEY_LABELS[id]} picks its own layer (“${KEY_LABELS[id]} + key”) on every page. `
    + `A quick tap on its own still opens page ${S.doc.page_keys.indexOf(id) + 1}.`
    + (id === "smooth_cut" ? " On a DaVinci page, SMTH CUT + CAM 1–7 opens Resolve's pages: Media, Cut, Edit, Fusion, Color, Fairlight, Deliver." : "")));
  return wrap;
}

function keyEditor(id) {
  const m = layer()[id] || {};
  const alt = KEY_ALT[id] ? ` (${KEY_ALT[id]})` : "";
  const title = fnLayer() ? `${layerPrefix(activeLayer())}${KEY_LABELS[id]}` : KEY_LABELS[id] + alt;
  const where = (fnLayer() ? `${keyLabel(activeLayer())} layer ` : "") + `on page ${S.page + 1} “${page().name}”`;
  const nodes = [heading(title, where)];

  const label = el("input");
  label.value = m.label || "";
  label.placeholder = describeAction(pressAction(m)) || "Name shown on the key";
  label.onchange = () => updateMapping(id, mm => { mm.label = label.value; });
  nodes.push(field("Label", label));

  nodes.push(actionSection("Press", pressAction(m), a => setPress(id, a),
    kind => m.hold ? "Fires when you let go quickly."
      : kind === "keys" ? "Held for as long as you hold the key." : ""));
  nodes.push(actionSection("Long press (hold ½ s)", m.hold, a => setHold(id, a),
    "Holding the key does this instead of the press action."));

  if (defaultName() === "DaVinci") {
    nodes.push(el("p", "info", "“DaVinci” leaves the key's normal Resolve function in place."));
  }
  return nodes;
}

// Editor for one action: a choice of kind plus the matching input.
function actionSection(title, current, onSet, note) {
  const actual = actionKind(current);
  // Resolve functions (your shortcut) and Resolve scripts share the "Resolve" choice.
  const kind = ["resolve_key", "solo_track", "select_track", "resolve_menu"].includes(actual) ? "resolve" : actual || "none";
  const box = el("div", "section");
  const kinds = [["none", defaultName()], ["keys", "Shortcut"], ["resolve", "Resolve"], ["text", "Type text"], ["run", "Open a program, file or URL"]];
  const chosen = (kinds.find(([k]) => k === kind) || kinds[0])[1];
  box.append(el("div", "field-label", `${title} · ${chosen}`));

  const seg = el("div", "segmented");
  for (const [k, text] of kinds) {
    const b = el("button", "kind-btn" + (k === kind ? " on" : ""));
    const iconName = k === "none" ? (defaultName() === "DaVinci" ? "davinci" : "none") : k;
    b.append(icon(iconName));
    b.title = text;
    b.setAttribute("aria-label", text);
    b.onclick = () => {
      if (k === kind) return;
      if (k === "none") return onSet(null);
      if (k === "resolve") return onSet({ resolve_key: RESOLVE_KEY_COMMANDS[0][0] });
      onSet({ [k]: "" });
      if (k === "keys") startRecording(v => onSet({ keys: v }));
    };
    seg.append(b);
  }
  box.append(seg);

  if (kind === "keys") {
    box.append(shortcutField("Shortcut", current.keys, v => onSet({ keys: v })));
  } else if (kind === "text") {
    const input = el("input");
    input.value = current.text;
    input.placeholder = "Text to type";
    input.onchange = () => onSet({ text: input.value });
    box.append(input);
  } else if (kind === "run") {
    const input = el("input");
    input.value = current.run;
    input.placeholder = "notepad.exe, C:\\path\\app.exe or https://…";
    input.onchange = () => onSet({ run: input.value.trim() });
    box.append(input);
  } else if (kind === "resolve") {
    box.append(...resolveEditor(current, onSet));
  }
  const noteText = typeof note === "function" ? note(kind) : note;
  if (kind !== "none" && noteText) box.append(el("span", "note", noteText));
  return box;
}

// Picker for a DaVinci Resolve command plus its settings.
function resolveEditor(current, onSet) {
  const command = RESOLVE_COMMANDS.find(c => c.id === current.resolve);
  const params = current.params || {};
  const select = el("select");
  const addGroup = (label, items) => {
    const og = document.createElement("optgroup");
    og.label = label;
    for (const [value, name] of items) {
      const o = el("option", null, name);
      o.value = value;
      og.append(o);
    }
    select.append(og);
  };
  // Functions are pressed with your own Resolve shortcut; scripts use Resolve's scripting.
  for (const group of [...new Set(RESOLVE_KEY_COMMANDS.map(c => c[2]))]) {
    addGroup(`${group} · your Resolve shortcut`,
      RESOLVE_KEY_COMMANDS.filter(c => c[2] === group).map(([id, name]) => ["key:" + id, name]));
  }
  for (const group of [...new Set(RESOLVE_MENU_ITEMS.map(m => m[2]))]) {
    addGroup(`${group} · Resolve menu (no shortcut needed)`,
      RESOLVE_MENU_ITEMS.filter(m => m[2] === group).map(([path, name]) => ["menu:" + path, name]));
  }
  addGroup("Tracks · next edits only on", [...Array.from({ length: 8 }, (_, i) => [`solo:${i + 1}`, `Only track ${i + 1} (video + audio)`]), ["solo:0", "All tracks"]]);
  addGroup("Tracks · select the clip under the playhead", Array.from({ length: 8 }, (_, i) => [`select:${i + 1}`, `Select clip on track ${i + 1} (with its sound)`]));
  for (const group of [...new Set(RESOLVE_COMMANDS.map(c => c.group))]) {
    addGroup(`${group} · script`, RESOLVE_COMMANDS.filter(c => c.group === group).map(c => ["script:" + c.id, c.name]));
  }
  select.value = current.resolve_menu !== undefined ? "menu:" + current.resolve_menu
    : current.resolve_key !== undefined ? "key:" + current.resolve_key
    : current.solo_track !== undefined ? "solo:" + current.solo_track
    : current.select_track !== undefined ? "select:" + current.select_track
    : "script:" + current.resolve;
  select.onchange = () => {
    const [type, id] = select.value.split(/:(.*)/s);
    onSet(type === "key" ? { resolve_key: id } : type === "solo" ? { solo_track: Number(id) }
      : type === "select" ? { select_track: Number(id) }
      : type === "menu" ? { resolve_menu: id } : { resolve: id });
  };
  const nodes = [select];
  if (current.resolve_menu !== undefined) {
    nodes.push(el("span", "note", `Clicks Resolve's menu item “${current.resolve_menu.replaceAll(">", "›")}” directly, so it needs no shortcut and works with any keyboard layout.`));
    return nodes;
  }
  if (current.select_track !== undefined) {
    nodes.push(el("span", "note", "Selects the clip under the playhead on this track, with its linked sound. With VIDEO ONLY or AUDIO ONLY lit, only that part. Uses only Resolve's own selection keys (select at playhead, then one track up or down), so no menu opens."));
    return nodes;
  }
  if (current.solo_track !== undefined) {
    nodes.push(el("span", "note", "Switches Resolve's Auto Track Selectors so only this video + audio track pair takes part in the next edits (ripple, trim). Start with all selectors on; the CAM light shows the chosen track."));
    return nodes;
  }
  if (current.resolve_key !== undefined) {
    nodes.push(el("span", "note", "Pressed with the shortcut your Resolve keyboard layout has for this function, so it follows your Resolve settings."));
    return nodes;
  }
  if (!command) return nodes;

  nodes.push(el("span", "note", command.help));
  const set = (key, value) => {
    const next = { ...params, [key]: value };
    for (const p of command.params) if (next[p.key] === p.default) delete next[p.key];
    onSet(Object.keys(next).length ? { resolve: command.id, params: next } : { resolve: command.id });
  };
  for (const p of command.params) {
    const value = params[p.key] !== undefined ? params[p.key] : p.default;
    let control;
    if (p.options.length) {
      control = el("select");
      for (const opt of p.options) {
        const o = el("option", null, opt);
        o.value = opt;
        control.append(o);
      }
    } else {
      control = el("input");
    }
    control.value = value;
    control.onchange = () => set(p.key, control.value);
    nodes.push(field(p.label, control));
  }

  const tryBtn = el("button", "ghost small", "Try in Resolve now");
  tryBtn.onclick = async () => {
    tryBtn.disabled = true;
    try {
      log("Resolve: " + await api.invoke("resolve_run", { command: command.id, params }));
    } catch (e) {
      log("Resolve error: " + e);
    }
    tryBtn.disabled = false;
  };
  nodes.push(tryBtn);
  return nodes;
}

function wheelEditor() {
  const w = page().wheel;
  const kind = !w ? "none" : w.davinci ? "davinci" : w.scroll ? (w.scroll.horizontal ? "hscroll" : "scroll") : "keys";
  const nodes = [heading("Jog wheel", `on page ${S.page + 1} “${page().name}”`)];
  const seg = el("div", "segmented");
  const options = [["davinci", "DaVinci"], ["none", "Nothing"], ["scroll", "Scroll"], ["hscroll", "Sideways"], ["keys", "Keys"]];
  for (const [k, label] of options) {
    const b = el("button", k === kind ? "on" : "", label);
    b.onclick = () => {
      page().wheel = k === "none" ? null
        : k === "davinci" ? { davinci: {} }
        : k === "scroll" ? { scroll: { horizontal: false } }
        : k === "hscroll" ? { scroll: { horizontal: true } }
        : { keys: { left: "left", right: "right" } };
      changed();
    };
    seg.append(b);
  }
  nodes.push(seg);
  if (kind === "keys") {
    nodes.push(shortcutField("Turn left", w.keys.left, v => { w.keys.left = v; changed(); }));
    nodes.push(shortcutField("Turn right", w.keys.right, v => { w.keys.right = v; changed(); }));
    nodes.push(el("p", "info", "One key press per wheel step. Try Vol − / Vol + or ← / →."));
  } else if (kind === "davinci") {
    nodes.push(el("p", "info", "Works like the Speed Editor's own wheel in Resolve: JOG, SHTL and SCRL switch the mode. "
      + "JOG and SCRL use values measured from the real device in Resolve; SHTL plays at the nearest of Resolve's speeds."));
  } else if (kind !== "none") {
    nodes.push(el("p", "info", "Turns like a mouse wheel in the window under the mouse pointer."));
  }
  return nodes;
}

// A text box plus a Record button that captures the next key combination.
function shortcutField(label, value, onSet, note) {
  const wrap = el("div", "shortcut");
  const input = el("input");
  input.value = value;
  input.placeholder = "e.g. ctrl+shift+m";
  input.spellcheck = false;
  const pretty = el("span", "pretty", value ? prettyShortcut(value) : "");
  const err = el("span", "note error");
  input.onchange = async () => {
    const v = input.value.trim().toLowerCase();
    try {
      await api.invoke("validate_shortcut", { text: v });
      onSet(v);
    } catch (e) {
      err.textContent = String(e);
    }
  };
  const rec = el("button", "ghost small", "Record");
  rec.onclick = () => startRecording(onSet, rec);
  wrap.append(input, rec);
  const f = field(label, wrap, note);
  f.append(pretty, err);
  return f;
}

let recorder = null;

function startRecording(onSet, button) {
  stopRecording();
  const mods = new Set();
  let main = null;
  const order = ["ctrl", "shift", "alt", "win"];
  const finish = () => {
    const keys = order.filter(m => mods.has(m));
    if (main) keys.push(main);
    stopRecording();
    if (keys.length) onSet(keys.join("+"));
  };
  const down = e => {
    e.preventDefault();
    e.stopPropagation();
    if (MODIFIER_CODES[e.code]) { mods.add(MODIFIER_CODES[e.code]); return; }
    if (e.ctrlKey) mods.add("ctrl");
    if (e.shiftKey) mods.add("shift");
    if (e.altKey) mods.add("alt");
    if (e.metaKey) mods.add("win");
    main = codeToKey(e.code);
    if (main) finish();
  };
  const up = e => {
    e.preventDefault();
    if (MODIFIER_CODES[e.code] && !main && mods.size) finish();
  };
  window.addEventListener("keydown", down, true);
  window.addEventListener("keyup", up, true);
  recorder = { down, up, button };
  if (button) { button.textContent = "Press keys…"; button.classList.add("recording"); }
  document.body.classList.add("recording");
}

function stopRecording() {
  if (!recorder) return;
  window.removeEventListener("keydown", recorder.down, true);
  window.removeEventListener("keyup", recorder.up, true);
  if (recorder.button) { recorder.button.textContent = "Record"; recorder.button.classList.remove("recording"); }
  document.body.classList.remove("recording");
  recorder = null;
}

// ---------------------------------------------------------------- palette

// The action list is folded away until you open it; it remembers the choice.
function showActions() {
  $("palette").classList.toggle("open", !!S.actionsOpen);
  $("paletteToggle").setAttribute("aria-expanded", String(!!S.actionsOpen));
  $("paletteToggle").title = S.actionsOpen ? "Hide the actions" : "Show the actions you can drag onto a key";
}

function renderPalette() {
  const groups = ["All", ...paletteGroups().map(g => g.group)];
  $("paletteGroups").replaceChildren(...groups.map(g => {
    const b = el("button", g === S.paletteGroup ? "on" : "", g);
    b.onclick = () => { S.paletteGroup = g; renderPalette(); };
    return b;
  }));
  const seg = el("div", "segmented compact");
  for (const [k, text] of [["press", "Press"], ["hold", "Long press"]]) {
    const b = el("button", S.assignTo === k ? "on" : "", text);
    b.onclick = () => { S.assignTo = k; renderPalette(); };
    seg.append(b);
  }
  seg.title = "Assign actions to the press or the long press of the key";
  $("assignTo").replaceChildren(seg);
  const q = S.search.trim().toLowerCase();
  const target = S.selected && S.selected !== "wheel" && !isPageKey(S.selected) ? S.selected : null;
  const items = paletteGroups()
    .filter(g => S.paletteGroup === "All" || g.group === S.paletteGroup)
    .flatMap(g => g.items.map(([name, action]) => ({ name, action, group: g.group })))
    .filter(i => !q || i.name.toLowerCase().includes(q) || String(i.action[actionKind(i.action)]).includes(q));
  $("paletteItems").replaceChildren(...items.map(i => {
    const chip = el("button", "chip");
    chip.draggable = true;
    const kind = actionKind(i.action);
    const head = el("span", "chip-head");
    const iconName = kind === "resolve" ? "script" : kind === "resolve_menu" ? "menu"
      : kind === "solo_track" || kind === "select_track" ? "tracks"
      : kind === "resolve_key" ? "resolve" : kind;
    head.append(icon(iconName, 14), el("span", "chip-name", i.name));
    chip.append(head);
    const detail = kind === "keys" ? prettyShortcut(i.action.keys)
      : kind === "text" ? "types " + i.action.text
      : kind === "resolve" ? "Resolve script · " + (RESOLVE_COMMANDS.find(c => c.id === i.action.resolve) || {}).group
      : kind === "resolve_key" ? "Resolve · " + (RESOLVE_KEY_COMMANDS.find(c => c[0] === i.action.resolve_key) || [])[2]
      : kind === "solo_track" || kind === "select_track" ? "Resolve · Tracks"
      : kind === "resolve_menu" ? "Resolve menu · " + i.action.resolve_menu.split(">")[0].trim()
      : i.action.run;
    if (kind === "keys") chip.append(el("span", "chip-detail", detail));
    if (["resolve", "resolve_key", "solo_track", "select_track", "resolve_menu"].includes(kind)) chip.classList.add("resolve");
    chip.title = `${i.name} · ${detail}\n` + (target ? `Click to assign to ${KEY_LABELS[target]}` : "Drag onto a key, or select a key first");
    chip.ondragstart = e => e.dataTransfer.setData("application/json", JSON.stringify(i.action));
    chip.onclick = () => { if (target) assign(target, structuredClone(i.action)); };
    return chip;
  }));
}

// ---------------------------------------------------------------- device events

function onRuntime(e) {
  switch (e.kind) {
    case "waiting": setStatus("waiting", "Waiting for Speed Editor"); log(e.text); break;
    case "yielded_to_resolve":
      setStatus("resolve", "DaVinci Resolve has the Speed Editor");
      log("DaVinci Resolve is running: it has the Speed Editor until it quits");
      S.leds = 0;
      S.deviceState = null;
      S.held.clear();
      render();
      break;
    case "connected":
      S.bus = e.bus;
      setStatus("ok", connectedText());
      log(`Speed Editor connected over ${e.bus}`);
      break;
    case "battery": {
      const box = $("battery");
      box.hidden = false;
      box.title = `Speed Editor battery: ${e.level}%${e.charging ? ", charging" : ""}`;
      box.className = "battery " + (e.charging ? "charging" : e.level <= 15 ? "low" : e.level <= 40 ? "mid" : "ok");
      const fill = el("span", "battery-fill");
      fill.style.width = Math.max(4, Math.min(100, e.level)) + "%";
      const icon = el("span", "battery-icon");
      icon.append(fill);
      box.replaceChildren(icon, el("span", null, `${e.level}%${e.charging ? " ⚡" : ""}`));
      if (e.level <= 15 && !e.charging) log(`Speed Editor battery low (${e.level}%): plug in the USB cable`);
      break;
    }
    case "disconnected": setStatus("waiting", "Reconnecting…"); log("Disconnected: " + e.text); break;
    case "stopped":
      setStatus("off", "Not connected");
      $("battery").hidden = true;
      S.leds = 0;
      S.deviceState = null;
      S.held.clear();
      render();
      break;
    case "status": log(e.text); break;
    case "state":
      if (S.deviceState && S.deviceState.custom !== e.custom) {
        log(e.custom ? `Custom mode: page ${e.page + 1}` : "Normal mode: mappings off");
      }
      S.deviceState = { custom: e.custom, page: e.page };
      if (e.custom) S.page = e.page;
      setStatus("ok", connectedText());
      render();
      break;
    case "keys": {
      const pressed = e.keys.filter(k => !S.held.has(k));
      S.held = new Set(e.keys);
      // While you edit here (this window in front), a key pressed on the
      // device selects it; while you work in Resolve, nothing changes.
      if (pressed.length && document.hasFocus()) {
        S.selected = pressed[pressed.length - 1];
      }
      render();
      for (const id of pressed) flash(document.querySelector(`.key[data-key="${id}"]`));
      break;
    }
    case "jog": turnWheel(e.mode, e.value); break;
    case "leds": S.leds = e.leds; updateLeds(); break;
  }
}

// The picture's LEDs mirror exactly what the app last sent to the device.
const ledOn = id => LED_BITS[id] !== undefined && (S.leds & (1 << LED_BITS[id])) !== 0;

// Updates only the LED dots, so blinking never disturbs editing.
function updateLeds() {
  for (const dot of document.querySelectorAll(".key .led")) {
    dot.classList.toggle("on", ledOn(dot.parentElement.dataset.key));
  }
}

// Restarts a short highlight animation on an element.
function flash(node) {
  if (!node) return;
  node.classList.remove("flash");
  void node.offsetWidth;
  node.classList.add("flash");
  // Drop the class afterwards: a node moved by a re-render would otherwise
  // replay the animation.
  node.addEventListener("animationend", () => node.classList.remove("flash"), { once: true });
}

// Wheel units per half turn, measured in absolute (shuttle) mode.
const WHEEL_HALF_TURN = 4096;
let wheelAngle = 0;

function turnWheel(mode, value) {
  // Mode switches report a zero position; that is not a turn.
  if (value === 0 && mode !== 1 && mode !== 3) return;
  const degrees = value * 180 / WHEEL_HALF_TURN;
  // Shuttle reports an absolute position; the other modes report movement.
  wheelAngle = mode === 1 || mode === 3 ? degrees : (wheelAngle + degrees) % 360;
  $("wheelSpin").style.transform = `rotate(${wheelAngle}deg)`;
  flash($("wheel"));
}

function connectedText() {
  const d = S.deviceState;
  const mode = !d ? "" : d.custom ? ` · Custom · page ${d.page + 1}` : " · Normal mode";
  return `Connected (${S.bus})${mode}`;
}

function setStatus(kind, text) {
  $("status").className = "status " + kind;
  $("status").title = text;
  const short = { ok: S.bus || "On", waiting: "…", resolve: "Resolve", off: "Off" };
  $("statusText").textContent = short[kind] || text;
}

// ---------------------------------------------------------------- wiring

async function save() {
  if (!dirty() || S.error) return;
  try {
    await api.invoke("save_profile", { doc: S.doc });
    S.savedJson = JSON.stringify(S.doc);
    log(S.connected ? "Saved and sent to the Speed Editor" : "Saved");
  } catch (e) {
    S.error = String(e);
  }
  render();
}

async function setConnected(on) {
  if (on) {
    try {
      await api.invoke("connect", { doc: S.doc, active: S.active, yieldToResolve: S.yieldToResolve });
      S.connected = true;
      setStatus("waiting", "Connecting…");
    } catch (err) {
      log(String(err));
      S.connected = false;
    }
  } else {
    await api.invoke("disconnect");
    S.connected = false;
    setStatus("off", "Not connected");
  }
  render();
}

// Per-computer convenience settings; the profile itself lives in its file.
function loadPrefs() {
  try {
    return JSON.parse(localStorage.getItem("studio-prefs") || "{}");
  } catch {
    return {};
  }
}

function savePrefs() {
  try {
    localStorage.setItem("studio-prefs", JSON.stringify({
      connected: S.connected, active: S.active, yieldToResolve: S.yieldToResolve, help: !!S.help,
      actions: !!S.actionsOpen,
    }));
  } catch {
    // Storage unavailable: the app still works, it just won't remember.
  }
}

async function init() {
  RESOLVE_COMMANDS = (await api.invoke("resolve_catalog")) || [];
  const loaded = await api.invoke("load_profile");
  S.doc = loaded.doc;
  S.path = loaded.path;
  S.savedJson = JSON.stringify(S.doc);
  await api.listen("runtime", ev => onRuntime(ev.payload));

  applyIcons();
  document.addEventListener("click", e => {
    const menu = $("settingsMenu");
    if (menu.open && !menu.contains(e.target)) menu.open = false;
  });
  $("paletteToggle").onclick = () => { S.actionsOpen = !S.actionsOpen; savePrefs(); showActions(); };
  $("saveBtn").onclick = save;
  $("revertBtn").onclick = () => {
    S.doc = JSON.parse(S.savedJson);
    S.page = Math.min(S.page, S.doc.pages.length - 1);
    changed();
  };
  $("exportBtn").onclick = async () => {
    try {
      const path = await api.invoke("export_profile", { doc: S.doc });
      if (path) log("Exported to " + path);
    } catch (e) {
      log("Export failed: " + e);
    }
  };
  $("importBtn").onclick = async () => {
    if (dirty() && !confirm("Replace your unsaved changes with the imported profile?")) return;
    try {
      const doc = await api.invoke("import_profile");
      if (!doc) return;
      S.doc = doc;
      S.page = 0;
      S.selected = null;
      log("Imported. Press Save to keep it on this computer.");
      changed();
    } catch (e) {
      log("Import failed: " + e);
    }
  };
  $("connectToggle").onchange = async e => {
    await setConnected(e.target.checked);
    savePrefs();
  };
  $("activeToggle").onchange = async e => {
    S.active = e.target.checked;
    await api.invoke("set_active", { active: S.active });
    log(S.active ? "Mappings active: keys now trigger their actions" : "Mappings paused: keys only light up here");
    savePrefs();
    render();
  };
  $("yieldToggle").onchange = async e => {
    S.yieldToResolve = e.target.checked;
    await api.invoke("set_yield_to_resolve", { yieldToResolve: S.yieldToResolve });
    log(S.yieldToResolve
      ? "While DaVinci Resolve runs, it gets the Speed Editor (native controls, your bindings pause)"
      : "The app keeps the Speed Editor, so your bindings work in Resolve. Start this app before Resolve.");
    savePrefs();
  };

  $("autostartToggle").onchange = async e => {
    try {
      await api.invoke("set_autostart", { enabled: e.target.checked });
      log(e.target.checked
        ? "Starts when you sign in, hidden in the tray, so it has the Speed Editor before Resolve starts"
        : "No longer starts when you sign in");
    } catch (err) {
      e.target.checked = !e.target.checked;
      log("Start with Windows: " + err);
    }
  };
  if (window.__TAURI__) {
    $("autostartToggle").checked = await api.invoke("autostart_enabled").catch(() => false);
  }
  if (/Mac/.test(navigator.platform || navigator.userAgent)) {
    // macOS names it differently, and has no pass-through (see README).
    $("autostartToggle").parentElement.querySelector("span:last-child").textContent = "Start at login";
  }

  // Restore the last connection settings, so the app picks up where it was.
  const prefs = loadPrefs();
  S.active = !!prefs.active;
  S.yieldToResolve = !!prefs.yieldToResolve;
  S.help = !!prefs.help;
  S.actionsOpen = !!prefs.actions;
  showActions();
  $("yieldToggle").checked = S.yieldToResolve;
  if (prefs.connected && window.__TAURI__) await setConnected(true);
  $("paletteSearch").oninput = e => { S.search = e.target.value; renderPalette(); };
  window.addEventListener("keydown", e => {
    if (recorder) return;
    if (e.ctrlKey && e.code === "KeyS") { e.preventDefault(); save(); }
    if (e.code === "Escape" && document.activeElement.tagName !== "INPUT") { S.selected = null; render(); }
  });
  window.addEventListener("beforeunload", e => { if (dirty()) e.preventDefault(); });
  new ResizeObserver(fitDevice).observe(document.querySelector(".device-area"));

  log(window.__TAURI__ ? "Profile loaded" : "Browser preview: device features are disabled");
  changed();
}

init();
