use crate::commands::{lock_err, AppState};
use crate::store::{Notebook, Store};
use std::path::Path;
use tauri::State;

pub(crate) fn watch_notebook(state: &State<'_, AppState>, root: &Path) -> Result<(), String> {
    let mut guard = state.file_watcher.lock().map_err(lock_err)?;
    guard
        .as_mut()
        .ok_or("file watcher unavailable")?
        .watch_notebook(root)
}

#[tauri::command]
pub fn check_external_changes(
    state: State<'_, AppState>,
) -> Result<(bool, Option<String>), String> {
    let guard = state.store.lock().map_err(lock_err)?;
    Ok(guard
        .as_ref()
        .map(Store::external_changes)
        .unwrap_or((false, None)))
}

#[tauri::command]
pub fn resolve_external_changes(
    state: State<'_, AppState>,
    reload: bool,
    content: Option<String>,
) -> Result<(Notebook, Option<String>), String> {
    let mut guard = state.store.lock().map_err(lock_err)?;
    let store = guard.as_mut().ok_or("no notebook open")?;
    let changed_page = store.resolve_external_changes(reload, content)?;
    let notebook = store.notebook.clone();
    let page = store.watched_page_path();
    drop(guard);
    if let Some(watcher) = state.file_watcher.lock().map_err(lock_err)?.as_ref() {
        watcher.watch_page(page);
    }
    Ok((notebook, changed_page))
}

#[tauri::command]
pub fn read_open_page(state: State<'_, AppState>, id: Option<String>) -> Result<String, String> {
    let mut guard = state.store.lock().map_err(lock_err)?;
    let store = guard.as_mut().ok_or("no notebook open")?;
    let content = match id {
        Some(id) => store.read_open_page(&id)?,
        None => {
            store.stop_watching_page();
            String::new()
        }
    };
    let page = store.watched_page_path();
    drop(guard);
    if let Some(watcher) = state.file_watcher.lock().map_err(lock_err)?.as_ref() {
        watcher.watch_page(page);
    }
    Ok(content)
}
