mod assets;
mod commands;
mod files;
mod git;
mod history;
mod hotkey;
mod import;
mod import_md;
mod import_mht;
mod lock;
mod search;
mod settings;
mod startup;
mod store;
mod tray;

use commands::AppState;
use percent_encoding::percent_decode_str;
use settings::Settings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

pub(crate) fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

const TICK: Duration = Duration::from_secs(60);
const IDLE_GRACE_MS: u64 = 30_000;
const FAILURE_BACKOFF: Duration = Duration::from_secs(60);

const ASSET_SCHEME: &str = "note-asset";

pub fn run() {
    let settings = Settings::load();
    tauri::Builder::default()
        .plugin(build_log_plugin(&settings))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![startup::HIDDEN_ARG]),
        ))
        .plugin(hotkey::plugin())
        .plugin(navigation_guard())
        .manage(AppState {
            store: Mutex::new(None),
            settings: Mutex::new(settings),
            closing: AtomicBool::new(false),
            quitting: AtomicBool::new(false),
            git_gate: Mutex::new(()),
            git_stop: Mutex::new(None),
            history_gate: Mutex::new(()),
        })
        .register_uri_scheme_protocol(ASSET_SCHEME, |ctx, request| {
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
            commands::get_view_positions,
            commands::set_view_positions,
            commands::search_pages,
            commands::save_image,
            commands::attach_file,
            commands::attach_file_bytes,
            commands::reveal_attachment,
            commands::trash_stats,
            commands::reveal_trash,
            commands::empty_trash,
            commands::inspect_mht,
            commands::import_mht,
            commands::inspect_md,
            commands::import_md,
            commands::reset_agents_md,
            commands::get_settings,
            commands::set_settings,
            commands::get_git_status,
            commands::set_git_snapshots,
            commands::page_revisions,
            commands::revision_text,
            commands::deleted_pages,
            commands::deleted_page_text,
            commands::restore_deleted_page,
            commands::restore_revision_assets
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

            let (geom, tray_enabled, start_on_login) = {
                let state = app.state::<AppState>();
                let s = state.settings.lock().map_err(commands::lock_err)?;
                (s.window.clone(), s.minimize_to_tray, s.start_on_login)
            };
            if let Some(g) = geom.filter(|g| g.width > 0 && g.height > 0) {
                let _ = win.set_position(tauri::PhysicalPosition::new(g.x, g.y));
                let _ = win.set_size(tauri::PhysicalSize::new(g.width, g.height));
                if g.maximized {
                    let _ = win.maximize();
                }
            }
            tray::sync(app.handle(), tray_enabled);
            startup::sync(app.handle(), start_on_login);
            hotkey::sync(app.handle());
            if !(tray_enabled && startup::launched_hidden()) {
                let _ = win.show();
            }

            let tx = spawn_git_ticker(app.handle().clone());
            if let Ok(mut stop) = app.state::<AppState>().git_stop.lock() {
                *stop = Some(tx);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if hide_to_tray(window, &state) {
                    api.prevent_close();
                    return;
                }
                if !state.closing.swap(true, Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.emit("mynote:flush-and-close", ());
                    let watchdog = window.clone();
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(1500));
                        let _ = watchdog.close();
                    });
                    return;
                }
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
                "tauri" => true,
                s if s == ASSET_SCHEME => true,
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
    if let Ok(mut stop) = state.git_stop.lock() {
        if let Some(tx) = stop.take() {
            let _ = tx.send(());
        }
    }
    let closed = state.store.lock().ok().and_then(|mut guard| guard.take());
    if let Some(store) = closed {
        commands::close_and_commit(&state, window.app_handle(), store);
    }
    let Ok(mut settings) = state.settings.lock() else {
        return;
    };
    if window.is_visible().unwrap_or(true) {
        capture_geometry(window, &mut settings);
    }
    settings.save();
}

/// Closing with `minimizeToTray` on parks the app in the tray instead of
/// quitting; only the tray's Quit item, which sets `quitting`, gets past this.
fn hide_to_tray(window: &tauri::Window, state: &tauri::State<'_, AppState>) -> bool {
    if state.quitting.load(Ordering::SeqCst) || state.closing.load(Ordering::SeqCst) {
        return false;
    }
    let Ok(mut settings) = state.settings.lock() else {
        return false;
    };
    if !settings.minimize_to_tray || window.app_handle().tray_by_id(tray::ID).is_none() {
        return false;
    }
    log::trace!("close requested — hiding to tray");
    capture_geometry(window, &mut settings);
    settings.save();
    drop(settings);
    let _ = window.hide();
    let _ = window.emit("mynote:flush", ());
    true
}

fn capture_geometry(window: &tauri::Window, settings: &mut Settings) {
    if window.is_minimized().unwrap_or(false) {
        return;
    }
    let maximized = window.is_maximized().unwrap_or(false);
    let (Ok(pos), Ok(size)) = (window.outer_position(), window.inner_size()) else {
        return;
    };
    if size.width > 0 && size.height > 0 {
        settings.window = Some(settings::WindowGeom {
            x: pos.x,
            y: pos.y,
            width: size.width,
            height: size.height,
            maximized,
        });
    }
}

fn serve_asset(
    app: &tauri::AppHandle,
    request: &tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Vec<u8>> {
    let path = percent_decode_str(request.uri().path()).decode_utf8_lossy();
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
    let file = root.join(store::ASSETS_DIR).join(rel);
    log::trace!("serving asset {rel}");
    match std::fs::read(&file) {
        Ok(bytes) => tauri::http::Response::builder()
            .status(200)
            .header("Content-Type", assets::content_type(&file))
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

fn spawn_git_ticker(app: tauri::AppHandle) -> mpsc::Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    let spawned = std::thread::Builder::new()
        .name("mynote-git".into())
        .stack_size(64 * 1024)
        .spawn(move || git_ticker_loop(app, rx));
    if let Err(e) = spawned {
        log::warn!("failed to spawn git ticker thread: {e}");
    }
    tx
}

fn git_ticker_loop(app: tauri::AppHandle, rx: mpsc::Receiver<()>) {
    let mut backoff_until = Instant::now();
    loop {
        match rx.recv_timeout(TICK) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            Err(mpsc::RecvTimeoutError::Timeout) => git_tick(&app, &mut backoff_until),
        }
    }
}

/// Must never hold `store` and `git_gate` at the same time, or this can
/// deadlock against the close path.
fn git_tick(app: &tauri::AppHandle, backoff_until: &mut Instant) {
    if Instant::now() < *backoff_until {
        return;
    }
    let state = app.state::<AppState>();

    let (root, session, seq) = {
        let Ok(guard) = state.store.lock() else { return };
        let Some(store) = guard.as_ref() else { return };
        if !snapshot_is_due(
            &store.notebook.git,
            store.change_seq(),
            store.committed_seq(),
            store.idle_ms(),
            store.since_commit_ms(),
        ) {
            return;
        }
        (store.root.clone(), store.session(), store.change_seq())
    };

    if !git::available() || !git::is_repo(&root) {
        return;
    }

    let outcome = {
        let Ok(_gate) = state.git_gate.try_lock() else {
            return;
        };
        git::snapshot(&root, git::SnapshotKind::Idle)
    };

    match outcome {
        Ok(_) => mark_committed_if_unchanged(&state, session, seq),
        Err(e) => {
            log::warn!("git idle snapshot failed: {e}");
            commands::emit_git_snapshot_failed(app);
            *backoff_until = Instant::now() + FAILURE_BACKOFF;
        }
    }
}

fn snapshot_is_due(
    cfg: &store::GitConfig,
    seq: u64,
    committed: u64,
    idle_ms: u64,
    since_commit_ms: u64,
) -> bool {
    cfg.enabled
        && seq != committed
        && idle_ms >= IDLE_GRACE_MS
        && since_commit_ms >= cfg.interval_secs.saturating_mul(1000)
}

/// The snapshot ran without the store lock, so anything edited or opened in
/// the meantime must not be credited to this commit.
fn mark_committed_if_unchanged(state: &AppState, session: u64, seq: u64) {
    let Ok(guard) = state.store.lock() else { return };
    let Some(store) = guard.as_ref() else { return };
    if store.session() == session && store.change_seq() == seq {
        store.mark_committed(seq);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use store::GitConfig;

    fn cfg(enabled: bool, interval_secs: u64) -> GitConfig {
        GitConfig {
            enabled,
            interval_secs,
        }
    }

    #[test]
    fn snapshot_is_due_matrix() {
        assert!(!snapshot_is_due(&cfg(false, 3600), 5, 0, 1_000_000, 1_000_000));
        assert!(!snapshot_is_due(&cfg(true, 3600), 5, 5, 1_000_000, 1_000_000));
        assert!(!snapshot_is_due(&cfg(true, 3600), 5, 0, 1_000, 3_600_000));
        assert!(!snapshot_is_due(&cfg(true, 3600), 5, 0, 60_000, 10_000));
        assert!(snapshot_is_due(&cfg(true, 3600), 5, 0, 60_000, 3_600_001));
        assert!(!snapshot_is_due(&cfg(true, 0), 5, 0, 1_000, 1_000_000));
        assert!(snapshot_is_due(&cfg(true, 0), 5, 0, 30_000, 0));
    }
}
