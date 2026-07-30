use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{mpsc, Mutex};
use std::time::Duration;
use tauri::{Emitter, State};

use crate::assets;
use crate::git::{self, SnapshotKind};
use crate::history;
use crate::import::{ImportOutcome, ImportPreview};
use crate::{import_md, import_mht};
use crate::search::{self, SearchResults};
use crate::settings::{self, Settings};
use crate::store::{CloseInfo, Notebook, OpenError, PageNode, Section, Store, UndoOutcome};

const OPEN_SNAPSHOT_DEADLINE: Duration = Duration::from_secs(20);
const CLOSE_SNAPSHOT_DEADLINE: Duration = Duration::from_secs(5);

pub struct AppState {
    pub store: Mutex<Option<Store>>,
    pub settings: Mutex<Settings>,
    pub closing: AtomicBool,
    pub git_gate: Mutex<()>,
    /// Signals the idle-snapshot ticker thread to stop.
    pub git_stop: Mutex<Option<mpsc::Sender<()>>>,
    /// Serializes history subprocess spawns — separate from `git_gate`,
    /// which serializes *writes* (index.lock). History reads
    /// (log/cat-file/ls-tree) never touch the index, so they never wait on a
    /// snapshot in flight; this only stops a key-mashed history pane from
    /// piling up processes. Taken with a blocking `lock()`: every holder is
    /// bounded by the git deadlines in `git.rs`, so waiters queue briefly
    /// instead of surfacing spurious "busy" errors. Never held together with
    /// the store lock (see the history section below), so it cannot deadlock.
    pub history_gate: Mutex<()>,
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

pub(crate) const GIT_SNAPSHOT_FAILED_EVENT: &str = "mynote:git-snapshot-failed";

/// Deliberately free of paths and git's own message — the details stay in
/// MyNote.log, which the notice points at.
pub(crate) fn emit_git_snapshot_failed(app: &tauri::AppHandle) {
    let _ = app.emit(
        GIT_SNAPSHOT_FAILED_EVENT,
        "git snapshot failed — see MyNote.log",
    );
}

fn with_store<T>(
    state: &State<'_, AppState>,
    f: impl FnOnce(&mut Store) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.store.lock().map_err(lock_err)?;
    let store = guard.as_mut().ok_or_else(|| "no notebook open".to_string())?;
    f(store)
}

fn close_current(state: &State<'_, AppState>, app: &tauri::AppHandle, guard: &mut Option<Store>) {
    let Some(store) = guard.take() else { return };
    let info = store.close();
    commit_on_close(state, app, &info);
}

pub(crate) fn commit_on_close(
    state: &State<'_, AppState>,
    app: &tauri::AppHandle,
    info: &CloseInfo,
) {
    if !info.git_enabled || !git::available() || !git::is_repo(&info.root) {
        return;
    }
    let Ok(_gate) = state.git_gate.try_lock() else {
        return;
    };
    if let Err(e) = git::snapshot(&info.root, SnapshotKind::Close, CLOSE_SNAPSHOT_DEADLINE) {
        log::warn!("git close snapshot failed: {e}");
        emit_git_snapshot_failed(app);
    }
}

fn commit_on_open(state: &State<'_, AppState>, app: &tauri::AppHandle, info: &NotebookInfo) {
    if !info.notebook.git.enabled || !git::available() {
        return;
    }
    let root = PathBuf::from(&info.root);
    let Ok(_gate) = state.git_gate.try_lock() else {
        return;
    };
    if let Err(e) = git::ensure_repo(&root) {
        log::warn!("git ensure_repo failed: {e}");
        emit_git_snapshot_failed(app);
        return;
    }
    if let Err(e) = git::snapshot(&root, SnapshotKind::Open, OPEN_SNAPSHOT_DEADLINE) {
        log::warn!("git open snapshot failed: {e}");
        emit_git_snapshot_failed(app);
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

// Git-touching commands are `async fn` on purpose: sync commands run inline in
// the WebView2 resource-request callback on the main thread, so a git
// subprocess (deadlines up to 20s) would freeze the whole UI. `async fn` moves
// the body onto tauri's async runtime pool; the bodies stay synchronous and
// never hold a mutex guard across an await (there are none).

#[tauri::command]
pub async fn open_notebook(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    path: Option<String>,
) -> Result<NotebookInfo, String> {
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
    close_current(&state, &app, &mut guard);
    let store = match Store::open(&root) {
        Ok(store) => store,
        // A lock conflict means the notebook itself is fine — just in use
        // elsewhere — so silently falling back to a different notebook would
        // hide that from the user instead of explaining it.
        Err(OpenError::Locked) => return Err(OpenError::Locked.into()),
        Err(OpenError::Other(e)) => {
            let fallback = settings::default_notebook_dir();
            if explicit || root == fallback {
                return Err(e);
            }
            Store::open(&fallback)?
        }
    };
    log::info!("opened notebook");
    let info = adopt_store(&state, &mut guard, store)?;
    drop(guard);
    commit_on_open(&state, &app, &info);
    Ok(info)
}

#[tauri::command]
pub async fn create_notebook(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    path: String,
) -> Result<NotebookInfo, String> {
    let root = PathBuf::from(&path);
    if root.join("notebook.json").exists() {
        return Err("that folder already contains a notebook — use Open instead".to_string());
    }
    let mut guard = state.store.lock().map_err(lock_err)?;
    close_current(&state, &app, &mut guard);
    let store = Store::open(&root)?;
    log::info!("created notebook");
    let info = adopt_store(&state, &mut guard, store)?;
    drop(guard);
    commit_on_open(&state, &app, &info);
    Ok(info)
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
    log::info!("create section");
    with_store(&state, |s| s.create_section(&name))
}

#[tauri::command]
pub fn rename_section(state: State<'_, AppState>, id: String, name: String) -> Result<(), String> {
    log::info!("rename section {id}");
    with_store(&state, |s| s.rename_section(&id, &name))
}

#[tauri::command]
pub fn move_section(state: State<'_, AppState>, id: String, index: usize) -> Result<(), String> {
    log::info!("move section {id} to index {index}");
    with_store(&state, |s| s.move_section(&id, index))
}

#[tauri::command]
pub fn delete_section(state: State<'_, AppState>, id: String) -> Result<(), String> {
    log::info!("delete section {id}");
    with_store(&state, |s| s.delete_section(&id))
}

#[tauri::command]
pub fn create_page(
    state: State<'_, AppState>,
    section_id: String,
    parent_id: Option<String>,
    after_id: Option<String>,
) -> Result<PageNode, String> {
    log::info!("create page in section {section_id} (parent {parent_id:?})");
    with_store(&state, |s| {
        s.create_page(&section_id, parent_id.as_deref(), after_id.as_deref())
    })
}

#[tauri::command]
pub fn read_page(state: State<'_, AppState>, id: String) -> Result<String, String> {
    log::trace!("read page {id}");
    with_store(&state, |s| s.read_page(&id))
}

#[tauri::command]
pub fn write_page(state: State<'_, AppState>, id: String, content: String) -> Result<String, String> {
    log::trace!("write page {id} ({} bytes)", content.len());
    with_store(&state, |s| s.write_page(&id, &content))
}

#[tauri::command]
pub fn rename_page(state: State<'_, AppState>, id: String, title: String) -> Result<(), String> {
    log::info!("rename page {id}");
    with_store(&state, |s| s.rename_page(&id, &title))
}

#[tauri::command]
pub fn delete_page(state: State<'_, AppState>, id: String) -> Result<(), String> {
    log::info!("delete page {id}");
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
    log::info!("move page {id} to section {to_section} (parent {parent_id:?}, index {index})");
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
    log::trace!("set expanded {id} = {expanded}");
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
) -> Result<SearchResults, String> {
    log::trace!("search ({mode}), {} chars", query.chars().count());
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
pub fn inspect_mht(state: State<'_, AppState>, paths: Vec<String>) -> Result<ImportPreview, String> {
    with_store(&state, |s| Ok(import_mht::inspect(s, &paths)))
}

#[tauri::command]
pub fn import_mht(state: State<'_, AppState>, paths: Vec<String>) -> Result<ImportOutcome, String> {
    log::info!("import {} OneNote file(s)", paths.len());
    with_store(&state, |s| import_mht::import(s, &paths))
}

#[tauri::command]
pub fn inspect_md(state: State<'_, AppState>, root: String) -> Result<ImportPreview, String> {
    with_store(&state, |s| import_md::inspect(s, &root))
}

#[tauri::command]
pub fn import_md(state: State<'_, AppState>, root: String) -> Result<ImportOutcome, String> {
    log::info!("import markdown folder");
    with_store(&state, |s| import_md::import(s, &root))
}

#[tauri::command]
pub fn reset_agents_md(state: State<'_, AppState>) -> Result<(), String> {
    log::info!("overwrite AGENTS.md with embedded template");
    with_store(&state, |s| s.write_agents_template())
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub available: bool,
    pub enabled: bool,
    pub repo: bool,
    pub interval_secs: u64,
}

fn git_status(store: &Store) -> GitStatus {
    GitStatus {
        available: git::available(),
        enabled: store.notebook.git.enabled,
        repo: git::is_repo(&store.root),
        interval_secs: store.notebook.git.interval_secs,
    }
}

#[tauri::command]
pub fn get_git_status(state: State<'_, AppState>) -> Result<GitStatus, String> {
    with_store(&state, |s| Ok(git_status(s)))
}

#[tauri::command]
pub async fn set_git_snapshots(
    state: State<'_, AppState>,
    app: tauri::AppHandle,
    enabled: bool,
    interval_secs: Option<u64>,
) -> Result<GitStatus, String> {
    let root = {
        let mut guard = state.store.lock().map_err(lock_err)?;
        let store = guard.as_mut().ok_or_else(|| "no notebook open".to_string())?;
        store.notebook.git.enabled = enabled;
        if let Some(secs) = interval_secs {
            store.notebook.git.interval_secs = secs.max(60);
        }
        store.save()?;
        log::info!(
            "git snapshots {} for this notebook",
            if enabled { "enabled" } else { "disabled" }
        );
        store.root.clone()
    }; // guard dropped here — ensure_repo/snapshot must never run while it's held
    if enabled && git::available() {
        if let Ok(_gate) = state.git_gate.try_lock() {
            match git::ensure_repo(&root) {
                Ok(()) => {
                    if let Err(e) = git::snapshot(&root, SnapshotKind::Open, OPEN_SNAPSHOT_DEADLINE) {
                        log::warn!("git snapshot failed: {e}");
                        emit_git_snapshot_failed(&app);
                    }
                }
                Err(e) => {
                    log::warn!("git ensure_repo failed: {e}");
                    emit_git_snapshot_failed(&app);
                }
            }
        }
    }
    with_store(&state, |s| Ok(git_status(s)))
}

// ---------------------------------------------------------------------------
// History — page revisions + deleted-page recovery. Every command clones the
// `root`/`notebook` it needs and drops the store lock *before* running any
// git subprocess, so browsing history never blocks autosave, typing, or tree
// edits on other windows onto the same store mutex. Reads queue on
// `history_gate` (serializing process spawns, not correctness); the
// writer phase of a restore takes the store lock again, never alongside
// `history_gate`.

#[tauri::command]
pub async fn page_revisions(state: State<'_, AppState>, id: String) -> Result<Vec<history::PageRevision>, String> {
    log::trace!("history: list page revisions");
    let root = with_store(&state, |s| Ok(s.root.clone()))?;
    let _gate = state.history_gate.lock().map_err(lock_err)?;
    history::page_revisions(&root, &id)
}

#[tauri::command]
pub async fn revision_text(state: State<'_, AppState>, id: String, sha: String) -> Result<history::RevisionText, String> {
    log::trace!("history: read a page revision");
    let root = with_store(&state, |s| Ok(s.root.clone()))?;
    let _gate = state.history_gate.lock().map_err(lock_err)?;
    history::revision_text(&root, &id, &sha)
}

#[tauri::command]
pub async fn deleted_pages(state: State<'_, AppState>) -> Result<history::DeletedHistory, String> {
    log::trace!("history: list deleted pages");
    let (root, notebook) = with_store(&state, |s| Ok((s.root.clone(), s.notebook.clone())))?;
    let _gate = state.history_gate.lock().map_err(lock_err)?;
    history::deleted_pages(&root, &notebook)
}

#[tauri::command]
pub async fn deleted_page_text(state: State<'_, AppState>, sha: String, id: String) -> Result<history::RevisionText, String> {
    log::trace!("history: read a deleted page");
    let root = with_store(&state, |s| Ok(s.root.clone()))?;
    let _gate = state.history_gate.lock().map_err(lock_err)?;
    history::deleted_page_text(&root, &sha, &id)
}

#[tauri::command]
pub async fn restore_deleted_page(
    state: State<'_, AppState>,
    sha: String,
    id: String,
    fallback_section_id: Option<String>,
) -> Result<history::RestoreOutcome, String> {
    let (root, notebook) = with_store(&state, |s| Ok((s.root.clone(), s.notebook.clone())))?;
    let recovered = {
        let _gate = state.history_gate.lock().map_err(lock_err)?;
        history::recover(&root, &notebook, &sha, &id, fallback_section_id.as_deref())?
    }; // gate dropped here — insert_restored (a write) must not run under it

    let page_count = recovered.page_count;
    let asset_count = recovered.asset_count;
    let requested_id = recovered.page.id.clone();
    let section_id = recovered.section_id.clone();
    let written = with_store(&state, |s| {
        s.insert_restored(&recovered.section_id, recovered.parent_id.as_deref(), vec![recovered.page])
    })?;
    let page = written.into_iter().next().ok_or("nothing was restored")?;
    log::info!("restored {page_count} page(s), {asset_count} asset(s) from history");
    Ok(history::RestoreOutcome {
        section_id,
        page_id: page.id.clone(),
        page_count,
        asset_count,
        renamed: page.id != requested_id,
    })
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
    log::info!("settings updated (log level: {})", guard.log_level);
    guard.save();
    Ok(())
}
