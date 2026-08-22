use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use percent_encoding::{percent_decode_str, utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::store::{self, flatten_pages, Store};

use crate::err;

pub const FILES_DIR: &str = "files";
pub const TRASH_DIR: &str = "trash";

pub fn files_dir_in(root: &Path, page_id: &str) -> PathBuf {
    root.join(FILES_DIR).join(page_id)
}

pub fn files_rel_dir(page_id: &str) -> String {
    format!("{FILES_DIR}/{page_id}/")
}

pub fn files_rel(page_id: &str, name: &str) -> String {
    files_rel_dir(page_id) + name
}

pub fn trash_dir_in(root: &Path, page_id: &str) -> PathBuf {
    root.join(TRASH_DIR).join(page_id)
}

const RESERVED_STEMS: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL",
    "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
    "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

const ILLEGAL_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

const MAX_NAME_LEN: usize = 120;

/// The stem/extension split a reserved-device check and a length cap both need:
/// a leading dot (`.gitignore`) has no real stem, so it is never mistaken for
/// one — unlike a naive `rsplit_once('.')`.
fn split_ext(name: &str) -> (&str, Option<&str>) {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, Some(ext)),
        _ => (name, None),
    }
}

/// Takes only the final path component (defeats a `../` payload from a raw
/// clipboard filename), replaces the Windows-illegal character set and
/// control chars, strips trailing dots/spaces, rejects a reserved device
/// stem, and caps total length while keeping the extension intact.
pub fn safe_file_name(raw: &str) -> Option<String> {
    let name = Path::new(raw).file_name()?.to_str()?;
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_control() || ILLEGAL_CHARS.contains(&c) { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        return None;
    }
    let (stem, _) = split_ext(trimmed);
    if RESERVED_STEMS.iter().any(|r| stem.eq_ignore_ascii_case(r)) {
        return None;
    }
    Some(cap_len(trimmed, MAX_NAME_LEN))
}

fn cap_len(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    match split_ext(name) {
        (stem, Some(ext)) => {
            let budget = max.saturating_sub(ext.chars().count() + 1).max(1);
            format!("{}.{ext}", stem.chars().take(budget).collect::<String>())
        }
        (stem, None) => stem.chars().take(max).collect(),
    }
}

fn dedupe(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_string();
    }
    let (stem, ext) = split_ext(name);
    let mut counter = 1;
    loop {
        let candidate = match ext {
            Some(ext) => format!("{stem}-{counter}.{ext}"),
            None => format!("{stem}-{counter}"),
        };
        if !dir.join(&candidate).exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Unreserved chars (RFC 3986) plus the ones stay unescaped so an ordinary
/// name is still readable in the raw markdown; everything else — spaces,
/// parens, quotes — is escaped so the link keeps working as CommonMark.
const LINK_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

fn encode_name(name: &str) -> String {
    utf8_percent_encode(name, LINK_ENCODE_SET).to_string()
}

pub fn attach_path(store: &Store, page_id: &str, source: &Path) -> Result<String, String> {
    if store.find_page(page_id).is_none() {
        return Err("page not found".into());
    }
    let raw = source
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid source path")?;
    let name = safe_file_name(raw).ok_or("invalid file name")?;
    let dir = files_dir_in(&store.root, page_id);
    fs::create_dir_all(&dir).map_err(err)?;
    let name = dedupe(&dir, &name);
    fs::copy(source, dir.join(&name)).map_err(err)?;
    Ok(files_rel(page_id, &encode_name(&name)))
}

pub fn attach_bytes(store: &Store, page_id: &str, name: &str, data_b64: &str) -> Result<String, String> {
    if store.find_page(page_id).is_none() {
        return Err("page not found".into());
    }
    let name = safe_file_name(name).ok_or("invalid file name")?;
    let bytes = STANDARD.decode(data_b64).map_err(err)?;
    let dir = files_dir_in(&store.root, page_id);
    fs::create_dir_all(&dir).map_err(err)?;
    let name = dedupe(&dir, &name);
    fs::write(dir.join(&name), bytes).map_err(err)?;
    Ok(files_rel(page_id, &encode_name(&name)))
}

fn continues_name(c: char) -> bool {
    !c.is_whitespace() && !matches!(c, '(' | ')' | '"' | '\'' | '<' | '>')
}

/// The shared link-scanning pass behind both `assets::referenced_assets` and
/// `referenced` below: walk every `"<dir>/<page-id>/<name>"` occurrence in a
/// page body. Names come back exactly as written in the markdown — encoded or
/// not is the caller's concern.
pub(crate) fn scan_refs(content: &str, dir: &str) -> Vec<(String, String)> {
    let prefix = format!("{dir}/");
    let mut found = Vec::new();
    let mut rest = content;
    while let Some(at) = rest.find(&prefix) {
        rest = &rest[at + prefix.len()..];
        let Some((page_id, tail)) = rest.split_once('/') else {
            continue;
        };
        if !store::is_page_id(page_id) {
            continue;
        }
        let name: String = tail.chars().take_while(|c| continues_name(*c)).collect();
        if !name.is_empty() {
            found.push((page_id.to_string(), name));
        }
    }
    found
}

/// Like `assets::referenced_assets`, but percent-decodes the name back to what
/// is actually on disk — an attachment's original filename, unlike a
/// generated image name, routinely contains a space or other character that
/// only survives the round trip encoded.
pub fn referenced(content: &str) -> Vec<(String, String)> {
    scan_refs(content, FILES_DIR)
        .into_iter()
        .map(|(page_id, raw)| {
            let name = percent_decode_str(&raw).decode_utf8_lossy().into_owned();
            (page_id, name)
        })
        .collect()
}

/// The `assets::prune` shape, except every removal is a move into
/// `trash/<page-id>/`, never an unlink — attachments are never versioned, so
/// deleting one for real would be unrecoverable.
pub fn prune_to_trash(store: &Store) -> Result<usize, String> {
    let dir = store.root.join(FILES_DIR);
    if !dir.is_dir() {
        return Ok(0);
    }
    let live_pages: HashSet<&str> = flatten_pages(&store.notebook)
        .iter()
        .map(|(_, page)| page.id.as_str())
        .collect();
    let mut moved = 0;

    // Read every live page once and pool their attachment links, since a file
    // under `files/<page-id>/` can be linked from a *different* page than the
    // one whose folder it lives in — checking only the owning page's own body
    // would trash a still-linked attachment.
    let mut readable_pages: HashSet<&str> = HashSet::new();
    let mut referenced_anywhere: HashSet<(String, String)> = HashSet::new();
    for &page_id in &live_pages {
        match store.read_page(page_id) {
            Ok(content) => {
                readable_pages.insert(page_id);
                referenced_anywhere.extend(referenced(&content));
            }
            Err(e) => log::warn!("skipping file prune for page {page_id}: {e}"),
        }
    }

    for entry in fs::read_dir(&dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        let page_dir = entry.path();
        if !page_dir.is_dir() {
            continue;
        }
        let page_id = entry.file_name().to_string_lossy().into_owned();
        if readable_pages.contains(page_id.as_str()) {
            moved += move_unreferenced(&store.root, &page_dir, &page_id, &referenced_anywhere)?;
        }
    }
    Ok(moved)
}

fn move_unreferenced(
    root: &Path,
    dir: &Path,
    page_id: &str,
    referenced_anywhere: &HashSet<(String, String)>,
) -> Result<usize, String> {
    let mut moved = 0;
    let mut kept = 0;
    for file in fs::read_dir(dir).map_err(err)? {
        let file = file.map_err(err)?;
        let name = file.file_name().to_string_lossy().into_owned();
        if referenced_anywhere.contains(&(page_id.to_string(), name.clone())) {
            kept += 1;
            continue;
        }
        move_one(root, page_id, &file.path(), &name)?;
        moved += 1;
    }
    if kept == 0 {
        let _ = fs::remove_dir(dir);
    }
    Ok(moved)
}

/// Moves every entry directly under `dir` — files and subdirectories alike —
/// into `trash/<page_id>/`, then drops the now-empty source directory.
pub(crate) fn move_dir_to_trash(root: &Path, dir: &Path, page_id: &str) -> Result<usize, String> {
    let mut moved = 0;
    for entry in fs::read_dir(dir).map_err(err)? {
        let entry = entry.map_err(err)?;
        let name = entry.file_name().to_string_lossy().into_owned();
        move_one(root, page_id, &entry.path(), &name)?;
        moved += 1;
    }
    let _ = fs::remove_dir(dir);
    Ok(moved)
}

pub(crate) fn move_file_to_trash(root: &Path, page_id: &str, path: &Path) -> Result<(), String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid file name")?
        .to_string();
    move_one(root, page_id, path, &name)
}

fn move_one(root: &Path, page_id: &str, path: &Path, name: &str) -> Result<(), String> {
    let trash = trash_dir_in(root, page_id);
    fs::create_dir_all(&trash).map_err(err)?;
    let dest_name = dedupe(&trash, name);
    fs::rename(path, trash.join(dest_name)).map_err(err)
}

/// The app's only unlink of a trashed file: user-initiated alone, never from a
/// close or a timer.
pub fn empty_trash(root: &Path) -> Result<usize, String> {
    let trash = root.join(TRASH_DIR);
    let (count, _) = trash_stats(root);
    if trash.is_dir() {
        fs::remove_dir_all(&trash).map_err(err)?;
    }
    Ok(count)
}

pub fn trash_stats(root: &Path) -> (usize, u64) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    walk_stats(&root.join(TRASH_DIR), &mut count, &mut bytes);
    (count, bytes)
}

fn walk_stats(dir: &Path, count: &mut usize, bytes: &mut u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_stats(&path, count, bytes);
        } else if let Ok(meta) = entry.metadata() {
            *count += 1;
            *bytes += meta.len();
        }
    }
}

/// The security boundary for `reveal_attachment`: rejects a path that isn't
/// under `files/` or that carries `..`/`\` (raw or percent-encoded) before it
/// ever touches the filesystem, then canonicalizes both the candidate and the
/// notebook root so a symlink planted inside `files/` cannot walk the reveal
/// outside the notebook.
pub fn resolve_attachment_path(root: &Path, rel: &str) -> Result<PathBuf, String> {
    let prefix = format!("{FILES_DIR}/");
    if !rel.starts_with(&prefix) || rel.contains("..") || rel.contains('\\') {
        return Err("invalid attachment path".into());
    }
    let decoded = percent_decode_str(rel).decode_utf8().map_err(err)?.into_owned();
    if decoded.contains("..") || decoded.contains('\\') {
        return Err("invalid attachment path".into());
    }
    let candidate = root.join(&decoded);
    let canon_root = root.canonicalize().map_err(err)?;
    let canon_candidate = candidate.canonicalize().map_err(err)?;
    if !canon_candidate.starts_with(&canon_root) {
        return Err("invalid attachment path".into());
    }
    Ok(canon_candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn safe_file_name_replaces_illegal_chars() {
        // `/` and `\` never reach `safe_file_name` as literal characters — they
        // are path separators, so `Path::file_name()` already strips everything
        // before the last one (covered by the path-collapsing test below).
        assert_eq!(safe_file_name("a<b>c:d\"e|f?g*h.txt").unwrap(), "a_b_c_d_e_f_g_h.txt");
    }

    #[test]
    fn safe_file_name_strips_trailing_dots_and_spaces() {
        assert_eq!(safe_file_name("trailing.dots...  ").unwrap(), "trailing.dots");
    }

    #[test]
    fn safe_file_name_rejects_reserved_stems() {
        assert!(safe_file_name("CON").is_none());
        assert!(safe_file_name("con.txt").is_none());
        assert!(safe_file_name("COM1").is_none());
        assert!(safe_file_name("LPT9.log").is_none());
        assert!(safe_file_name("Console.txt").is_some());
        assert!(safe_file_name(".gitignore").is_some());
    }

    #[test]
    fn safe_file_name_caps_length_while_preserving_extension() {
        let name = format!("{}.txt", "x".repeat(200));
        let capped = safe_file_name(&name).unwrap();
        assert_eq!(capped.chars().count(), MAX_NAME_LEN);
        assert!(capped.ends_with(".txt"));
    }

    #[test]
    fn safe_file_name_rejects_empty_results() {
        assert!(safe_file_name("").is_none());
        assert!(safe_file_name("   ").is_none());
        assert!(safe_file_name("...").is_none());
    }

    #[test]
    fn safe_file_name_collapses_a_path_to_its_final_component() {
        assert_eq!(safe_file_name("../../evil.sh").unwrap(), "evil.sh");
    }

    #[test]
    fn attach_path_copies_keeps_name_and_dedupes_a_collision() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("Q3 budget.xlsx");
        fs::write(&src, b"data").unwrap();

        let rel1 = attach_path(&store, &page.id, &src).unwrap();
        assert!(dir.path().join(files_dir_in(dir.path(), &page.id)).join("Q3 budget.xlsx").exists());
        assert_eq!(rel1, files_rel(&page.id, "Q3%20budget.xlsx"));

        let rel2 = attach_path(&store, &page.id, &src).unwrap();
        assert!(files_dir_in(dir.path(), &page.id).join("Q3 budget-1.xlsx").exists());
        assert_eq!(rel2, files_rel(&page.id, "Q3%20budget-1.xlsx"));
    }

    #[test]
    fn attach_path_rejects_unknown_page() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("a.txt");
        fs::write(&src, b"x").unwrap();
        assert!(attach_path(&store, "nope", &src).is_err());
    }

    #[test]
    fn referenced_round_trips_a_name_with_spaces_through_percent_encoding() {
        let id = "11111111-1111-1111-1111-111111111111";
        let body = format!("# T\n\n[Q3 budget.xlsx](files/{id}/Q3%20budget.xlsx)\n");
        assert_eq!(referenced(&body), vec![(id.to_string(), "Q3 budget.xlsx".to_string())]);
    }

    #[test]
    fn prune_to_trash_moves_unreferenced_files_of_live_pages_only() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let src_dir = tempdir().unwrap();
        fs::write(src_dir.path().join("kept.txt"), b"k").unwrap();
        fs::write(src_dir.path().join("orphan.txt"), b"o").unwrap();
        let kept_rel = attach_path(&store, &page.id, &src_dir.path().join("kept.txt")).unwrap();
        attach_path(&store, &page.id, &src_dir.path().join("orphan.txt")).unwrap();
        store
            .write_page(&page.id, &format!("# T\n\n[k]({kept_rel})\n"))
            .unwrap();

        let ghost_id = "22222222-2222-2222-2222-222222222222";
        let ghost_dir = files_dir_in(dir.path(), ghost_id);
        fs::create_dir_all(&ghost_dir).unwrap();
        fs::write(ghost_dir.join("stray.bin"), b"g").unwrap();

        let moved = prune_to_trash(&store).unwrap();
        assert_eq!(moved, 1);

        let page_files = files_dir_in(dir.path(), &page.id);
        assert!(page_files.join("kept.txt").exists());
        assert!(!page_files.join("orphan.txt").exists());
        assert!(trash_dir_in(dir.path(), &page.id).join("orphan.txt").exists());

        assert!(ghost_dir.exists());
    }

    #[test]
    fn a_deleted_pages_attachments_survive_prune_and_go_to_trash_at_close() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("note.pdf");
        fs::write(&src, b"pdf").unwrap();
        attach_path(&store, &page.id, &src).unwrap();

        store.delete_page(&page.id).unwrap();

        assert_eq!(prune_to_trash(&store).unwrap(), 0);
        assert!(files_dir_in(dir.path(), &page.id).join("note.pdf").exists());

        store.purge_pages_deleted_this_session();
        assert!(!files_dir_in(dir.path(), &page.id).exists());
        assert!(trash_dir_in(dir.path(), &page.id).join("note.pdf").exists());
    }

    #[test]
    fn prune_to_trash_keeps_a_file_referenced_only_from_another_page() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let owner = store.create_page(&sid, None, None).unwrap();
        let borrower = store.create_page(&sid, None, None).unwrap();

        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("shared.pdf");
        fs::write(&src, b"s").unwrap();
        let shared_rel = attach_path(&store, &owner.id, &src).unwrap();

        // owner's own body no longer links it, but another live page does
        store.write_page(&owner.id, "# Owner\n\nno links here\n").unwrap();
        store
            .write_page(&borrower.id, &format!("# Borrower\n\n[shared]({shared_rel})\n"))
            .unwrap();

        let moved = prune_to_trash(&store).unwrap();
        assert_eq!(moved, 0);
        assert!(files_dir_in(dir.path(), &owner.id).join("shared.pdf").exists());
    }

    #[test]
    fn close_moves_a_subdirectory_of_attachments_into_trash_instead_of_deleting_it() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let nested = files_dir_in(dir.path(), &page.id).join("subfolder");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("inside.bin"), b"n").unwrap();
        store.delete_page(&page.id).unwrap();

        store.purge_pages_deleted_this_session();
        assert!(!files_dir_in(dir.path(), &page.id).exists());
        assert!(trash_dir_in(dir.path(), &page.id).join("subfolder").join("inside.bin").exists());
    }

    #[test]
    fn trashing_dedupes_a_collision_inside_trash() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        fs::create_dir_all(files_dir_in(dir.path(), &page.id)).unwrap();
        fs::write(files_dir_in(dir.path(), &page.id).join("dup.txt"), b"fresh").unwrap();
        let existing_trash = trash_dir_in(dir.path(), &page.id);
        fs::create_dir_all(&existing_trash).unwrap();
        fs::write(existing_trash.join("dup.txt"), b"already-here").unwrap();
        store.delete_page(&page.id).unwrap();

        store.purge_pages_deleted_this_session();
        assert_eq!(fs::read(existing_trash.join("dup.txt")).unwrap(), b"already-here");
        assert_eq!(fs::read(existing_trash.join("dup-1.txt")).unwrap(), b"fresh");
    }

    #[test]
    fn empty_trash_is_the_only_thing_that_unlinks_trashed_files() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();

        let src_dir = tempdir().unwrap();
        let src = src_dir.path().join("note.pdf");
        fs::write(&src, b"pdf").unwrap();
        attach_path(&store, &page.id, &src).unwrap();
        store.delete_page(&page.id).unwrap();

        let stray_id = "33333333-3333-3333-3333-333333333333";
        fs::write(store.page_path(stray_id), "# Stray\n").unwrap();

        let info = store.close();
        let (count, bytes) = trash_stats(&info.root);
        assert_eq!(count, 2);
        assert!(bytes > 0);
        assert!(trash_dir_in(&info.root, &page.id).join("note.pdf").exists());
        assert!(trash_dir_in(&info.root, stray_id).join(format!("{stray_id}.md")).exists());

        assert_eq!(empty_trash(&info.root).unwrap(), 2);
        assert_eq!(trash_stats(&info.root), (0, 0));
        assert!(!info.root.join(TRASH_DIR).exists());
        assert_eq!(empty_trash(&info.root).unwrap(), 0);
    }

    #[test]
    fn resolve_attachment_path_rejects_traversal_wrong_prefix_and_absolute_paths() {
        let dir = tempdir().unwrap();
        let page_id = "11111111-1111-1111-1111-111111111111";
        let files_dir = files_dir_in(dir.path(), page_id);
        fs::create_dir_all(&files_dir).unwrap();
        fs::write(files_dir.join("a.txt"), "hi").unwrap();

        assert!(resolve_attachment_path(dir.path(), &format!("files/{page_id}/../../a.txt")).is_err());
        assert!(resolve_attachment_path(dir.path(), &format!("assets/{page_id}/a.txt")).is_err());
        assert!(resolve_attachment_path(dir.path(), "/etc/passwd").is_err());
        assert!(resolve_attachment_path(dir.path(), &files_rel(page_id, "a.txt")).is_ok());
    }

    #[test]
    fn resolve_attachment_path_rejects_a_symlink_escaping_root() {
        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "shh").unwrap();
        let page_id = "44444444-4444-4444-4444-444444444444";
        let files_dir = files_dir_in(dir.path(), page_id);
        fs::create_dir_all(&files_dir).unwrap();
        let link = files_dir.join("secret.txt");

        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(outside.path().join("secret.txt"), &link).is_ok();
        #[cfg(not(windows))]
        let made = std::os::unix::fs::symlink(outside.path().join("secret.txt"), &link).is_ok();
        if !made {
            eprintln!("skipping: could not create a symlink in this environment");
            return;
        }

        assert!(resolve_attachment_path(dir.path(), &files_rel(page_id, "secret.txt")).is_err());
    }
}
