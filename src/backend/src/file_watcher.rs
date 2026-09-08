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

pub struct DirectoryWatcher {
    watcher: RecommendedWatcher,
    watched_root: Option<PathBuf>,
}

impl DirectoryWatcher {
    fn new(
        mut on_event: impl FnMut(notify::Result<Event>) + Send + 'static,
    ) -> Result<Self, String> {
        let watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            match &result {
                Ok(event)
                    if !event.need_rescan()
                        && matches!(event.kind, notify::EventKind::Access(_)) =>
                {
                    return;
                }
                Err(_) => log::warn!("directory watcher failed"),
                _ => {}
            }
            on_event(result);
        })
        .map_err(crate::err)?;
        Ok(Self {
            watcher,
            watched_root: None,
        })
    }

    fn watch(&mut self, root: &Path) -> Result<(), String> {
        if self.watched_root.as_deref() == Some(root) {
            return Ok(());
        }
        self.watcher
            .watch(root, RecursiveMode::NonRecursive)
            .map_err(crate::err)?;
        if let Some(previous) = &self.watched_root {
            if let Err(error) = self.watcher.unwatch(previous) {
                let _ = self.watcher.unwatch(root);
                return Err(crate::err(error));
            }
        }
        self.watched_root = Some(root.to_path_buf());
        Ok(())
    }
}

pub fn watch_directory_and_scan(
    root: &Path,
    scan_directory: impl FnMut() + Send + 'static,
) -> Result<DirectoryWatcher, String> {
    let scan_directory = Arc::new(Mutex::new(scan_directory));
    let event_callback = Arc::clone(&scan_directory);
    let mut watcher = DirectoryWatcher::new(move |result| {
        if result.is_ok() {
            if let Ok(mut callback) = event_callback.lock() {
                callback();
            }
        }
    })?;
    watcher.watch(root)?;
    scan_directory.lock().map_err(crate::err)?();
    Ok(watcher)
}

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
        event.need_rescan()
            || event
                .paths
                .iter()
                .any(|path| path == &notebook || self.page.as_ref() == Some(path))
    }
}

pub struct FileWatcher {
    watcher: DirectoryWatcher,
    scope: Arc<Mutex<WatchScope>>,
}

impl FileWatcher {
    pub fn new(app: tauri::AppHandle) -> Result<Self, String> {
        let scope = Arc::new(Mutex::new(WatchScope::default()));
        let callback_scope = Arc::clone(&scope);
        let (changed_tx, changed_rx) = mpsc::channel();
        let callback_app = app.clone();
        let watcher = DirectoryWatcher::new(move |result| match result {
            Ok(event) => {
                let generation = callback_scope
                    .lock()
                    .ok()
                    .and_then(|scope| scope.accepts(&event).then_some(scope.generation));
                if let Some(generation) = generation {
                    let _ = changed_tx.send(generation);
                }
            }
            Err(_) => {
                let _ = callback_app.emit(FAILED_EVENT, "file watching failed — see MyNote.log");
            }
        })?;
        let worker_scope = Arc::clone(&scope);
        std::thread::Builder::new()
            .name("mynote-file-events".into())
            .spawn(move || emit_coalesced_changes(app, worker_scope, changed_rx))
            .map_err(|error| error.to_string())?;
        Ok(Self { watcher, scope })
    }

    pub fn watch_notebook(&mut self, root: &Path) -> Result<(), String> {
        let root = std::fs::canonicalize(root).map_err(|error| error.to_string())?;
        if self.watcher.watched_root.as_ref() == Some(&root) {
            return Ok(());
        }
        self.watcher.watch(&root)?;
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
    use std::time::Instant;

    fn wait_for_file_event(events: &mpsc::Receiver<Event>, file: &Path) {
        let file = std::fs::canonicalize(file).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let event = events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap_or_else(|error| {
                    panic!("waiting for file event for {}: {error}", file.display())
                });
            if event
                .paths
                .iter()
                .any(|path| std::fs::canonicalize(path).is_ok_and(|path| path == file))
            {
                return;
            }
        }
    }

    #[test]
    fn directory_watcher_switches_roots_and_preserves_watch_on_failure() {
        let root = tempfile::tempdir().unwrap();
        let first = root.path().join("first");
        let second = root.path().join("second");
        std::fs::create_dir(&first).unwrap();
        let (event_tx, event_rx) = mpsc::channel();
        let mut watcher = DirectoryWatcher::new(move |result| {
            if let Ok(event) = result {
                let _ = event_tx.send(event);
            }
        })
        .unwrap();
        watcher.watch(&first).unwrap();
        assert!(watcher.watch(&second).is_err());
        let first_file = first.join("still-watched");
        std::fs::write(&first_file, []).unwrap();
        wait_for_file_event(&event_rx, &first_file);

        std::fs::create_dir(&second).unwrap();
        watcher.watch(&second).unwrap();
        let second_file = second.join("now-watched");
        std::fs::write(&second_file, []).unwrap();
        wait_for_file_event(&event_rx, &second_file);
    }

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
        assert!(scope
            .accepts(&Event::new(notify::EventKind::Other).set_flag(notify::event::Flag::Rescan)));
    }
}
