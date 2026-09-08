pub mod commands;

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;
use tauri::Emitter;

use crate::store::NOTEBOOK_FILE;

pub const CHANGED_EVENT: &str = "mynote:files-changed";
pub const FAILED_EVENT: &str = "mynote:file-watch-failed";
const COALESCE: Duration = Duration::from_millis(100);

#[derive(Default)]
struct WatchScope {
    generation: u64,
    root: Option<PathBuf>,
    page: Option<PathBuf>,
}

impl WatchScope {
    fn accepts(&self, event: &Event) -> bool {
        let Some(root) = &self.root else { return false };
        let notebook = root.join(NOTEBOOK_FILE);
        event
            .paths
            .iter()
            .any(|path| path == &notebook || self.page.as_ref() == Some(path))
    }
}

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    scope: Arc<Mutex<WatchScope>>,
    watched_root: Option<PathBuf>,
}

impl FileWatcher {
    pub fn new(app: tauri::AppHandle) -> Result<Self, String> {
        let scope = Arc::new(Mutex::new(WatchScope::default()));
        let callback_scope = Arc::clone(&scope);
        let (changed_tx, changed_rx) = mpsc::channel();
        let callback_app = app.clone();
        let watcher =
            notify::recommended_watcher(move |result: notify::Result<Event>| match result {
                Ok(event) => {
                    let generation = callback_scope
                        .lock()
                        .ok()
                        .and_then(|scope| scope.accepts(&event).then_some(scope.generation));
                    if let Some(generation) = generation {
                        let _ = changed_tx.send(generation);
                    }
                }
                Err(error) => {
                    log::warn!("file watcher failed: {error}");
                    let _ =
                        callback_app.emit(FAILED_EVENT, "file watching failed — see MyNote.log");
                }
            })
            .map_err(|error| error.to_string())?;
        let worker_scope = Arc::clone(&scope);
        std::thread::Builder::new()
            .name("mynote-file-events".into())
            .spawn(move || emit_coalesced_changes(app, worker_scope, changed_rx))
            .map_err(|error| error.to_string())?;
        Ok(Self {
            watcher,
            scope,
            watched_root: None,
        })
    }

    pub fn watch_notebook(&mut self, root: &Path) -> Result<(), String> {
        let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
        if self.watched_root.as_ref() == Some(&root) {
            return Ok(());
        }
        self.watcher
            .watch(&root, RecursiveMode::NonRecursive)
            .map_err(|error| error.to_string())?;
        if let Some(previous) = self.watched_root.take() {
            if let Err(error) = self.watcher.unwatch(&previous) {
                let _ = self.watcher.unwatch(&root);
                self.watched_root = Some(previous);
                return Err(error.to_string());
            }
        }
        self.watched_root = Some(root.clone());
        if let Ok(mut scope) = self.scope.lock() {
            scope.generation = scope.generation.wrapping_add(1);
            scope.root = Some(root);
            scope.page = None;
        }
        Ok(())
    }

    pub fn watch_page(&self, page: Option<PathBuf>) {
        if let Ok(mut scope) = self.scope.lock() {
            scope.generation = scope.generation.wrapping_add(1);
            scope.page = page.map(|path| std::fs::canonicalize(&path).unwrap_or(path));
        }
    }
}

fn emit_coalesced_changes(
    app: tauri::AppHandle,
    scope: Arc<Mutex<WatchScope>>,
    changed_rx: mpsc::Receiver<u64>,
) {
    while let Ok(mut generation) = changed_rx.recv() {
        while let Ok(next) = changed_rx.recv_timeout(COALESCE) {
            generation = next;
        }
        let current = scope
            .lock()
            .ok()
            .is_some_and(|scope| scope.generation == generation);
        if current {
            let _ = app.emit(CHANGED_EVENT, ());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_accepts_only_the_notebook_and_open_page() {
        let root = PathBuf::from("notebook");
        let page = root.join("page.md");
        let scope = WatchScope {
            generation: 1,
            root: Some(root.clone()),
            page: Some(page.clone()),
        };
        assert!(
            scope.accepts(&Event::new(notify::EventKind::Any).add_path(root.join(NOTEBOOK_FILE)))
        );
        assert!(scope.accepts(&Event::new(notify::EventKind::Any).add_path(page)));
        assert!(!scope.accepts(&Event::new(notify::EventKind::Any).add_path(root.join("other.md"))));
    }
}
