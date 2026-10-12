//! Loading and saving the configuration file.

use crate::model::{Config, Connection, Device, Layout, OutputSettings, Params, Source};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub const CONFIG_VERSION: u32 = 1;

/// What a brand-new install starts with: one simulated strip, so there is
/// something to play with before any hardware is connected.
pub fn first_run_config() -> Config {
    Config {
        version: CONFIG_VERSION,
        devices: vec![Device {
            id: "demo-strip".into(),
            name: "Demo strip".into(),
            power: true,
            connection: Connection::Simulator,
            layout: Layout::Strip { count: 60, reversed: false },
            output: OutputSettings { brightness: 1.0, ..Default::default() },
            source: Source::Effect { effect: "rainbow".into(), params: Params::new() },
        }],
        ..Default::default()
    }
}

pub fn load(path: &Path) -> Config {
    let Ok(text) = std::fs::read_to_string(path) else {
        return first_run_config();
    };
    match serde_json::from_str::<Config>(&text) {
        Ok(mut config) => {
            config.version = CONFIG_VERSION;
            config
        }
        Err(e) => {
            // Never silently throw away someone's setup: keep the unreadable
            // file next to the new one.
            let backup = path.with_extension("broken.json");
            let _ = std::fs::copy(path, &backup);
            eprintln!("OpenLMI: could not read {} ({e}); kept a copy at {}", path.display(), backup.display());
            first_run_config()
        }
    }
}

/// Writes via a temporary file so a crash mid-write can't corrupt the config.
pub fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(config)?)?;
    std::fs::rename(&tmp, path)
}

/// Debounces saves: callers flag the config dirty as often as they like and a
/// background thread writes it at most a couple of times per second.
#[derive(Clone)]
pub struct Saver {
    pub path: PathBuf,
    dirty: Arc<AtomicBool>,
}

impl Saver {
    pub fn new(path: PathBuf) -> Self {
        Saver { path, dirty: Arc::new(AtomicBool::new(false)) }
    }

    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Relaxed);
    }

    /// Returns true if a save is pending, clearing the flag.
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openlmi-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("nested").join("config.json");
        let mut config = first_run_config();
        config.devices[0].name = "Shelf".into();
        save(&path, &config).unwrap();
        assert_eq!(load(&path), config);
        assert!(!path.with_extension("json.tmp").exists());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_file_gives_the_demo_setup() {
        let config = load(Path::new("definitely/not/here.json"));
        assert_eq!(config.devices.len(), 1);
    }

    #[test]
    fn a_corrupt_file_is_backed_up_not_lost() {
        let dir = temp_dir("corrupt");
        let path = dir.join("config.json");
        std::fs::write(&path, "{ this is not json").unwrap();
        let config = load(&path);
        assert_eq!(config.devices.len(), 1);
        assert_eq!(std::fs::read_to_string(dir.join("config.broken.json")).unwrap(), "{ this is not json");
        std::fs::remove_dir_all(dir).ok();
    }
}
