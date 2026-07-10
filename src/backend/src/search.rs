use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Serialize;

use crate::store::{flatten_pages, PageNode, Section, Store};

const MAX_HITS: usize = 200;
const SNIPPET_CHARS: usize = 240;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub page_id: String,
    pub section_id: String,
    pub section_name: String,
    pub title: String,
    /// 1-based body line, 0 for a title match
    pub line_no: usize,
    pub snippet: String,
    pub ranges: Vec<(usize, usize)>,
    #[serde(skip)]
    pub score: i64,
}

pub fn search(store: &Store, query: &str, mode: &str) -> Result<Vec<SearchHit>, String> {
    if query.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut hits = match mode {
        "regex" => regex_search(store, query)?,
        _ => fuzzy_search(store, query),
    };
    hits.sort_by(|a, b| b.score.cmp(&a.score));
    hits.truncate(MAX_HITS);
    Ok(hits)
}

fn fuzzy_search(store: &Store, query: &str) -> Vec<SearchHit> {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut hits = Vec::new();

    for (section, page) in flatten_pages(&store.notebook) {
        indices.clear();
        let haystack = Utf32Str::new(&page.title, &mut buf);
        if let Some(score) = pattern.indices(haystack, &mut matcher, &mut indices) {
            let ranges = indices_to_ranges(&mut indices);
            hits.push(make_hit(section, page, 0, &page.title, ranges, score as i64 * 2));
        }
        let Ok(content) = store.read_page(&page.id) else {
            continue;
        };
        for (line_idx, line) in content.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            indices.clear();
            let haystack = Utf32Str::new(line, &mut buf);
            if let Some(score) = pattern.indices(haystack, &mut matcher, &mut indices) {
                let ranges = indices_to_ranges(&mut indices);
                hits.push(make_hit(section, page, line_idx + 1, line, ranges, score as i64));
            }
        }
    }
    hits
}

fn regex_search(store: &Store, query: &str) -> Result<Vec<SearchHit>, String> {
    let re = regex::RegexBuilder::new(query)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("invalid regex: {e}"))?;
    let mut hits = Vec::new();

    for (section, page) in flatten_pages(&store.notebook) {
        let title_ranges = regex_ranges(&re, &page.title);
        if !title_ranges.is_empty() {
            let score = title_ranges.len() as i64 * 20;
            hits.push(make_hit(section, page, 0, &page.title, title_ranges, score));
        }
        let Ok(content) = store.read_page(&page.id) else {
            continue;
        };
        for (line_idx, line) in content.lines().enumerate() {
            let ranges = regex_ranges(&re, line);
            if !ranges.is_empty() {
                let score = ranges.len() as i64 * 10;
                hits.push(make_hit(section, page, line_idx + 1, line, ranges, score));
            }
        }
    }
    Ok(hits)
}

fn make_hit(
    section: &Section,
    page: &PageNode,
    line_no: usize,
    line: &str,
    ranges: Vec<(usize, usize)>,
    score: i64,
) -> SearchHit {
    let (snippet, ranges) = window_snippet(line, &ranges);
    SearchHit {
        page_id: page.id.clone(),
        section_id: section.id.clone(),
        section_name: section.name.clone(),
        title: page.title.clone(),
        line_no,
        snippet,
        ranges,
        score,
    }
}

fn regex_ranges(re: &regex::Regex, text: &str) -> Vec<(usize, usize)> {
    re.find_iter(text)
        .filter(|m| m.end() > m.start())
        .map(|m| byte_to_char_range(text, m.start(), m.end()))
        .collect()
}

fn byte_to_char_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let char_start = text[..start].chars().count();
    let char_end = char_start + text[start..end].chars().count();
    (char_start, char_end)
}

fn indices_to_ranges(indices: &mut Vec<u32>) -> Vec<(usize, usize)> {
    indices.sort_unstable();
    indices.dedup();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &i in indices.iter() {
        let i = i as usize;
        match out.last_mut() {
            Some(last) if last.1 == i => last.1 = i + 1,
            _ => out.push((i, i + 1)),
        }
    }
    out
}

fn window_snippet(line: &str, ranges: &[(usize, usize)]) -> (String, Vec<(usize, usize)>) {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= SNIPPET_CHARS {
        return (line.to_string(), ranges.to_vec());
    }
    let first = ranges.first().map(|r| r.0).unwrap_or(0);
    let start = first
        .saturating_sub(60)
        .min(chars.len().saturating_sub(SNIPPET_CHARS));
    let end = (start + SNIPPET_CHARS).min(chars.len());
    let snippet: String = chars[start..end].iter().collect();
    let shifted = ranges
        .iter()
        .filter(|r| r.0 >= start && r.1 <= end)
        .map(|r| (r.0 - start, r.1 - start))
        .collect();
    (snippet, shifted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use tempfile::tempdir;

    fn store_with_pages() -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let a = store.create_page(&sid, None, None).unwrap();
        store
            .write_page(&a.id, "# Grocery List\n\nbuy milk and eggs\ncall the plumber\n")
            .unwrap();
        let b = store.create_page(&sid, None, None).unwrap();
        store
            .write_page(&b.id, "# Meeting Notes\n\ndiscussed deployment pipeline\n")
            .unwrap();
        (dir, store)
    }

    #[test]
    fn fuzzy_finds_title_and_body() {
        let (_dir, store) = store_with_pages();
        let hits = search(&store, "grocery", "fuzzy").unwrap();
        assert!(hits.iter().any(|h| h.title == "Grocery List" && h.line_no == 0));
        let hits = search(&store, "milk", "fuzzy").unwrap();
        let body_hit = hits.iter().find(|h| h.line_no > 0).unwrap();
        assert!(body_hit.snippet.contains("milk"));
        assert!(!body_hit.ranges.is_empty());
        let (start, end) = body_hit.ranges[0];
        let matched: String = body_hit.snippet.chars().skip(start).take(end - start).collect();
        assert_eq!(matched.to_lowercase(), "milk");
    }

    #[test]
    fn regex_matches_with_ranges() {
        let (_dir, store) = store_with_pages();
        let hits = search(&store, r"m\w+k", "regex").unwrap();
        assert!(!hits.is_empty());
        let hit = hits.iter().find(|h| h.snippet.contains("milk")).unwrap();
        let (start, end) = hit.ranges[0];
        let matched: String = hit.snippet.chars().skip(start).take(end - start).collect();
        assert_eq!(matched, "milk");
    }

    #[test]
    fn invalid_regex_is_an_error() {
        let (_dir, store) = store_with_pages();
        assert!(search(&store, r"([unclosed", "regex").is_err());
    }

    #[test]
    fn empty_query_returns_nothing() {
        let (_dir, store) = store_with_pages();
        assert!(search(&store, "   ", "fuzzy").unwrap().is_empty());
    }

    #[test]
    fn long_lines_are_windowed() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let page = store.create_page(&sid, None, None).unwrap();
        let long = format!("# Long\n\n{}needle{}\n", "x".repeat(500), "y".repeat(500));
        store.write_page(&page.id, &long).unwrap();
        let hits = search(&store, "needle", "regex").unwrap();
        let hit = &hits[0];
        assert!(hit.snippet.chars().count() <= 240);
        let (start, end) = hit.ranges[0];
        let matched: String = hit.snippet.chars().skip(start).take(end - start).collect();
        assert_eq!(matched, "needle");
    }
}
