use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::SystemTime;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShellConfig {
    pub dark_mode: bool,
    pub notch_enabled: bool,
    pub widgets_enabled: bool,
    pub dock_enabled: bool,
}

impl Default for ShellConfig {
    fn default() -> Self {
        Self {
            dark_mode: true,
            notch_enabled: true,
            widgets_enabled: true,
            dock_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockConfig {
    pub base_size: f32,
    pub max_scale: f32,
    pub influence_radius: f32,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            base_size: 54.0,
            max_scale: 1.55,
            influence_radius: 140.0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PositionsConfig {
    pub dock_x: Option<i32>,
    pub dock_y: Option<i32>,
    pub widgets_x: Option<i32>,
    pub widgets_y: Option<i32>,
    pub notch_x: Option<i32>,
    pub notch_y: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OxideConfig {
    pub shell: ShellConfig,
    pub dock: DockConfig,
    pub positions: PositionsConfig,
    #[serde(skip)]
    pub last_modified: Option<SystemTime>,
}

impl OxideConfig {
    pub fn config_dir() -> PathBuf {
        if let Ok(app_data) = std::env::var("APPDATA") {
            PathBuf::from(app_data).join("OxideShell")
        } else {
            PathBuf::from(".oxideshell")
        }
    }

    pub fn scripts_dir() -> PathBuf {
        Self::config_dir().join("scripts")
    }

    pub fn config_file_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_file_path();
        if path.exists() {
            if let Ok(mut file) = File::open(&path) {
                let mut contents = String::new();
                if file.read_to_string(&mut contents).is_ok() {
                    if let Ok(mut config) = toml::from_str::<OxideConfig>(&contents) {
                        config.last_modified = std::fs::metadata(&path)
                            .and_then(|m| m.modified())
                            .ok();
                        return config;
                    }
                }
            }
        }
        let default_config = Self::default();
        let _ = default_config.save();
        default_config
    }

    pub fn save(&self) -> Result<()> {
        let dir = Self::config_dir();
        if !dir.exists() {
            create_dir_all(&dir)?;
        }
        let path = Self::config_file_path();
        let toml_str = toml::to_string_pretty(self).unwrap_or_default();
        let mut file = File::create(path)?;
        file.write_all(toml_str.as_bytes())?;
        Ok(())
    }

    /// Checks if config.toml was modified on disk; if so, reloads and returns true.
    pub fn check_reload(&mut self) -> bool {
        let path = Self::config_file_path();
        if let Ok(meta) = std::fs::metadata(&path) {
            if let Ok(mtime) = meta.modified() {
                if self.last_modified.is_none_or(|last| mtime > last) {
                    *self = Self::load();
                    return true;
                }
            }
        }
        false
    }
}
