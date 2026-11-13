use serde::{Deserialize, Serialize};
use std::fs::{create_dir_all, File};
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use crate::error::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinnedApp {
    pub name: String,
    pub target_path: String,
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReminderItem {
    pub id: u64,
    pub text: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OxideConfig {
    pub dock_base_size: f32,
    pub dock_max_scale: f32,
    pub dock_influence_radius: f32,
    pub notch_enabled: bool,
    pub widgets_enabled: bool,
    pub dark_mode: bool,
    pub spring_stiffness: f32,
    pub spring_damping: f32,
    pub weather_city: String,
    pub pinned_apps: Vec<PinnedApp>,
    pub reminders: Vec<ReminderItem>,
}

impl Default for OxideConfig {
    fn default() -> Self {
        Self {
            dock_base_size: 56.0,
            dock_max_scale: 1.55,
            dock_influence_radius: 140.0,
            notch_enabled: true,
            widgets_enabled: true,
            dark_mode: true,
            spring_stiffness: 280.0,
            spring_damping: 28.0,
            weather_city: "Cupertino".to_string(),
            pinned_apps: vec![
                PinnedApp {
                    name: "Explorer".to_string(),
                    target_path: "explorer.exe".to_string(),
                    icon_path: None,
                },
                PinnedApp {
                    name: "Terminal".to_string(),
                    target_path: "wt.exe".to_string(),
                    icon_path: None,
                },
                PinnedApp {
                    name: "Edge".to_string(),
                    target_path: "msedge.exe".to_string(),
                    icon_path: None,
                },
                PinnedApp {
                    name: "Settings".to_string(),
                    target_path: "ms-settings:".to_string(),
                    icon_path: None,
                },
            ],
            reminders: vec![
                ReminderItem {
                    id: 1,
                    text: "Review Rust memory telemetry (< 12MB)".to_string(),
                    completed: true,
                },
                ReminderItem {
                    id: 2,
                    text: "Fine-tune squircle continuous curvature".to_string(),
                    completed: false,
                },
                ReminderItem {
                    id: 3,
                    text: "Verify DWM hardware glass composite".to_string(),
                    completed: false,
                },
            ],
        }
    }
}

impl OxideConfig {
    pub fn config_dir() -> PathBuf {
        if let Ok(app_data) = std::env::var("APPDATA") {
            PathBuf::from(app_data).join("OxideShell")
        } else {
            PathBuf::from(".oxideshell")
        }
    }

    pub fn config_file_path() -> PathBuf {
        Self::config_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_file_path();
        if path.exists() {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                if let Ok(config) = serde_json::from_reader(reader) {
                    return config;
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
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)?;
        Ok(())
    }
}
