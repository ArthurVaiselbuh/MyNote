use super::{extract_title, flatten_pages, Notebook, Store, StoredNotebook, NOTEBOOK_FILE};
use crate::err;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct ExternalFiles {
    notebook_disk: RefCell<Option<Vec<u8>>>,
    watched_page: RefCell<Option<(String, Vec<u8>)>>,
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

    pub fn watched_page_path(&self) -> Option<PathBuf> {
        self.external_files
            .watched_page
            .borrow()
            .as_ref()
            .map(|(id, _)| self.page_path(id))
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
        let incoming_notebook = if reload && notebook_changed {
            let bytes = fs::read(self.root.join(NOTEBOOK_FILE)).map_err(err)?;
            let mut notebook: Notebook = serde_json::from_slice(&bytes).map_err(err)?;
            notebook.last_view = self.notebook.last_view.clone();
            Some((notebook, bytes))
        } else {
            None
        };
        let page_removed = incoming_notebook.as_ref().is_some_and(|(notebook, _)| {
            self.external_files
                .watched_page
                .borrow()
                .as_ref()
                .is_some_and(|(id, _)| {
                    !flatten_pages(notebook)
                        .iter()
                        .any(|(_, page)| &page.id == id)
                })
        });
        let incoming_page = if reload && !page_removed {
            page_changed
                .as_ref()
                .map(|id| self.read_page(id))
                .transpose()?
        } else {
            None
        };
        if let Some((notebook, bytes)) = incoming_notebook {
            self.notebook = notebook;
            *self.external_files.notebook_disk.borrow_mut() = Some(bytes);
            *self.last_saved_json.borrow_mut() =
                serde_json::to_string_pretty(&StoredNotebook::from(&self.notebook)).map_err(err)?;
            self.undo_stack.clear();
            self.redo_stack.clear();
            self.session_deleted.clear();
        } else if notebook_changed {
            self.last_saved_json.borrow_mut().clear();
            self.save()?;
        }
        if page_removed {
            self.stop_watching_page();
        } else if let Some(id) = page_changed.as_ref() {
            if reload {
                let text = incoming_page.unwrap();
                self.remember_page_write(id, &text);
                if let Some(title) = extract_title(&text) {
                    if let Some(node) = self.find_page_mut(id) {
                        node.title = title;
                    }
                    self.save()?;
                }
            } else {
                let text = content
                    .or_else(|| {
                        self.external_files
                            .watched_page
                            .borrow()
                            .as_ref()
                            .and_then(|(_, bytes)| String::from_utf8(bytes.clone()).ok())
                    })
                    .ok_or("No loaded page content")?;
                self.write_page(id, &text)?;
            }
        }
        Ok(page_changed)
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
