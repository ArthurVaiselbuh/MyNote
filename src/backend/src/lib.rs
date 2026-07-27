mod assets;
mod commands;
mod import;
mod import_md;
mod import_mht;
mod lock;
mod search;
mod settings;
mod store;

use commands::AppState;
use percent_encoding::percent_decode_str;
use settings::Settings;
use std::sync::Mutex;
use tauri::Manager;

pub fn run() {
    let settings = Settings::load();
    tauri::Builder::default()
        .plugin(build_log_plugin(&settings))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(navigation_guard())
        .manage(AppState {
            store: Mutex::new(None),
            settings: Mutex::new(settings),
        })
        .register_uri_scheme_protocol("note-asset", |ctx, request| {
            serve_asset(ctx.app_handle(), &request)
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_notebook,
            commands::create_notebook,
            commands::list_recent_notebooks,
            commands::get_tree,
            commands::create_section,
            commands::rename_section,
            commands::move_section,
            commands::delete_section,
            commands::create_page,
            commands::read_page,
            commands::write_page,
            commands::rename_page,
            commands::delete_page,
            commands::move_page,
            commands::undo,
            commands::redo,
            commands::set_expanded,
            commands::set_last_view,
            commands::search_pages,
            commands::save_image,
            commands::inspect_mht,
            commands::import_mht,
            commands::inspect_md,
            commands::import_md,
            commands::reset_agents_md,
            commands::get_settings,
            commands::set_settings
        ])
        .setup(|app| {
            let mut builder =
                tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                    .title("MyNote")
                    .inner_size(1280.0, 840.0)
                    .min_inner_size(640.0, 400.0)
                    .visible(false);
            // wry always calls SetAdditionalBrowserArguments on the WebView2 COM
            // options (even just its own defaults), which per WebView2's docs means
            // it ignores WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS entirely — this is the
            // only path left for the e2e harness to open a CDP debug port.
            if let Ok(port) = std::env::var("MYNOTE_E2E_CDP_PORT") {
                let mut args = format!(
                    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
                );
                if let Ok(log_file) = std::env::var("MYNOTE_E2E_LOG_FILE") {
                    args.push_str(&format!(" --enable-logging --v=1 --log-file=\"{log_file}\""));
                }
                builder = builder.additional_browser_args(&args);
            }
            let win = builder.build()?;

            let geom = app
                .state::<AppState>()
                .settings
                .lock()
                .ok()
                .and_then(|s| s.window.clone());
            if let Some(g) = geom.filter(|g| g.width > 0 && g.height > 0) {
                let _ = win.set_position(tauri::PhysicalPosition::new(g.x, g.y));
                let _ = win.set_size(tauri::PhysicalSize::new(g.width, g.height));
                if g.maximized {
                    let _ = win.maximize();
                }
            }
            let _ = win.show();
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                persist_on_close(window);
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running MyNote");
}

fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(|_, url| {
            let allowed = match url.scheme() {
                "tauri" | "note-asset" => true,
                "http" | "https" => url
                    .host_str()
                    .is_some_and(|h| h == "localhost" || h.ends_with(".localhost")),
                _ => false,
            };
            if !allowed {
                log::warn!("blocked webview navigation to a non-app url");
            }
            allowed
        })
        .build()
}

fn build_log_plugin<R: tauri::Runtime>(settings: &Settings) -> tauri::plugin::TauriPlugin<R> {
    const MAX_LOG_SIZE: u128 = 5 * 1024 * 1024;
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .target(tauri_plugin_log::Target::new(
            tauri_plugin_log::TargetKind::Folder {
                path: settings::app_dir(),
                file_name: Some(settings::LOG_FILE_STEM.to_string()),
            },
        ))
        .max_file_size(MAX_LOG_SIZE)
        .rotation_strategy(tauri_plugin_log::RotationStrategy::KeepOne)
        .level(settings.log_level_filter())
        .build()
}

fn persist_on_close(window: &tauri::Window) {
    log::trace!("window close requested — persisting notebook and settings");
    let state = window.state::<AppState>();
    if let Ok(mut guard) = state.store.lock() {
        if let Some(store) = guard.take() {
            store.purge_deleted_files();
            let _ = assets::prune(&store);
            let _ = store.save();
        }
    }
    let settings_lock = state.settings.lock();
    if let Ok(mut s) = settings_lock {
        if !window.is_minimized().unwrap_or(false) {
            let maximized = window.is_maximized().unwrap_or(false);
            if let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) {
                if size.width > 0 && size.height > 0 {
                    s.window = Some(settings::WindowGeom {
                        x: pos.x,
                        y: pos.y,
                        width: size.width,
                        height: size.height,
                        maximized,
                    });
                }
            }
        }
        s.save();
    }
}

fn serve_asset(
    app: &tauri::AppHandle,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let path = percent_decode_str(request.uri().path())
        .decode_utf8_lossy()
        .to_string();
    let rel = path.trim_start_matches('/');
    if rel.is_empty() || rel.contains("..") || rel.contains('\\') {
        return not_found();
    }
    let root = app
        .state::<AppState>()
        .store
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|s| s.root.clone()));
    let Some(root) = root else {
        return not_found();
    };
    let file = root.join("assets").join(rel);
    log::trace!("serving asset {rel}");
    match std::fs::read(&file) {
        Ok(bytes) => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", content_type(&file))
            .header("Access-Control-Allow-Origin", "*")
            .body(bytes)
            .unwrap_or_else(|_| not_found()),
        Err(e) => {
            log::warn!("asset {rel} not readable: {e}");
            not_found()
        }
    }
}

fn not_found() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .body(Vec::new())
        .expect("static response")
}

fn content_type(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        _ => "application/octet-stream",
    }
}
