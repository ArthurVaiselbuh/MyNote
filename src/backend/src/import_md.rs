use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::import::{
    name_or, section_exists, DupTracker, ImportOutcome, ImportPreview, PagePreview, SectionPreview,
};
use crate::store::{extract_title, new_id, set_title, PageNode, Store};

use crate::err;

struct MdPage {
    title: String,
    // H1-normalized and ready to write verbatim into a `<uuid>.md` file; empty
    // under `Bodies::Skip`
    content: String,
    children: Vec<MdPage>,
}

struct MdSection {
    name: String,
    pages: Vec<MdPage>,
}

struct Entry {
    path: PathBuf,
    name: String,
    is_dir: bool,
}

// The preview needs only titles, so it walks the same tree without pulling
// every markdown file into memory.
#[derive(Clone, Copy, PartialEq)]
enum Bodies {
    Read,
    Skip,
}

pub fn inspect(store: &Store, root: &str) -> Result<ImportPreview, String> {
    let sections = build_sections(Path::new(root), Bodies::Skip)?;
    let mut dup = DupTracker::new(store);
    let previews = sections
        .into_iter()
        .map(|section| SectionPreview {
            exists: section_exists(store, &section.name),
            error: None,
            pages: preview_pages(section.pages, &mut dup),
            name: section.name,
        })
        .collect();
    Ok(ImportPreview::from_sections(previews))
}

pub fn import(store: &mut Store, root: &str) -> Result<ImportOutcome, String> {
    let mut outcome = ImportOutcome::default();
    for section in build_sections(Path::new(root), Bodies::Read)? {
        let pages = write_pages(store, section.pages)?;
        outcome.add_section(store, section.name, pages);
    }
    store.save()?;
    Ok(outcome)
}

fn preview_pages(pages: Vec<MdPage>, dup: &mut DupTracker) -> Vec<PagePreview> {
    pages
        .into_iter()
        .map(|page| PagePreview {
            duplicate: dup.mark(&page.title),
            title: page.title,
            children: preview_pages(page.children, dup),
        })
        .collect()
}

fn write_pages(store: &Store, pages: Vec<MdPage>) -> Result<Vec<PageNode>, String> {
    let mut out = Vec::new();
    for page in pages {
        let id = new_id();
        fs::write(store.page_path(&id), &page.content).map_err(err)?;
        let children = write_pages(store, page.children)?;
        out.push(PageNode {
            id,
            title: page.title,
            expanded: true,
            children,
        });
    }
    Ok(out)
}

// ---------- filesystem -> MdSection tree ----------

// Top-level subfolders become sections; loose top-level .md files collect into
// one section named after the chosen folder (only when any exist). A README
// directly inside a section folder is just another page — unlike a nested
// folder, a section has no body to absorb it into.
fn build_sections(root: &Path, bodies: Bodies) -> Result<Vec<MdSection>, String> {
    let entries = sorted_entries(root)?;
    let mut sections = Vec::new();
    let mut loose = Vec::new();
    for entry in &entries {
        if entry.is_dir {
            let pages = build_pages(&sorted_entries(&entry.path)?, bodies)?;
            if !pages.is_empty() {
                sections.push(MdSection {
                    name: entry.name.clone(),
                    pages,
                });
            }
        } else if is_md(&entry.name) {
            loose.push(read_md_page(&entry.path, bodies)?);
        }
    }
    if !loose.is_empty() {
        sections.push(MdSection {
            name: name_or(root.file_name(), "Imported"),
            pages: loose,
        });
    }
    if sections.is_empty() {
        return Err("no markdown files found in that folder".into());
    }
    Ok(sections)
}

fn build_pages<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    bodies: Bodies,
) -> Result<Vec<MdPage>, String> {
    let mut pages = Vec::new();
    for entry in entries {
        if entry.is_dir {
            if let Some(page) = folder_to_page(&entry.path, &entry.name, bodies)? {
                pages.push(page);
            }
        } else if is_md(&entry.name) {
            pages.push(read_md_page(&entry.path, bodies)?);
        }
    }
    Ok(pages)
}

// A subfolder becomes a parent page. Its body is a README/index if present
// (absorbed, not re-listed), otherwise a stub `# <folder>`. Folders with no
// markdown anywhere are skipped rather than shown as empty stubs.
fn folder_to_page(path: &Path, name: &str, bodies: Bodies) -> Result<Option<MdPage>, String> {
    let entries = sorted_entries(path)?;
    let body_at = entries
        .iter()
        .position(|e| !e.is_dir && is_folder_body(&e.name));
    let children = build_pages(
        entries
            .iter()
            .enumerate()
            .filter(|(i, _)| Some(*i) != body_at)
            .map(|(_, entry)| entry),
        bodies,
    )?;
    if body_at.is_none() && children.is_empty() {
        return Ok(None);
    }
    let content = match body_at {
        _ if bodies == Bodies::Skip => String::new(),
        Some(i) => set_title(&fs::read_to_string(&entries[i].path).map_err(err)?, name),
        None => format!("# {name}\n"),
    };
    Ok(Some(MdPage {
        title: name.to_string(),
        content,
        children,
    }))
}

fn read_md_page(path: &Path, bodies: Bodies) -> Result<MdPage, String> {
    let raw = match bodies {
        Bodies::Read => fs::read_to_string(path).map_err(err)?,
        Bodies::Skip => read_heading_line(path)?,
    };
    let title = extract_title(&raw).unwrap_or_else(|| name_or(path.file_stem(), "Untitled"));
    let content = match bodies {
        Bodies::Read => set_title(&raw, &title),
        Bodies::Skip => String::new(),
    };
    Ok(MdPage {
        title,
        content,
        children: vec![],
    })
}

// `extract_title` reads no further than the first non-blank line, so this is
// all the preview needs from a page.
fn read_heading_line(path: &Path) -> Result<String, String> {
    let mut reader = BufReader::new(fs::File::open(path).map_err(err)?);
    let mut line = String::new();
    while reader.read_line(&mut line).map_err(err)? > 0 {
        if !line.trim().is_empty() {
            break;
        }
        line.clear();
    }
    Ok(line)
}

fn sorted_entries(dir: &Path) -> Result<Vec<Entry>, String> {
    let mut entries: Vec<Entry> = fs::read_dir(dir)
        .map_err(err)?
        .filter_map(Result::ok)
        .map(|e| {
            let path = e.path();
            Entry {
                is_dir: path.is_dir(),
                name: e.file_name().to_string_lossy().into_owned(),
                path,
            }
        })
        .collect();
    entries.sort_by_cached_key(|e| e.name.to_lowercase());
    Ok(entries)
}

fn is_md(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, ext)| {
        ext.eq_ignore_ascii_case("md") || ext.eq_ignore_ascii_case("markdown")
    })
}

fn is_folder_body(name: &str) -> bool {
    name.eq_ignore_ascii_case("readme.md") || name.eq_ignore_ascii_case("index.md")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn scratch() -> (tempfile::TempDir, PathBuf, Store) {
        let dir = tempdir().unwrap();
        let root = dir.path().join("Export");
        let store = Store::open(&dir.path().join("nb")).unwrap();
        (dir, root, store)
    }

    fn sample_tree(root: &Path) {
        write(&root.join("Work").join("a.md"), "# Alpha\n\nbody\n");
        write(
            &root.join("Work").join("Meetings").join("standup.md"),
            "just text, no heading\n",
        );
        write(
            &root.join("Personal").join("README.md"),
            "# Personal notes\n\ndetail\n",
        );
        write(&root.join("loose.md"), "# Loose\n\ntop level\n");
    }

    #[test]
    fn inspect_reports_sections_and_nesting() {
        let (_dir, root, store) = scratch();
        sample_tree(&root);

        let preview = inspect(&store, root.to_str().unwrap()).unwrap();
        let names: Vec<_> = preview.sections.iter().map(|s| s.name.as_str()).collect();
        // top folders first (sorted), then the loose section named after root
        assert_eq!(names, vec!["Personal", "Work", "Export"]);
        // Alpha, Meetings (stub), standup, Personal notes, Loose
        assert_eq!(preview.page_count, 5);

        let work = preview.sections.iter().find(|s| s.name == "Work").unwrap();
        assert_eq!(work.pages[0].title, "Alpha");
        assert_eq!(work.pages[1].title, "Meetings");
        assert_eq!(work.pages[1].children.len(), 1);
        assert_eq!(work.pages[1].children[0].title, "standup");

        // Personal is a top-level folder -> section; its README is a normal page
        let personal = preview
            .sections
            .iter()
            .find(|s| s.name == "Personal")
            .unwrap();
        assert_eq!(personal.pages[0].title, "Personal notes");
    }

    #[test]
    fn import_creates_tree_and_uuid_files() {
        let (_dir, root, mut store) = scratch();
        sample_tree(&root);

        let outcome = import(&mut store, root.to_str().unwrap()).unwrap();
        assert_eq!(outcome.page_count, 5);
        assert_eq!(outcome.section_ids.len(), 3);

        let work = store
            .notebook
            .sections
            .iter()
            .find(|s| s.name == "Work")
            .unwrap();
        let meetings = &work.pages[1];
        assert_eq!(meetings.title, "Meetings");
        assert_eq!(meetings.children.len(), 1);

        // the H1-less source page gets a heading derived from its filename stem
        let standup = &meetings.children[0];
        let body = store.read_page(&standup.id).unwrap();
        assert!(body.starts_with("# standup\n"));
        assert!(body.contains("just text, no heading"));

        // every page landed as a <uuid>.md file in the notebook root
        assert!(store.page_path(&work.pages[0].id).exists());

        let nb_root = store.root.clone();
        drop(store);
        let reopened = Store::open(&nb_root).unwrap();
        assert!(reopened.notebook.sections.iter().any(|s| s.name == "Export"));
    }

    #[test]
    fn nested_subfolder_becomes_stub_parent_page() {
        let (_dir, root, mut store) = scratch();
        // Work has no direct md, only a subfolder with one -> Work section,
        // Sub becomes a stub parent page holding the child.
        write(&root.join("Work").join("Sub").join("child.md"), "# Child\n");

        import(&mut store, root.to_str().unwrap()).unwrap();
        let work = store
            .notebook
            .sections
            .iter()
            .find(|s| s.name == "Work")
            .unwrap();
        assert_eq!(work.pages[0].title, "Sub");
        let stub = store.read_page(&work.pages[0].id).unwrap();
        assert_eq!(stub, "# Sub\n");
        assert_eq!(work.pages[0].children[0].title, "Child");
    }

    #[test]
    fn markdown_extensions_are_case_insensitive() {
        let (_dir, root, store) = scratch();
        write(&root.join("Work").join("a.MD"), "# Upper\n");
        write(&root.join("Work").join("b.Markdown"), "# Long\n");
        write(&root.join("Work").join("c.txt"), "# Ignored\n");

        let preview = inspect(&store, root.to_str().unwrap()).unwrap();
        let titles: Vec<_> = preview.sections[0]
            .pages
            .iter()
            .map(|p| p.title.as_str())
            .collect();
        assert_eq!(titles, vec!["Upper", "Long"]);
    }

    #[test]
    fn only_the_first_folder_body_is_absorbed() {
        let (_dir, root, mut store) = scratch();
        let sub = root.join("Work").join("Sub");
        write(&sub.join("index.md"), "# Index body\n");
        write(&sub.join("README.md"), "# Readme body\n");

        import(&mut store, root.to_str().unwrap()).unwrap();
        let work = store
            .notebook
            .sections
            .iter()
            .find(|s| s.name == "Work")
            .unwrap();
        // sorted: index.md precedes README.md, so index becomes the folder body
        assert_eq!(store.read_page(&work.pages[0].id).unwrap(), "# Sub\n");
        assert_eq!(work.pages[0].children.len(), 1);
        assert_eq!(work.pages[0].children[0].title, "Readme body");
    }

    #[test]
    fn empty_folder_yields_no_markdown_error() {
        let (dir, _root, store) = scratch();
        let empty = dir.path().join("Empty");
        fs::create_dir_all(&empty).unwrap();
        assert!(inspect(&store, empty.to_str().unwrap()).is_err());
    }

    #[test]
    fn duplicate_titles_flagged_against_notebook() {
        let (_dir, root, mut store) = scratch();
        write(&root.join("Work").join("a.md"), "# Notes\n");
        // the default notebook ships a section but no pages; add a page named Notes
        let sid = store.notebook.sections[0].id.clone();
        let p = store.create_page(&sid, None, None).unwrap();
        store.rename_page(&p.id, "Notes").unwrap();

        let preview = inspect(&store, root.to_str().unwrap()).unwrap();
        assert_eq!(preview.duplicate_count, 1);
    }
}
