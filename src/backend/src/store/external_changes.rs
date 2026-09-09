use super::{
    atomic_write, extract_title, flatten_pages, Notebook, Store, StoredNotebook, UndoOp,
    UndoOutcome, NOTEBOOK_FILE,
};
use crate::err;
use std::cell::RefCell;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

#[derive(Default, Clone)]
pub(super) struct ExternalFiles {
    notebook_disk: RefCell<Option<Vec<u8>>>,
    watched_page: RefCell<Option<(String, Vec<u8>)>>,
}

pub(super) struct ReloadState {
    notebook: Option<Notebook>,
    page: Option<PageSnapshot>,
    selected_page: Option<String>,
}

struct PageSnapshot {
    id: String,
    content: Option<String>,
}

struct PreparedReload {
    previous: ReloadState,
    incoming: ReloadState,
    notebook_bytes: Option<Vec<u8>>,
}

struct FileSnapshot {
    path: PathBuf,
    bytes: Option<Vec<u8>>,
}

struct ReloadBackup {
    notebook: Notebook,
    external_files: ExternalFiles,
    last_saved_json: String,
    files: Vec<FileSnapshot>,
}

fn read_optional_file(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(err(error)),
    }
}

impl Store {
    pub fn read_open_page(&self, id: &str) -> Result<String, String> {
        let content = self.read_page(id)?;
        *self.external_files.watched_page.borrow_mut() =
            Some((id.to_string(), content.as_bytes().to_vec()));
        Ok(content)
    }

    pub fn stop_watching_page(&self) {
        *self.external_files.watched_page.borrow_mut() = None;
    }

    fn watched_page_id(&self) -> Option<String> {
        self.external_files
            .watched_page
            .borrow()
            .as_ref()
            .map(|(id, _)| id.clone())
    }

    pub fn watched_page_path(&self) -> Option<PathBuf> {
        self.watched_page_id().map(|id| self.page_path(&id))
    }

    pub fn external_changes(&self) -> (bool, Option<String>) {
        let notebook = fs::read(self.root.join(NOTEBOOK_FILE)).ok()
            != *self.external_files.notebook_disk.borrow();
        let page = self
            .external_files
            .watched_page
            .borrow()
            .as_ref()
            .and_then(|(id, bytes)| {
                (fs::read(self.page_path(id)).ok().as_ref() != Some(bytes)).then(|| id.clone())
            });
        (notebook, page)
    }

    pub fn ensure_no_external_changes(&self) -> Result<(), String> {
        let (notebook, page) = self.external_changes();
        if notebook || page.is_some() {
            return Err(
                "Files changed externally. Choose Reload or Overwrite before saving.".into(),
            );
        }
        Ok(())
    }

    pub fn resolve_external_changes(
        &mut self,
        reload: bool,
        content: Option<String>,
    ) -> Result<Option<String>, String> {
        let (notebook_changed, page_changed) = self.external_changes();
        if !notebook_changed && page_changed.is_none() {
            return Ok(None);
        }
        if reload {
            let prepared =
                self.prepare_reload(notebook_changed, page_changed.as_deref(), content)?;
            self.run_reload_transaction(None, |store| store.accept_reload(&prepared))?;
            self.record_undo(UndoOp::ReloadExternal {
                target: Box::new(prepared.previous),
            });
        } else {
            let page = match page_changed.as_ref() {
                Some(id) => Some(PageSnapshot {
                    id: id.clone(),
                    content: Some(self.loaded_page_content(content)?),
                }),
                None => None,
            };
            self.run_reload_transaction(page_changed.as_deref(), |store| {
                if notebook_changed {
                    store.last_saved_json.borrow_mut().clear();
                }
                if let Some(page) = &page {
                    store.restore_page_snapshot(page)?;
                }
                store.save()
            })?;
        }
        Ok(page_changed)
    }

    fn loaded_page_content(&self, content: Option<String>) -> Result<String, String> {
        match content {
            Some(content) => Ok(content),
            None => self
                .external_files
                .watched_page
                .borrow()
                .as_ref()
                .map(|(_, bytes)| String::from_utf8(bytes.clone()).map_err(err))
                .ok_or_else(|| "No loaded page content".to_string())?,
        }
    }

    fn prepare_reload(
        &self,
        notebook_changed: bool,
        changed_page: Option<&str>,
        content: Option<String>,
    ) -> Result<PreparedReload, String> {
        let selected_page = self.watched_page_id();
        let notebook_bytes = if notebook_changed {
            Some(fs::read(self.root.join(NOTEBOOK_FILE)).map_err(err)?)
        } else {
            None
        };
        let notebook = notebook_bytes
            .as_ref()
            .map(|bytes| serde_json::from_slice::<Notebook>(bytes).map_err(err))
            .transpose()?;
        let page_removed = notebook.as_ref().is_some_and(|notebook| {
            selected_page.as_ref().is_some_and(|id| {
                !flatten_pages(notebook)
                    .iter()
                    .any(|(_, page)| &page.id == id)
            })
        });
        let page_scope =
            changed_page.or_else(|| page_removed.then_some(selected_page.as_deref()).flatten());
        let previous_page = page_scope
            .map(|id| {
                self.loaded_page_content(content)
                    .map(|content| PageSnapshot {
                        id: id.into(),
                        content: Some(content),
                    })
            })
            .transpose()?;
        let incoming_page = page_scope
            .map(|id| {
                let content = if page_removed {
                    None
                } else {
                    Some(self.read_page(id)?)
                };
                Ok::<_, String>(PageSnapshot {
                    id: id.into(),
                    content,
                })
            })
            .transpose()?;
        Ok(PreparedReload {
            previous: ReloadState {
                notebook: notebook_changed.then(|| self.notebook.clone()),
                page: previous_page,
                selected_page: selected_page.clone(),
            },
            incoming: ReloadState {
                notebook,
                page: incoming_page,
                selected_page,
            },
            notebook_bytes,
        })
    }

    fn accept_reload(&mut self, prepared: &PreparedReload) -> Result<(), String> {
        if let Some(notebook) = &prepared.incoming.notebook {
            let last_view = self.notebook.last_view.clone();
            self.notebook = notebook.clone();
            self.notebook.last_view = last_view;
            *self.external_files.notebook_disk.borrow_mut() = prepared.notebook_bytes.clone();
            *self.last_saved_json.borrow_mut() =
                serde_json::to_string_pretty(&StoredNotebook::from(&self.notebook)).map_err(err)?;
        }
        if let Some(page) = &prepared.incoming.page {
            if let Some(content) = &page.content {
                self.remember_page_write(&page.id, content);
                self.sync_page_title(&page.id, content);
            } else {
                self.stop_watching_page();
            }
        }
        self.save()
    }

    fn sync_page_title(&mut self, id: &str, content: &str) {
        if let Some(title) = extract_title(content) {
            if let Some(page) = self.find_page_mut(id) {
                page.title = title;
            }
        }
    }

    fn restore_page_snapshot(&mut self, page: &PageSnapshot) -> Result<(), String> {
        if let Some(content) = &page.content {
            if self.find_page(&page.id).is_none() {
                return Err("Cannot restore content for a page missing from the notebook".into());
            }
            atomic_write(&self.page_path(&page.id), content.as_bytes())?;
            self.remember_page_write(&page.id, content);
            self.sync_page_title(&page.id, content);
            self.touch();
        }
        Ok(())
    }

    fn apply_reload_state(
        &mut self,
        state: &ReloadState,
        label: &str,
    ) -> Result<UndoOutcome, String> {
        if let Some(notebook) = &state.notebook {
            let last_view = self.notebook.last_view.clone();
            self.notebook = notebook.clone();
            self.notebook.last_view = last_view;
            self.sync_titles_from_pages();
        }
        if let Some(page) = &state.page {
            self.restore_page_snapshot(page)?;
        }
        let page_id = state
            .selected_page
            .as_ref()
            .filter(|id| self.find_page(id).is_some())
            .cloned();
        if self
            .watched_page_id()
            .is_some_and(|id| self.find_page(&id).is_none())
        {
            self.stop_watching_page();
        }
        self.save()?;
        Ok(UndoOutcome {
            label: label.into(),
            reload_page: true,
            section_id: page_id.as_ref().and_then(|id| self.section_of(id)),
            page_id,
        })
    }

    fn capture_reload_state(&self, scope: &ReloadState) -> Result<ReloadState, String> {
        let page = scope
            .page
            .as_ref()
            .map(|page| {
                let content = self
                    .find_page(&page.id)
                    .map(|_| self.read_page(&page.id))
                    .transpose()?;
                Ok::<_, String>(PageSnapshot {
                    id: page.id.clone(),
                    content,
                })
            })
            .transpose()?;
        Ok(ReloadState {
            notebook: scope.notebook.as_ref().map(|_| self.notebook.clone()),
            page,
            selected_page: self.watched_page_id(),
        })
    }

    fn sync_titles_from_pages(&mut self) {
        let ids: Vec<String> = flatten_pages(&self.notebook)
            .iter()
            .map(|(_, page)| page.id.clone())
            .collect();
        for id in ids {
            if let Ok(content) = self.read_page(&id) {
                self.sync_page_title(&id, &content);
            }
        }
    }

    pub(super) fn exchange_reload_state(
        &mut self,
        target: &ReloadState,
        label: &str,
    ) -> Result<(ReloadState, UndoOutcome), String> {
        let inverse = self.capture_reload_state(target)?;
        let outcome = self
            .run_reload_transaction(target.page.as_ref().map(|page| page.id.as_str()), |store| {
                store.apply_reload_state(target, label)
            })?;
        Ok((inverse, outcome))
    }

    fn run_reload_transaction<T>(
        &mut self,
        page_id: Option<&str>,
        update: impl FnOnce(&mut Self) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut paths = vec![self.root.join(NOTEBOOK_FILE)];
        if let Some(id) = page_id {
            paths.push(self.page_path(id));
        }
        let files = paths
            .into_iter()
            .map(|path| read_optional_file(&path).map(|bytes| FileSnapshot { path, bytes }))
            .collect::<Result<_, _>>()?;
        let backup = ReloadBackup {
            notebook: self.notebook.clone(),
            external_files: self.external_files.clone(),
            last_saved_json: self.last_saved_json.borrow().clone(),
            files,
        };
        match update(self) {
            Ok(result) => Ok(result),
            Err(error) => match self.restore_reload_backup(backup) {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!(
                    "{error}; could not roll back reload: {rollback_error}"
                )),
            },
        }
    }

    fn restore_reload_backup(&mut self, backup: ReloadBackup) -> Result<(), String> {
        self.notebook = backup.notebook;
        self.external_files = backup.external_files;
        *self.last_saved_json.borrow_mut() = backup.last_saved_json;
        let mut errors = Vec::new();
        for file in backup.files {
            if read_optional_file(&file.path).ok().as_ref() == Some(&file.bytes) {
                continue;
            }
            let result = match file.bytes {
                Some(bytes) => atomic_write(&file.path, &bytes),
                None => fs::remove_file(&file.path).map_err(err),
            };
            if let Err(error) = result {
                errors.push(error);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    pub(super) fn remember_notebook_write(&self, bytes: &[u8]) {
        *self.external_files.notebook_disk.borrow_mut() = Some(bytes.to_vec());
    }

    pub(super) fn remember_page_write(&self, id: &str, content: &str) {
        if let Some((watched_id, bytes)) = self.external_files.watched_page.borrow_mut().as_mut() {
            if watched_id == id {
                *bytes = content.as_bytes().to_vec();
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{atomic_write, new_id, parse_notebook, OpenError};

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn external_page_reload_undo_redo_restores_content_and_title() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.write_page(&page.id, "# Local\n\nOriginal").unwrap();
        store.read_open_page(&page.id).unwrap();
        fs::write(store.page_path(&page.id), "# External\n\nChanged").unwrap();
        store.resolve_external_changes(true, None).unwrap();
        let outcome = store.undo().unwrap().unwrap();
        assert!(outcome.reload_page);
        assert_eq!(outcome.page_id, Some(page.id.clone()));
        assert_eq!(store.read_page(&page.id).unwrap(), "# Local\n\nOriginal");
        assert_eq!(store.find_page(&page.id).unwrap().title, "Local");
        assert_eq!(store.external_changes(), (false, None));
        store.redo().unwrap().unwrap();
        assert_eq!(store.read_page(&page.id).unwrap(), "# External\n\nChanged");
        assert_eq!(store.find_page(&page.id).unwrap().title, "External");
        assert_eq!(store.external_changes(), (false, None));
    }

    #[test]
    fn external_page_redo_captures_edits_made_after_reload() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.write_page(&page.id, "# Local\n\nOriginal").unwrap();
        store.read_open_page(&page.id).unwrap();
        fs::write(store.page_path(&page.id), "# External\n\nChanged").unwrap();
        store.resolve_external_changes(true, None).unwrap();
        store
            .write_page(&page.id, "# External\n\nLater edits")
            .unwrap();
        store.undo().unwrap().unwrap();
        assert_eq!(store.read_page(&page.id).unwrap(), "# Local\n\nOriginal");
        store.redo().unwrap().unwrap();
        assert_eq!(
            store.read_page(&page.id).unwrap(),
            "# External\n\nLater edits"
        );
        store.undo().unwrap().unwrap();
        store
            .write_page(&page.id, "# Local\n\nEdits after undo")
            .unwrap();
        store.redo().unwrap().unwrap();
        assert_eq!(
            store.read_page(&page.id).unwrap(),
            "# External\n\nLater edits"
        );
        store.undo().unwrap().unwrap();
        assert_eq!(
            store.read_page(&page.id).unwrap(),
            "# Local\n\nEdits after undo"
        );
    }

    #[test]
    fn metadata_reload_keeps_titles_from_page_files_when_undoing() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.write_page(&page.id, "# File title\n\nBody").unwrap();
        store.read_open_page(&page.id).unwrap();
        let mut external = store.notebook.clone();
        external.sections[0].name = "External section".into();
        external.sections[0].pages[0].title = "Stale cached title".into();
        fs::write(
            store.root.join(NOTEBOOK_FILE),
            serde_json::to_vec(&external).unwrap(),
        )
        .unwrap();
        store.resolve_external_changes(true, None).unwrap();
        store.rename_page(&page.id, "Later title").unwrap();
        store.undo().unwrap().unwrap();
        assert_eq!(store.find_page(&page.id).unwrap().title, "Later title");
        store.redo().unwrap().unwrap();
        assert_eq!(store.find_page(&page.id).unwrap().title, "Later title");
    }

    #[test]
    fn notebook_reload_preserves_existing_undo_history() {
        let (_dir, mut store) = open_store();
        let original = store.notebook.sections[0].name.clone();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.delete_page(&page.id).unwrap();
        let mut notebook = store.notebook.clone();
        notebook.sections[0].name = "External section".into();
        fs::write(
            store.root.join(NOTEBOOK_FILE),
            serde_json::to_vec(&notebook).unwrap(),
        )
        .unwrap();
        store.resolve_external_changes(true, None).unwrap();
        store.undo().unwrap().unwrap();
        assert_eq!(store.notebook.sections[0].name, original);
        assert!(store.find_page(&page.id).is_none());
        store.undo().unwrap().unwrap();
        assert!(store.find_page(&page.id).is_some());
        store.redo().unwrap().unwrap();
        store.redo().unwrap().unwrap();
        assert_eq!(store.notebook.sections[0].name, "External section");
        assert!(store.find_page(&page.id).is_none());
        assert_eq!(store.external_changes(), (false, None));
    }

    #[test]
    fn undo_reload_recovers_an_externally_removed_open_page() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.write_page(&page.id, "# Local\n\nSaved").unwrap();
        store.read_open_page(&page.id).unwrap();
        let mut notebook = store.notebook.clone();
        notebook.sections[0].pages.clear();
        fs::write(
            store.root.join(NOTEBOOK_FILE),
            serde_json::to_vec(&notebook).unwrap(),
        )
        .unwrap();
        fs::remove_file(store.page_path(&page.id)).unwrap();
        store.resolve_external_changes(true, None).unwrap();
        store.undo().unwrap().unwrap();
        assert!(store.find_page(&page.id).is_some());
        assert_eq!(store.read_open_page(&page.id).unwrap(), "# Local\n\nSaved");
        store
            .write_page(&page.id, "# Recovered\n\nMore edits")
            .unwrap();
        store.redo().unwrap().unwrap();
        assert!(store.find_page(&page.id).is_none());
        assert!(store.page_path(&page.id).exists());
        assert_eq!(store.external_changes(), (false, None));
        store.undo().unwrap().unwrap();
        assert_eq!(
            store.read_page(&page.id).unwrap(),
            "# Recovered\n\nMore edits"
        );
    }

    #[test]
    fn notebook_history_captures_creates_and_section_names_on_each_exchange() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let original_name = store.notebook.sections[0].name.clone();
        let mut external = store.notebook.clone();
        external.sections[0].name = "External".into();
        fs::write(
            store.root.join(NOTEBOOK_FILE),
            serde_json::to_vec(&external).unwrap(),
        )
        .unwrap();
        store.resolve_external_changes(true, None).unwrap();
        let created = store.create_page(&section, None, None).unwrap();
        store.rename_section(&section, "Later name").unwrap();
        store.read_open_page(&created.id).unwrap();
        store.undo().unwrap().unwrap();
        assert!(store.find_page(&created.id).is_none());
        assert_eq!(store.notebook.sections[0].name, original_name);
        assert!(store.page_path(&created.id).exists());
        let second = store.create_page(&section, None, None).unwrap();
        let outcome = store.redo().unwrap().unwrap();
        assert_eq!(outcome.page_id, Some(created.id.clone()));
        assert!(store.find_page(&created.id).is_some());
        assert!(store.find_page(&second.id).is_none());
        assert_eq!(store.notebook.sections[0].name, "Later name");
        store.undo().unwrap().unwrap();
        assert!(store.find_page(&second.id).is_some());
        assert!(store.find_page(&created.id).is_none());
    }

    fn open_reloaded_page() -> (tempfile::TempDir, Store, String) {
        let (dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.write_page(&page.id, "# Local").unwrap();
        store.read_open_page(&page.id).unwrap();
        fs::write(store.page_path(&page.id), "# External").unwrap();
        store.resolve_external_changes(true, None).unwrap();
        (dir, store, page.id)
    }

    #[test]
    fn failed_page_restore_keeps_history_available_for_retry() {
        let (_dir, mut store, id) = open_reloaded_page();
        let blocked_write = store.page_path(&id).with_extension("tmp");
        fs::create_dir(&blocked_write).unwrap();
        assert!(store.undo().is_err());
        assert_eq!(store.read_page(&id).unwrap(), "# External");
        assert_eq!(store.find_page(&id).unwrap().title, "External");
        assert!(store.redo().unwrap().is_none());
        assert_eq!(store.external_changes(), (false, None));
        fs::remove_dir(blocked_write).unwrap();
        store.undo().unwrap().unwrap();
        assert_eq!(store.read_page(&id).unwrap(), "# Local");
    }

    #[test]
    fn failed_notebook_save_rolls_back_page_content_and_preserves_both_stacks() {
        let (_dir, mut store, id) = open_reloaded_page();
        let notebook_before = fs::read(store.root.join(NOTEBOOK_FILE)).unwrap();
        let blocked_write = store.root.join(NOTEBOOK_FILE).with_extension("tmp");
        fs::create_dir(&blocked_write).unwrap();
        assert!(store.undo().is_err());
        assert_eq!(store.read_page(&id).unwrap(), "# External");
        assert_eq!(store.find_page(&id).unwrap().title, "External");
        assert_eq!(
            fs::read(store.root.join(NOTEBOOK_FILE)).unwrap(),
            notebook_before
        );
        assert_eq!(store.external_changes(), (false, None));
        assert!(store.redo().unwrap().is_none());
        fs::remove_dir(&blocked_write).unwrap();
        store.undo().unwrap().unwrap();
        fs::create_dir(&blocked_write).unwrap();
        assert!(store.redo().is_err());
        assert_eq!(store.read_page(&id).unwrap(), "# Local");
        assert!(store.undo().unwrap().is_none());
        fs::remove_dir(blocked_write).unwrap();
        store.redo().unwrap().unwrap();
        assert_eq!(store.read_page(&id).unwrap(), "# External");
    }

    #[test]
    fn failed_initial_reload_keeps_external_files_pending() {
        let (_dir, mut store, id) = open_reloaded_page();
        fs::write(store.page_path(&id), "# Another external title").unwrap();
        let blocked_write = store.root.join(NOTEBOOK_FILE).with_extension("tmp");
        fs::create_dir(&blocked_write).unwrap();
        assert!(store.resolve_external_changes(true, None).is_err());
        assert_eq!(store.find_page(&id).unwrap().title, "External");
        assert_eq!(store.read_page(&id).unwrap(), "# Another external title");
        assert_eq!(store.external_changes(), (false, Some(id.clone())));
        fs::remove_dir(blocked_write).unwrap();
        store.resolve_external_changes(true, None).unwrap();
        store.undo().unwrap().unwrap();
        assert_eq!(store.read_page(&id).unwrap(), "# External");
        store.undo().unwrap().unwrap();
        assert_eq!(store.read_page(&id).unwrap(), "# Local");
    }

    #[test]
    fn undo_manual_reload_restores_unsaved_buffer() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.read_open_page(&page.id).unwrap();
        fs::write(store.page_path(&page.id), "# External").unwrap();
        store
            .resolve_external_changes(true, Some("# Local\n\nUnsaved".into()))
            .unwrap();
        store.undo().unwrap().unwrap();
        assert_eq!(store.read_page(&page.id).unwrap(), "# Local\n\nUnsaved");
    }

    #[test]
    fn watcher_ignores_own_writes_and_detects_replaced_and_deleted_pages() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.read_open_page(&page.id).unwrap();
        store.write_page(&page.id, "# Local\n\nBody").unwrap();
        store.rename_page(&page.id, "Renamed").unwrap();
        assert_eq!(store.external_changes(), (false, None));
        atomic_write(&store.page_path(&page.id), b"# External\n\nChanged").unwrap();
        assert_eq!(store.external_changes(), (false, Some(page.id.clone())));
        assert!(store.ensure_no_external_changes().is_err());
        store.resolve_external_changes(true, None).unwrap();
        assert_eq!(store.find_page(&page.id).unwrap().title, "External");
        assert_eq!(store.external_changes(), (false, None));
        fs::remove_file(store.page_path(&page.id)).unwrap();
        assert!(store.resolve_external_changes(true, None).is_err());
        store
            .resolve_external_changes(false, Some("# Recovered\n\nUnsaved".into()))
            .unwrap();
        assert_eq!(store.read_page(&page.id).unwrap(), "# Recovered\n\nUnsaved");
        assert_eq!(store.external_changes(), (false, None));
    }

    #[test]
    fn watcher_reloads_metadata_without_releasing_the_notebook_lock() {
        let (_dir, mut store) = open_store();
        let mut notebook = store.notebook.clone();
        notebook.sections[0].name = "External section".into();
        fs::write(
            store.root.join(NOTEBOOK_FILE),
            serde_json::to_vec(&notebook).unwrap(),
        )
        .unwrap();
        assert_eq!(store.external_changes(), (true, None));
        store.resolve_external_changes(true, None).unwrap();
        assert_eq!(store.notebook.sections[0].name, "External section");
        assert!(matches!(Store::open(&store.root), Err(OpenError::Locked)));
        assert_eq!(store.external_changes(), (false, None));
        fs::write(store.root.join(NOTEBOOK_FILE), b"invalid").unwrap();
        assert!(store.resolve_external_changes(true, None).is_err());
        assert_eq!(store.notebook.sections[0].name, "External section");
        store.resolve_external_changes(false, None).unwrap();
        assert_eq!(
            parse_notebook(&store.root.join(NOTEBOOK_FILE))
                .unwrap()
                .sections[0]
                .name,
            "External section"
        );
        assert_eq!(store.external_changes(), (false, None));
    }

    #[test]
    fn watcher_only_tracks_the_open_page_and_close_preserves_external_additions() {
        let (dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let first = store.create_page(&section, None, None).unwrap();
        let second = store.create_page(&section, None, None).unwrap();
        store.read_open_page(&first.id).unwrap();
        store.read_open_page(&second.id).unwrap();
        fs::write(store.page_path(&first.id), b"# Unwatched").unwrap();
        assert_eq!(store.external_changes(), (false, None));
        let added = new_id();
        fs::write(store.page_path(&added), b"# External addition").unwrap();
        fs::write(store.root.join(NOTEBOOK_FILE), b"external metadata").unwrap();
        store.close();
        assert!(dir.path().join(format!("{added}.md")).exists());
        assert_eq!(
            fs::read(dir.path().join(NOTEBOOK_FILE)).unwrap(),
            b"external metadata"
        );
    }

    #[test]
    fn watcher_reloads_a_notebook_that_removes_the_open_page_file() {
        let (_dir, mut store) = open_store();
        let section = store.notebook.sections[0].id.clone();
        let page = store.create_page(&section, None, None).unwrap();
        store.read_open_page(&page.id).unwrap();
        let mut notebook = store.notebook.clone();
        notebook.sections[0].pages.clear();
        fs::write(
            store.root.join(NOTEBOOK_FILE),
            serde_json::to_vec(&notebook).unwrap(),
        )
        .unwrap();
        fs::remove_file(store.page_path(&page.id)).unwrap();
        store.resolve_external_changes(true, None).unwrap();
        assert!(store.notebook.sections[0].pages.is_empty());
        assert_eq!(store.external_changes(), (false, None));
    }
}
