use crate::commands::AppState;
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

pub const ID: &str = "mynote";
const SHOW_ITEM: &str = "tray-show";
const QUIT_ITEM: &str = "tray-quit";

pub fn sync(app: &AppHandle, enabled: bool) {
    if !enabled {
        app.remove_tray_by_id(ID);
        return;
    }
    if app.tray_by_id(ID).is_some() {
        return;
    }
    match install(app) {
        Ok(()) => log::trace!("tray icon installed"),
        Err(e) => log::warn!("failed to install tray icon: {e}"),
    }
}

fn install(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, SHOW_ITEM, "Show MyNote", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ITEM, "Quit MyNote", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    TrayIconBuilder::with_id(ID)
        .icon(tauri::include_image!("./icons/32x32.png"))
        .tooltip("MyNote")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            SHOW_ITEM => reveal_window(app),
            QUIT_ITEM => quit_app(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

pub fn reveal_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };
    log::trace!("revealing window");
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
}

/// The only path that closes the window for real while `minimizeToTray` is on —
/// close-to-tray checks this flag before hiding instead of closing.
fn quit_app(app: &AppHandle) {
    app.state::<AppState>().quitting.store(true, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.close();
    }
}
