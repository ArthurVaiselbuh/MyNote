use log::LevelFilter;
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
    pub recent_notebooks: Vec<String>,
    pub zoom: f64,
    pub text_color: String,
    pub background_color: String,
    pub panel_color: String,
    pub accent_color: String,
    pub heading_color: String,
    pub focus_alpha: f64,
    pub scroll_speed: f64,
    pub tree_width: u32,
    pub log_level: String,
    pub window: Option<WindowGeom>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            notebook_path: None,
            recent_notebooks: vec![],
            zoom: 1.0,
            text_color: "#d4d4d4".into(),
            background_color: "#1e1f22".into(),
            panel_color: "#26282b".into(),
            accent_color: "#5aa0f2".into(),
            heading_color: "#d4d4d4".into(),
            focus_alpha: 0.5,
            scroll_speed: 1.0,
            tree_width: 300,
            log_level: "info".into(),
            window: None,
        }
    }
}

fn config_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
    }
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join("Library").join("Application Support"))
            .unwrap_or_else(|| PathBuf::from("."));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg);
        }
        return std::env::var_os("HOME")
            .map(|home| PathBuf::from(home).join(".config"))
            .unwrap_or_else(|| PathBuf::from("."));
    }
}

pub fn app_dir() -> PathBuf {
    config_root().join("MyNote")
}

pub fn settings_path() -> PathBuf {
    app_dir().join("settings.json")
}

pub fn default_notebook_dir() -> PathBuf {
    app_dir().join("notebook")
}

pub const LOG_FILE_STEM: &str = "MyNote";

impl Settings {
    pub fn log_level_filter(&self) -> LevelFilter {
        match self.log_level.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => LevelFilter::Off,
            "error" => LevelFilter::Error,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "warn" | "warning" => LevelFilter::Warn,
            "verbose" | "trace" => LevelFilter::Trace,
            _ => LevelFilter::Info,
        }
    }
}

pub const RECENT_LIMIT: usize = 5;

impl Settings {
    pub fn remember_notebook(&mut self, path: &str) {
        self.recent_notebooks
            .retain(|p| !p.eq_ignore_ascii_case(path));
        self.recent_notebooks.insert(0, path.to_string());
        self.recent_notebooks.truncate(RECENT_LIMIT);
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remember_notebook_keeps_most_recent_first_without_duplicates() {
        let mut s = Settings::default();
        s.remember_notebook("C:\\a");
        s.remember_notebook("C:\\b");
        s.remember_notebook("C:\\A");
        assert_eq!(s.recent_notebooks, vec!["C:\\A", "C:\\b"]);
    }

    #[test]
    fn log_level_parses_known_names_and_defaults_to_warn() {
        let level = |name: &str| {
            let mut s = Settings::default();
            s.log_level = name.into();
            s.log_level_filter()
        };
        assert_eq!(level("off"), LevelFilter::Off);
        assert_eq!(level("error"), LevelFilter::Error);
        assert_eq!(level("warn"), LevelFilter::Warn);
        assert_eq!(level("INFO"), LevelFilter::Info);
        assert_eq!(level("verbose"), LevelFilter::Trace);
        assert_eq!(level("trace"), LevelFilter::Trace);
        assert_eq!(level("nonsense"), LevelFilter::Info);
        assert_eq!(Settings::default().log_level_filter(), LevelFilter::Info);
    }

    #[test]
    fn remember_notebook_truncates_to_limit() {
        let mut s = Settings::default();
        for i in 0..(RECENT_LIMIT + 3) {
            s.remember_notebook(&format!("C:\\nb{i}"));
        }
        assert_eq!(s.recent_notebooks.len(), RECENT_LIMIT);
        assert_eq!(s.recent_notebooks[0], format!("C:\\nb{}", RECENT_LIMIT + 2));
    }
}
