//! Speed Editor Studio: edit a profile visually and run it.
//!
//! The UI (ui/) edits a `ProfileDoc`. This side validates and saves it, and
//! owns the runtime. "Connected" means the app has taken the device over
//! (Resolve must be closed); "mappings active" decides whether key presses
//! fire their actions or only light up in the editor.
//!
//! The app lives in the system tray: closing the window hides it, Quit in the
//! tray menu ends it. "Start with Windows" starts it hidden at sign-in, so it
//! holds the Speed Editor before DaVinci Resolve can. A second start just
//! shows the running app.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::collections::BTreeMap;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use mapper::keyboard::Shortcut;
use mapper::{Profile, ProfileDoc};
use runtime::resolve::ResolveBridge;
use runtime::{Event, Options, Runtime};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::DialogExt;

const EXAMPLE_PROFILE: &str = include_str!("../../../profiles/example.toml");

/// Started with this argument (by "Start with Windows"), the window stays hidden.
const HIDDEN_ARG: &str = "--hidden";
const TRAY_ID: &str = "main";
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "Speed Editor Studio";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

struct AppState {
    path: PathBuf,
    runtime: Mutex<Option<Runtime>>,
}

/// Runtime events as sent to the UI.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum UiEvent {
    Waiting { text: String },
    Connected { bus: String },
    Battery { charging: bool, level: u8 },
    Jog { mode: u8, value: i32 },
    Leds { leds: u32 },
    Disconnected { text: String },
    Keys { keys: Vec<&'static str> },
    State { custom: bool, page: usize },
    Status { text: String },
    YieldedToResolve,
    Stopped,
}

impl From<Event> for UiEvent {
    fn from(event: Event) -> Self {
        match event {
            Event::Waiting(text) => UiEvent::Waiting { text },
            Event::Connected { bus } => UiEvent::Connected { bus },
            Event::Battery { charging, level } => UiEvent::Battery { charging, level },
            Event::Jog { mode, value } => UiEvent::Jog { mode, value },
            Event::Leds(leds) => UiEvent::Leds { leds },
            Event::Disconnected(text) => UiEvent::Disconnected { text },
            Event::Keys(keys) => UiEvent::Keys { keys: keys.iter().map(|k| k.id()).collect() },
            Event::State { custom, page } => UiEvent::State { custom, page },
            Event::Status(text) => UiEvent::Status { text },
            Event::YieldedToResolve => UiEvent::YieldedToResolve,
            Event::Stopped => UiEvent::Stopped,
        }
    }
}

#[derive(Serialize)]
struct Loaded {
    path: String,
    doc: ProfileDoc,
}

/// Profile location: %APPDATA%\OpenSpeedEditor\profile.toml
fn profile_path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("OpenSpeedEditor").join("profile.toml")
}

#[tauri::command]
fn load_profile(state: State<AppState>) -> Result<Loaded, String> {
    let text = match std::fs::read_to_string(&state.path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => EXAMPLE_PROFILE.to_string(),
        Err(e) => return Err(format!("cannot read {}: {e}", state.path.display())),
    };
    let doc = ProfileDoc::from_toml(&text).map_err(|e| e.to_string())?;
    Ok(Loaded { path: state.path.display().to_string(), doc })
}

#[tauri::command]
fn example_profile() -> Result<ProfileDoc, String> {
    ProfileDoc::from_toml(EXAMPLE_PROFILE).map_err(|e| e.to_string())
}

#[tauri::command]
fn validate_profile(doc: ProfileDoc) -> Result<(), String> {
    Profile::from_doc(&doc).map(drop).map_err(|e| e.to_string())
}

#[tauri::command]
fn validate_shortcut(text: String) -> Result<(), String> {
    Shortcut::parse(&text).map(drop)
}

/// Validates, saves and, when connected, applies the profile immediately.
#[tauri::command]
fn save_profile(doc: ProfileDoc, state: State<AppState>) -> Result<(), String> {
    let profile = Profile::from_doc(&doc).map_err(|e| e.to_string())?;
    if let Some(dir) = state.path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Write next to the target first so a crash never leaves half a file.
    let tmp = state.path.with_extension("toml.tmp");
    std::fs::write(&tmp, doc.to_toml()).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &state.path).map_err(|e| e.to_string())?;
    if let Some(runtime) = state.runtime.lock().unwrap().as_ref() {
        runtime.set_profile(profile);
    }
    Ok(())
}

/// Asks where to save and writes the profile there, e.g. to move it to
/// another computer. Returns the chosen path, or None if cancelled.
#[tauri::command]
async fn export_profile(doc: ProfileDoc, app: AppHandle) -> Result<Option<String>, String> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Export Speed Editor profile")
        .set_file_name("speed-editor-profile.toml")
        .add_filter("Speed Editor profile", &["toml"])
        .blocking_save_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    std::fs::write(&path, doc.to_toml()).map_err(|e| e.to_string())?;
    Ok(Some(path.display().to_string()))
}

/// Asks for a profile file and returns its contents for the editor. Nothing
/// is saved until the user presses Save.
#[tauri::command]
async fn import_profile(app: AppHandle) -> Result<Option<ProfileDoc>, String> {
    let Some(path) = app
        .dialog()
        .file()
        .set_title("Import Speed Editor profile")
        .add_filter("Speed Editor profile", &["toml"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = path.into_path().map_err(|e| e.to_string())?;
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    ProfileDoc::from_toml(&text).map(Some).map_err(|e| format!("{}: {e}", path.display()))
}

/// The DaVinci Resolve commands the editor can offer.
#[tauri::command]
fn resolve_catalog() -> &'static [mapper::resolve::Command] {
    mapper::resolve::COMMANDS
}

/// Checks that Resolve can be reached through its scripting API.
#[tauri::command]
async fn resolve_test() -> Result<String, String> {
    let mut bridge = ResolveBridge::default();
    bridge.request("ping", &Default::default(), Duration::from_secs(10))
}

/// Runs one Resolve command now, to try a mapping from the editor.
#[tauri::command]
async fn resolve_run(command: String, params: BTreeMap<String, String>) -> Result<String, String> {
    let params = mapper::resolve::resolve_params(&command, &params)?;
    let mut bridge = ResolveBridge::default();
    bridge.request(&command, &params, Duration::from_secs(10))
}

/// Takes the device over with the given (unsaved) profile.
#[tauri::command]
fn connect(
    doc: ProfileDoc,
    active: bool,
    yield_to_resolve: bool,
    app: AppHandle,
    state: State<AppState>,
) -> Result<(), String> {
    let profile = Profile::from_doc(&doc).map_err(|e| e.to_string())?;
    let mut slot = state.runtime.lock().unwrap();
    if let Some(old) = slot.take() {
        old.stop();
    }
    *slot = Some(Runtime::spawn(profile, Options { active, yield_to_resolve, pass_through: true }, move |event| {
        if let Some(text) = tray_text(&event) {
            set_tray_status(&app, &text);
        }
        let _ = app.emit("runtime", UiEvent::from(event));
    }));
    Ok(())
}

/// Releases the device and every key the app pressed.
#[tauri::command]
fn disconnect(app: AppHandle, state: State<AppState>) {
    if let Some(runtime) = state.runtime.lock().unwrap().take() {
        runtime.stop();
    }
    set_tray_status(&app, "not connected");
}

/// Whether the app starts with Windows (hidden, in the tray).
#[tauri::command]
fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_VALUE]).is_ok()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let command = format!("\"{}\" {HIDDEN_ARG}", exe.display());
        reg(&["add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &command, "/f"])
    } else if autostart_enabled() {
        reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"])
    } else {
        Ok(())
    }
}

/// Runs reg.exe on the current user's registry.
fn reg(args: &[&str]) -> Result<(), String> {
    let status = Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("cannot run reg: {e}"))?;
    if status.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&status.stderr).trim().to_string()) }
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Lets go of the device (so every key is released and the emulated Speed
/// Editor is unplugged) before the app ends.
fn quit(app: &AppHandle) {
    if let Some(runtime) = app.state::<AppState>().runtime.lock().unwrap().take() {
        runtime.stop();
    }
    app.exit(0);
}

/// The tray tooltip line for runtime events that change the connection.
fn tray_text(event: &Event) -> Option<String> {
    match event {
        Event::Connected { bus } => Some(format!("connected ({bus})")),
        Event::Waiting(text) => Some(text.clone()),
        Event::Disconnected(_) => Some("reconnecting…".into()),
        Event::YieldedToResolve => Some("DaVinci Resolve has the Speed Editor".into()),
        Event::Stopped => Some("not connected".into()),
        _ => None,
    }
}

fn set_tray_status(app: &AppHandle, text: &str) {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        // Windows shows at most 127 characters.
        let tip: String = format!("Speed Editor Studio: {text}").chars().take(127).collect();
        let _ = tray.set_tooltip(Some(tip));
    }
}

fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show Speed Editor Studio", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit_item])?;
    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .tooltip("Speed Editor Studio")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[tauri::command]
fn set_active(active: bool, state: State<AppState>) {
    if let Some(runtime) = state.runtime.lock().unwrap().as_ref() {
        runtime.set_active(active);
    }
}

#[tauri::command]
fn set_yield_to_resolve(yield_to_resolve: bool, state: State<AppState>) {
    if let Some(runtime) = state.runtime.lock().unwrap().as_ref() {
        runtime.set_yield_to_resolve(yield_to_resolve);
    }
}

/// Sends an edited profile to the running device without saving it.
#[tauri::command]
fn preview_profile(doc: ProfileDoc, state: State<AppState>) -> Result<(), String> {
    let profile = Profile::from_doc(&doc).map_err(|e| e.to_string())?;
    if let Some(runtime) = state.runtime.lock().unwrap().as_ref() {
        runtime.set_profile(profile);
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        // A second start (e.g. from the Start menu while it runs from sign-in)
        // shows the running app instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_window(app)))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            build_tray(app)?;
            if !std::env::args().any(|a| a == HIDDEN_ARG) {
                show_window(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window keeps the app running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .manage(AppState { path: profile_path(), runtime: Mutex::new(None) })
        .invoke_handler(tauri::generate_handler![
            load_profile,
            example_profile,
            validate_profile,
            validate_shortcut,
            save_profile,
            export_profile,
            import_profile,
            resolve_catalog,
            resolve_test,
            resolve_run,
            connect,
            disconnect,
            set_active,
            set_yield_to_resolve,
            preview_profile,
            autostart_enabled,
            set_autostart,
        ])
        .run(tauri::generate_context!())
        .expect("failed to start Speed Editor Studio");
}
