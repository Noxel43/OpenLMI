//! OpenLMI - Open Microcontroller LED Integration.
//!
//! The Rust side owns everything time-critical: rendering, screen and audio
//! capture, and talking to hardware. The web UI edits the configuration and
//! polls for preview frames.

pub mod ambient;
pub mod audio;
pub mod color;
mod commands;
pub mod effects;
pub mod engine;
pub mod firmware;
pub mod layout;
pub mod model;
pub mod output;
#[cfg(windows)]
pub mod screen_dxgi;
pub mod script;
pub mod store;

use engine::EngineHandle;
use model::Config;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};

pub struct AppState {
    pub config: Mutex<Config>,
    pub saver: store::Saver,
    pub engine: EngineHandle,
}

impl AppState {
    /// Applies a change to the configuration, hands the result to the render
    /// loop and schedules a save.
    pub fn update<T>(&self, change: impl FnOnce(&mut Config) -> T) -> T {
        let mut config = self.config.lock();
        let result = change(&mut config);
        self.engine.set_config(&config);
        self.saver.mark_dirty();
        result
    }

    fn save_now(&self) {
        self.saver.take_dirty();
        let config = self.config.lock().clone();
        if let Err(e) = store::save(&self.saver.path, &config) {
            eprintln!("OpenLMI: could not save settings: {e}");
        }
    }
}

/// Where settings live. A `data` folder next to the executable switches to
/// portable mode; `OPENLMI_CONFIG_DIR` overrides everything.
fn config_dir(app: &AppHandle) -> PathBuf {
    if let Some(dir) = std::env::var_os("OPENLMI_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(portable) = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("data"))) {
        if portable.is_dir() {
            return portable;
        }
    }
    app.path().app_config_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn toggle_all_lights(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.update(|config| {
        let any_on = config.devices.iter().any(|d| d.power);
        for device in &mut config.devices {
            device.power = !any_on;
        }
    });
    let _ = app.emit("config-changed", ());
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open OpenLMI", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "Lights on / off", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &toggle, &quit])?;

    let mut tray = TrayIconBuilder::with_id("main")
        .tooltip("OpenLMI")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main_window(app),
            "toggle" => toggle_all_lights(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let handle = app.handle().clone();
            let path = config_dir(&handle).join("config.json");
            let config = store::load(&path);
            let start_hidden = config.settings.start_minimized && std::env::args().any(|a| a == "--minimized");

            let state = AppState {
                engine: EngineHandle::start(config.clone()),
                config: Mutex::new(config),
                saver: store::Saver::new(path),
            };
            app.manage(state);

            // Debounced saving: sliders can change settings many times a second.
            let saver_handle = handle.clone();
            std::thread::Builder::new().name("openlmi-saver".into()).spawn(move || loop {
                std::thread::sleep(Duration::from_millis(700));
                let state = saver_handle.state::<AppState>();
                if state.saver.take_dirty() {
                    let config = state.config.lock().clone();
                    if let Err(e) = store::save(&state.saver.path, &config) {
                        eprintln!("OpenLMI: could not save settings: {e}");
                    }
                }
            })?;

            build_tray(&handle)?;
            if !start_hidden {
                show_main_window(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.config.lock().settings.close_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::get_config,
            commands::save_device,
            commands::delete_device,
            commands::reorder_devices,
            commands::save_settings,
            commands::save_animation,
            commands::delete_animation,
            commands::save_script,
            commands::delete_script,
            commands::save_preset,
            commands::delete_preset,
            commands::import_config,
            commands::layout_preview,
            commands::ambient_zones,
            commands::get_frame,
            commands::get_status,
            commands::get_audio,
            commands::get_screen,
            commands::list_serial_ports,
            commands::list_monitors,
            commands::list_audio_inputs,
            commands::identify,
            commands::paint,
            commands::check_script,
            commands::firmware_sketch,
            commands::animation_sketch,
            commands::write_text_file,
            commands::read_text_file,
        ])
        .build(tauri::generate_context!())
        .expect("error while starting OpenLMI");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Leave the LEDs dark and the settings on disk.
            if let Some(state) = handle.try_state::<AppState>() {
                state.save_now();
                state.engine.shutdown();
            }
        }
    });
}
