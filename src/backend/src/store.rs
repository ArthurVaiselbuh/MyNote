use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::Instant;

use crate::lock::{LockError, NotebookLock};

use crate::err;

#[derive(Debug)]
pub enum OpenError {
    /// Another live process already holds the notebook's lock file.
    Locked,
    Other(String),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenError::Locked => {
                write!(f, "this notebook is already open in another MyNote window")
            }
            OpenError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl From<OpenError> for String {
    fn from(e: OpenError) -> String {
        e.to_string()
    }
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// True for any UUID-shaped string (36 chars, hyphens at 8/13/18/23, hex
/// elsewhere) — how every page file is named. Used to recognize page files
/// among arbitrary git history paths and as a defensive argv check before a
/// caller-supplied id reaches a git subprocess.
pub fn is_page_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    bytes.iter().enumerate().all(|(i, &b)| {
        if matches!(i, 8 | 13 | 18 | 23) {
            b == b'-'
        } else {
            b.is_ascii_hexdigit()
        }
    })
}

pub fn page_rel(id: &str) -> String {
    format!("{id}.md")
}

pub fn page_id_from_rel(rel: &str) -> Option<String> {
    let name = rel.strip_suffix(".md")?;
    if name.contains('/') {
        return None; // e.g. assets/<id>/foo.png deleted alongside the page
    }
    is_page_id(name).then(|| name.to_string())
}

pub fn page_path_in(root: &Path, id: &str) -> PathBuf {
    root.join(page_rel(id))
}

pub fn assets_dir_in(root: &Path, page_id: &str) -> PathBuf {
    root.join(ASSETS_DIR).join(page_id)
}

pub fn assets_rel_dir(page_id: &str) -> String {
    format!("{ASSETS_DIR}/{page_id}/")
}

pub fn assets_rel(page_id: &str, name: &str) -> String {
    assets_rel_dir(page_id) + name
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

fn default_git_interval_secs() -> u64 {
    1200
}

/// Snapshots are a safety net, not a versioning tool — anything faster than
/// this would churn the log without buying recoverability.
const MIN_GIT_INTERVAL_SECS: u64 = 60;

fn default_git_enabled() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct GitConfig {
    #[serde(default = "default_git_enabled")]
    pub enabled: bool,
    #[serde(default = "default_git_interval_secs")]
    pub interval_secs: u64,
}

impl Default for GitConfig {
    fn default() -> GitConfig {
        GitConfig {
            enabled: default_git_enabled(),
            interval_secs: default_git_interval_secs(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Notebook {
    pub sections: Vec<Section>,
    #[serde(default)]
    pub last_view: LastView,
    #[serde(default)]
    pub git: GitConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredNotebook<'a> {
    sections: Vec<StoredSection<'a>>,
    git: &'a GitConfig,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredSection<'a> {
    id: &'a str,
    name: &'a str,
    pages: Vec<StoredPage<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredPage<'a> {
    id: &'a str,
    title: &'a str,
    children: Vec<StoredPage<'a>>,
}

impl<'a> From<&'a Notebook> for StoredNotebook<'a> {
    fn from(notebook: &'a Notebook) -> StoredNotebook<'a> {
        StoredNotebook {
            sections: notebook
                .sections
                .iter()
                .map(|s| StoredSection {
                    id: &s.id,
                    name: &s.name,
                    pages: stored_pages(&s.pages),
                })
                .collect(),
            git: &notebook.git,
        }
    }
}

fn stored_pages(list: &[PageNode]) -> Vec<StoredPage<'_>> {
    list.iter()
        .map(|n| StoredPage {
            id: &n.id,
            title: &n.title,
            children: stored_pages(&n.children),
        })
        .collect()
}

/// Where the reader was on one page, in both view modes. Scroll offsets are
/// device pixels, the cursor a character offset into the body.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ViewPos {
    #[serde(default)]
    pub editor_scroll_top: f64,
    #[serde(default)]
    pub editor_cursor: usize,
    #[serde(default)]
    pub preview_scroll_top: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
struct UserState {
    #[serde(default)]
    last_view: LastView,
    #[serde(default)]
    collapsed: Vec<String>,
    #[serde(default)]
    view_positions: BTreeMap<String, ViewPos>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StoredUserState<'a> {
    last_view: &'a LastView,
    collapsed: &'a [String],
    view_positions: &'a BTreeMap<String, ViewPos>,
}

fn collect_collapsed(list: &[PageNode], out: &mut Vec<String>) {
    for node in list {
        if !node.expanded {
            out.push(node.id.clone());
        }
        collect_collapsed(&node.children, out);
    }
}

fn apply_collapsed(list: &mut [PageNode], collapsed: &HashSet<String>) {
    for node in list.iter_mut() {
        node.expanded = !collapsed.contains(&node.id);
        apply_collapsed(&mut node.children, collapsed);
    }
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

pub const NOTEBOOK_FILE: &str = "notebook.json";
pub const USER_STATE_FILE: &str = "notebook.user.json";
pub const ASSETS_DIR: &str = "assets";

const AGENTS_TEMPLATE: &str = include_str!("../templates/agents-template.md");

fn epoch() -> Instant {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    *EPOCH.get_or_init(Instant::now)
}

pub fn now_ms() -> u64 {
    epoch().elapsed().as_millis() as u64
}

static NEXT_SESSION: AtomicU64 = AtomicU64::new(1);

pub struct CloseInfo {
    pub root: PathBuf,
    pub git_enabled: bool,
}

pub struct Store {
    pub root: PathBuf,
    pub notebook: Notebook,
    undo_stack: Vec<UndoOp>,
    redo_stack: Vec<UndoOp>,
    session_deleted: HashSet<String>,
    view_positions: BTreeMap<String, ViewPos>,
    #[allow(dead_code)]
    lock: NotebookLock,

    session: u64,
    change_seq: AtomicU64,
    committed_seq: AtomicU64,
    last_change_ms: AtomicU64,
    last_commit_ms: AtomicU64,
    last_saved_json: RefCell<String>,
    last_saved_user_json: RefCell<String>,
}

impl Store {
    pub fn open(root: &Path) -> Result<Store, OpenError> {
        fs::create_dir_all(root).map_err(|e| OpenError::Other(err(e)))?;
        let lock = NotebookLock::acquire(root).map_err(|e| match e {
            LockError::HeldElsewhere => OpenError::Locked,
            LockError::Io(msg) => OpenError::Other(msg),
        })?;
        let mut notebook = load_notebook(root).map_err(OpenError::Other)?;
        let mut view_positions = BTreeMap::new();
        if let Some(user) = load_user_state(root) {
            notebook.last_view = user.last_view;
            let collapsed: HashSet<String> = user.collapsed.into_iter().collect();
            for section in notebook.sections.iter_mut() {
                apply_collapsed(&mut section.pages, &collapsed);
            }
            // pages that went away between sessions would otherwise accumulate
            let live: HashSet<&str> = flatten_pages(&notebook)
                .iter()
                .map(|(_, page)| page.id.as_str())
                .collect();
            view_positions = user
                .view_positions
                .into_iter()
                .filter(|(id, _)| live.contains(id.as_str()))
                .collect();
        }
        let mut store = Store {
            root: root.to_path_buf(),
            notebook,
            undo_stack: vec![],
            redo_stack: vec![],
            session_deleted: HashSet::new(),
            view_positions,
            lock,
            session: NEXT_SESSION.fetch_add(1, Ordering::Relaxed),
            change_seq: AtomicU64::new(0),
            committed_seq: AtomicU64::new(0),
            last_change_ms: AtomicU64::new(now_ms()),
            last_commit_ms: AtomicU64::new(now_ms()),
            last_saved_json: RefCell::new(String::new()),
            last_saved_user_json: RefCell::new(String::new()),
        };
        if store.notebook.sections.is_empty() {
            store.notebook.sections.push(Section {
                id: new_id(),
                name: "Notes".into(),
                pages: vec![],
            });
        }
        // before `save()` drops the legacy inline copies from notebook.json
        store.save_view_state().map_err(OpenError::Other)?;
        store.save().map_err(OpenError::Other)?;
        if !store.agents_md_path().exists() {
            store.write_agents_template().map_err(OpenError::Other)?;
        }
        Ok(store)
    }

    fn agents_md_path(&self) -> PathBuf {
        self.root.join("AGENTS.md")
    }

    pub fn write_agents_template(&self) -> Result<(), String> {
        atomic_write(&self.agents_md_path(), AGENTS_TEMPLATE.as_bytes())
    }

    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(&StoredNotebook::from(&self.notebook)).map_err(err)?;
        if *self.last_saved_json.borrow() == json {
            return Ok(());
        }
        let path = self.root.join(NOTEBOOK_FILE);
        if path.exists() {
            let _ = fs::copy(&path, self.root.join(format!("{NOTEBOOK_FILE}.bak")));
        }
        atomic_write(&path, json.as_bytes())?;
        *self.last_saved_json.borrow_mut() = json;
        self.touch();
        Ok(())
    }

    fn save_view_state(&self) -> Result<(), String> {
        let mut collapsed = Vec::new();
        for section in &self.notebook.sections {
            collect_collapsed(&section.pages, &mut collapsed);
        }
        let json = serde_json::to_string_pretty(&StoredUserState {
            last_view: &self.notebook.last_view,
            collapsed: &collapsed,
            view_positions: &self.view_positions,
        })
        .map_err(err)?;
        if *self.last_saved_user_json.borrow() == json {
            return Ok(());
        }
        atomic_write(&self.user_state_path(), json.as_bytes())?;
        *self.last_saved_user_json.borrow_mut() = json;
        log::trace!("view state saved ({} collapsed)", collapsed.len());
        Ok(())
    }

    fn user_state_path(&self) -> PathBuf {
        self.root.join(USER_STATE_FILE)
    }

    pub fn session(&self) -> u64 {
        self.session
    }

    pub fn change_seq(&self) -> u64 {
        self.change_seq.load(Ordering::Relaxed)
    }

    pub fn committed_seq(&self) -> u64 {
        self.committed_seq.load(Ordering::Relaxed)
    }

    pub fn idle_ms(&self) -> u64 {
        now_ms().saturating_sub(self.last_change_ms.load(Ordering::Relaxed))
    }

    pub fn since_commit_ms(&self) -> u64 {
        now_ms().saturating_sub(self.last_commit_ms.load(Ordering::Relaxed))
    }

    pub fn mark_committed(&self, seq: u64) {
        self.committed_seq.store(seq, Ordering::Relaxed);
        self.last_commit_ms.store(now_ms(), Ordering::Relaxed);
    }

    fn touch(&self) {
        self.change_seq.fetch_add(1, Ordering::Relaxed);
        self.last_change_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn page_path(&self, id: &str) -> PathBuf {
        page_path_in(&self.root, id)
    }

    pub fn assets_dir(&self, page_id: &str) -> PathBuf {
        assets_dir_in(&self.root, page_id)
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
        let mut ids = Vec::new();
        for page in &section.pages {
            collect_ids(page, &mut ids);
        }
        self.session_deleted.extend(ids);
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
        let assets = self.assets_dir(id);
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
        atomic_write(
            &self.page_path(&id),
            format!("# Untitled\n\n_{stamp}_\n\n").as_bytes(),
        )?;
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
            self.expand_page(pid);
        }
        self.save()?;
        Ok(node)
    }

    pub fn read_page(&self, id: &str) -> Result<String, String> {
        fs::read_to_string(self.page_path(id)).map_err(err)
    }

    pub fn write_page(&mut self, id: &str, content: &str) -> Result<String, String> {
        let existing_title = &self.find_page(id).ok_or("page not found")?.title;
        atomic_write(&self.page_path(id), content.as_bytes())?;
        let title = extract_title(content).unwrap_or_else(|| existing_title.clone());
        if title == *existing_title {
            self.touch();
            return Ok(title);
        }
        if let Some(node) = self.find_page_mut(id) {
            node.title = title.clone();
        }
        self.save()?;
        Ok(title)
    }

    pub fn rename_page(&mut self, id: &str, title: &str) -> Result<(), String> {
        let title = non_empty(title, "Untitled");
        let content = self.read_page(id).unwrap_or_default();
        atomic_write(&self.page_path(id), set_title(&content, &title).as_bytes())?;
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
            let _ = self.save_view_state();
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
            self.expand_page(pid);
        }
        Ok(())
    }

    pub fn set_expanded(&mut self, id: &str, expanded: bool) -> Result<(), String> {
        let node = self.find_page_mut(id).ok_or("page not found")?;
        node.expanded = expanded;
        self.save_view_state()
    }

    pub fn view_positions(&self) -> &BTreeMap<String, ViewPos> {
        &self.view_positions
    }

    pub fn set_view_positions(&mut self, entries: Vec<(String, ViewPos)>) -> Result<(), String> {
        for (page_id, pos) in entries {
            if self.find_page(&page_id).is_some() {
                self.view_positions.insert(page_id, pos);
            }
        }
        self.save_view_state()
    }

    pub fn set_git_snapshots(
        &mut self,
        enabled: bool,
        interval_secs: Option<u64>,
    ) -> Result<(), String> {
        self.notebook.git.enabled = enabled;
        if let Some(secs) = interval_secs {
            self.notebook.git.interval_secs = secs.max(MIN_GIT_INTERVAL_SECS);
        }
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
        self.save_view_state()
    }

    fn is_descendant(&self, ancestor_id: &str, node_id: &str) -> bool {
        match self.find_page(ancestor_id) {
            Some(ancestor) => find_node(&ancestor.children, node_id).is_some(),
            None => false,
        }
    }

    pub fn find_page(&self, id: &str) -> Option<&PageNode> {
        find_page_in(&self.notebook, id)
    }

    fn find_page_mut(&mut self, id: &str) -> Option<&mut PageNode> {
        self.notebook
            .sections
            .iter_mut()
            .find_map(|s| find_node_mut(&mut s.pages, id))
    }

    fn expand_page(&mut self, id: &str) {
        if let Some(node) = self.find_page_mut(id) {
            node.expanded = true;
        }
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
                let section_id = self.section_of(&node.id);
                let new_op = self.apply_delete_page(&node.id)?;
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

    pub fn close(self) -> CloseInfo {
        self.purge_deleted_files();
        let _ = crate::assets::prune(&self);
        let _ = self.save();
        let _ = self.save_view_state();
        CloseInfo {
            root: self.root.clone(),
            git_enabled: self.notebook.git.enabled,
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
        locate_page_in(&self.notebook, id)
            .map(|(section, _, parent, index)| (section.id.clone(), parent, index))
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

    pub fn id_available(&self, id: &str) -> bool {
        id_available_in(&self.root, &self.notebook, id)
    }

    fn write_incoming(&mut self, page: IncomingPage) -> Result<PageNode, String> {
        let id = if is_page_id(&page.id) && self.id_available(&page.id) {
            page.id.clone()
        } else {
            new_id()
        };
        let content = if id == page.id {
            page.content
        } else {
            // the id changed, so the page's own asset links must follow it —
            // otherwise every image in the restored page renders broken.
            page.content.replace(&assets_rel_dir(&page.id), &assets_rel_dir(&id))
        };
        atomic_write(&self.page_path(&id), content.as_bytes())?;
        for (name, bytes) in &page.assets {
            write_asset_file(&self.assets_dir(&id), name, bytes)?;
        }
        // defensive: a restore must never leave a recovered file eligible
        // for purge by an unrelated still-pending delete of the same id.
        self.session_deleted.remove(&id);

        let title = extract_title(&content).unwrap_or_else(|| "Untitled".to_string());
        let mut children = Vec::with_capacity(page.children.len());
        for child in page.children {
            children.push(self.write_incoming(child)?);
        }
        Ok(PageNode { id, title, expanded: true, children })
    }

    /// Writes a recovered subtree through the normal file + tree-save path
    /// (same `atomic_write`, same title-from-H1 sync as every other write) and
    /// appends it under `parent_id` in `section_id`. `parent_id` is used only
    /// when it names a page that's actually in that section right now — a
    /// restored subtree never recreates a missing ancestor, it lands at the
    /// section's top level instead. Nesting *within* the incoming subtree
    /// (children/grandchildren) is always preserved. Not on the undo stack:
    /// a restore is a create, like `create_page`/import.
    pub fn insert_restored(
        &mut self,
        section_id: &str,
        parent_id: Option<&str>,
        pages: Vec<IncomingPage>,
    ) -> Result<Vec<PageNode>, String> {
        if !self.notebook.sections.iter().any(|s| s.id == section_id) {
            return Err("section not found".to_string());
        }
        if pages.is_empty() {
            return Ok(Vec::new());
        }
        let target_parent = parent_id.and_then(|pid| {
            let section = self.notebook.sections.iter().find(|s| s.id == section_id)?;
            find_node(&section.pages, pid)?;
            Some(pid.to_string())
        });

        let mut written = Vec::with_capacity(pages.len());
        for page in pages {
            written.push(self.write_incoming(page)?);
        }

        {
            let list = self.pages_list_mut(section_id, target_parent.as_deref())?;
            list.extend(written.iter().cloned());
        }
        if let Some(pid) = &target_parent {
            self.expand_page(pid);
        }
        self.save()?;
        Ok(written)
    }

    pub fn restore_assets(&self, assets: &[IncomingAsset]) -> Result<usize, String> {
        let mut written = 0;
        for asset in assets {
            if !is_page_id(&asset.page_id) {
                continue;
            }
            if write_asset_file(&self.assets_dir(&asset.page_id), &asset.name, &asset.bytes)? {
                written += 1;
            }
        }
        if written > 0 {
            self.touch();
        }
        Ok(written)
    }
}

fn write_asset_file(dir: &Path, name: &str, bytes: &[u8]) -> Result<bool, String> {
    let Some(safe_name) = Path::new(name).file_name().and_then(|f| f.to_str()) else {
        return Ok(false);
    };
    let path = dir.join(safe_name);
    if path.exists() {
        return Ok(false);
    }
    fs::create_dir_all(dir).map_err(err)?;
    fs::write(&path, bytes).map_err(err)?;
    Ok(true)
}

/// One asset file ready to be written back under `assets/<page_id>/`.
pub struct IncomingAsset {
    pub page_id: String,
    pub name: String,
    pub bytes: Vec<u8>,
}

/// One recovered page (and its recovered subtree) ready to be written by
/// `Store::insert_restored`. `content` is the full markdown body including
/// its `# Title` line; `assets` are `(file name, bytes)` pairs that land
/// under the page's (possibly reassigned) `assets/<id>/`.
pub struct IncomingPage {
    pub id: String,
    pub content: String,
    pub assets: Vec<(String, Vec<u8>)>,
    pub children: Vec<IncomingPage>,
}

fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, contents).map_err(err)?;
    fs::rename(&tmp, path).map_err(err)
}

fn parse_notebook(path: &Path) -> Result<Notebook, String> {
    let text = fs::read_to_string(path).map_err(err)?;
    serde_json::from_str(&text).map_err(err)
}

fn load_notebook(root: &Path) -> Result<Notebook, String> {
    let main = root.join(NOTEBOOK_FILE);
    let backup = root.join(format!("{NOTEBOOK_FILE}.bak"));
    if main.exists() {
        match parse_notebook(&main) {
            Ok(notebook) => return Ok(notebook),
            Err(parse_error) => {
                if let Ok(notebook) = parse_notebook(&backup) {
                    log::warn!("notebook.json unreadable; recovered from backup");
                    return Ok(notebook);
                }
                return Err(parse_error);
            }
        }
    }
    if let Ok(notebook) = parse_notebook(&backup) {
        log::warn!("notebook.json missing; recovered from backup");
        return Ok(notebook);
    }
    Ok(Notebook {
        sections: vec![],
        last_view: LastView::default(),
        git: GitConfig::default(),
    })
}

fn load_user_state(root: &Path) -> Option<UserState> {
    let path = root.join(USER_STATE_FILE);
    if !path.exists() {
        return None;
    }
    match fs::read_to_string(&path).map_err(err).and_then(|t| serde_json::from_str(&t).map_err(err)) {
        Ok(state) => Some(state),
        Err(_) => {
            log::warn!("{USER_STATE_FILE} unreadable; view state reset");
            None
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

pub(crate) fn find_node<'a>(list: &'a [PageNode], id: &str) -> Option<&'a PageNode> {
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

pub(crate) fn collect_ids(node: &PageNode, out: &mut Vec<String>) {
    out.push(node.id.clone());
    for child in &node.children {
        collect_ids(child, out);
    }
}

pub(crate) fn subtree_len(node: &PageNode) -> usize {
    1 + node.children.iter().map(subtree_len).sum::<usize>()
}

/// The tree lookups the `Store` methods delegate to, taking a borrowed
/// `Notebook` so `history.rs` can run them against one parsed out of a git
/// blob, which has no `Store` behind it.
pub(crate) fn find_page_in<'a>(notebook: &'a Notebook, id: &str) -> Option<&'a PageNode> {
    notebook.sections.iter().find_map(|s| find_node(&s.pages, id))
}

pub(crate) fn locate_page_in<'a>(
    notebook: &'a Notebook,
    id: &str,
) -> Option<(&'a Section, &'a PageNode, Option<String>, usize)> {
    fn walk<'a>(
        list: &'a [PageNode],
        parent: Option<&str>,
        id: &str,
    ) -> Option<(&'a PageNode, Option<String>, usize)> {
        for (i, node) in list.iter().enumerate() {
            if node.id == id {
                return Some((node, parent.map(str::to_string), i));
            }
            if let Some(found) = walk(&node.children, Some(&node.id), id) {
                return Some(found);
            }
        }
        None
    }
    notebook
        .sections
        .iter()
        .find_map(|section| walk(&section.pages, None, id).map(|(n, p, i)| (section, n, p, i)))
}

/// True when `id` is free to reuse for a restore: no live tree node, and no
/// leftover file on disk either (a pending-purge delete from this same
/// session, or a stray `.md` the app never wrote, must never be clobbered).
pub(crate) fn id_available_in(root: &Path, notebook: &Notebook, id: &str) -> bool {
    find_page_in(notebook, id).is_none() && !page_path_in(root, id).exists()
}

pub fn flatten_pages(notebook: &Notebook) -> Vec<(&Section, &PageNode)> {
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

pub(crate) fn extract_title(content: &str) -> Option<String> {
    let first = content.lines().find(|l| !l.trim().is_empty())?;
    let title = first.trim().strip_prefix("# ")?.trim();
    if title.is_empty() {
        None
    } else {
        Some(title.to_string())
    }
}

pub(crate) fn set_title(content: &str, title: &str) -> String {
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
    fn open_seeds_agents_md_but_never_overwrites_it() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("AGENTS.md");
        {
            Store::open(dir.path()).unwrap();
            assert_eq!(fs::read_to_string(&path).unwrap(), AGENTS_TEMPLATE);
        }
        fs::write(&path, "user edits").unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "user edits");
        store.write_agents_template().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), AGENTS_TEMPLATE);
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
    fn write_page_skips_tree_save_when_title_unchanged() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        store
            .write_page(&page.id, "# Untitled\n\nfirst body\n")
            .unwrap();
        let json_before = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        fs::remove_file(dir.path().join("notebook.json.bak")).unwrap();
        let seq_before = store.change_seq();

        store
            .write_page(&page.id, "# Untitled\n\nsecond body\n")
            .unwrap();

        assert!(!dir.path().join("notebook.json.bak").exists());
        let json_after = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        assert_eq!(json_before, json_after);
        assert!(store.change_seq() > seq_before);
        assert_eq!(
            store.read_page(&page.id).unwrap(),
            "# Untitled\n\nsecond body\n"
        );
    }

    #[test]
    fn write_page_saves_tree_when_title_changes() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        fs::remove_file(dir.path().join("notebook.json.bak")).unwrap();

        store
            .write_page(&page.id, "# New Title\n\nbody\n")
            .unwrap();

        assert!(dir.path().join("notebook.json.bak").exists());
        assert_eq!(store.find_page(&page.id).unwrap().title, "New Title");
    }

    #[test]
    fn save_is_a_noop_when_nothing_changed() {
        let (dir, store) = open_store();
        assert!(!dir.path().join("notebook.json.bak").exists());
        store.save().unwrap();
        assert!(!dir.path().join("notebook.json.bak").exists());
    }

    #[test]
    fn content_mutations_bump_change_seq() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let seq0 = store.change_seq();
        let page = store.create_page(&sid, None, None).unwrap();
        assert!(store.change_seq() > seq0);
        let seq1 = store.change_seq();
        store.rename_page(&page.id, "Renamed").unwrap();
        assert!(store.change_seq() > seq1);
    }

    #[test]
    fn view_state_does_not_bump_change_seq_or_rewrite_notebook_json() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        let seq = store.change_seq();
        let notebook_before = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        fs::remove_file(dir.path().join("notebook.json.bak")).unwrap();

        store.set_expanded(&page.id, false).unwrap();
        store
            .set_last_view(Some(sid), Some(page.id.clone()))
            .unwrap();

        assert_eq!(store.change_seq(), seq);
        assert_eq!(
            fs::read_to_string(dir.path().join("notebook.json")).unwrap(),
            notebook_before
        );
        assert!(!dir.path().join("notebook.json.bak").exists());
    }

    #[test]
    fn notebook_json_carries_no_view_state() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = store.create_page(&sid, None, None).unwrap();
        store.set_expanded(&page.id, false).unwrap();
        store.set_last_view(Some(sid), Some(page.id.clone())).unwrap();
        store.rename_page(&page.id, "Structure").unwrap();

        let json = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        assert!(!json.contains("lastView"));
        assert!(!json.contains("expanded"));
        assert!(json.contains("\"title\": \"Structure\""));
        assert!(json.contains("\"git\""));
    }

    #[test]
    fn view_state_round_trips_through_the_user_file() {
        let dir = tempdir().unwrap();
        let sid;
        let collapsed_id;
        let open_id;
        {
            let mut store = Store::open(dir.path()).unwrap();
            sid = section_id(&store);
            collapsed_id = store.create_page(&sid, None, None).unwrap().id;
            open_id = store.create_page(&sid, None, None).unwrap().id;
            store.set_expanded(&collapsed_id, false).unwrap();
            store
                .set_last_view(Some(sid.clone()), Some(open_id.clone()))
                .unwrap();
        }
        let user_json = fs::read_to_string(dir.path().join(USER_STATE_FILE)).unwrap();
        assert!(user_json.contains("lastView"));
        assert!(user_json.contains(&collapsed_id));

        let store = Store::open(dir.path()).unwrap();
        assert!(!store.find_page(&collapsed_id).unwrap().expanded);
        assert!(store.find_page(&open_id).unwrap().expanded);
        assert_eq!(store.notebook.last_view.section_id.as_deref(), Some(sid.as_str()));
        assert_eq!(store.notebook.last_view.page_id.as_deref(), Some(open_id.as_str()));
    }

    #[test]
    fn view_positions_survive_a_reopen_and_drop_pages_that_went_away() {
        let dir = tempdir().unwrap();
        let kept_id;
        let gone_id;
        {
            let mut store = Store::open(dir.path()).unwrap();
            let sid = section_id(&store);
            kept_id = store.create_page(&sid, None, None).unwrap().id;
            gone_id = store.create_page(&sid, None, None).unwrap().id;
            let pos = ViewPos {
                editor_scroll_top: 120.5,
                editor_cursor: 42,
                preview_scroll_top: 300.0,
            };
            store
                .set_view_positions(vec![
                    (kept_id.clone(), pos.clone()),
                    (gone_id.clone(), pos),
                    ("never-existed".to_string(), ViewPos::default()),
                ])
                .unwrap();
            store.delete_page(&gone_id).unwrap();
        }
        let user_json = fs::read_to_string(dir.path().join(USER_STATE_FILE)).unwrap();
        assert!(user_json.contains("viewPositions"));
        assert!(!user_json.contains("never-existed"));
        let notebook_json = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        assert!(!notebook_json.contains("viewPositions"));

        let store = Store::open(dir.path()).unwrap();
        assert_eq!(store.view_positions().len(), 1);
        let kept = store.view_positions().get(&kept_id).unwrap();
        assert_eq!(kept.editor_cursor, 42);
        assert_eq!(kept.editor_scroll_top, 120.5);
        assert_eq!(kept.preview_scroll_top, 300.0);
    }

    #[test]
    fn legacy_inline_view_state_migrates_on_open() {
        let dir = tempdir().unwrap();
        let sid = new_id();
        let collapsed_id = new_id();
        let child_id = new_id();
        let legacy = format!(
            r#"{{
              "sections": [{{
                "id": "{sid}",
                "name": "Notes",
                "pages": [{{
                  "id": "{collapsed_id}",
                  "title": "Parent",
                  "expanded": false,
                  "children": [{{ "id": "{child_id}", "title": "Child", "expanded": true, "children": [] }}]
                }}]
              }}],
              "lastView": {{ "sectionId": "{sid}", "pageId": "{child_id}" }}
            }}"#
        );
        fs::write(dir.path().join("notebook.json"), legacy).unwrap();

        {
            let store = Store::open(dir.path()).unwrap();
            assert!(!store.find_page(&collapsed_id).unwrap().expanded);
            assert_eq!(store.notebook.last_view.page_id.as_deref(), Some(child_id.as_str()));
        }
        let json = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        assert!(!json.contains("lastView"));
        assert!(!json.contains("expanded"));

        // and the migrated state survives the rewrite
        let reopened = Store::open(dir.path()).unwrap();
        assert!(!reopened.find_page(&collapsed_id).unwrap().expanded);
        assert_eq!(
            reopened.notebook.last_view.page_id.as_deref(),
            Some(child_id.as_str())
        );
    }

    #[test]
    fn a_corrupt_or_missing_user_file_opens_with_defaults() {
        let dir = tempdir().unwrap();
        let pid;
        {
            let mut store = Store::open(dir.path()).unwrap();
            let sid = section_id(&store);
            pid = store.create_page(&sid, None, None).unwrap().id;
            store.set_expanded(&pid, false).unwrap();
            store.set_last_view(Some(sid), Some(pid.clone())).unwrap();
        }
        fs::write(dir.path().join(USER_STATE_FILE), b"{ not json").unwrap();
        {
            let store = Store::open(dir.path()).unwrap();
            assert!(store.find_page(&pid).unwrap().expanded);
            assert!(store.notebook.last_view.page_id.is_none());
        }
        fs::remove_file(dir.path().join(USER_STATE_FILE)).unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(store.find_page(&pid).unwrap().expanded);
        assert!(store.notebook.last_view.page_id.is_none());
    }

    #[test]
    fn notebook_git_config_defaults_and_round_trips() {
        let (dir, mut store) = open_store();
        assert!(store.notebook.git.enabled);
        assert_eq!(store.notebook.git.interval_secs, 1200);

        store.notebook.git.enabled = false;
        store.notebook.git.interval_secs = 120;
        store.save().unwrap();
        drop(store);

        let reopened = Store::open(dir.path()).unwrap();
        assert!(!reopened.notebook.git.enabled);
        assert_eq!(reopened.notebook.git.interval_secs, 120);
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
    fn open_recovers_from_backup_when_notebook_json_is_corrupt() {
        let dir = tempdir().unwrap();
        let pid;
        {
            let mut store = Store::open(dir.path()).unwrap();
            let sid = section_id(&store);
            let page = store.create_page(&sid, None, None).unwrap();
            pid = page.id.clone();
            store.create_section("Two").unwrap();
        }
        assert!(dir.path().join("notebook.json.bak").exists());
        fs::write(dir.path().join("notebook.json"), b"{ this is not json").unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(store.find_page(&pid).is_some());
    }

    #[test]
    fn open_fails_when_both_notebook_and_backup_are_corrupt() {
        let dir = tempdir().unwrap();
        Store::open(dir.path()).unwrap();
        fs::write(dir.path().join("notebook.json"), b"{ broken").unwrap();
        fs::write(dir.path().join("notebook.json.bak"), b"{ also broken").unwrap();
        assert!(Store::open(dir.path()).is_err());
    }

    #[test]
    fn open_refuses_a_notebook_already_held_by_a_live_store() {
        let dir = tempdir().unwrap();
        let first = Store::open(dir.path()).unwrap();
        match Store::open(dir.path()) {
            Err(OpenError::Locked) => {}
            Err(OpenError::Other(e)) => panic!("expected OpenError::Locked, got Other({e})"),
            Ok(_) => panic!("expected OpenError::Locked, got Ok"),
        }
        drop(first);
        // once the first handle is gone the lock is released, crash or not
        assert!(Store::open(dir.path()).is_ok());
    }

    #[test]
    fn is_page_id_accepts_uuids_and_rejects_junk() {
        assert!(is_page_id(&new_id()));
        assert!(is_page_id("11111111-1111-1111-1111-111111111111"));
        assert!(!is_page_id("notebook.json"));
        assert!(!is_page_id("AGENTS.md"));
        assert!(!is_page_id("-1111111-1111-1111-1111-111111111111"));
        assert!(!is_page_id("11111111-1111-1111-1111-11111111111")); // too short
        assert!(!is_page_id("1111111g-1111-1111-1111-111111111111")); // non-hex
    }

    #[test]
    fn id_available_is_false_for_a_file_on_disk_without_a_tree_node() {
        let (_dir, store) = open_store();
        let orphan_id = new_id();
        fs::write(store.page_path(&orphan_id), "# X\n").unwrap();
        assert!(!store.id_available(&orphan_id));
        assert!(store.id_available(&new_id()));
    }

    #[test]
    fn insert_restored_reuses_a_free_id_and_syncs_title_from_h1() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let id = new_id();
        let page = IncomingPage {
            id: id.clone(),
            content: "# Recovered\n\nbody\n".to_string(),
            assets: vec![],
            children: vec![],
        };
        let written = store.insert_restored(&sid, None, vec![page]).unwrap();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].id, id);
        assert_eq!(written[0].title, "Recovered");
        assert_eq!(store.find_page(&id).unwrap().title, "Recovered");
        assert_eq!(
            fs::read_to_string(dir.path().join(format!("{id}.md"))).unwrap(),
            "# Recovered\n\nbody\n"
        );
    }

    #[test]
    fn insert_restored_mints_a_new_id_when_taken_and_rewrites_asset_links() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let live = store.create_page(&sid, None, None).unwrap(); // occupies its own id, irrelevant here
        let taken_id = live.id.clone();
        let page = IncomingPage {
            id: taken_id.clone(),
            content: format!("# Recovered\n\n![x](assets/{taken_id}/img.png)\n"),
            assets: vec![("img.png".to_string(), vec![1, 2, 3])],
            children: vec![],
        };
        let written = store.insert_restored(&sid, None, vec![page]).unwrap();
        assert_ne!(written[0].id, taken_id, "the live id must not be clobbered");
        let new_id = written[0].id.clone();
        let content = store.read_page(&new_id).unwrap();
        assert!(content.contains(&format!("assets/{new_id}/img.png")));
        assert!(!content.contains(&format!("assets/{taken_id}/img.png")));
        assert!(store.root.join("assets").join(&new_id).join("img.png").exists());
    }

    #[test]
    fn insert_restored_preserves_nesting() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let grandchild = IncomingPage {
            id: new_id(),
            content: "# Grandchild\n".to_string(),
            assets: vec![],
            children: vec![],
        };
        let child = IncomingPage {
            id: new_id(),
            content: "# Child\n".to_string(),
            assets: vec![],
            children: vec![grandchild],
        };
        let parent = IncomingPage {
            id: new_id(),
            content: "# Parent\n".to_string(),
            assets: vec![],
            children: vec![child],
        };
        let written = store.insert_restored(&sid, None, vec![parent]).unwrap();
        assert_eq!(written[0].title, "Parent");
        assert_eq!(written[0].children[0].title, "Child");
        assert_eq!(written[0].children[0].children[0].title, "Grandchild");
        let parent_node = store.find_page(&written[0].id).unwrap();
        assert_eq!(parent_node.children[0].children[0].title, "Grandchild");
    }

    #[test]
    fn insert_restored_appends_to_the_section_top_level_when_parent_is_gone() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let page = IncomingPage {
            id: new_id(),
            content: "# Orphaned\n".to_string(),
            assets: vec![],
            children: vec![],
        };
        // "not-a-real-page" never existed in this section
        let written = store.insert_restored(&sid, Some("not-a-real-page"), vec![page]).unwrap();
        assert_eq!(store.notebook.sections[0].pages.last().unwrap().id, written[0].id);
    }

    #[test]
    fn insert_restored_does_not_clobber_an_existing_asset_file() {
        let (_dir, mut store) = open_store();
        let sid = section_id(&store);
        let id = new_id();
        let assets_dir = store.root.join("assets").join(&id);
        fs::create_dir_all(&assets_dir).unwrap();
        fs::write(assets_dir.join("img.png"), b"current bytes").unwrap();
        let page = IncomingPage {
            id: id.clone(),
            content: "# R\n".to_string(),
            assets: vec![("img.png".to_string(), b"historical bytes".to_vec())],
            children: vec![],
        };
        store.insert_restored(&sid, None, vec![page]).unwrap();
        assert_eq!(fs::read(assets_dir.join("img.png")).unwrap(), b"current bytes");
    }

    #[test]
    fn insert_restored_bumps_change_seq_and_writes_notebook_json() {
        let (dir, mut store) = open_store();
        let sid = section_id(&store);
        let seq_before = store.change_seq();
        let page = IncomingPage {
            id: new_id(),
            content: "# R\n".to_string(),
            assets: vec![],
            children: vec![],
        };
        store.insert_restored(&sid, None, vec![page]).unwrap();
        assert!(store.change_seq() > seq_before);
        let json = fs::read_to_string(dir.path().join("notebook.json")).unwrap();
        assert!(json.contains("\"title\": \"R\""));
    }

    #[test]
    fn insert_restored_rejects_an_unknown_section() {
        let (_dir, mut store) = open_store();
        let page = IncomingPage {
            id: new_id(),
            content: "# R\n".to_string(),
            assets: vec![],
            children: vec![],
        };
        assert!(store.insert_restored("no-such-section", None, vec![page]).is_err());
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
