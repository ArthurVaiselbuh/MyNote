use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct WindowGeom {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub notebook_path: Option<String>,
    pub zoom: f64,
    pub text_color: String,
    pub background_color: String,
    pub panel_color: String,
    pub accent_color: String,
    pub focus_alpha: f64,
    pub scroll_speed: f64,
    pub window: Option<WindowGeom>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            notebook_path: None,
            zoom: 1.0,
            text_color: "#d4d4d4".into(),
            background_color: "#1e1f22".into(),
            panel_color: "#26282b".into(),
            accent_color: "#5aa0f2".into(),
            focus_alpha: 0.5,
            scroll_speed: 1.0,
            window: None,
        }
    }
}

pub fn app_dir() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("MyNote")
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

pub fn default_notebook_dir() -> PathBuf {
    app_dir().join("notebook")
}

impl Settings {
    pub fn load() -> Settings {
        fs::read_to_string(settings_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let _ = fs::create_dir_all(app_dir());
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(settings_path(), json);
        }
    }
}
