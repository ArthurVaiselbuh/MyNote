use serde::Serialize;
use std::collections::HashSet;
use std::ffi::OsStr;

use crate::store::{flatten_pages, new_id, subtree_len, PageNode, Section, Store};

// OneNote import produces flat sections; Markdown-folder import nests them.

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PagePreview {
    pub title: String,
    pub duplicate: bool,
    pub children: Vec<PagePreview>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SectionPreview {
    pub name: String,
    pub exists: bool,
    pub error: Option<String>,
    pub pages: Vec<PagePreview>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub sections: Vec<SectionPreview>,
    pub page_count: usize,
    pub duplicate_count: usize,
}

impl ImportPreview {
    pub fn from_sections(sections: Vec<SectionPreview>) -> Self {
        let (page_count, duplicate_count) =
            sections.iter().fold((0, 0), |(pages, dups), section| {
                let (section_pages, section_dups) = tally(&section.pages);
                (pages + section_pages, dups + section_dups)
            });
        ImportPreview {
            sections,
            page_count,
            duplicate_count,
        }
    }
}

fn tally(pages: &[PagePreview]) -> (usize, usize) {
    pages.iter().fold((0, 0), |(count, dups), page| {
        let (child_count, child_dups) = tally(&page.children);
        (
            count + 1 + child_count,
            dups + usize::from(page.duplicate) + child_dups,
        )
    })
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub section_ids: Vec<String>,
    pub page_count: usize,
}

impl ImportOutcome {
    pub fn add_section(&mut self, store: &mut Store, name: String, pages: Vec<PageNode>) {
        let id = new_id();
        self.page_count += pages.iter().map(subtree_len).sum::<usize>();
        self.section_ids.push(id.clone());
        store.notebook.sections.push(Section { id, name, pages });
    }
}

// Flags a title as a duplicate against an earlier page in the same import
// batch as well as against the notebook (case-insensitive).
pub struct DupTracker {
    seen: HashSet<String>,
}

impl DupTracker {
    pub fn new(store: &Store) -> Self {
        let seen = flatten_pages(&store.notebook)
            .into_iter()
            .map(|(_, p)| p.title.to_lowercase())
            .collect();
        DupTracker { seen }
    }

    pub fn mark(&mut self, title: &str) -> bool {
        !self.seen.insert(title.to_lowercase())
    }
}

pub fn section_exists(store: &Store, name: &str) -> bool {
    store
        .notebook
        .sections
        .iter()
        .any(|s| s.name.eq_ignore_ascii_case(name))
}

pub fn name_or(component: Option<&OsStr>, fallback: &str) -> String {
    component
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.into())
}
