use serde::Serialize;
use std::collections::HashSet;

use crate::store::{flatten_pages, Store};

// Shared preview/outcome shapes for every importer so the import pane renders
// one tree regardless of source. OneNote import produces flat sections (no
// page children); Markdown-folder import nests them.

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PagePreview {
    pub title: String,
    pub duplicate: bool,
    #[serde(default)]
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
        let mut page_count = 0;
        let mut duplicate_count = 0;
        for section in &sections {
            tally(&section.pages, &mut page_count, &mut duplicate_count);
        }
        ImportPreview {
            sections,
            page_count,
            duplicate_count,
        }
    }
}

fn tally(pages: &[PagePreview], page_count: &mut usize, duplicate_count: &mut usize) {
    for page in pages {
        *page_count += 1;
        if page.duplicate {
            *duplicate_count += 1;
        }
        tally(&page.children, page_count, duplicate_count);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportOutcome {
    pub section_ids: Vec<String>,
    pub page_count: usize,
}

// Flags a title as a duplicate when it collides with an existing notebook page
// or an earlier page in the same import batch (case-insensitive).
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
