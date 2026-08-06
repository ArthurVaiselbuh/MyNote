use log::LevelFilter;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    pub peek_width: u32,
    pub log_level: String,
    pub minimize_to_tray: bool,
    pub start_on_login: bool,
    /// Command id -> chord list, holding only what the user changed; an empty
    /// list means deliberately unassigned. The frontend owns the vocabulary
    /// (see keys/bindings.ts) — the backend just round-trips it.
    pub keybindings: BTreeMap<String, Vec<String>>,
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
            peek_width: 460,
            log_level: "info".into(),
            minimize_to_tray: false,
            start_on_login: false,
            keybindings: BTreeMap::new(),
            window: None,
        }
    }
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

fn working_dir() -> PathBuf {
    PathBuf::from(".")
}

#[cfg(target_os = "windows")]
fn config_root() -> PathBuf {
    env_path("APPDATA").unwrap_or_else(working_dir)
}

#[cfg(target_os = "macos")]
fn config_root() -> PathBuf {
    env_path("HOME")
        .map(|home| home.join("Library").join("Application Support"))
        .unwrap_or_else(working_dir)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn config_root() -> PathBuf {
    env_path("XDG_CONFIG_HOME")
        .or_else(|| env_path("HOME").map(|home| home.join(".config")))
        .unwrap_or_else(working_dir)
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
pub const RECENT_LIMIT: usize = 5;

impl Settings {
    pub fn log_level_filter(&self) -> LevelFilter {
        match self.log_level.trim().to_ascii_lowercase().as_str() {
            "off" | "none" => LevelFilter::Off,
            "error" => LevelFilter::Error,
            "warn" | "warning" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "verbose" | "trace" => LevelFilter::Trace,
            _ => LevelFilter::Info,
        }
    }

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
    fn log_level_parses_known_names_and_defaults_to_info() {
        let level = |name: &str| {
            Settings {
                log_level: name.into(),
                ..Settings::default()
            }
            .log_level_filter()
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
