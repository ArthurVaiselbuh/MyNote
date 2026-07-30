//! In-app page history: revisions of a live page, and recovery of pages
//! whose files were deleted from a past snapshot. Built entirely on the
//! read-only queries in `git.rs`; the only writer is `Store::insert_restored`.
//!
//! Every function here takes a `root`/`notebook` snapshot rather than `&Store`
//! on purpose: callers clone those two fields out from under the store lock
//! and drop the guard *before* calling in, so a multi-second git subprocess
//! never blocks every other command (autosave, typing, tree edits — all of
//! which serialize on the same store mutex).

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use crate::git;
use crate::store::{self, Notebook, PageNode, Section};

/// How many `notebook.json` revisions to probe, walking backward from a
/// deletion, looking for the newest one that still has the page's node.
/// Deferred deletion means the node is usually gone from `notebook.json`
/// several commits before the file itself is purged at close, so this has
/// to look further back than "one commit" — see the `deleted_pages` doc.
const NB_PROBE_DEPTH: usize = 8;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PageRevision {
    pub sha: String,
    pub at: i64,
    pub subject: String,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RevisionText {
    pub text: String,
    pub truncated: bool,
    pub missing: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeletedChild {
    pub id: String,
    pub title: String,
    pub children: Vec<DeletedChild>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeletedItem {
    pub sha: String,
    pub at: i64,
    pub id: String,
    pub title: String,
    pub section_id: Option<String>,
    pub section_name: Option<String>,
    pub section_exists: bool,
    pub parent_id: Option<String>,
    pub parent_exists: bool,
    /// False when no `notebook.json` snapshot in the probe window still had
    /// this id — the title then falls back to the page's own `.md` H1 and
    /// there's no section/nesting information to show.
    pub resolved: bool,
    pub page_count: usize,
    pub children: Vec<DeletedChild>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeletedHistory {
    pub items: Vec<DeletedItem>,
    pub hit_cap: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RestoreOutcome {
    pub section_id: String,
    pub page_id: String,
    pub page_count: usize,
    pub asset_count: usize,
    pub renamed: bool,
}

pub struct Recovered {
    pub section_id: String,
    pub parent_id: Option<String>,
    pub page: store::IncomingPage,
    pub page_count: usize,
    pub asset_count: usize,
}

fn page_path(root: &Path, id: &str) -> PathBuf {
    root.join(format!("{id}.md"))
}

fn find_node_in<'a>(list: &'a [PageNode], id: &str) -> Option<&'a PageNode> {
    for node in list {
        if node.id == id {
            return Some(node);
        }
        if let Some(found) = find_node_in(&node.children, id) {
            return Some(found);
        }
    }
    None
}

fn find_page_in<'a>(notebook: &'a Notebook, id: &str) -> Option<&'a PageNode> {
    notebook.sections.iter().find_map(|s| find_node_in(&s.pages, id))
}

/// Mirrors `Store::id_available`: free to reuse only when there's neither a
/// live tree node nor a leftover file on disk.
fn id_available(root: &Path, notebook: &Notebook, id: &str) -> bool {
    find_page_in(notebook, id).is_none() && !page_path(root, id).exists()
}

/// Revisions of one live page, newest first.
pub fn page_revisions(root: &Path, id: &str) -> Result<Vec<PageRevision>, String> {
    if !store::is_page_id(id) {
        return Err("bad page id".to_string());
    }
    let revs = git::log_file(root, &format!("{id}.md"), git::MAX_PAGE_REVISIONS)?;
    Ok(revs
        .into_iter()
        .map(|r| PageRevision { sha: r.sha, at: r.at, subject: r.subject })
        .collect())
}

/// The text of one page at one revision. An empty `sha` means "now, on
/// disk" — the live buffer, not a git blob.
pub fn revision_text(root: &Path, id: &str, sha: &str) -> Result<RevisionText, String> {
    if !store::is_page_id(id) {
        return Err("bad page id".to_string());
    }
    if sha.is_empty() {
        let text = fs::read_to_string(page_path(root, id)).map_err(|e| e.to_string())?;
        return Ok(RevisionText { text, truncated: false, missing: false });
    }
    if !git::is_commit_sha(sha) {
        return Err("invalid revision".to_string());
    }
    let blob = git::read_blob_text(root, sha, &format!("{id}.md"))?;
    Ok(RevisionText { text: blob.text, truncated: blob.truncated, missing: blob.missing })
}

/// The text of a page as it stood right before `sha` deleted it.
pub fn deleted_page_text(root: &Path, sha: &str, id: &str) -> Result<RevisionText, String> {
    if !store::is_page_id(id) {
        return Err("bad page id".to_string());
    }
    if !git::is_commit_sha(sha) {
        return Err("invalid revision".to_string());
    }
    let blob = git::read_blob_text(root, &format!("{sha}~1"), &format!("{id}.md"))?;
    Ok(RevisionText { text: blob.text, truncated: blob.truncated, missing: blob.missing })
}

fn find_with_parent<'a>(
    list: &'a [PageNode],
    parent: Option<&str>,
    id: &str,
) -> Option<(&'a PageNode, Option<String>)> {
    for node in list {
        if node.id == id {
            return Some((node, parent.map(str::to_string)));
        }
        if let Some(found) = find_with_parent(&node.children, Some(&node.id), id) {
            return Some(found);
        }
    }
    None
}

/// Finds `id` anywhere in a historical notebook snapshot, returning its node,
/// the section it lived in, and its immediate parent id (if it was nested).
fn find_in_notebook<'a>(nb: &'a Notebook, id: &str) -> Option<(&'a PageNode, &'a Section, Option<String>)> {
    for section in &nb.sections {
        if let Some((node, parent)) = find_with_parent(&section.pages, None, id) {
            return Some((node, section, parent));
        }
    }
    None
}

fn to_deleted_child(node: &PageNode) -> DeletedChild {
    DeletedChild {
        id: node.id.clone(),
        title: node.title.clone(),
        children: node.children.iter().map(to_deleted_child).collect(),
    }
}

fn count_tree(children: &[DeletedChild]) -> usize {
    children.iter().map(|c| 1 + count_tree(&c.children)).sum()
}

fn collect_covered(children: &[DeletedChild], out: &mut HashSet<String>) {
    for c in children {
        out.insert(c.id.clone());
        collect_covered(&c.children, out);
    }
}

/// Pages recoverable right now, newest deletion first. Grouped implicitly by
/// `(sha, at)` — the frontend clusters same-commit items itself.
///
/// Deferred deletion (see `store.rs`) means a page's node is dropped from
/// `notebook.json` the instant it's deleted, but its `.md` file — and so its
/// appearance in git history as a `D` — only lands at the *next* snapshot
/// after the notebook is closed or switched. An idle snapshot can commit the
/// node-less `notebook.json` well before that, so the deleting commit itself
/// frequently doesn't touch `notebook.json` at all. To recover title/section/
/// nesting, this walks `notebook.json`'s own history backward from each
/// deletion's timestamp looking for the newest snapshot that still has the
/// node — which also still has the *whole* subtree as it stood then, so no
/// separate matching against other deleted ids is needed for nesting.
pub fn deleted_pages(root: &Path, notebook: &Notebook) -> Result<DeletedHistory, String> {
    let groups = git::deleted_md_groups(root, git::MAX_DELETE_COMMITS)?;
    let hit_cap = groups.len() >= git::MAX_DELETE_COMMITS;
    if groups.is_empty() {
        return Ok(DeletedHistory { items: Vec::new(), hit_cap });
    }

    // newest group wins per id; only ids that are genuinely gone right now
    // (not live, and not a same-session pending-purge file) are candidates.
    let mut seen = HashSet::new();
    let mut candidates: Vec<(&git::DeletedGroup, String)> = Vec::new();
    for group in &groups {
        for id in &group.ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            if id_available(root, notebook, id) {
                candidates.push((group, id.clone()));
            }
        }
    }
    if candidates.is_empty() {
        return Ok(DeletedHistory { items: Vec::new(), hit_cap });
    }

    let nb_revs = git::log_file(root, "notebook.json", git::MAX_PAGE_REVISIONS).unwrap_or_default();

    let mut probe_shas: Vec<String> = Vec::new();
    let mut probe_idx_of: HashMap<String, usize> = HashMap::new();
    let mut probes_for: Vec<Vec<usize>> = Vec::with_capacity(candidates.len());
    for (group, _) in &candidates {
        let start = nb_revs.iter().position(|r| r.at <= group.at).unwrap_or(nb_revs.len());
        let mut idxs = Vec::new();
        for rev in nb_revs[start..].iter().take(NB_PROBE_DEPTH) {
            let pos = *probe_idx_of.entry(rev.sha.clone()).or_insert_with(|| {
                probe_shas.push(rev.sha.clone());
                probe_shas.len() - 1
            });
            idxs.push(pos);
        }
        probes_for.push(idxs);
    }

    let mut specs: Vec<String> = probe_shas.iter().map(|s| format!("{s}:notebook.json")).collect();
    let nb_specs_len = specs.len();
    for (group, id) in &candidates {
        specs.push(format!("{}~1:{id}.md", group.sha));
    }
    type Blobs = Vec<Option<Vec<u8>>>;
    let blobs = git::read_blobs(root, &specs).unwrap_or_default();
    let (nb_blobs, md_blobs): (Blobs, Blobs) = if blobs.len() == specs.len() {
        (blobs[..nb_specs_len].to_vec(), blobs[nb_specs_len..].to_vec())
    } else {
        (vec![None; nb_specs_len], vec![None; candidates.len()])
    };
    let nb_cache: Vec<Option<Notebook>> = nb_blobs
        .into_iter()
        .map(|b| b.and_then(|bytes| serde_json::from_slice::<Notebook>(&bytes).ok()))
        .collect();

    let mut items: Vec<DeletedItem> = Vec::with_capacity(candidates.len());
    let mut covered: HashSet<String> = HashSet::new();

    for (i, (group, id)) in candidates.iter().enumerate() {
        let mut resolved_node: Option<&PageNode> = None;
        let mut resolved_section: Option<&Section> = None;
        let mut resolved_parent: Option<String> = None;
        for &pidx in &probes_for[i] {
            let Some(Some(nb)) = nb_cache.get(pidx) else { continue };
            if let Some((node, section, parent)) = find_in_notebook(nb, id) {
                resolved_node = Some(node);
                resolved_section = Some(section);
                resolved_parent = parent;
                break;
            }
        }

        let (title, children) = match resolved_node {
            Some(node) => (node.title.clone(), node.children.iter().map(to_deleted_child).collect::<Vec<_>>()),
            None => {
                let title = md_blobs
                    .get(i)
                    .and_then(|b| b.as_ref())
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .and_then(store::extract_title)
                    .unwrap_or_else(|| "Untitled".to_string());
                (title, Vec::new())
            }
        };
        collect_covered(&children, &mut covered);

        let (section_id, section_name, section_exists) = match resolved_section {
            Some(s) => (
                Some(s.id.clone()),
                Some(s.name.clone()),
                notebook.sections.iter().any(|live| live.id == s.id),
            ),
            None => (None, None, false),
        };
        let parent_exists = resolved_parent
            .as_deref()
            .is_some_and(|pid| find_page_in(notebook, pid).is_some());
        let page_count = 1 + count_tree(&children);

        items.push(DeletedItem {
            sha: group.sha.clone(),
            at: group.at,
            id: id.clone(),
            title,
            section_id,
            section_name,
            section_exists,
            parent_id: resolved_parent,
            parent_exists,
            resolved: resolved_node.is_some(),
            page_count,
            children,
        });
    }

    // an id absorbed as a descendant of another (newer) item isn't its own
    // top-level entry — it's already shown nested under that item.
    items.retain(|it| !covered.contains(&it.id));

    Ok(DeletedHistory { items, hit_cap })
}

fn collect_ids(node: &PageNode, out: &mut Vec<String>) {
    out.push(node.id.clone());
    for c in &node.children {
        collect_ids(c, out);
    }
}

fn count_page_node(children: &[PageNode]) -> usize {
    children.iter().map(|c| 1 + count_page_node(&c.children)).sum()
}

/// Fetches page bodies + assets for a whole recovered subtree in a constant
/// number of processes regardless of its size: one batch for every page's
/// `.md` blob, one tree listing for `assets/`, one batch for every matched
/// asset blob. A child whose blob is missing at `parent_sha` (and its own
/// descendants) is dropped rather than failing the whole restore.
fn build_incoming(root: &Path, sha: &str, node: PageNode) -> Result<(store::IncomingPage, usize), String> {
    let mut ids = Vec::new();
    collect_ids(&node, &mut ids);
    let parent_sha = format!("{sha}~1");

    let md_specs: Vec<String> = ids.iter().map(|id| format!("{parent_sha}:{id}.md")).collect();
    let md_blobs = git::read_blobs(root, &md_specs)?;

    let asset_paths = git::list_tree_files(root, &parent_sha, "assets/").unwrap_or_default();
    let mut asset_specs = Vec::new();
    let mut asset_owner: Vec<(String, String)> = Vec::new();
    for id in &ids {
        let prefix = format!("assets/{id}/");
        for path in &asset_paths {
            if let Some(name) = path.strip_prefix(&prefix) {
                asset_specs.push(format!("{parent_sha}:{path}"));
                asset_owner.push((id.clone(), name.to_string()));
            }
        }
    }
    let asset_blobs = if asset_specs.is_empty() {
        Vec::new()
    } else {
        git::read_blobs(root, &asset_specs)?
    };

    let mut assets_by_id: HashMap<String, Vec<(String, Vec<u8>)>> = HashMap::new();
    let mut asset_count = 0;
    for (i, (owner_id, name)) in asset_owner.into_iter().enumerate() {
        if let Some(Some(bytes)) = asset_blobs.get(i) {
            assets_by_id.entry(owner_id).or_default().push((name, bytes.clone()));
            asset_count += 1;
        }
    }

    fn build(
        node: PageNode,
        ids: &[String],
        md_blobs: &[Option<Vec<u8>>],
        assets_by_id: &mut HashMap<String, Vec<(String, Vec<u8>)>>,
    ) -> Option<store::IncomingPage> {
        let idx = ids.iter().position(|i| *i == node.id)?;
        let bytes = md_blobs.get(idx)?.clone()?;
        let content = String::from_utf8(bytes).ok()?;
        let assets = assets_by_id.remove(&node.id).unwrap_or_default();
        let children = node
            .children
            .into_iter()
            .filter_map(|c| build(c, ids, md_blobs, assets_by_id))
            .collect();
        Some(store::IncomingPage { id: node.id, content, assets, children })
    }

    let incoming = build(node, &ids, &md_blobs, &mut assets_by_id)
        .ok_or_else(|| "that page's content is no longer available".to_string())?;
    Ok((incoming, asset_count))
}

/// Resolves everything a restore needs — content, assets, and where it goes
/// — without writing anything. The caller (a Tauri command) re-locks the
/// store to actually write via `Store::insert_restored`; this function only
/// reads. `sha`/`id` are re-validated here rather than trusted from the
/// frontend, and placement is re-derived from history rather than taking the
/// client's word for it.
pub fn recover(
    root: &Path,
    notebook: &Notebook,
    sha: &str,
    id: &str,
    fallback_section_id: Option<&str>,
) -> Result<Recovered, String> {
    if !store::is_page_id(id) {
        return Err("bad page id".to_string());
    }
    if !git::is_commit_sha(sha) {
        return Err("invalid revision".to_string());
    }
    if !id_available(root, notebook, id) {
        return Err("that page already exists".to_string());
    }

    let at = git::commit_time(root, sha)?;
    let nb_revs = git::log_file(root, "notebook.json", git::MAX_PAGE_REVISIONS).unwrap_or_default();
    let start = nb_revs.iter().position(|r| r.at <= at).unwrap_or(nb_revs.len());

    let mut found: Option<(PageNode, String, Option<String>)> = None;
    for rev in nb_revs[start..].iter().take(NB_PROBE_DEPTH) {
        let Ok(blob) = git::read_blob_text(root, &rev.sha, "notebook.json") else { continue };
        let Ok(nb) = serde_json::from_str::<Notebook>(&blob.text) else { continue };
        if let Some((node, section, parent)) = find_in_notebook(&nb, id) {
            found = Some((node.clone(), section.id.clone(), parent));
            break;
        }
    }

    let (node, hist_section_id, hist_parent_id) = match found {
        Some(v) => v,
        None => {
            let title = git::read_blob_text(root, &format!("{sha}~1"), &format!("{id}.md"))
                .ok()
                .and_then(|b| store::extract_title(&b.text))
                .unwrap_or_else(|| "Untitled".to_string());
            (PageNode { id: id.to_string(), title, expanded: true, children: Vec::new() }, String::new(), None)
        }
    };

    let section_id = if notebook.sections.iter().any(|s| s.id == hist_section_id) {
        hist_section_id
    } else if let Some(fb) = fallback_section_id.filter(|f| notebook.sections.iter().any(|s| s.id == *f)) {
        fb.to_string()
    } else {
        notebook
            .sections
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| "no section available".to_string())?
    };
    let parent_id = hist_parent_id.filter(|pid| find_page_in(notebook, pid).is_some());

    let page_count = 1 + count_page_node(&node.children);
    let (page, asset_count) = build_incoming(root, sha, node)?;

    Ok(Recovered { section_id, parent_id, page, page_count, asset_count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets;
    use crate::git::SnapshotKind;
    use crate::store::Store;
    use std::time::Duration;
    use tempfile::tempdir;

    macro_rules! need_git {
        () => {
            if !git::available() {
                eprintln!("skipping: git not found on PATH");
                return;
            }
        };
    }

    const TINY_PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    fn snapshot(root: &Path, kind: SnapshotKind) -> bool {
        git::snapshot(root, kind, Duration::from_secs(20)).unwrap()
    }

    #[test]
    fn page_revisions_and_revision_text_round_trip() {
        need_git!();
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        git::ensure_repo(&store.root).unwrap();
        assert!(snapshot(&store.root, SnapshotKind::Open));

        store.write_page(&page.id, "# T\n\nv2\n").unwrap();
        assert!(snapshot(&store.root, SnapshotKind::Idle));

        let revs = page_revisions(&store.root, &page.id).unwrap();
        assert_eq!(revs.len(), 2);

        let now = revision_text(&store.root, &page.id, "").unwrap();
        assert_eq!(now.text, "# T\n\nv2\n");
        let old = revision_text(&store.root, &page.id, &revs[1].sha).unwrap();
        assert!(old.text.starts_with("# Untitled"));
    }

    #[test]
    fn deleted_pages_is_empty_with_no_repo_or_no_deletions() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        // no repo at all yet
        assert!(deleted_pages(&store.root, &store.notebook).is_err());
    }

    #[test]
    fn deleted_pages_and_recover_round_trip_a_subtree_with_assets() {
        need_git!();
        let dir = tempdir().unwrap();
        let (parent_id, child_id, img_rel, sid) = {
            let mut store = Store::open(dir.path()).unwrap();
            let sid = store.notebook.sections[0].id.clone();
            let parent = store.create_page(&sid, None, None).unwrap();
            let child = store.create_page(&sid, Some(&parent.id), None).unwrap();
            let img = assets::save_image(&store, &parent.id, TINY_PNG_B64, "png").unwrap();
            store
                .write_page(&parent.id, &format!("# Parent\n\n![x]({img})\n"))
                .unwrap();
            store.write_page(&child.id, "# Child\n\nbody\n").unwrap();
            git::ensure_repo(&store.root).unwrap();
            assert!(snapshot(&store.root, SnapshotKind::Open));

            store.delete_page(&parent.id).unwrap();
            let info = store.close();
            assert!(git::snapshot(&info.root, SnapshotKind::Close, Duration::from_secs(5)).unwrap());
            (parent.id, child.id, img, sid)
        };

        let store = Store::open(dir.path()).unwrap();
        let history = deleted_pages(&store.root, &store.notebook).unwrap();
        assert_eq!(history.items.len(), 1, "the child must be nested, not a second top-level item");
        let item = &history.items[0];
        assert_eq!(item.id, parent_id);
        assert_eq!(item.title, "Parent");
        assert!(item.resolved);
        assert_eq!(item.section_id.as_deref(), Some(sid.as_str()));
        assert!(item.section_exists);
        assert_eq!(item.page_count, 2);
        assert_eq!(item.children.len(), 1);
        assert_eq!(item.children[0].id, child_id);
        assert_eq!(item.children[0].title, "Child");

        let recovered = recover(&store.root, &store.notebook, &item.sha, &item.id, None).unwrap();
        assert_eq!(recovered.section_id, sid);
        assert_eq!(recovered.parent_id, None);
        assert_eq!(recovered.page_count, 2);
        assert_eq!(recovered.asset_count, 1);
        assert_eq!(recovered.page.id, parent_id, "the freed id should be reused");
        assert!(recovered.page.content.contains(&img_rel));
        assert_eq!(recovered.page.children.len(), 1);
        assert_eq!(recovered.page.children[0].id, child_id);
        assert_eq!(recovered.page.children[0].content, "# Child\n\nbody\n");
        assert_eq!(recovered.page.assets.len(), 1);

        let mut store = store;
        let written = store
            .insert_restored(&recovered.section_id, recovered.parent_id.as_deref(), vec![recovered.page])
            .unwrap();
        assert_eq!(written[0].id, parent_id);
        assert_eq!(written[0].children[0].id, child_id);
        assert_eq!(store.read_page(&parent_id).unwrap(), format!("# Parent\n\n![x]({img_rel})\n"));
        assert!(dir.path().join(&img_rel).exists());

        // now genuinely gone again from the recoverable set
        let history_after = deleted_pages(&store.root, &store.notebook).unwrap();
        assert!(history_after.items.iter().all(|i| i.id != parent_id));

        assert!(snapshot(&store.root, SnapshotKind::Idle));
        let revs = page_revisions(&store.root, &parent_id).unwrap();
        assert!(revs.len() >= 3, "create, delete, and recover snapshots should all be visible");
        let texts: Vec<RevisionText> = revs
            .iter()
            .map(|r| revision_text(&store.root, &parent_id, &r.sha).unwrap())
            .collect();
        assert_eq!(texts.iter().filter(|t| t.missing).count(), 1, "only the delete commit is missing");
    }

    #[test]
    fn recover_rejects_a_live_id() {
        need_git!();
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        git::ensure_repo(&store.root).unwrap();
        assert!(snapshot(&store.root, SnapshotKind::Open));

        // fabricate a plausible-looking sha; recover must fail before ever
        // trusting it, because the id is still live
        let fake_sha = "a".repeat(40);
        assert!(recover(&store.root, &store.notebook, &fake_sha, &page.id, None).is_err());
    }

    #[test]
    fn revision_text_and_deleted_page_text_reject_bad_input() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        assert!(revision_text(&store.root, "not-a-uuid", "").is_err());
        assert!(revision_text(&store.root, &store::new_id(), "not-a-sha").is_err());
        assert!(deleted_page_text(&store.root, "not-a-sha", &store::new_id()).is_err());
    }
}
