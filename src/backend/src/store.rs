use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct PageNode {
    pub id: String,
    pub title: String,
    #[serde(default = "default_true")]
    pub expanded: bool,
    #[serde(default)]
    pub children: Vec<PageNode>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub pages: Vec<PageNode>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct LastView {
    #[serde(default)]
    pub section_id: Option<String>,
    #[serde(default)]
    pub page_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub sections: Vec<Section>,
    #[serde(default)]
    pub last_view: LastView,
}

enum UndoOp {
    DeletePage {
        node: PageNode,
        section_id: String,
        parent_id: Option<String>,
        index: usize,
    },
    DeleteSection {
        section: Section,
        index: usize,
        default_added: Option<String>,
    },
    MovePage {
        id: String,
        from_section: String,
        from_parent: Option<String>,
        from_index: usize,
        to_section: String,
        to_parent: Option<String>,
        to_index: usize,
    },
}

impl UndoOp {
    fn kind(&self) -> &'static str {
        match self {
            UndoOp::DeletePage { .. } => "delete page",
            UndoOp::DeleteSection { .. } => "delete section",
            UndoOp::MovePage { .. } => "move page",
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UndoOutcome {
    pub label: String,
    pub section_id: Option<String>,
    pub page_id: Option<String>,
}

const UNDO_LIMIT: usize = 100;

pub struct Store {
    pub root: PathBuf,
    pub notebook: Notebook,
    undo_stack: Vec<UndoOp>,
    redo_stack: Vec<UndoOp>,
    session_deleted: HashSet<String>,
}

impl Store {
    pub fn open(root: &Path) -> Result<Store, String> {
        fs::create_dir_all(root).map_err(err)?;
        let file = root.join("notebook.json");
        let notebook: Notebook = if file.exists() {
            let text = fs::read_to_string(&file).map_err(err)?;
            serde_json::from_str(&text).map_err(err)?
        } else {
            Notebook {
                sections: vec![],
                last_view: LastView::default(),
            }
        };
        let mut store = Store {
            root: root.to_path_buf(),
            notebook,
            undo_stack: vec![],
            redo_stack: vec![],
            session_deleted: HashSet::new(),
        };
        if store.notebook.sections.is_empty() {
            store.notebook.sections.push(Section {
                id: new_id(),
                name: "Notes".into(),
                pages: vec![],
            });
        }
        store.save()?;
        Ok(store)
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&self.notebook).map_err(err)?;
        fs::write(self.root.join("notebook.json"), json).map_err(err)?;
        Ok(())
    }

    pub fn page_path(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    pub fn create_section(&mut self, name: &str) -> Result<Section, String> {
        let section = Section {
            id: new_id(),
            name: non_empty(name, "New Section"),
            pages: vec![],
        };
        self.notebook.sections.push(section.clone());
        self.save()?;
        Ok(section)
    }

    pub fn rename_section(&mut self, id: &str, name: &str) -> Result<(), String> {
        let section = self
            .notebook
            .sections
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or("section not found")?;
        section.name = non_empty(name, "Untitled Section");
        self.save()
    }

    pub fn move_section(&mut self, id: &str, index: usize) -> Result<(), String> {
        let from = self
            .notebook
            .sections
            .iter()
            .position(|s| s.id == id)
            .ok_or("section not found")?;
        let section = self.notebook.sections.remove(from);
        let idx = index.min(self.notebook.sections.len());
        self.notebook.sections.insert(idx, section);
        self.save()
    }

    pub fn delete_section(&mut self, id: &str) -> Result<(), String> {
        let op = self.apply_delete_section(id)?;
        self.record_undo(op);
        self.save()
    }

    fn apply_delete_section(&mut self, id: &str) -> Result<UndoOp, String> {
        let index = self
            .notebook
            .sections
            .iter()
            .position(|s| s.id == id)
            .ok_or("section not found")?;
        let section = self.notebook.sections.remove(index);
        for page in &section.pages {
            let mut ids = Vec::new();
            collect_ids(page, &mut ids);
            self.session_deleted.extend(ids);
        }
        let mut default_added = None;
        if self.notebook.sections.is_empty() {
            let default = Section {
                id: new_id(),
                name: "Notes".into(),
                pages: vec![],
            };
            default_added = Some(default.id.clone());
            self.notebook.sections.push(default);
        }
        Ok(UndoOp::DeleteSection {
            section,
            index,
            default_added,
        })
    }

    fn remove_page_files(&self, id: &str) {
        let _ = fs::remove_file(self.page_path(id));
        let assets = self.root.join("assets").join(id);
        if assets.is_dir() {
            let _ = fs::remove_dir_all(assets);
        }
    }

    pub fn create_page(
        &mut self,
        section_id: &str,
        parent_id: Option<&str>,
        after_id: Option<&str>,
    ) -> Result<PageNode, String> {
        let id = new_id();
        let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M");
        fs::write(
            self.page_path(&id),
            format!("# Untitled\n\n_{stamp}_\n\n"),
        )
        .map_err(err)?;
        let node = PageNode {
            id: id.clone(),
            title: "Untitled".into(),
            expanded: true,
            children: vec![],
        };
        {
            let list = self.pages_list_mut(section_id, parent_id)?;
            let idx = after_id
                .and_then(|a| list.iter().position(|n| n.id == a).map(|i| i + 1))
                .unwrap_or(list.len());
            list.insert(idx, node.clone());
        }
        if let Some(pid) = parent_id {
            if let Some(parent) = self.find_page_mut(pid) {
                parent.expanded = true;
            }
        }
        self.save()?;
        Ok(node)
    }

    pub fn read_page(&self, id: &str) -> Result<String, String> {
        fs::read_to_string(self.page_path(id)).map_err(err)
    }

    pub fn write_page(&mut self, id: &str, content: &str) -> Result<String, String> {
        let existing_title = self
            .find_page(id)
            .ok_or("page not found")?
            .title
            .clone();
        fs::write(self.page_path(id), content).map_err(err)?;
        let title = extract_title(content).unwrap_or(existing_title);
        if let Some(node) = self.find_page_mut(id) {
            node.title = title.clone();
        }
        self.save()?;
        Ok(title)
    }

    pub fn rename_page(&mut self, id: &str, title: &str) -> Result<(), String> {
        let title = non_empty(title, "Untitled");
        let content = self.read_page(id).unwrap_or_default();
        fs::write(self.page_path(id), set_title(&content, &title)).map_err(err)?;
        let node = self.find_page_mut(id).ok_or("page not found")?;
        node.title = title;
        self.save()
    }

    pub fn delete_page(&mut self, id: &str) -> Result<(), String> {
        let op = self.apply_delete_page(id)?;
        self.record_undo(op);
        self.save()
    }

    fn apply_delete_page(&mut self, id: &str) -> Result<UndoOp, String> {
        let (section_id, parent_id, index) = self.locate_page(id).ok_or("page not found")?;
        let node = self.detach_page(id).ok_or("page not found")?;
        let mut ids = Vec::new();
        collect_ids(&node, &mut ids);
        self.session_deleted.extend(ids);
        if self.notebook.last_view.page_id.as_deref() == Some(id) {
            self.notebook.last_view.page_id = None;
        }
        Ok(UndoOp::DeletePage {
            node,
            section_id,
            parent_id,
            index,
        })
    }

    /// `index` is the position in the destination list counted after the page
    /// has been detached from its old spot.
    pub fn move_page(
        &mut self,
        id: &str,
        to_section: &str,
        parent_id: Option<&str>,
        index: usize,
    ) -> Result<(), String> {
        let (from_section, from_parent, from_index) =
            self.locate_page(id).ok_or("page not found")?;
        self.apply_move(id, to_section, parent_id, index)?;
        self.record_undo(UndoOp::MovePage {
            id: id.to_string(),
            from_section,
            from_parent,
            from_index,
            to_section: to_section.to_string(),
            to_parent: parent_id.map(str::to_string),
            to_index: index,
        });
        self.save()
    }

    fn apply_move(
        &mut self,
        id: &str,
        to_section: &str,
        parent_id: Option<&str>,
        index: usize,
    ) -> Result<(), String> {
        if parent_id == Some(id) {
            return Err("cannot move a page into itself".into());
        }
        let section = self
            .notebook
            .sections
            .iter()
            .find(|s| s.id == to_section)
            .ok_or("section not found")?;
        if let Some(pid) = parent_id {
            if self.is_descendant(id, pid) {
                return Err("cannot move a page into its own subtree".into());
            }
            if find_node(&section.pages, pid).is_none() {
                return Err("parent not found in target section".into());
            }
        }
        let node = self.detach_page(id).ok_or("page not found")?;
        let list = self.pages_list_mut(to_section, parent_id)?;
        let idx = index.min(list.len());
        list.insert(idx, node);
        if let Some(pid) = parent_id {
            if let Some(parent) = self.find_page_mut(pid) {
                parent.expanded = true;
            }
        }
        Ok(())
    }

    pub fn set_expanded(&mut self, id: &str, expanded: bool) -> Result<(), String> {
        let node = self.find_page_mut(id).ok_or("page not found")?;
        node.expanded = expanded;
        self.save()
    }

    pub fn set_last_view(
        &mut self,
        section_id: Option<String>,
        page_id: Option<String>,
    ) -> Result<(), String> {
        self.notebook.last_view = LastView {
            section_id,
            page_id,
        };
        self.save()
    }

    fn is_descendant(&self, ancestor_id: &str, node_id: &str) -> bool {
        match self.find_page(ancestor_id) {
            Some(ancestor) => find_node(&ancestor.children, node_id).is_some(),
            None => false,
        }
    }

    pub fn find_page(&self, id: &str) -> Option<&PageNode> {
        self.notebook
            .sections
            .iter()
            .find_map(|s| find_node(&s.pages, id))
    }

    fn find_page_mut(&mut self, id: &str) -> Option<&mut PageNode> {
        self.notebook
            .sections
            .iter_mut()
            .find_map(|s| find_node_mut(&mut s.pages, id))
    }

    fn record_undo(&mut self, op: UndoOp) {
        self.push_undo_keeping_redo(op);
        self.redo_stack.clear();
    }

    fn push_undo_keeping_redo(&mut self, op: UndoOp) {
        self.undo_stack.push(op);
        if self.undo_stack.len() > UNDO_LIMIT {
            self.undo_stack.remove(0);
        }
    }

    pub fn undo(&mut self) -> Result<Option<UndoOutcome>, String> {
        let Some(op) = self.undo_stack.pop() else {
            return Ok(None);
        };
        log::info!("undo: {}", op.kind());
        let outcome = match &op {
            UndoOp::DeletePage {
                node,
                section_id,
                parent_id,
                index,
            } => {
                self.insert_page_at(node.clone(), section_id, parent_id.as_deref(), *index);
                UndoOutcome {
                    label: format!("restored \"{}\"", node.title),
                    section_id: self.section_of(&node.id),
                    page_id: Some(node.id.clone()),
                }
            }
            UndoOp::DeleteSection {
                section,
                index,
                default_added,
            } => {
                if let Some(did) = default_added {
                    if let Some(pos) = self
                        .notebook
                        .sections
                        .iter()
                        .position(|s| &s.id == did && s.pages.is_empty())
                    {
                        self.notebook.sections.remove(pos);
                    }
                }
                let idx = (*index).min(self.notebook.sections.len());
                self.notebook.sections.insert(idx, section.clone());
                UndoOutcome {
                    label: format!("restored section \"{}\"", section.name),
                    section_id: Some(section.id.clone()),
                    page_id: None,
                }
            }
            UndoOp::MovePage {
                id,
                from_section,
                from_parent,
                from_index,
                ..
            } => {
                self.apply_move(id, from_section, from_parent.as_deref(), *from_index)?;
                let title = self.find_page(id).map(|n| n.title.clone()).unwrap_or_default();
                UndoOutcome {
                    label: format!("moved \"{title}\" back"),
                    section_id: Some(from_section.clone()),
                    page_id: Some(id.clone()),
                }
            }
        };
        self.redo_stack.push(op);
        self.save()?;
        Ok(Some(outcome))
    }

    pub fn redo(&mut self) -> Result<Option<UndoOutcome>, String> {
        let Some(op) = self.redo_stack.pop() else {
            return Ok(None);
        };
        log::info!("redo: {}", op.kind());
        let outcome = match op {
            UndoOp::DeletePage { node, .. } => {
                let title = node.title.clone();
                let new_op = self.apply_delete_page(&node.id)?;
                let section_id = match &new_op {
                    UndoOp::DeletePage { section_id, .. } => Some(section_id.clone()),
                    _ => None,
                };
                self.push_undo_keeping_redo(new_op);
                UndoOutcome {
                    label: format!("deleted \"{title}\""),
                    section_id,
                    page_id: None,
                }
            }
            UndoOp::DeleteSection { section, .. } => {
                let name = section.name.clone();
                let new_op = self.apply_delete_section(&section.id)?;
                self.push_undo_keeping_redo(new_op);
                UndoOutcome {
                    label: format!("deleted section \"{name}\""),
                    section_id: None,
                    page_id: None,
                }
            }
            UndoOp::MovePage {
                id,
                to_section,
                to_parent,
                to_index,
                ..
            } => {
                let (from_section, from_parent, from_index) =
                    self.locate_page(&id).ok_or("page not found")?;
                self.apply_move(&id, &to_section, to_parent.as_deref(), to_index)?;
                self.push_undo_keeping_redo(UndoOp::MovePage {
                    id: id.clone(),
                    from_section,
                    from_parent,
                    from_index,
                    to_section: to_section.clone(),
                    to_parent,
                    to_index,
                });
                let title = self.find_page(&id).map(|n| n.title.clone()).unwrap_or_default();
                UndoOutcome {
                    label: format!("moved \"{title}\""),
                    section_id: Some(to_section),
                    page_id: Some(id),
                }
            }
        };
        self.save()?;
        Ok(Some(outcome))
    }

    /// Deleted pages keep their files on disk for the whole session so undo
    /// can restore them; this runs only on clean close / notebook switch.
    pub fn purge_deleted_files(&self) {
        for id in &self.session_deleted {
            if self.find_page(id).is_none() {
                self.remove_page_files(id);
            }
        }
    }

    fn insert_page_at(
        &mut self,
        node: PageNode,
        section_id: &str,
        parent_id: Option<&str>,
        index: usize,
    ) {
        let node = match self.pages_list_mut(section_id, parent_id) {
            Ok(list) => {
                let idx = index.min(list.len());
                list.insert(idx, node);
                return;
            }
            Err(_) => node,
        };
        if let Some(section) = self.notebook.sections.first_mut() {
            section.pages.push(node);
        }
    }

    fn section_of(&self, id: &str) -> Option<String> {
        self.notebook
            .sections
            .iter()
            .find(|s| find_node(&s.pages, id).is_some())
            .map(|s| s.id.clone())
    }

    fn locate_page(&self, id: &str) -> Option<(String, Option<String>, usize)> {
        fn walk(
            list: &[PageNode],
            parent: Option<&str>,
            id: &str,
        ) -> Option<(Option<String>, usize)> {
            for (i, node) in list.iter().enumerate() {
                if node.id == id {
                    return Some((parent.map(str::to_string), i));
                }
                if let Some(found) = walk(&node.children, Some(&node.id), id) {
                    return Some(found);
                }
            }
            None
        }
        for section in &self.notebook.sections {
            if let Some((parent, index)) = walk(&section.pages, None, id) {
                return Some((section.id.clone(), parent, index));
            }
        }
        None
    }

    fn detach_page(&mut self, id: &str) -> Option<PageNode> {
        for section in self.notebook.sections.iter_mut() {
            if let Some(node) = detach(&mut section.pages, id) {
                return Some(node);
            }
        }
        None
    }

    fn pages_list_mut(
        &mut self,
        section_id: &str,
        parent_id: Option<&str>,
    ) -> Result<&mut Vec<PageNode>, String> {
        let section = self
            .notebook
            .sections
            .iter_mut()
            .find(|s| s.id == section_id)
            .ok_or("section not found")?;
        match parent_id {
            None => Ok(&mut section.pages),
            Some(pid) => find_node_mut(&mut section.pages, pid)
                .map(|n| &mut n.children)
                .ok_or_else(|| "parent not found".to_string()),
        }
    }
}

fn non_empty(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn find_node<'a>(list: &'a [PageNode], id: &str) -> Option<&'a PageNode> {
    for node in list {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_node(&node.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_node_mut<'a>(list: &'a mut [PageNode], id: &str) -> Option<&'a mut PageNode> {
    for node in list.iter_mut() {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_node_mut(&mut node.children, id) {
            return Some(found);
        }
    }
    None
}

fn detach(list: &mut Vec<PageNode>, id: &str) -> Option<PageNode> {
    if let Some(i) = list.iter().position(|n| n.id == id) {
        return Some(list.remove(i));
    }
    for node in list.iter_mut() {
        if let Some(found) = detach(&mut node.children, id) {
            return Some(found);
        }
    }
    None
}

fn collect_ids(node: &PageNode, out: &mut Vec<String>) {
    out.push(node.id.clone());
    for child in &node.children {
        collect_ids(child, out);
    }
}

pub fn flatten_pages<'a>(notebook: &'a Notebook) -> Vec<(&'a Section, &'a PageNode)> {
    fn walk<'a>(
        section: &'a Section,
        list: &'a [PageNode],
        out: &mut Vec<(&'a Section, &'a PageNode)>,
    ) {
        for node in list {
            out.push((section, node));
            walk(section, &node.children, out);
        }
    }
    let mut out = Vec::new();
    for section in &notebook.sections {
        walk(section, &section.pages, &mut out);
    }
    out
}

fn extract_title(content: &str) -> Option<String> {
    let first = content.lines().find(|l| !l.trim().is_empty())?;
    let title = first.trim().strip_prefix("# ")?.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

fn set_title(content: &str, title: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    if let Some(pos) = lines.iter().position(|l| !l.trim().is_empty()) {
        if lines[pos].trim().starts_with("# ") {
            let mut out = String::new();
            for (i, line) in lines.iter().enumerate() {
                if i == pos {
                    out.push_str(&format!("# {title}"));
                } else {
                    out.push_str(line);
                }
                out.push('\n');
            }
            return out;
        }
    }
    format!("# {title}\n\n{content}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn open_store() -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        (dir, store)
    }

    fn section_id(store: &Store) -> String {
        store.notebook.sections[0].id.clone()
    }

    #[test]
    fn open_creates_default_section() {
        let (dir, store) = open_store();
        assert_eq!(store.notebook.sections.len(), 1);
        assert_eq!(store.notebook.sections[0].name, "Notes");
        assert!(dir.path().join("notebook.json").exists());
    }

    #[test]
    fn create_page_writes_stamped_file() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        let content = store.read_page(&page.id).unwrap();
        assert!(content.starts_with("# Untitled\n"));
        assert!(content.contains(&chrono::Local::now().format("%Y-%m-%d").to_string()));
    }

    #[test]
    fn write_page_syncs_title_from_h1() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        let title = store
            .write_page(&page.id, "# Meeting Notes\n\nbody text\n")
            .unwrap();
        assert_eq!(title, "Meeting Notes");
        assert_eq!(store.find_page(&page.id).unwrap().title, "Meeting Notes");
    }

    #[test]
    fn rename_page_rewrites_h1() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        store.rename_page(&page.id, "Renamed").unwrap();
        let content = store.read_page(&page.id).unwrap();
        assert!(content.starts_with("# Renamed\n"));
        assert_eq!(store.find_page(&page.id).unwrap().title, "Renamed");
    }

    #[test]
    fn rename_prepends_h1_when_missing() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        store.write_page(&page.id, "no heading here\n").unwrap();
        store.rename_page(&page.id, "Now Titled").unwrap();
        let content = store.read_page(&page.id).unwrap();
        assert!(content.starts_with("# Now Titled\n"));
        assert!(content.contains("no heading here"));
    }

    #[test]
    fn move_page_reorders_siblings() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        let c = store.create_page(&sid, None, None).unwrap();
        // move a below b: detached list is [b, c], insert at 1 -> [b, a, c]
        store.move_page(&a.id, &sid, None, 1).unwrap();
        let ids: Vec<_> = store.notebook.sections[0]
            .pages
            .iter()
            .map(|p| p.id.clone())
            .collect();
        assert_eq!(ids, vec![b.id, a.id, c.id]);
    }

    #[test]
    fn move_page_reparents_and_expands_parent() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let parent = store.create_page(&sid, None, None).unwrap();
        let child = store.create_page(&sid, None, None).unwrap();
        store.set_expanded(&parent.id, false).unwrap();
        store.move_page(&child.id, &sid, Some(&parent.id), 0).unwrap();
        let parent_node = store.find_page(&parent.id).unwrap();
        assert_eq!(parent_node.children.len(), 1);
        assert_eq!(parent_node.children[0].id, child.id);
        assert!(parent_node.expanded);
    }

    #[test]
    fn move_page_across_sections() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let other = store.create_section("Other").unwrap();
        let page = store.create_page(&sid, None, None).unwrap();
        store.move_page(&page.id, &other.id, None, 0).unwrap();
        assert!(store.notebook.sections[0].pages.is_empty());
        assert_eq!(store.notebook.sections[1].pages[0].id, page.id);
    }

    #[test]
    fn move_section_reorders() {
        let (_dir, mut store) = open_store();
        let a = section_id(&store);
        let b = store.create_section("B").unwrap();
        let c = store.create_section("C").unwrap();
        // move a below b: detached list is [B, C], insert at 1 -> [B, A, C]
        store.move_section(&a, 1).unwrap();
        let ids: Vec<_> = store.notebook.sections.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids, vec![b.id.clone(), a, c.id.clone()]);
        // move c to the front
        store.move_section(&c.id, 0).unwrap();
        let ids: Vec<_> = store.notebook.sections.iter().map(|s| s.id.clone()).collect();
        assert_eq!(ids[0], c.id);
    }

    #[test]
    fn move_into_own_subtree_is_rejected() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let parent = store.create_page(&sid, None, None).unwrap();
        let child = store
            .create_page(&sid, Some(&parent.id), None)
            .unwrap();
        assert!(store
            .move_page(&parent.id, &sid, Some(&child.id), 0)
            .is_err());
        assert!(store.move_page(&parent.id, &sid, Some(&parent.id), 0).is_err());
    }

    #[test]
    fn delete_page_keeps_files_until_purge() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let parent = store.create_page(&sid, None, None).unwrap();
        let child = store
            .create_page(&sid, Some(&parent.id), None)
            .unwrap();
        store.delete_page(&parent.id).unwrap();
        assert!(store.find_page(&parent.id).is_none());
        assert!(dir.path().join(format!("{}.md", parent.id)).exists());
        assert!(dir.path().join(format!("{}.md", child.id)).exists());
        store.purge_deleted_files();
        assert!(!dir.path().join(format!("{}.md", parent.id)).exists());
        assert!(!dir.path().join(format!("{}.md", child.id)).exists());
    }

    #[test]
    fn purge_spares_pages_restored_by_undo() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        store.delete_page(&a.id).unwrap();
        store.delete_page(&b.id).unwrap();
        store.undo().unwrap().unwrap(); // restores b
        store.purge_deleted_files();
        assert!(!dir.path().join(format!("{}.md", a.id)).exists());
        assert!(dir.path().join(format!("{}.md", b.id)).exists());
    }

    #[test]
    fn undo_restores_deleted_page_at_original_position() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        store.delete_page(&a.id).unwrap();
        let outcome = store.undo().unwrap().unwrap();
        assert_eq!(outcome.page_id.as_deref(), Some(a.id.as_str()));
        let ids: Vec<_> = store.notebook.sections[0]
            .pages
            .iter()
            .map(|p| p.id.clone())
            .collect();
        assert_eq!(ids, vec![a.id.clone(), b.id]);
        let content = store.read_page(&a.id).unwrap();
        assert!(content.starts_with("# Untitled\n"));
    }

    #[test]
    fn redo_deletes_page_again() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        store.delete_page(&a.id).unwrap();
        store.undo().unwrap().unwrap();
        store.redo().unwrap().unwrap();
        assert!(store.find_page(&a.id).is_none());
        // and undo brings it back once more
        store.undo().unwrap().unwrap();
        assert!(store.find_page(&a.id).is_some());
    }

    #[test]
    fn undo_delete_section_restores_pages_and_drops_auto_default() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        store.delete_section(&sid).unwrap();
        assert_ne!(store.notebook.sections[0].id, sid);
        let outcome = store.undo().unwrap().unwrap();
        assert_eq!(outcome.section_id.as_deref(), Some(sid.as_str()));
        assert_eq!(store.notebook.sections.len(), 1);
        assert_eq!(store.notebook.sections[0].id, sid);
        assert!(store.find_page(&page.id).is_some());
    }

    #[test]
    fn undo_and_redo_move_page() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        let c = store.create_page(&sid, None, None).unwrap();
        store.move_page(&a.id, &sid, None, 1).unwrap(); // [b, a, c]
        store.undo().unwrap().unwrap();
        let order = |s: &Store| -> Vec<String> {
            s.notebook.sections[0].pages.iter().map(|p| p.id.clone()).collect()
        };
        assert_eq!(order(&store), vec![a.id.clone(), b.id.clone(), c.id.clone()]);
        store.redo().unwrap().unwrap();
        assert_eq!(order(&store), vec![b.id, a.id, c.id]);
    }

    #[test]
    fn undo_restores_subpage_demotion() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        store.move_page(&b.id, &sid, Some(&a.id), 0).unwrap();
        assert_eq!(store.find_page(&a.id).unwrap().children.len(), 1);
        store.undo().unwrap().unwrap();
        assert!(store.find_page(&a.id).unwrap().children.is_empty());
        assert_eq!(store.notebook.sections[0].pages.len(), 2);
    }

    #[test]
    fn new_delete_clears_redo_stack() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let a = store.create_page(&sid, None, None).unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        store.delete_page(&a.id).unwrap();
        store.undo().unwrap().unwrap();
        store.delete_page(&b.id).unwrap();
        assert!(store.redo().unwrap().is_none());
    }

    #[test]
    fn undo_on_empty_stack_returns_none() {
        let (_dir, mut store) = open_store();
        assert!(store.undo().unwrap().is_none());
        assert!(store.redo().unwrap().is_none());
    }

    #[test]
    fn delete_last_section_recreates_default() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        store.delete_section(&sid).unwrap();
        assert_eq!(store.notebook.sections.len(), 1);
        assert_eq!(store.notebook.sections[0].name, "Notes");
    }

    #[test]
    fn notebook_roundtrips_through_disk() {
        let dir = tempdir().unwrap();
        let sid;
        let pid;
        {
            let mut store = Store::open(dir.path()).unwrap();
            sid = section_id(&store);
            let page = store.create_page(&sid, None, None).unwrap();
            pid = page.id.clone();
            store.rename_page(&pid, "Persisted").unwrap();
            store.set_expanded(&pid, false).unwrap();
        }
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.notebook.sections[0].id, sid);
        let page = store.find_page(&pid).unwrap();
        assert_eq!(page.title, "Persisted");
        assert!(!page.expanded);
    }
}
