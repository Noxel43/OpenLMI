//! Everything the UI can ask the backend to do.

use crate::ambient::{self, MonitorInfo};
use crate::audio::{self, AudioDeviceInfo};
use crate::color::PALETTES;
use crate::effects::{self, EffectInfo};
use crate::engine::{AudioSnapshot, DeviceStatus, Identify};
use crate::firmware::{self, FirmwareOpts};
use crate::layout::{self, LayoutPreview};
use crate::model::{AmbientSettings, Animation, Config, Device, Layout, Preset, Script, Settings};
use crate::output::{self, PortInfo};
use crate::script;
use crate::AppState;
use base64::Engine as _;
use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;
use tauri::ipc::Response;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaletteInfo {
    name: &'static str,
    colors: &'static [&'static str],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptExample {
    name: &'static str,
    code: &'static str,
}

/// Everything the UI needs at start-up, in one round trip.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    config: Config,
    effects: Vec<EffectInfo>,
    palettes: Vec<PaletteInfo>,
    script_examples: Vec<ScriptExample>,
    version: &'static str,
    config_path: String,
}

#[tauri::command]
pub fn bootstrap(state: State<AppState>) -> Bootstrap {
    Bootstrap {
        config: state.config.lock().clone(),
        effects: effects::infos(),
        palettes: PALETTES.iter().map(|(name, colors)| PaletteInfo { name, colors }).collect(),
        script_examples: script::EXAMPLES.iter().map(|(name, code)| ScriptExample { name, code }).collect(),
        version: env!("CARGO_PKG_VERSION"),
        config_path: state.saver.path.display().to_string(),
    }
}

#[tauri::command]
pub fn get_config(state: State<AppState>) -> Config {
    state.config.lock().clone()
}

// --- devices -----------------------------------------------------------------

#[tauri::command]
pub fn save_device(state: State<AppState>, device: Device) {
    let mut config = state.config.lock();
    match config.devices.iter_mut().find(|d| d.id == device.id) {
        Some(existing) => {
            // Editing one device is the hot path (slider drags), so only that
            // device is sent to the render loop.
            *existing = device;
            state.engine.set_device(existing);
        }
        None => {
            config.devices.push(device);
            state.engine.set_config(&config);
        }
    }
    state.saver.mark_dirty();
}

#[tauri::command]
pub fn delete_device(state: State<AppState>, id: String) {
    state.update(|c| c.devices.retain(|d| d.id != id));
}

#[tauri::command]
pub fn reorder_devices(state: State<AppState>, ids: Vec<String>) {
    state.update(|c| {
        c.devices.sort_by_key(|d| ids.iter().position(|id| *id == d.id).unwrap_or(usize::MAX));
    });
}

#[tauri::command]
pub fn save_settings(state: State<AppState>, settings: Settings) {
    state.update(|c| c.settings = settings);
}

// --- library -----------------------------------------------------------------

fn upsert<T>(list: &mut Vec<T>, item: T, same: impl Fn(&T, &T) -> bool) {
    match list.iter_mut().find(|existing| same(existing, &item)) {
        Some(existing) => *existing = item,
        None => list.push(item),
    }
}

#[tauri::command]
pub fn save_animation(state: State<AppState>, animation: Animation) {
    state.update(|c| upsert(&mut c.animations, animation, |a, b| a.id == b.id));
}

#[tauri::command]
pub fn delete_animation(state: State<AppState>, id: String) {
    state.update(|c| c.animations.retain(|a| a.id != id));
}

#[tauri::command]
pub fn save_script(state: State<AppState>, script: Script) {
    state.update(|c| upsert(&mut c.scripts, script, |a, b| a.id == b.id));
}

#[tauri::command]
pub fn delete_script(state: State<AppState>, id: String) {
    state.update(|c| c.scripts.retain(|s| s.id != id));
}

#[tauri::command]
pub fn save_preset(state: State<AppState>, preset: Preset) {
    state.update(|c| upsert(&mut c.presets, preset, |a, b| a.id == b.id));
}

#[tauri::command]
pub fn delete_preset(state: State<AppState>, id: String) {
    state.update(|c| c.presets.retain(|p| p.id != id));
}

/// Replaces the whole configuration with a previously exported one.
#[tauri::command]
pub fn import_config(state: State<AppState>, json: String) -> Result<Config, String> {
    let mut imported: Config = serde_json::from_str(&json).map_err(|e| format!("That file is not an OpenLMI backup: {e}"))?;
    imported.version = crate::store::CONFIG_VERSION;
    Ok(state.update(|c| {
        *c = imported;
        c.clone()
    }))
}

// --- layout helpers ----------------------------------------------------------

#[tauri::command]
pub fn layout_preview(layout: Layout) -> LayoutPreview {
    layout::build(&layout).preview()
}

/// Screen areas each LED samples in ambient mode, for the overlay in the UI.
#[tauri::command]
pub fn ambient_zones(layout: Layout, settings: AmbientSettings, aspect: f32) -> Vec<[f32; 4]> {
    ambient::sample_rects(&layout::build(&layout), &settings, if aspect > 0.0 { aspect } else { 16.0 / 9.0 })
}

// --- live data ---------------------------------------------------------------

/// Latest preview frame for a device as raw RGB bytes.
#[tauri::command]
pub fn get_frame(state: State<AppState>, id: String) -> Response {
    Response::new(state.engine.shared.frames.lock().get(&id).cloned().unwrap_or_default())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    devices: HashMap<String, DeviceStatus>,
    fps: f32,
    capture_error: Option<String>,
}

#[tauri::command]
pub fn get_status(state: State<AppState>) -> Status {
    Status {
        devices: state.engine.shared.status.lock().clone(),
        fps: *state.engine.shared.fps.lock(),
        capture_error: state.engine.screen.error(),
    }
}

/// Current audio analysis. Calling this keeps the analyser running, so the
/// visualiser works even when no device is using an audio effect.
#[tauri::command]
pub fn get_audio(state: State<AppState>) -> AudioSnapshot {
    *state.engine.shared.ui_audio.lock() = Some(Instant::now());
    state.engine.shared.audio.lock().clone()
}

/// The downsampled screen as the LEDs see it: width, height, then RGB bytes.
/// Calling this keeps capture running for that display.
#[tauri::command]
pub fn get_screen(state: State<AppState>, monitor: String) -> Response {
    state.engine.screen.request(&monitor);
    let Some(grid) = state.engine.screen.grid(&monitor) else {
        return Response::new(Vec::new());
    };
    let mut bytes = Vec::with_capacity(6 + grid.px.len() * 3);
    bytes.push(grid.w as u8);
    bytes.push(grid.h as u8);
    // Picture area inside any black bars, in 1/255ths: left, top, right, bottom.
    bytes.extend(grid.content.iter().map(|v| (v * 255.0).round() as u8));
    for px in &grid.px {
        bytes.extend_from_slice(&px.to_u8());
    }
    Response::new(bytes)
}

// --- hardware discovery --------------------------------------------------------

#[tauri::command(async)]
pub fn list_serial_ports() -> Vec<PortInfo> {
    output::list_serial_ports()
}

#[tauri::command(async)]
pub fn list_monitors() -> Vec<MonitorInfo> {
    ambient::list_monitors()
}

#[tauri::command(async)]
pub fn list_audio_inputs() -> Vec<AudioDeviceInfo> {
    audio::list_inputs()
}

// --- wiring test and live painting -----------------------------------------------

/// Shows a wiring test pattern; `mode` "off" returns to normal.
#[tauri::command]
pub fn identify(state: State<AppState>, id: String, mode: String, index: Option<usize>) {
    state.engine.identify(&id, Identify::parse(&mode, index));
}

/// Shows fixed pixels (base64 RGB) on a device while it is being painted in
/// the animation editor. `None` hands the device back to its source.
#[tauri::command]
pub fn paint(state: State<AppState>, id: String, pixels: Option<String>) {
    let bytes = pixels.and_then(|p| base64::engine::general_purpose::STANDARD.decode(p).ok());
    state.engine.paint(&id, bytes);
}

// --- scripts and firmware --------------------------------------------------------

#[tauri::command(async)]
pub fn check_script(code: String) -> Result<(), String> {
    script::check(&code)
}

#[tauri::command]
pub fn firmware_sketch(opts: FirmwareOpts) -> String {
    firmware::receiver_sketch(&opts)
}

#[tauri::command]
pub fn animation_sketch(state: State<AppState>, id: String, opts: FirmwareOpts) -> Result<String, String> {
    let config = state.config.lock();
    let anim = config.animations.iter().find(|a| a.id == id).ok_or("That animation no longer exists")?;
    Ok(firmware::animation_sketch(anim, &opts))
}

// --- files chosen through the native dialogs -------------------------------------

#[tauri::command(async)]
pub fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|e| format!("Could not write {path}: {e}"))
}

#[tauri::command(async)]
pub fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|e| format!("Could not read {path}: {e}"))
}
