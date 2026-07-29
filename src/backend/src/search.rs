use std::cmp::Reverse;

use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Serialize;

use crate::store::{flatten_pages, PageNode, Section, Store};

const MAX_HITS: usize = 200;
const SNIPPET_CHARS: usize = 240;

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    /// the literal keywords/phrases the query split into — empty in regex mode
    pub terms: Vec<String>,
}

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
    pub rank: Rank,
}

/// field order is the ranking order: matching more of the query's distinct
/// terms always outweighs matching one term more often
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    pub terms_matched: usize,
    pub occurrences: usize,
    pub title_matched: bool,
}

struct Term {
    text: String,
    /// quoted terms are matched verbatim — never split, never fuzzy
    quoted: bool,
}

/// a matched line's index within `searchable_lines`, with its char ranges
type LineRanges = (usize, Vec<(usize, usize)>);

pub fn search(store: &Store, query: &str, mode: &str) -> Result<SearchResults, String> {
    if query.trim().is_empty() {
        return Ok(SearchResults { hits: vec![], terms: vec![] });
    }
    let (mut hits, terms) = match mode {
        "regex" => (regex_search(store, query)?, vec![]),
        _ => keyword_search(store, query),
    };
    hits.sort_by_key(|hit| Reverse(hit.rank));
    hits.truncate(MAX_HITS);
    Ok(SearchResults { hits, terms })
}

// ---------- keyword ("fuzzy" mode) ----------

fn keyword_search(store: &Store, query: &str) -> (Vec<SearchHit>, Vec<String>) {
    let terms = parse_terms(query);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut hits = Vec::new();
    for (section, page) in flatten_pages(&store.notebook) {
        let content = page_content(store, page);
        if let Some(hit) = page_hit(section, page, &content, &terms, &mut matcher) {
            hits.push(hit);
        }
    }
    (hits, terms.into_iter().map(|t| t.text).collect())
}

fn parse_terms(query: &str) -> Vec<Term> {
    let mut terms = Vec::new();
    let mut rest = query.trim_start();
    while !rest.is_empty() {
        let (text, quoted, tail) = match rest.strip_prefix('"') {
            Some(after_quote) => {
                let (phrase, tail) = after_quote.split_once('"').unwrap_or((after_quote, ""));
                (phrase, true, tail)
            }
            None => {
                let (word, tail) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
                (word, false, tail)
            }
        };
        if !text.trim().is_empty() {
            terms.push(Term { text: text.trim().to_string(), quoted });
        }
        rest = tail.trim_start();
    }
    terms
}

/// scores the whole page, then reports the single line that best represents it
fn page_hit(
    section: &Section,
    page: &PageNode,
    content: &str,
    terms: &[Term],
    matcher: &mut Matcher,
) -> Option<SearchHit> {
    let lines = searchable_lines(content);
    let lowered: Vec<Vec<char>> = lines.iter().map(|(_, text)| lower_chars(text)).collect();

    let mut ranges_per_line: Vec<Vec<(usize, usize)>> = vec![Vec::new(); lines.len()];
    let mut terms_per_line: Vec<usize> = vec![0; lines.len()];
    let mut rank = Rank::default();

    for term in terms {
        let mut matched_lines = literal_lines(&lowered, &lower_chars(&term.text));
        if matched_lines.is_empty() && !term.quoted {
            matched_lines.extend(best_fuzzy_line(&lines, &term.text, matcher));
        }
        if matched_lines.is_empty() {
            continue;
        }
        rank.terms_matched += 1;
        for (line_idx, ranges) in matched_lines {
            rank.occurrences += ranges.len();
            rank.title_matched |= lines[line_idx].0 == 0;
            terms_per_line[line_idx] += 1;
            ranges_per_line[line_idx].extend(ranges);
        }
    }
    if rank.terms_matched == 0 {
        return None;
    }

    let best = (0..lines.len())
        .filter(|&i| terms_per_line[i] > 0)
        .min_by_key(|&i| (Reverse(terms_per_line[i]), Reverse(ranges_per_line[i].len()), i))?;
    let (line_no, text) = lines[best];
    let ranges = merge_ranges(std::mem::take(&mut ranges_per_line[best]));
    Some(make_hit(section, page, line_no, text, ranges, rank))
}

fn literal_lines(lowered: &[Vec<char>], needle: &[char]) -> Vec<LineRanges> {
    lowered
        .iter()
        .enumerate()
        .map(|(idx, haystack)| (idx, literal_ranges(haystack, needle)))
        .filter(|(_, ranges)| !ranges.is_empty())
        .collect()
}

fn literal_ranges(haystack: &[char], needle: &[char]) -> Vec<(usize, usize)> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = 0;
    while at + needle.len() <= haystack.len() {
        if haystack[at..at + needle.len()] == *needle {
            out.push((at, at + needle.len()));
            at += needle.len();
        } else {
            at += 1;
        }
    }
    out
}

/// typo tolerance for a bare keyword the page never spells out literally
fn best_fuzzy_line(
    lines: &[(usize, &str)],
    needle: &str,
    matcher: &mut Matcher,
) -> Option<LineRanges> {
    let atom = Atom::new(
        needle,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let mut buf = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut best: Option<(u16, LineRanges)> = None;
    for (line_idx, (_, text)) in lines.iter().enumerate() {
        indices.clear();
        let haystack = Utf32Str::new(text, &mut buf);
        let Some(score) = atom.indices(haystack, matcher, &mut indices) else {
            continue;
        };
        if best.as_ref().map_or(true, |(top, _)| score > *top) {
            let ranges = indices.iter().map(|&i| (i as usize, i as usize + 1)).collect();
            best = Some((score, (line_idx, merge_ranges(ranges))));
        }
    }
    best.map(|(_, line)| line)
}

// ---------- regex ----------

fn regex_search(store: &Store, query: &str) -> Result<Vec<SearchHit>, String> {
    let re = regex::RegexBuilder::new(query)
        .case_insensitive(true)
        .build()
        .map_err(|e| format!("invalid regex: {e}"))?;
    let mut hits = Vec::new();

    for (section, page) in flatten_pages(&store.notebook) {
        let content = page_content(store, page);
        for (line_no, text) in searchable_lines(&content) {
            let ranges = regex_ranges(&re, text);
            if ranges.is_empty() {
                continue;
            }
            let rank = Rank {
                terms_matched: 1,
                occurrences: ranges.len(),
                title_matched: line_no == 0,
            };
            hits.push(make_hit(section, page, line_no, text, ranges, rank));
        }
    }
    Ok(hits)
}

fn regex_ranges(re: &regex::Regex, text: &str) -> Vec<(usize, usize)> {
    re.find_iter(text)
        .filter(|m| m.end() > m.start())
        .map(|m| byte_to_char_range(text, m.start(), m.end()))
        .collect()
}

// ---------- shared ----------

/// an unreadable page still searches by its cached title
fn page_content(store: &Store, page: &PageNode) -> String {
    store
        .read_page(&page.id)
        .unwrap_or_else(|_| format!("# {}\n", page.title))
}

/// the leading `# Title` reports as line 0 with the hashes stripped, so a title
/// match never duplicates as a body hit
fn searchable_lines(content: &str) -> Vec<(usize, &str)> {
    content
        .lines()
        .enumerate()
        .map(|(idx, line)| match line.strip_prefix("# ") {
            Some(title) if idx == 0 => (0, title),
            _ => (idx + 1, line),
        })
        .filter(|(_, text)| !text.trim().is_empty())
        .collect()
}

/// one lowercase char per source char, so match indices stay aligned with the
/// original line even where Unicode would fold to a different length
fn lower_chars(text: &str) -> Vec<char> {
    text.chars()
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .collect()
}

fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (start, end) in ranges {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

fn make_hit(
    section: &Section,
    page: &PageNode,
    line_no: usize,
    line: &str,
    ranges: Vec<(usize, usize)>,
    rank: Rank,
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
        rank,
    }
}

fn byte_to_char_range(text: &str, start: usize, end: usize) -> (usize, usize) {
    let char_start = text[..start].chars().count();
    let char_end = char_start + text[start..end].chars().count();
    (char_start, char_end)
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

    fn hits(store: &Store, query: &str, mode: &str) -> Vec<SearchHit> {
        search(store, query, mode).unwrap().hits
    }

    fn matched(hit: &SearchHit, idx: usize) -> String {
        let (start, end) = hit.ranges[idx];
        hit.snippet.chars().skip(start).take(end - start).collect()
    }

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

    fn store_with(pages: &[&str]) -> (tempfile::TempDir, Store) {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        for content in pages {
            let page = store.create_page(&sid, None, None).unwrap();
            store.write_page(&page.id, content).unwrap();
        }
        (dir, store)
    }

    #[test]
    fn finds_title_and_body() {
        let (_dir, store) = store_with_pages();
        let found = hits(&store, "grocery", "fuzzy");
        assert!(found.iter().any(|h| h.title == "Grocery List" && h.line_no == 0));
        let found = hits(&store, "milk", "fuzzy");
        let body_hit = found.iter().find(|h| h.line_no > 0).unwrap();
        assert!(body_hit.snippet.contains("milk"));
        assert_eq!(matched(body_hit, 0), "milk");
    }

    #[test]
    fn one_hit_per_page() {
        let (_dir, store) = store_with(&["# Log\n\nmilk\nmilk\nmilk\n"]);
        assert_eq!(hits(&store, "milk", "fuzzy").len(), 1);
    }

    #[test]
    fn more_terms_matched_outranks_more_occurrences() {
        let (_dir, store) = store_with(&[
            "# Loud\n\nalpha alpha alpha\nalpha alpha alpha\n",
            "# Quiet\n\nalpha\n\nbeta\n",
        ]);
        let found = hits(&store, "alpha beta", "fuzzy");
        assert_eq!(found[0].title, "Quiet");
        assert_eq!(found[0].rank.terms_matched, 2);
        assert_eq!(found[1].title, "Loud");
    }

    #[test]
    fn occurrences_break_the_tie_within_one_term() {
        let (_dir, store) = store_with(&["# Few\n\nalpha\n", "# Many\n\nalpha alpha alpha\n"]);
        let found = hits(&store, "alpha", "fuzzy");
        assert_eq!(found[0].title, "Many");
        assert_eq!(found[0].rank.occurrences, 3);
    }

    #[test]
    fn keywords_may_live_on_different_lines() {
        let (_dir, store) = store_with(&["# Split\n\nalpha here\n\nbeta there\n"]);
        let found = hits(&store, "alpha beta", "fuzzy");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rank.terms_matched, 2);
    }

    #[test]
    fn quoted_phrase_is_matched_whole() {
        let (_dir, store) = store_with(&[
            "# Together\n\nthe deployment pipeline broke\n",
            "# Apart\n\npipeline notes\n\ndeployment notes\n",
        ]);
        let found = hits(&store, "\"deployment pipeline\"", "fuzzy");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Together");
        assert_eq!(matched(&found[0], 0), "deployment pipeline");
    }

    #[test]
    fn quoted_phrase_skips_the_fuzzy_fallback() {
        let (_dir, store) = store_with(&["# Page\n\ndeployment of the pipeline\n"]);
        assert!(hits(&store, "\"deployment pipeline\"", "fuzzy").is_empty());
        assert_eq!(hits(&store, "deployment pipeline", "fuzzy").len(), 1);
    }

    #[test]
    fn quoted_and_bare_terms_mix() {
        let (_dir, store) = store_with(&["# Mixed\n\nred herring\n\nblue whale\n"]);
        let results = search(&store, "\"red herring\" whale", "fuzzy").unwrap();
        assert_eq!(results.terms, vec!["red herring", "whale"]);
        assert_eq!(results.hits[0].rank.terms_matched, 2);
    }

    #[test]
    fn fuzzy_fallback_still_catches_typos() {
        let (_dir, store) = store_with(&["# Deployment\n\nnothing here\n"]);
        assert_eq!(hits(&store, "deploymnt", "fuzzy").len(), 1);
    }

    #[test]
    fn best_line_carries_the_most_terms() {
        let (_dir, store) = store_with(&["# Page\n\nalpha alone\nalpha and beta\n"]);
        let found = hits(&store, "alpha beta", "fuzzy");
        assert_eq!(found[0].snippet, "alpha and beta");
    }

    #[test]
    fn overlapping_terms_merge_into_one_range() {
        let (_dir, store) = store_with(&["# Page\n\nmilkshake\n"]);
        let found = hits(&store, "milk milks", "fuzzy");
        assert_eq!(found[0].ranges, vec![(0, 5)]);
    }

    #[test]
    fn regex_matches_with_ranges() {
        let (_dir, store) = store_with_pages();
        let found = hits(&store, r"m\w+k", "regex");
        assert!(!found.is_empty());
        let hit = found.iter().find(|h| h.snippet.contains("milk")).unwrap();
        assert_eq!(matched(hit, 0), "milk");
    }

    #[test]
    fn regex_reports_a_title_match_once() {
        let (_dir, store) = store_with_pages();
        let found = hits(&store, "Grocery", "regex");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line_no, 0);
        assert_eq!(found[0].snippet, "Grocery List");
    }

    #[test]
    fn regex_keeps_one_hit_per_line() {
        let (_dir, store) = store_with(&["# Log\n\nmilk\nmilk\n"]);
        assert_eq!(hits(&store, "milk", "regex").len(), 2);
    }

    #[test]
    fn invalid_regex_is_an_error() {
        let (_dir, store) = store_with_pages();
        assert!(search(&store, r"([unclosed", "regex").is_err());
    }

    #[test]
    fn empty_query_returns_nothing() {
        let (_dir, store) = store_with_pages();
        assert!(hits(&store, "   ", "fuzzy").is_empty());
        assert!(hits(&store, "\"\"", "fuzzy").is_empty());
    }

    #[test]
    fn long_lines_are_windowed() {
        let (_dir, store) = store_with(&[&format!(
            "# Long\n\n{}needle{}\n",
            "x".repeat(500),
            "y".repeat(500)
        )]);
        let found = hits(&store, "needle", "regex");
        assert!(found[0].snippet.chars().count() <= 240);
        assert_eq!(matched(&found[0], 0), "needle");
    }
}
