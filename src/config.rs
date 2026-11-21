use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LockMode {
    Exact,
    Range,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppTheme {
    Dark,
    Light,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub osd_hidden: bool,
    pub lock_enabled: bool,
    pub lock_mode: LockMode,
    pub exact_volume: u32,
    pub min_volume: u32,
    pub max_volume: u32,
    pub lock_unmuted: bool,
    pub start_with_windows: bool,
    pub minimize_to_tray: bool,
    pub theme: AppTheme,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            osd_hidden: true,
            lock_enabled: true,
            lock_mode: LockMode::Exact,
            exact_volume: 50,
            min_volume: 20,
            max_volume: 80,
            lock_unmuted: true,
            start_with_windows: false,
            minimize_to_tray: true,
            theme: AppTheme::Dark,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        if let Ok(appdata) = std::env::var("APPDATA") {
            let dir = PathBuf::from(appdata).join("VolumeMaster");
            let _ = fs::create_dir_all(&dir);
            dir.join("config.json")
        } else {
            PathBuf::from("volume_master_config.json")
        }
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            if let Ok(data) = fs::read_to_string(&path) {
                if let Ok(config) = serde_json::from_str::<AppConfig>(&data) {
                    return config;
                }
            }
        }
        let default_config = Self::default();
        let _ = default_config.save();
        default_config
    }

    pub fn save(&self) -> bool {
        let path = Self::config_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            fs::write(path, json).is_ok()
        } else {
            false
        }
    }
}
