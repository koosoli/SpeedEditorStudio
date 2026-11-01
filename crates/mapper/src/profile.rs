//! Profiles: the user's pages and mappings, stored as TOML.
//!
//! The editable document (`ProfileDoc`, what the file and the GUI hold) is
//! kept separate from the validated form (`Profile`, what the engine runs),
//! so every mistake is reported when a profile is loaded or saved, never in
//! the middle of a key press.

use std::collections::{BTreeMap, HashMap};
use std::fmt::{self, Write as _};
use std::path::Path;

use serde::{Deserialize, Serialize};
use speed_editor::Key;

use crate::davinci_wheel::DaVinciWheelConfig;
use crate::keyboard::Shortcut;

pub const FORMAT_VERSION: u32 = 1;

/// Wheel units per step at slow speed, measured on a real device.
const DEFAULT_WHEEL_STEP: i32 = 360;

/// A profile as written in the file: names and shortcut strings, unvalidated.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileDoc {
    pub version: u32,
    pub page_keys: Vec<String>,
    pub toggle_chord: Vec<String>,
    /// Page key that, when held, switches every page to its Fn layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fn_key: Option<String>,
    /// More page keys that work like `fn_key` when held with another key,
    /// each with its own layer (`[pages.layers.<key>]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub modifier_keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wheel_step: Option<i32>,
    pub pages: Vec<PageDoc>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PageDoc {
    pub name: String,
    /// Keys without a mapping keep their native DaVinci Resolve function.
    #[serde(default, skip_serializing_if = "is_false")]
    pub davinci_defaults: bool,
    #[serde(default)]
    pub wheel: Option<WheelDoc>,
    #[serde(default)]
    pub keys: BTreeMap<String, KeyDoc>,
    /// Fn layer: used on this page while the profile's `fn_key` is held.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fn_keys: BTreeMap<String, KeyDoc>,
    /// Layers of the `modifier_keys`, by modifier: used while it is held.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<String, BTreeMap<String, KeyDoc>>,
}

/// One action. Exactly one field is set.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionDoc {
    /// Shortcut such as "ctrl+shift+m".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    /// Program, file or URL to open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// Text to type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// DaVinci Resolve command id (see `resolve::COMMANDS`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve: Option<String>,
    /// Resolve keyboard command (e.g. "markIn"): pressed with whatever
    /// shortcut the user's Resolve layout has for it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve_key: Option<String>,
    /// Limit Resolve's next edits to video track n + audio track n (via the
    /// Auto Track Selector); 0 switches all tracks back on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solo_track: Option<u8>,
    /// Selects the clip under the playhead on video track n, with its linked
    /// audio (keyboard only, no menu).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_track: Option<u8>,
    /// Resolve menu item, e.g. "Trim > Ripple > Start to Playhead": clicked
    /// directly, no shortcut needed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve_menu: Option<String>,
    /// Settings of the Resolve command.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, String>>,
}

/// A key's mapping: what a press does, what a long press does, and the
/// label shown on the key in the editor. The press fields are inline so
/// simple mappings stay one short line: `cam1 = { keys = "f13" }`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyDoc {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keys: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solo_track: Option<u8>,
    /// Selects the clip under the playhead on video track n, with its linked
    /// audio (keyboard only, no menu).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_track: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolve_menu: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<ActionDoc>,
}

impl KeyDoc {
    pub fn press(&self) -> ActionDoc {
        ActionDoc {
            keys: self.keys.clone(),
            run: self.run.clone(),
            text: self.text.clone(),
            resolve: self.resolve.clone(),
            resolve_key: self.resolve_key.clone(),
            solo_track: self.solo_track,
            select_track: self.select_track,
            resolve_menu: self.resolve_menu.clone(),
            params: self.params.clone(),
        }
    }
}

impl ActionDoc {
    fn toml_fields(&self) -> Vec<String> {
        let s = |v: &str| toml::Value::String(v.to_string()).to_string();
        let mut fields: Vec<String> =
            [
                ("keys", &self.keys),
                ("run", &self.run),
                ("text", &self.text),
                ("resolve", &self.resolve),
                ("resolve_key", &self.resolve_key),
                ("resolve_menu", &self.resolve_menu),
            ]
                .into_iter()
                .filter_map(|(name, value)| value.as_ref().map(|v| format!("{name} = {}", s(v))))
                .collect();
        if let Some(n) = self.select_track {
            fields.push(format!("select_track = {n}"));
        }
        if let Some(n) = self.solo_track {
            fields.push(format!("solo_track = {n}"));
        }
        if let Some(params) = self.params.as_ref().filter(|p| !p.is_empty()) {
            let inner: Vec<String> = params.iter().map(|(k, v)| format!("{k} = {}", s(v))).collect();
            fields.push(format!("params = {{ {} }}", inner.join(", ")));
        }
        fields
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum WheelDoc {
    Keys { left: String, right: String },
    Scroll {
        #[serde(default)]
        horizontal: bool,
    },
    /// Resolve's own JOG/SHTL/SCRL behaviour (see `davinci_wheel`). The
    /// numbers are optional fine-tuning; the defaults are measured values.
    Davinci {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        jog_units: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        scroll_units: Option<i32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fps: Option<f64>,
    },
}

impl ProfileDoc {
    pub fn from_toml(text: &str) -> Result<ProfileDoc, ProfileError> {
        toml::from_str(text).map_err(|e| ProfileError(e.to_string()))
    }

    /// Writes the profile in the same layout as the hand-written example,
    /// one line per key.
    pub fn to_toml(&self) -> String {
        let s = |v: &str| toml::Value::String(v.to_string()).to_string();
        let list = |items: &[String]| items.iter().map(|i| s(i)).collect::<Vec<_>>().join(", ");
        let mut out = String::new();
        out.push_str(PROFILE_HEADER);
        let _ = writeln!(out, "version = {}", self.version);
        let _ = writeln!(out, "page_keys = [{}]", list(&self.page_keys));
        let _ = writeln!(out, "toggle_chord = [{}]", list(&self.toggle_chord));
        if let Some(fn_key) = &self.fn_key {
            let _ = writeln!(out, "fn_key = {}", s(fn_key));
        }
        if !self.modifier_keys.is_empty() {
            let _ = writeln!(out, "modifier_keys = [{}]", list(&self.modifier_keys));
        }
        if let Some(step) = self.wheel_step {
            let _ = writeln!(out, "wheel_step = {step}");
        }
        for page in &self.pages {
            let _ = writeln!(out, "\n[[pages]]\nname = {}", s(&page.name));
            if page.davinci_defaults {
                out.push_str("davinci_defaults = true\n");
            }
            match &page.wheel {
                None => {}
                Some(WheelDoc::Scroll { horizontal: false }) => out.push_str("wheel = { scroll = {} }\n"),
                Some(WheelDoc::Scroll { horizontal: true }) => {
                    out.push_str("wheel = { scroll = { horizontal = true } }\n")
                }
                Some(WheelDoc::Keys { left, right }) => {
                    let _ = writeln!(out, "wheel = {{ keys = {{ left = {}, right = {} }} }}", s(left), s(right));
                }
                Some(WheelDoc::Davinci { jog_units, scroll_units, fps }) => {
                    let mut tuning = Vec::new();
                    if let Some(v) = jog_units {
                        tuning.push(format!("jog_units = {v}"));
                    }
                    if let Some(v) = scroll_units {
                        tuning.push(format!("scroll_units = {v}"));
                    }
                    if let Some(v) = fps {
                        tuning.push(format!("fps = {v:?}"));
                    }
                    let _ = writeln!(out, "wheel = {{ davinci = {{ {} }} }}", tuning.join(", "));
                }
            }
            let mut tables = vec![("keys".to_string(), &page.keys), ("fn_keys".to_string(), &page.fn_keys)];
            tables.extend(page.layers.iter().map(|(modifier, keys)| (format!("layers.{modifier}"), keys)));
            for (table, keys) in tables {
                if keys.is_empty() {
                    continue;
                }
                let _ = writeln!(out, "\n[pages.{table}]");
                for (key, mapping) in keys {
                    let mut fields = mapping.press().toml_fields();
                    if let Some(label) = &mapping.label {
                        fields.push(format!("label = {}", s(label)));
                    }
                    if let Some(hold) = &mapping.hold {
                        fields.push(format!("hold = {{ {} }}", hold.toml_fields().join(", ")));
                    }
                    let _ = writeln!(out, "{key} = {{ {} }}", fields.join(", "));
                }
            }
        }
        out
    }
}

const PROFILE_HEADER: &str = "\
# Open Speed Editor Mapper profile
#
# The first page key is the start page; the active page's key blinks.
# Pressing all toggle_chord keys together switches mappings off and on.
#
# Actions:   { keys = \"ctrl+shift+m\" }  held as long as the key is held
#            { run = \"calc.exe\" }       opens a program, file or URL
#            { text = \"Hello\" }         types text
#            { resolve_key = \"markIn\" }  Resolve function, pressed with your Resolve shortcut
#            { resolve_menu = \"Trim > Trim Start\" }  clicks a Resolve menu item, no shortcut needed
#            { resolve = \"add_marker\", params = { color = \"Red\" } }
#                                           runs a DaVinci Resolve command directly
# Extras:    label = \"Scene 1\"          name shown on the key in the editor
#            hold = { keys = \"f14\" }    long press (half a second) does this instead
# Fn layer:  while fn_key is held, each page uses its [pages.fn_keys],
#            e.g. hold CUT and press IN. A quick tap of fn_key still
#            selects its page.
# Wheel:     { scroll = {} } or { scroll = { horizontal = true } }
#            { keys = { left = \"left\", right = \"right\" } }
#            { davinci = {} }   Resolve's own JOG / SHTL / SCRL behaviour

";

/// Something a key can do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// A shortcut. Held for as long as the Speed Editor key is held, unless
    /// the key also has a long-press action (then it is tapped).
    Keys(Shortcut),
    /// Opens a program, file or URL.
    Run(String),
    /// Types text.
    Text(String),
    /// Runs a DaVinci Resolve command through its scripting API, with its
    /// settings (defaults filled in).
    Resolve { command: String, params: BTreeMap<String, String> },
    /// Presses the user's Resolve shortcut for this keyboard command.
    ResolveKey(String),
    /// Only video track n + audio track n take part in Resolve's next edits
    /// (0 = all tracks again).
    SoloTrack(u8),
    /// Selects the clip under the playhead on video track n (with its linked
    /// audio).
    SelectTrack(u8),
    /// Clicks a Resolve menu item (path from the top menu).
    ResolveMenu(Vec<String>),
}

/// A key's validated mapping. At least one of `press` and `hold` is set.
/// Without `press`, a short press does nothing here, so on a page that keeps
/// DaVinci's behaviour the key's normal function stays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub press: Option<Action>,
    /// Long-press action.
    pub hold: Option<Action>,
}

/// What the wheel does on a page.
#[derive(Clone, Debug, PartialEq)]
pub enum Wheel {
    /// Taps `left` or `right` once per wheel step.
    Keys { left: Shortcut, right: Shortcut },
    /// Turns like a mouse wheel.
    Scroll { horizontal: bool },
    /// Behaves like Resolve's own wheel; JOG, SHTL and SCRL switch its mode.
    Davinci(DaVinciWheelConfig),
}

#[derive(Clone, Debug)]
pub struct Page {
    pub name: String,
    /// Keys without a mapping keep their native DaVinci Resolve function.
    pub davinci_defaults: bool,
    pub keys: HashMap<Key, Mapping>,
    /// Layers used while a modifier (the `fn_key` or a `modifier_keys` key)
    /// is held, by modifier.
    pub layers: HashMap<Key, HashMap<Key, Mapping>>,
    pub wheel: Option<Wheel>,
}

impl Page {
    /// The layer of `modifier` (empty if the page has none).
    pub fn layer(&self, modifier: Key) -> Option<&HashMap<Key, Mapping>> {
        self.layers.get(&modifier)
    }
}

/// A validated profile.
#[derive(Clone, Debug)]
pub struct Profile {
    /// One page per page key; the first page is the start page.
    pub pages: Vec<Page>,
    pub page_keys: Vec<Key>,
    /// Pressed together, these keys switch between custom and normal mode.
    pub toggle_chord: Vec<Key>,
    /// Held, it switches the active page to its Fn layer.
    pub fn_key: Option<Key>,
    /// Every key that is a modifier when held with another key: the
    /// `fn_key` first, then the `modifier_keys`.
    pub modifiers: Vec<Key>,
    pub wheel_step: i32,
}

#[derive(Debug)]
pub struct ProfileError(pub String);

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProfileError {}

fn err<T>(message: impl Into<String>) -> Result<T, ProfileError> {
    Err(ProfileError(message.into()))
}

fn key(id: &str) -> Result<Key, ProfileError> {
    Key::from_id(id).ok_or_else(|| ProfileError(format!("unknown Speed Editor key {id:?}")))
}

fn shortcut(text: &str) -> Result<Shortcut, ProfileError> {
    Shortcut::parse(text).map_err(ProfileError)
}

/// Validates an action; `None` when no field is set.
fn action(doc: &ActionDoc, place: &str) -> Result<Option<Action>, ProfileError> {
    let set = [&doc.keys, &doc.run, &doc.text, &doc.resolve, &doc.resolve_key, &doc.resolve_menu]
        .iter()
        .filter(|v| v.is_some())
        .count()
        + usize::from(doc.solo_track.is_some())
        + usize::from(doc.select_track.is_some());
    if set > 1 {
        return err(format!("{place}: choose one of keys, run, text, resolve, resolve_key, resolve_menu, solo_track or select_track"));
    }
    if let Some(menu) = &doc.resolve_menu {
        let path: Vec<String> = menu.split('>').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect();
        if path.len() < 2 {
            return err(format!("{place}: a Resolve menu item needs a menu and an item, e.g. \"Edit > Undo\""));
        }
        return Ok(Some(Action::ResolveMenu(path)));
    }
    if let Some(n) = doc.select_track {
        if !(1..=8).contains(&n) {
            return err(format!("{place}: select_track must be 1-8"));
        }
        return Ok(Some(Action::SelectTrack(n)));
    }
    if let Some(n) = doc.solo_track {
        if n > 8 {
            return err(format!("{place}: solo_track must be 1-8, or 0 for all tracks"));
        }
        return Ok(Some(Action::SoloTrack(n)));
    }
    if let Some(command) = &doc.resolve_key {
        if command.trim().is_empty() {
            return err(format!("{place}: choose a Resolve keyboard command"));
        }
        return Ok(Some(Action::ResolveKey(command.trim().to_string())));
    }
    if doc.params.is_some() && doc.resolve.is_none() {
        return err(format!("{place}: params only go with a resolve command"));
    }
    let empty = |v: &str| v.trim().is_empty();
    Ok(match (&doc.keys, &doc.run, &doc.text, &doc.resolve) {
        (Some(k), ..) => Some(Action::Keys(shortcut(k).map_err(|e| ProfileError(format!("{place}: {e}")))?)),
        (_, Some(r), ..) if empty(r) => return err(format!("{place}: the program to open is empty")),
        (_, Some(r), ..) => Some(Action::Run(r.clone())),
        (_, _, Some(t), _) if t.is_empty() => return err(format!("{place}: the text to type is empty")),
        (_, _, Some(t), _) => Some(Action::Text(t.clone())),
        (_, _, _, Some(command)) => {
            let given = doc.params.clone().unwrap_or_default();
            let params = crate::resolve::resolve_params(command, &given).map_err(|e| ProfileError(format!("{place}: {e}")))?;
            Some(Action::Resolve { command: command.clone(), params })
        }
        _ => None,
    })
}

impl Profile {
    pub fn from_toml(text: &str) -> Result<Profile, ProfileError> {
        Profile::from_doc(&ProfileDoc::from_toml(text)?)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Profile, ProfileError> {
        let text = std::fs::read_to_string(path).map_err(|e| ProfileError(e.to_string()))?;
        Profile::from_toml(&text)
    }

    /// Validates a document.
    pub fn from_doc(file: &ProfileDoc) -> Result<Profile, ProfileError> {
        if file.version != FORMAT_VERSION {
            return err(format!("profile version {} is not supported (expected {FORMAT_VERSION})", file.version));
        }

        let page_keys = file.page_keys.iter().map(|id| key(id)).collect::<Result<Vec<_>, _>>()?;
        if page_keys.is_empty() {
            return err("page_keys needs at least one key");
        }
        if page_keys.len() != file.pages.len() {
            return err(format!("{} page keys but {} pages; each page key selects one page", page_keys.len(), file.pages.len()));
        }
        if has_duplicates(&page_keys) {
            return err("page_keys contains the same key twice");
        }

        let toggle_chord = file.toggle_chord.iter().map(|id| key(id)).collect::<Result<Vec<_>, _>>()?;
        if toggle_chord.len() < 2 || has_duplicates(&toggle_chord) {
            return err("toggle_chord needs at least two different keys");
        }
        if let Some(k) = toggle_chord.iter().find(|k| !page_keys.contains(k)) {
            return err(format!("toggle_chord key {:?} must also be a page key", k.id()));
        }

        let fn_key = file.fn_key.as_deref().map(key).transpose()?;
        if let Some(k) = fn_key.filter(|k| !page_keys.contains(k)) {
            return err(format!("fn_key {:?} must be a page key", k.id()));
        }
        let mut modifiers: Vec<Key> = fn_key.into_iter().collect();
        for id in &file.modifier_keys {
            let k = key(id)?;
            if !page_keys.contains(&k) {
                return err(format!("modifier key {id:?} must be a page key"));
            }
            if !modifiers.contains(&k) {
                modifiers.push(k);
            }
        }

        let pages = file
            .pages
            .iter()
            .map(|page| {
                let keys = mappings(&page.keys, &page_keys, &format!("page {:?}", page.name))?;
                let mut layers = HashMap::new();
                if let Some(fn_key) = fn_key {
                    layers.insert(fn_key, mappings(&page.fn_keys, &page_keys, &format!("page {:?} Fn layer", page.name))?);
                } else if !page.fn_keys.is_empty() {
                    return err(format!("page {:?} has an Fn layer but the profile has no fn_key", page.name));
                }
                for (id, table) in &page.layers {
                    let modifier = key(id)?;
                    if !modifiers.contains(&modifier) || Some(modifier) == fn_key {
                        return err(format!("page {:?}: layer {id:?} needs {id:?} in modifier_keys", page.name));
                    }
                    let place = format!("page {:?} layer {id:?}", page.name);
                    layers.insert(modifier, mappings(table, &page_keys, &place)?);
                }
                let wheel = match &page.wheel {
                    None => None,
                    Some(WheelDoc::Keys { left, right }) => {
                        Some(Wheel::Keys { left: shortcut(left)?, right: shortcut(right)? })
                    }
                    Some(WheelDoc::Scroll { horizontal }) => Some(Wheel::Scroll { horizontal: *horizontal }),
                    Some(WheelDoc::Davinci { jog_units, scroll_units, fps }) => {
                        let mut config = DaVinciWheelConfig::default();
                        if let Some(v) = jog_units {
                            config.jog_units = *v;
                        }
                        if let Some(v) = scroll_units {
                            config.scroll_units = *v;
                        }
                        if let Some(v) = fps {
                            config.fps = *v;
                        }
                        if config.jog_units <= 0 || config.scroll_units <= 0 || config.fps <= 0.0 {
                            return err(format!("page {:?}: wheel tuning values must be positive", page.name));
                        }
                        Some(Wheel::Davinci(config))
                    }
                };
                Ok(Page { name: page.name.clone(), davinci_defaults: page.davinci_defaults, keys, layers, wheel })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let wheel_step = file.wheel_step.unwrap_or(DEFAULT_WHEEL_STEP);
        if wheel_step <= 0 {
            return err("wheel_step must be positive");
        }

        Ok(Profile { pages, page_keys, toggle_chord, fn_key, modifiers, wheel_step })
    }
}

/// Validates one table of key mappings. Page keys can't be mapped.
fn mappings(
    docs: &BTreeMap<String, KeyDoc>,
    page_keys: &[Key],
    place: &str,
) -> Result<HashMap<Key, Mapping>, ProfileError> {
    let mut keys = HashMap::new();
    for (id, doc) in docs {
        let k = key(id)?;
        let place = format!("{place}, key {id:?}");
        if page_keys.contains(&k) {
            return err(format!("{place}: page keys cannot be mapped"));
        }
        let press = action(&doc.press(), &place)?;
        let hold = match &doc.hold {
            Some(h) => action(h, &format!("{place} (long press)"))?,
            None => None,
        };
        if press.is_some() || hold.is_some() {
            keys.insert(k, Mapping { press, hold });
        }
    }
    Ok(keys)
}

fn is_false(value: &bool) -> bool {
    !value
}

fn has_duplicates(keys: &[Key]) -> bool {
    keys.iter().enumerate().any(|(i, k)| keys[..i].contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
        version = 1
        page_keys = ["cut", "dissolve"]
        toggle_chord = ["cut", "dissolve"]

        [[pages]]
        name = "One"
        wheel = { scroll = {} }
        keys.cam1 = { keys = "ctrl+c" }

        [[pages]]
        name = "Two"
        keys.cam1 = { run = "notepad.exe" }
    "#;

    #[test]
    fn loads_minimal_profile() {
        let p = Profile::from_toml(MINIMAL).unwrap();
        assert_eq!(p.pages.len(), 2);
        assert_eq!(p.pages[0].wheel, Some(Wheel::Scroll { horizontal: false }));
        assert_eq!(p.pages[1].keys[&Key::Cam1].press, Some(Action::Run("notepad.exe".into())));
        assert_eq!(p.wheel_step, 360);
    }

    #[test]
    fn loads_label_text_and_long_press() {
        let text = MINIMAL.replace(
            r#"keys.cam1 = { keys = "ctrl+c" }"#,
            r#"keys.cam1 = { text = "Hi", label = "Greet", hold = { keys = "f13" } }
        keys.cam2 = { hold = { run = "calc.exe" } }"#,
        );
        let doc = ProfileDoc::from_toml(&text).unwrap();
        assert_eq!(ProfileDoc::from_toml(&doc.to_toml()).unwrap(), doc);
        let p = Profile::from_doc(&doc).unwrap();
        let cam1 = &p.pages[0].keys[&Key::Cam1];
        assert_eq!(cam1.press, Some(Action::Text("Hi".into())));
        assert_eq!(cam1.hold, Some(Action::Keys(Shortcut::parse("f13").unwrap())));
        assert_eq!(p.pages[0].keys[&Key::Cam2].press, None);
    }

    #[test]
    fn loads_resolve_command_with_defaults() {
        let text = MINIMAL.replace(
            r#"{ keys = "ctrl+c" }"#,
            r#"{ resolve = "add_marker", params = { color = "Red" } }"#,
        );
        let doc = ProfileDoc::from_toml(&text).unwrap();
        assert_eq!(ProfileDoc::from_toml(&doc.to_toml()).unwrap(), doc);
        let p = Profile::from_doc(&doc).unwrap();
        let Some(Action::Resolve { command, params }) = &p.pages[0].keys[&Key::Cam1].press else {
            panic!("expected a Resolve action");
        };
        assert_eq!(command, "add_marker");
        assert_eq!((params["color"].as_str(), params["name"].as_str()), ("Red", ""));
        let bad = MINIMAL.replace(r#"{ keys = "ctrl+c" }"#, r#"{ resolve = "teleport" }"#);
        assert!(Profile::from_toml(&bad).is_err());
    }

    #[test]
    fn rejects_two_actions_in_one() {
        let text = MINIMAL.replace(r#"{ keys = "ctrl+c" }"#, r#"{ keys = "ctrl+c", run = "calc.exe" }"#);
        assert!(Profile::from_toml(&text).unwrap_err().0.contains("choose one"));
    }

    #[test]
    fn rejects_mapping_a_page_key() {
        let text = MINIMAL.replace("keys.cam1 = { run", "keys.cut = { run");
        assert!(Profile::from_toml(&text).unwrap_err().0.contains("page key"));
    }

    #[test]
    fn rejects_page_count_mismatch() {
        let text = MINIMAL.replace(r#"["cut", "dissolve"]
        toggle"#, r#"["cut", "dissolve", "smooth_cut"]
        toggle"#);
        assert!(Profile::from_toml(&text).is_err());
    }

    #[test]
    fn rejects_unknown_shortcut_key() {
        let text = MINIMAL.replace("ctrl+c", "ctrl+nope");
        assert!(Profile::from_toml(&text).is_err());
    }

    #[test]
    fn saved_profile_round_trips() {
        let doc = ProfileDoc::from_toml(include_str!("../../../profiles/example.toml")).unwrap();
        let saved = doc.to_toml();
        assert_eq!(ProfileDoc::from_toml(&saved).unwrap(), doc);
        Profile::from_toml(&saved).unwrap();
    }

    #[test]
    fn example_profile_is_valid() {
        let text = include_str!("../../../profiles/example.toml");
        let p = Profile::from_toml(text).unwrap();
        assert_eq!(p.page_keys, vec![Key::Cut, Key::Dissolve, Key::SmoothCut]);
    }
}
