use std::fs;
use std::path::{Path, PathBuf};

use crate::import::{
    section_exists, DupTracker, ImportOutcome, ImportPreview, PagePreview, SectionPreview,
};
use crate::store::{extract_title, new_id, set_title, PageNode, Section, Store};

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// A source page resolved from disk: content is already H1-normalized and ready
// to write verbatim into a `<uuid>.md` file.
struct MdPage {
    title: String,
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

pub fn inspect(store: &Store, root: &str) -> Result<ImportPreview, String> {
    let sections = build_sections(Path::new(root))?;
    if sections.is_empty() {
        return Err("no markdown files found in that folder".into());
    }
    let mut dup = DupTracker::new(store);
    let previews = sections
        .into_iter()
        .map(|section| SectionPreview {
            exists: section_exists(store, &section.name),
            error: None,
            pages: preview_pages(&section.pages, &mut dup),
            name: section.name,
        })
        .collect();
    Ok(ImportPreview::from_sections(previews))
}

pub fn import(store: &mut Store, root: &str) -> Result<ImportOutcome, String> {
    let sections = build_sections(Path::new(root))?;
    if sections.is_empty() {
        return Err("no markdown files found in that folder".into());
    }
    let mut outcome = ImportOutcome {
        section_ids: vec![],
        page_count: 0,
    };
    for section in sections {
        let id = new_id();
        let pages = write_pages(store, &section.pages, &mut outcome.page_count)?;
        store.notebook.sections.push(Section {
            id: id.clone(),
            name: section.name,
            pages,
        });
        outcome.section_ids.push(id);
    }
    store.save()?;
    Ok(outcome)
}

fn preview_pages(pages: &[MdPage], dup: &mut DupTracker) -> Vec<PagePreview> {
    pages
        .iter()
        .map(|page| PagePreview {
            duplicate: dup.mark(&page.title),
            title: page.title.clone(),
            children: preview_pages(&page.children, dup),
        })
        .collect()
}

fn write_pages(store: &Store, pages: &[MdPage], count: &mut usize) -> Result<Vec<PageNode>, String> {
    let mut out = Vec::new();
    for page in pages {
        let id = new_id();
        fs::write(store.page_path(&id), &page.content).map_err(err)?;
        *count += 1;
        let children = write_pages(store, &page.children, count)?;
        out.push(PageNode {
            id,
            title: page.title.clone(),
            expanded: true,
            children,
        });
    }
    Ok(out)
}

// ---------- filesystem -> MdSection tree ----------

// Top-level subfolders become sections; loose top-level .md files collect into
// one section named after the chosen folder (only when any exist).
fn build_sections(root: &Path) -> Result<Vec<MdSection>, String> {
    let entries = sorted_entries(root)?;
    let mut sections = Vec::new();
    let mut loose = Vec::new();
    for entry in &entries {
        if entry.is_dir {
            let pages = build_pages(&entry.path)?;
            if !pages.is_empty() {
                sections.push(MdSection {
                    name: entry.name.clone(),
                    pages,
                });
            }
        } else if is_md(&entry.name) {
            loose.push(read_md_page(&entry.path)?);
        }
    }
    if !loose.is_empty() {
        sections.push(MdSection {
            name: dir_name(root),
            pages: loose,
        });
    }
    Ok(sections)
}

// Contents of a section-level folder: subfolders nest as parent pages, .md
// files become pages. A README here is just another page (a section has no
// body to absorb it).
fn build_pages(dir: &Path) -> Result<Vec<MdPage>, String> {
    build_pages_from(&sorted_entries(dir)?, None)
}

fn build_pages_from(entries: &[Entry], exclude: Option<&Path>) -> Result<Vec<MdPage>, String> {
    let mut pages = Vec::new();
    for entry in entries {
        if entry.is_dir {
            if let Some(page) = folder_to_page(&entry.path, &entry.name)? {
                pages.push(page);
            }
        } else if is_md(&entry.name) && Some(entry.path.as_path()) != exclude {
            pages.push(read_md_page(&entry.path)?);
        }
    }
    Ok(pages)
}

// A subfolder becomes a parent page. Its body is a README/index if present
// (absorbed, not re-listed), otherwise a stub `# <folder>`. Folders with no
// markdown anywhere are skipped rather than shown as empty stubs.
fn folder_to_page(path: &Path, name: &str) -> Result<Option<MdPage>, String> {
    let entries = sorted_entries(path)?;
    let readme = entries
        .iter()
        .find(|e| !e.is_dir && is_folder_body(&e.name))
        .map(|e| e.path.clone());
    let children = build_pages_from(&entries, readme.as_deref())?;
    let content = match &readme {
        Some(body) => set_title(&fs::read_to_string(body).map_err(err)?, name),
        None => {
            if children.is_empty() {
                return Ok(None);
            }
            format!("# {name}\n")
        }
    };
    Ok(Some(MdPage {
        title: name.to_string(),
        content,
        children,
    }))
}

fn read_md_page(path: &Path) -> Result<MdPage, String> {
    let raw = fs::read_to_string(path).map_err(err)?;
    let title = extract_title(&raw).unwrap_or_else(|| file_stem(path));
    let content = set_title(&raw, &title);
    Ok(MdPage {
        title,
        content,
        children: vec![],
    })
}

fn sorted_entries(dir: &Path) -> Result<Vec<Entry>, String> {
    let mut entries: Vec<Entry> = fs::read_dir(dir)
        .map_err(err)?
        .filter_map(Result::ok)
        .map(|e| {
            let path = e.path();
            Entry {
                is_dir: path.is_dir(),
                name: e.file_name().to_string_lossy().to_string(),
                path,
            }
        })
        .collect();
    entries.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(entries)
}

fn is_md(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown")
}

fn is_folder_body(name: &str) -> bool {
    name.eq_ignore_ascii_case("readme.md") || name.eq_ignore_ascii_case("index.md")
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Untitled".into())
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Imported".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    // Builds:
    //   root/Work/a.md              (# Alpha)
    //   root/Work/Meetings/standup.md  (no H1)
    //   root/Personal/README.md     (# Personal notes)
    //   root/loose.md               (# Loose)
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
        let dir = tempdir().unwrap();
        let root = dir.path().join("Export");
        sample_tree(&root);
        let store = Store::open(&dir.path().join("nb")).unwrap();

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
        let personal = preview.sections.iter().find(|s| s.name == "Personal").unwrap();
        assert_eq!(personal.pages[0].title, "Personal notes");
    }

    #[test]
    fn import_creates_tree_and_uuid_files() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("Export");
        sample_tree(&root);
        let mut store = Store::open(&dir.path().join("nb")).unwrap();

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

        let root = store.root.clone();
        drop(store);
        let reopened = Store::open(&root).unwrap();
        assert!(reopened.notebook.sections.iter().any(|s| s.name == "Export"));
    }

    #[test]
    fn nested_subfolder_becomes_stub_parent_page() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("Export");
        // Work has no direct md, only a subfolder with one -> Work section,
        // Sub becomes a stub parent page holding the child.
        write(&root.join("Work").join("Sub").join("child.md"), "# Child\n");
        let mut store = Store::open(&dir.path().join("nb")).unwrap();

        import(&mut store, root.to_str().unwrap()).unwrap();
        let work = store.notebook.sections.iter().find(|s| s.name == "Work").unwrap();
        assert_eq!(work.pages[0].title, "Sub");
        let stub = store.read_page(&work.pages[0].id).unwrap();
        assert_eq!(stub, "# Sub\n");
        assert_eq!(work.pages[0].children[0].title, "Child");
    }

    #[test]
    fn empty_folder_yields_no_markdown_error() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("Empty");
        fs::create_dir_all(&root).unwrap();
        let store = Store::open(&dir.path().join("nb")).unwrap();
        assert!(inspect(&store, root.to_str().unwrap()).is_err());
    }

    #[test]
    fn duplicate_titles_flagged_against_notebook() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("Export");
        write(&root.join("Work").join("a.md"), "# Notes\n");
        let store = Store::open(&dir.path().join("nb")).unwrap();
        // the default notebook ships a section but no pages; add a page named Notes
        let mut store = store;
        let sid = store.notebook.sections[0].id.clone();
        let p = store.create_page(&sid, None, None).unwrap();
        store.rename_page(&p.id, "Notes").unwrap();

        let preview = inspect(&store, root.to_str().unwrap()).unwrap();
        assert_eq!(preview.duplicate_count, 1);
    }
}
