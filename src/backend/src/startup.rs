use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// Passed to the copy of MyNote the OS launches at login, so it can come up
/// straight into the tray instead of stealing the screen at every sign-in.
pub const HIDDEN_ARG: &str = "--hidden";

pub fn sync(app: &AppHandle, enabled: bool) {
    let launcher = app.autolaunch();
    let applied = if enabled {
        launcher.enable()
    } else {
        launcher.disable()
    };
    if let Err(e) = applied {
        log::warn!("failed to set start-on-login to {enabled}: {e}");
    }
}

pub fn launched_hidden() -> bool {
    std::env::args().any(|arg| arg == HIDDEN_ARG)
}
