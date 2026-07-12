use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;

use crate::assets;
use crate::import_mht::{self, FilePreview, ImportOutcome};
use crate::search::{self, SearchHit};
use crate::settings::{self, Settings};
use crate::store::{Notebook, PageNode, Section, Store, UndoOutcome};

pub struct AppState {
    pub store: Mutex<Option<Store>>,
    pub settings: Mutex<Settings>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NotebookInfo {
    pub root: String,
    pub notebook: Notebook,
}

fn lock_err<T>(_: T) -> String {
    "state lock poisoned".to_string()
}

fn with_store<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Store) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.store.lock().map_err(lock_err)?;
    let store = guard.as_mut().ok_or_else(|| "no notebook open".to_string())?;
    f(store)
}

fn close_current(guard: &mut Option<Store>) {
    if let Some(old) = guard.take() {
        old.purge_deleted_files();
        let _ = assets::prune(&old);
        let _ = old.save();
    }
}

fn adopt_store(
    state: &State<'_, AppState>,
    guard: &mut Option<Store>,
    store: Store,
) -> Result<NotebookInfo, String> {
    let root = store.root.to_string_lossy().to_string();
    {
        let mut s = state.settings.lock().map_err(lock_err)?;
        s.notebook_path = Some(root.clone());
        s.remember_notebook(&root);
        s.save();
    }
    let info = NotebookInfo {
        root,
        notebook: store.notebook.clone(),
    };
    *guard = Some(store);
    Ok(info)
}

fn notebook_root_from(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path);
    if p.is_file() {
        if p.file_name().is_some_and(|n| n.eq_ignore_ascii_case("notebook.json")) {
            return Ok(p.parent().map(Into::into).unwrap_or(p));
        }
        return Err("not a notebook: select its notebook.json file".to_string());
    }
    Ok(p)
}

#[tauri::command]
pub fn open_notebook(state: State<'_, AppState>, path: Option<String>) -> Result<NotebookInfo, String> {
    let explicit = path.is_some();
    let root: PathBuf = match &path {
        Some(p) => notebook_root_from(p)?,
        None => {
            let s = state.settings.lock().map_err(lock_err)?;
            s.notebook_path
                .clone()
                .map(PathBuf::from)
                .unwrap_or_else(settings::default_notebook_dir)
        }
    };
    let mut guard = state.store.lock().map_err(lock_err)?;
    close_current(&mut guard);
    let store = match Store::open(&root) {
        Ok(store) => store,
        Err(e) => {
            let fallback = settings::default_notebook_dir();
            if explicit || root == fallback {
                return Err(e);
            }
            Store::open(&fallback)?
        }
    };
    adopt_store(&state, &mut guard, store)
}

#[tauri::command]
pub fn create_notebook(state: State<'_, AppState>, path: String) -> Result<NotebookInfo, String> {
    let root = PathBuf::from(&path);
    if root.join("notebook.json").exists() {
        return Err("that folder already contains a notebook — use Open instead".to_string());
    }
    let mut guard = state.store.lock().map_err(lock_err)?;
    close_current(&mut guard);
    let store = Store::open(&root)?;
    adopt_store(&state, &mut guard, store)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RecentNotebook {
    pub path: String,
    pub name: String,
    pub exists: bool,
}

#[tauri::command]
pub fn list_recent_notebooks(state: State<'_, AppState>) -> Result<Vec<RecentNotebook>, String> {
    let s = state.settings.lock().map_err(lock_err)?;
    Ok(s.recent_notebooks
        .iter()
        .map(|p| {
            let root = PathBuf::from(p);
            RecentNotebook {
                path: p.clone(),
                name: root
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.clone()),
                exists: root.join("notebook.json").is_file(),
            }
        })
        .collect())
}

#[tauri::command]
pub fn get_tree(state: State<'_, AppState>) -> Result<Notebook, String> {
    with_store(&state, |s| Ok(s.notebook.clone()))
}

#[tauri::command]
pub fn create_section(state: State<'_, AppState>, name: String) -> Result<Section, String> {
    with_store(&state, |s| s.create_section(&name))
}

#[tauri::command]
pub fn rename_section(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    with_store(&state, |s| s.rename_section(&id, &name))
}

#[tauri::command]
pub fn move_section(state: State<'_, AppState>, id: String, index: usize) -> Result<(), String> {
    with_store(&state, |s| s.move_section(&id, index))
}

#[tauri::command]
pub fn delete_section(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_store(&state, |s| s.delete_section(&id))
}

#[tauri::command]
pub fn create_page(
    state: State<'_, AppState>,
    section_id: String,
    parent_id: Option<String>,
    after_id: Option<String>,
) -> Result<PageNode, String> {
    with_store(&state, |s| {
        s.create_page(&section_id, parent_id.as_deref(), after_id.as_deref())
    })
}

#[tauri::command]
pub fn read_page(state: State<'_, AppState>, id: String) -> Result<String, String> {
    with_store(&state, |s| s.read_page(&id))
}

#[tauri::command]
pub fn write_page(state: State<'_, AppState>, id: String, content: String) -> Result<String, String> {
    with_store(&state, |s| s.write_page(&id, &content))
}

#[tauri::command]
pub fn rename_page(state: State<'_, AppState>, id: String, title: String) -> Result<(), String> {
    with_store(&state, |s| s.rename_page(&id, &title))
}

#[tauri::command]
pub fn delete_page(state: State<'_, AppState>, id: String) -> Result<(), String> {
    with_store(&state, |s| s.delete_page(&id))
}

#[tauri::command]
pub fn move_page(
    state: State<'_, AppState>,
    id: String,
    to_section: String,
    parent_id: Option<String>,
    index: usize,
) -> Result<(), String> {
    with_store(&state, |s| {
        s.move_page(&id, &to_section, parent_id.as_deref(), index)
    })
}

#[tauri::command]
pub fn undo(state: State<'_, AppState>) -> Result<Option<UndoOutcome>, String> {
    with_store(&state, |s| s.undo())
}

#[tauri::command]
pub fn redo(state: State<'_, AppState>) -> Result<Option<UndoOutcome>, String> {
    with_store(&state, |s| s.redo())
}

#[tauri::command]
pub fn set_expanded(state: State<'_, AppState>, id: String, expanded: bool) -> Result<(), String> {
    with_store(&state, |s| s.set_expanded(&id, expanded))
}

#[tauri::command]
pub fn set_last_view(
    state: State<'_, AppState>,
    section_id: Option<String>,
    page_id: Option<String>,
) -> Result<(), String> {
    with_store(&state, |s| s.set_last_view(section_id, page_id))
}

#[tauri::command]
pub fn search_pages(
    state: State<'_, AppState>,
    query: String,
    mode: String,
) -> Result<Vec<SearchHit>, String> {
    with_store(&state, |s| search::search(s, &query, &mode))
}

#[tauri::command]
pub fn save_image(
    state: State<'_, AppState>,
    page_id: String,
    data: String,
    ext: String,
) -> Result<String, String> {
    with_store(&state, |s| assets::save_image(s, &page_id, &data, &ext))
}

#[tauri::command]
pub fn inspect_mht(state: State<'_, AppState>, paths: Vec<String>) -> Result<Vec<FilePreview>, String> {
    with_store(&state, |s| Ok(import_mht::inspect(s, &paths)))
}

#[tauri::command]
pub fn import_mht(state: State<'_, AppState>, paths: Vec<String>) -> Result<ImportOutcome, String> {
    with_store(&state, |s| import_mht::import(s, &paths))
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state.settings.lock().map_err(lock_err).map(|s| s.clone())
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<(), String> {
    let mut guard = state.settings.lock().map_err(lock_err)?;
    let window = guard.window.clone();
    // window geometry and the recents MRU are backend-owned; ignore stale copies
    let recents = std::mem::take(&mut guard.recent_notebooks);
    *guard = settings;
    guard.recent_notebooks = recents;
    if guard.window.is_none() {
        guard.window = window;
    }
    guard.save();
    Ok(())
}
