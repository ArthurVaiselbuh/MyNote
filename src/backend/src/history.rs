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
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::ops::Range;
use std::path::Path;

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
    /// None when no `notebook.json` snapshot in the probe window still had
    /// this id — the title then falls back to the page's own `.md` H1 and
    /// there's no section/nesting information to show.
    pub section_id: Option<String>,
    pub section_name: Option<String>,
    pub section_exists: bool,
    pub parent_id: Option<String>,
    pub parent_exists: bool,
    pub page_count: usize,
    pub children: Vec<DeletedChild>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DeletedHistory {
    pub items: Vec<DeletedItem>,
    pub hit_cap: bool,
}

pub struct Recovered {
    pub section_id: String,
    pub parent_id: Option<String>,
    pub page: store::IncomingPage,
    pub page_count: usize,
    pub asset_count: usize,
}

fn check_page_id(id: &str) -> Result<(), String> {
    store::is_page_id(id).then_some(()).ok_or_else(|| "bad page id".to_string())
}

fn check_sha(sha: &str) -> Result<(), String> {
    git::is_commit_sha(sha).then_some(()).ok_or_else(|| "invalid revision".to_string())
}

impl From<git::BlobText> for RevisionText {
    fn from(blob: git::BlobText) -> Self {
        RevisionText { text: blob.text, truncated: blob.truncated, missing: blob.missing }
    }
}

fn page_text_at(root: &Path, rev: &str, id: &str) -> Result<RevisionText, String> {
    Ok(git::read_blob_text(root, rev, &store::page_rel(id))?.into())
}

fn title_or_untitled(text: Option<&str>) -> String {
    text.and_then(store::extract_title).unwrap_or_else(|| "Untitled".to_string())
}

/// Revisions of one live page, newest first.
pub fn page_revisions(root: &Path, id: &str) -> Result<Vec<PageRevision>, String> {
    check_page_id(id)?;
    let revs = git::log_file(root, &store::page_rel(id), git::MAX_PAGE_REVISIONS)?;
    Ok(revs
        .into_iter()
        .map(|r| PageRevision { sha: r.sha, at: r.at, subject: r.subject })
        .collect())
}

/// The text of one page at one revision. An empty `sha` means "now, on
/// disk" — the live buffer, not a git blob.
pub fn revision_text(root: &Path, id: &str, sha: &str) -> Result<RevisionText, String> {
    check_page_id(id)?;
    if sha.is_empty() {
        let text = fs::read_to_string(store::page_path_in(root, id)).map_err(|e| e.to_string())?;
        return Ok(RevisionText { text, truncated: false, missing: false });
    }
    check_sha(sha)?;
    page_text_at(root, sha, id)
}

pub fn missing_revision_assets(
    root: &Path,
    id: &str,
    sha: &str,
) -> Result<Vec<store::IncomingAsset>, String> {
    check_page_id(id)?;
    check_sha(sha)?;
    let revision = page_text_at(root, sha, id)?;
    if revision.missing {
        return Ok(Vec::new());
    }
    let absent: Vec<(String, String)> = crate::assets::referenced_assets(&revision.text)
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|(owner, name)| !store::assets_dir_in(root, owner).join(name).exists())
        .collect();

    let specs: Vec<String> = absent
        .iter()
        .map(|(owner, name)| format!("{sha}:{}", store::assets_rel(owner, name)))
        .collect();
    Ok(git::read_blobs(root, &specs)?
        .into_iter()
        .zip(absent)
        .filter_map(|(blob, (page_id, name))| {
            Some(store::IncomingAsset { page_id, name, bytes: blob? })
        })
        .collect())
}

/// The text of a page as it stood right before `sha` deleted it.
pub fn deleted_page_text(root: &Path, sha: &str, id: &str) -> Result<RevisionText, String> {
    check_page_id(id)?;
    check_sha(sha)?;
    page_text_at(root, &git::parent_of(sha), id)
}

struct Placement<'a> {
    node: &'a PageNode,
    section: &'a Section,
    parent_id: Option<String>,
}

fn placement_in<'a>(nb: &'a Notebook, id: &str) -> Option<Placement<'a>> {
    store::locate_page_in(nb, id)
        .map(|(section, node, parent_id, _)| Placement { node, section, parent_id })
}

/// `nb_revs` is newest-first, so the snapshots at or before `at` are a
/// contiguous tail and the probe window is a slice of it.
fn probe_window(nb_revs: &[git::Revision], at: i64) -> Range<usize> {
    let start = nb_revs.partition_point(|r| r.at > at);
    start..(start + NB_PROBE_DEPTH).min(nb_revs.len())
}

fn notebook_spec(rev: &str) -> String {
    format!("{rev}:{}", store::NOTEBOOK_FILE)
}

fn to_deleted_child(node: &PageNode) -> DeletedChild {
    DeletedChild {
        id: node.id.clone(),
        title: node.title.clone(),
        children: node.children.iter().map(to_deleted_child).collect(),
    }
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

    // newest group wins per id; only ids that are genuinely gone right now
    // (not live, and not a same-session pending-purge file) are candidates.
    let mut seen: HashSet<&str> = HashSet::new();
    let mut candidates: Vec<(&git::DeletedGroup, &str)> = Vec::new();
    for group in &groups {
        for id in &group.ids {
            if seen.insert(id) && store::id_available_in(root, notebook, id) {
                candidates.push((group, id));
            }
        }
    }
    if candidates.is_empty() {
        return Ok(DeletedHistory { items: Vec::new(), hit_cap });
    }

    let nb_revs =
        git::log_file(root, store::NOTEBOOK_FILE, git::MAX_PAGE_REVISIONS).unwrap_or_default();
    let windows: Vec<Range<usize>> =
        candidates.iter().map(|(group, _)| probe_window(&nb_revs, group.at)).collect();
    let probed: Vec<usize> =
        windows.iter().cloned().flatten().collect::<BTreeSet<_>>().into_iter().collect();

    let mut specs: Vec<String> =
        probed.iter().map(|&i| notebook_spec(&nb_revs[i].sha)).collect();
    let nb_specs_len = specs.len();
    specs.extend(candidates.iter().map(|(group, id)| {
        format!("{}:{}", git::parent_of(&group.sha), store::page_rel(id))
    }));

    let mut blobs = git::read_blobs(root, &specs).unwrap_or_default();
    blobs.resize(specs.len(), None);
    let md_blobs = blobs.split_off(nb_specs_len);
    let snapshots: HashMap<usize, Notebook> = probed
        .iter()
        .zip(blobs)
        .filter_map(|(&i, bytes)| Some((i, serde_json::from_slice(&bytes?).ok()?)))
        .collect();

    let mut items: Vec<DeletedItem> = Vec::with_capacity(candidates.len());
    let mut covered: HashSet<String> = HashSet::new();

    for (i, (group, id)) in candidates.iter().enumerate() {
        let placement = windows[i]
            .clone()
            .filter_map(|probe| snapshots.get(&probe))
            .find_map(|nb| placement_in(nb, id));

        let (title, children): (String, Vec<DeletedChild>) = match &placement {
            Some(placed) => (
                placed.node.title.clone(),
                placed.node.children.iter().map(to_deleted_child).collect(),
            ),
            None => (
                title_or_untitled(md_blobs[i].as_deref().and_then(|b| std::str::from_utf8(b).ok())),
                Vec::new(),
            ),
        };
        collect_covered(&children, &mut covered);

        let section = placement.as_ref().map(|placed| placed.section);
        let parent_id = placement.as_ref().and_then(|placed| placed.parent_id.clone());
        items.push(DeletedItem {
            sha: group.sha.clone(),
            at: group.at,
            id: (*id).to_string(),
            title,
            section_id: section.map(|s| s.id.clone()),
            section_name: section.map(|s| s.name.clone()),
            section_exists: section
                .is_some_and(|s| notebook.sections.iter().any(|live| live.id == s.id)),
            parent_exists: parent_id
                .as_deref()
                .is_some_and(|pid| store::find_page_in(notebook, pid).is_some()),
            parent_id,
            page_count: placement.as_ref().map_or(1, |placed| store::subtree_len(placed.node)),
            children,
        });
    }

    // an id absorbed as a descendant of another (newer) item isn't its own
    // top-level entry — it's already shown nested under that item.
    items.retain(|it| !covered.contains(&it.id));

    Ok(DeletedHistory { items, hit_cap })
}

/// Fetches page bodies + assets for a whole recovered subtree in a constant
/// number of processes regardless of its size: one batch for every page's
/// `.md` blob, one tree listing for `assets/`, one batch for every matched
/// asset blob. A child whose blob is missing at `parent_sha` (and its own
/// descendants) is dropped rather than failing the whole restore.
fn build_incoming(root: &Path, sha: &str, node: PageNode) -> Result<(store::IncomingPage, usize), String> {
    let mut ids = Vec::new();
    store::collect_ids(&node, &mut ids);
    let parent_sha = git::parent_of(sha);

    let md_specs: Vec<String> =
        ids.iter().map(|id| format!("{parent_sha}:{}", store::page_rel(id))).collect();
    let mut bodies: HashMap<String, Vec<u8>> = ids
        .iter()
        .cloned()
        .zip(git::read_blobs(root, &md_specs)?)
        .filter_map(|(id, blob)| Some((id, blob?)))
        .collect();

    let owned: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let assets_prefix = format!("{}/", store::ASSETS_DIR);
    let asset_paths = git::list_tree_files(root, &parent_sha, &assets_prefix).unwrap_or_default();
    let mut asset_specs = Vec::new();
    let mut asset_owners: Vec<(&str, &str)> = Vec::new();
    for path in &asset_paths {
        let Some((owner, name)) =
            path.strip_prefix(&assets_prefix).and_then(|rest| rest.split_once('/'))
        else {
            continue;
        };
        if owned.contains(owner) {
            asset_specs.push(format!("{parent_sha}:{path}"));
            asset_owners.push((owner, name));
        }
    }

    let mut assets_by_id: HashMap<&str, Vec<(String, Vec<u8>)>> = HashMap::new();
    let mut asset_count = 0;
    for ((owner, name), blob) in asset_owners.into_iter().zip(git::read_blobs(root, &asset_specs)?) {
        if let Some(bytes) = blob {
            assets_by_id.entry(owner).or_default().push((name.to_string(), bytes));
            asset_count += 1;
        }
    }

    fn build(
        node: PageNode,
        bodies: &mut HashMap<String, Vec<u8>>,
        assets_by_id: &mut HashMap<&str, Vec<(String, Vec<u8>)>>,
    ) -> Option<store::IncomingPage> {
        let content = String::from_utf8(bodies.remove(&node.id)?).ok()?;
        let assets = assets_by_id.remove(node.id.as_str()).unwrap_or_default();
        let children = node
            .children
            .into_iter()
            .filter_map(|c| build(c, bodies, assets_by_id))
            .collect();
        Some(store::IncomingPage { id: node.id, content, assets, children })
    }

    let incoming = build(node, &mut bodies, &mut assets_by_id)
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
    check_page_id(id)?;
    check_sha(sha)?;
    if !store::id_available_in(root, notebook, id) {
        return Err("that page already exists".to_string());
    }

    let at = git::commit_time(root, sha)?;
    let nb_revs =
        git::log_file(root, store::NOTEBOOK_FILE, git::MAX_PAGE_REVISIONS).unwrap_or_default();
    let specs: Vec<String> =
        nb_revs[probe_window(&nb_revs, at)].iter().map(|r| notebook_spec(&r.sha)).collect();
    let found = git::read_blobs(root, &specs)
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .filter_map(|bytes| serde_json::from_slice::<Notebook>(&bytes).ok())
        .find_map(|nb| {
            placement_in(&nb, id)
                .map(|placed| (placed.node.clone(), placed.section.id.clone(), placed.parent_id))
        });

    let (node, hist_section_id, hist_parent_id) = match found {
        Some(v) => v,
        None => {
            let blob = page_text_at(root, &git::parent_of(sha), id).ok();
            let title = title_or_untitled(blob.as_ref().map(|b| b.text.as_str()));
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
    let parent_id = hist_parent_id.filter(|pid| store::find_page_in(notebook, pid).is_some());

    let page_count = store::subtree_len(&node);
    let (page, asset_count) = build_incoming(root, sha, node)?;

    Ok(Recovered { section_id, parent_id, page, page_count, asset_count })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets;
    use crate::git::{need_git, SnapshotKind};
    use crate::store::Store;
    use tempfile::tempdir;

    const TINY_PNG_B64: &str =
        "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    fn snapshot(root: &Path, kind: SnapshotKind) -> bool {
        git::snapshot(root, kind).unwrap()
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
            assert!(git::snapshot(&info.root, SnapshotKind::Close).unwrap());
            (parent.id, child.id, img, sid)
        };

        let store = Store::open(dir.path()).unwrap();
        let history = deleted_pages(&store.root, &store.notebook).unwrap();
        assert_eq!(history.items.len(), 1, "the child must be nested, not a second top-level item");
        let item = &history.items[0];
        assert_eq!(item.id, parent_id);
        assert_eq!(item.title, "Parent");
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
    fn missing_revision_assets_brings_back_a_pruned_image() {
        need_git!();
        let dir = tempdir().unwrap();
        let (page_id, img_rel) = {
            let mut store = Store::open(dir.path()).unwrap();
            let sid = store.notebook.sections[0].id.clone();
            let page = store.create_page(&sid, None, None).unwrap();
            let img = assets::save_image(&store, &page.id, TINY_PNG_B64, "png").unwrap();
            store.write_page(&page.id, &format!("# Shot\n\n![x]({img})\n")).unwrap();
            git::ensure_repo(&store.root).unwrap();
            assert!(snapshot(&store.root, SnapshotKind::Open));

            store.write_page(&page.id, "# Shot\n\nthe image is gone now\n").unwrap();
            let info = store.close();
            assert!(git::snapshot(&info.root, SnapshotKind::Close).unwrap());
            (page.id, img)
        };
        assert!(!dir.path().join(&img_rel).exists(), "close should have pruned it");

        let store = Store::open(dir.path()).unwrap();
        let revs = page_revisions(&store.root, &page_id).unwrap();
        let with_image = revs.last().unwrap();

        let recovered = missing_revision_assets(&store.root, &page_id, &with_image.sha).unwrap();
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].page_id, page_id);
        assert_eq!(store.restore_assets(&recovered).unwrap(), 1);
        assert!(dir.path().join(&img_rel).exists());

        assert!(
            missing_revision_assets(&store.root, &page_id, &with_image.sha).unwrap().is_empty(),
            "nothing is missing once it is back on disk"
        );
    }

    #[test]
    fn missing_revision_assets_is_empty_for_a_revision_with_no_images() {
        need_git!();
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        store.write_page(&page.id, "# Plain\n\nno pictures\n").unwrap();
        git::ensure_repo(&store.root).unwrap();
        assert!(snapshot(&store.root, SnapshotKind::Open));

        let revs = page_revisions(&store.root, &page.id).unwrap();
        assert!(missing_revision_assets(&store.root, &page.id, &revs[0].sha).unwrap().is_empty());
        assert!(missing_revision_assets(&store.root, &page.id, "not-a-sha").is_err());
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
