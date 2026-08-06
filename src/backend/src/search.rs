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

/// variant order is the ranking order: a stronger way of matching sorts higher
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strategy {
    #[default]
    Fuzzy,
    Partial,
    Word,
    Phrase,
}

/// field order is the ranking order: matching more of the query's distinct
/// terms always outweighs matching one term more often
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Rank {
    pub terms_matched: usize,
    pub tier: Strategy,
    pub occurrences: usize,
    pub title_matched: bool,
}

struct Term {
    text: String,
    lowered: Vec<char>,
    /// quoted terms are matched verbatim — absent here means never fuzzy
    fuzzy: Option<Atom>,
}

struct ScoredLine<'a> {
    line_no: usize,
    text: &'a str,
    lowered: Vec<char>,
    terms_hit: usize,
    ranges: Vec<(usize, usize)>,
}

struct LineHit {
    line: usize,
    ranges: Vec<(usize, usize)>,
}

/// Anything the frontend does not spell `"regex"` searches by keyword, so an
/// older frontend never fails a query against a newer backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    Keyword,
    Regex,
}

impl From<&str> for SearchMode {
    fn from(mode: &str) -> Self {
        match mode {
            "regex" => Self::Regex,
            _ => Self::Keyword,
        }
    }
}

pub fn search(store: &Store, query: &str, mode: SearchMode) -> Result<SearchResults, String> {
    if query.trim().is_empty() {
        return Ok(SearchResults { hits: vec![], terms: vec![] });
    }
    let (mut hits, terms) = match mode {
        SearchMode::Regex => (regex_search(store, query)?, vec![]),
        SearchMode::Keyword => keyword_search(store, query),
    };
    hits.sort_by_key(|hit| Reverse(hit.rank));
    hits.truncate(MAX_HITS);
    Ok(SearchResults { hits, terms })
}

// ---------- keyword search ----------

fn keyword_search(store: &Store, query: &str) -> (Vec<SearchHit>, Vec<String>) {
    let terms = parse_terms(query);
    let phrase = whole_query_phrase(query);
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut hits = Vec::new();
    for (section, page) in flatten_pages(&store.notebook) {
        let content = page_content(store, page);
        if let Some(hit) = page_hit(section, page, &content, &terms, &phrase, &mut matcher) {
            hits.push(hit);
        }
    }
    (hits, terms.into_iter().map(|t| t.text).collect())
}

fn whole_query_phrase(query: &str) -> Vec<char> {
    let words: Vec<&str> = query
        .split(|c: char| c == '"' || c.is_whitespace())
        .filter(|word| !word.is_empty())
        .collect();
    lower_chars(&words.join(" "))
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
        let text = text.trim();
        if !text.is_empty() {
            terms.push(Term {
                lowered: lower_chars(text),
                fuzzy: (!quoted).then(|| fuzzy_atom(text)),
                text: text.to_string(),
            });
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
    phrase: &[char],
    matcher: &mut Matcher,
) -> Option<SearchHit> {
    let mut lines = scored_lines(content);
    let mut rank = Rank::default();
    let mut weakest_strategy = Strategy::Phrase;

    for term in terms {
        let mut strategy = Strategy::Word;
        let mut hits = literal_hits(&lines, &term.lowered);
        if hits.is_empty() {
            let Some(atom) = &term.fuzzy else { continue };
            hits.extend(best_fuzzy_hit(&lines, atom, matcher));
            strategy = Strategy::Fuzzy;
        } else if !any_hit_word_bounded(&lines, &hits) {
            strategy = Strategy::Partial;
        }
        if hits.is_empty() {
            continue;
        }
        rank.terms_matched += 1;
        weakest_strategy = weakest_strategy.min(strategy);
        for hit in hits {
            let line = &mut lines[hit.line];
            rank.occurrences += hit.ranges.len();
            rank.title_matched |= line.line_no == 0;
            line.terms_hit += 1;
            line.ranges.extend(hit.ranges);
        }
    }
    if rank.terms_matched == 0 {
        return None;
    }
    rank.tier = if query_appears_verbatim(&lines, phrase) {
        Strategy::Phrase
    } else {
        weakest_strategy
    };

    let best = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.terms_hit > 0)
        .max_by_key(|(idx, line)| (line.terms_hit, line.ranges.len(), Reverse(*idx)))
        .map(|(idx, _)| idx)?;
    let best = lines.swap_remove(best);
    let ranges = merge_ranges(best.ranges);
    Some(make_hit(section, page, best.line_no, best.text, ranges, rank))
}

fn scored_lines(content: &str) -> Vec<ScoredLine<'_>> {
    searchable_lines(content)
        .into_iter()
        .map(|(line_no, text)| ScoredLine {
            line_no,
            text,
            lowered: lower_chars(text),
            terms_hit: 0,
            ranges: Vec::new(),
        })
        .collect()
}

fn literal_hits(lines: &[ScoredLine], needle: &[char]) -> Vec<LineHit> {
    lines
        .iter()
        .enumerate()
        .map(|(line, scored)| LineHit { line, ranges: literal_ranges(&scored.lowered, needle) })
        .filter(|hit| !hit.ranges.is_empty())
        .collect()
}

fn query_appears_verbatim(lines: &[ScoredLine], phrase: &[char]) -> bool {
    lines
        .iter()
        .any(|line| any_range_word_bounded(&line.lowered, &literal_ranges(&line.lowered, phrase)))
}

fn any_hit_word_bounded(lines: &[ScoredLine], hits: &[LineHit]) -> bool {
    hits.iter()
        .any(|hit| any_range_word_bounded(&lines[hit.line].lowered, &hit.ranges))
}

fn any_range_word_bounded(haystack: &[char], ranges: &[(usize, usize)]) -> bool {
    ranges
        .iter()
        .any(|&(start, end)| is_word_bounded(haystack, start, end))
}

fn is_word_bounded(haystack: &[char], start: usize, end: usize) -> bool {
    let opens = start == 0 || !is_word_char(haystack[start - 1]);
    let closes = end == haystack.len() || !is_word_char(haystack[end]);
    opens && closes
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
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

fn fuzzy_atom(needle: &str) -> Atom {
    Atom::new(
        needle,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    )
}

/// typo tolerance for a bare keyword the page never spells out literally
fn best_fuzzy_hit(lines: &[ScoredLine], atom: &Atom, matcher: &mut Matcher) -> Option<LineHit> {
    let mut buf = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut best: Option<(u16, LineHit)> = None;
    for (line, scored) in lines.iter().enumerate() {
        indices.clear();
        let haystack = Utf32Str::new(scored.text, &mut buf);
        let Some(score) = atom.indices(haystack, matcher, &mut indices) else {
            continue;
        };
        if best.as_ref().map_or(true, |(top, _)| score > *top) {
            let ranges = indices.iter().map(|&i| (i as usize, i as usize + 1)).collect();
            best = Some((score, LineHit { line, ranges: merge_ranges(ranges) }));
        }
    }
    best.map(|(_, hit)| hit)
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
                ..Rank::default()
            };
            hits.push(make_hit(section, page, line_no, text, ranges, rank));
        }
    }
    Ok(hits)
}

/// matches arrive in ascending byte order, so one cursor carries the char
/// count forward instead of recounting the line for every match
fn regex_ranges(re: &regex::Regex, text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let (mut counted_bytes, mut counted_chars) = (0, 0);
    for m in re.find_iter(text).filter(|m| m.end() > m.start()) {
        counted_chars += text[counted_bytes..m.start()].chars().count();
        counted_bytes = m.start();
        ranges.push((counted_chars, counted_chars + text[m.range()].chars().count()));
    }
    ranges
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

fn window_snippet(line: &str, ranges: &[(usize, usize)]) -> (String, Vec<(usize, usize)>) {
    if line.chars().count() <= SNIPPET_CHARS {
        return (line.to_string(), ranges.to_vec());
    }
    let chars: Vec<char> = line.chars().collect();
    let first = ranges.first().map_or(0, |r| r.0);
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

    fn hits(store: &Store, query: &str, mode: SearchMode) -> Vec<SearchHit> {
        search(store, query, mode).unwrap().hits
    }

    fn matched(hit: &SearchHit, idx: usize) -> String {
        let (start, end) = hit.ranges[idx];
        hit.snippet.chars().skip(start).take(end - start).collect()
    }

    fn store_with_pages() -> (tempfile::TempDir, Store) {
        store_with(&[
            "# Grocery List\n\nbuy milk and eggs\ncall the plumber\n",
            "# Meeting Notes\n\ndiscussed deployment pipeline\n",
        ])
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
        let found = hits(&store, "grocery", SearchMode::Keyword);
        assert!(found.iter().any(|h| h.title == "Grocery List" && h.line_no == 0));
        let found = hits(&store, "milk", SearchMode::Keyword);
        let body_hit = found.iter().find(|h| h.line_no > 0).unwrap();
        assert!(body_hit.snippet.contains("milk"));
        assert_eq!(matched(body_hit, 0), "milk");
    }

    #[test]
    fn one_hit_per_page() {
        let (_dir, store) = store_with(&["# Log\n\nmilk\nmilk\nmilk\n"]);
        assert_eq!(hits(&store, "milk", SearchMode::Keyword).len(), 1);
    }

    #[test]
    fn more_terms_matched_outranks_more_occurrences() {
        let (_dir, store) = store_with(&[
            "# Loud\n\nalpha alpha alpha\nalpha alpha alpha\n",
            "# Quiet\n\nalpha\n\nbeta\n",
        ]);
        let found = hits(&store, "alpha beta", SearchMode::Keyword);
        assert_eq!(found[0].title, "Quiet");
        assert_eq!(found[0].rank.terms_matched, 2);
        assert_eq!(found[1].title, "Loud");
    }

    #[test]
    fn occurrences_break_the_tie_within_one_term() {
        let (_dir, store) = store_with(&["# Few\n\nalpha\n", "# Many\n\nalpha alpha alpha\n"]);
        let found = hits(&store, "alpha", SearchMode::Keyword);
        assert_eq!(found[0].title, "Many");
        assert_eq!(found[0].rank.occurrences, 3);
    }

    #[test]
    fn keywords_may_live_on_different_lines() {
        let (_dir, store) = store_with(&["# Split\n\nalpha here\n\nbeta there\n"]);
        let found = hits(&store, "alpha beta", SearchMode::Keyword);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].rank.terms_matched, 2);
    }

    #[test]
    fn quoted_phrase_is_matched_whole() {
        let (_dir, store) = store_with(&[
            "# Together\n\nthe deployment pipeline broke\n",
            "# Apart\n\npipeline notes\n\ndeployment notes\n",
        ]);
        let found = hits(&store, "\"deployment pipeline\"", SearchMode::Keyword);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].title, "Together");
        assert_eq!(matched(&found[0], 0), "deployment pipeline");
    }

    #[test]
    fn quoted_phrase_skips_the_fuzzy_fallback() {
        let (_dir, store) = store_with(&["# Page\n\ndeployment of the pipeline\n"]);
        assert!(hits(&store, "\"deployment pipeline\"", SearchMode::Keyword).is_empty());
        assert_eq!(hits(&store, "deployment pipeline", SearchMode::Keyword).len(), 1);
    }

    #[test]
    fn quoted_and_bare_terms_mix() {
        let (_dir, store) = store_with(&["# Mixed\n\nred herring\n\nblue whale\n"]);
        let results = search(&store, "\"red herring\" whale", SearchMode::Keyword).unwrap();
        assert_eq!(results.terms, vec!["red herring", "whale"]);
        assert_eq!(results.hits[0].rank.terms_matched, 2);
    }

    #[test]
    fn fuzzy_fallback_still_catches_typos() {
        let (_dir, store) = store_with(&["# Deployment\n\nnothing here\n"]);
        assert_eq!(hits(&store, "deploymnt", SearchMode::Keyword).len(), 1);
    }

    #[test]
    fn best_line_carries_the_most_terms() {
        let (_dir, store) = store_with(&["# Page\n\nalpha alone\nalpha and beta\n"]);
        let found = hits(&store, "alpha beta", SearchMode::Keyword);
        assert_eq!(found[0].snippet, "alpha and beta");
    }

    #[test]
    fn overlapping_terms_merge_into_one_range() {
        let (_dir, store) = store_with(&["# Page\n\nmilkshake\n"]);
        let found = hits(&store, "milk milks", SearchMode::Keyword);
        assert_eq!(found[0].ranges, vec![(0, 5)]);
    }

    #[test]
    fn whole_word_outranks_a_substring_only_match() {
        let (_dir, store) = store_with(&[
            "# Shake\n\nmilkshake milkshake milkshake\n",
            "# Fridge\n\nbuy milk\n",
        ]);
        let found = hits(&store, "milk", SearchMode::Keyword);
        assert_eq!(found[0].title, "Fridge");
        assert_eq!(found[0].rank.tier, Strategy::Phrase);
        assert_eq!(found[1].rank.tier, Strategy::Partial);
    }

    #[test]
    fn substring_outranks_a_fuzzy_only_match() {
        let (_dir, store) = store_with(&["# Typo\n\nm i l l k y way\n", "# Shake\n\nmilkshake\n"]);
        let found = hits(&store, "milk", SearchMode::Keyword);
        assert_eq!(found[0].title, "Shake");
        assert_eq!(found[0].rank.tier, Strategy::Partial);
        assert_eq!(found[1].rank.tier, Strategy::Fuzzy);
    }

    #[test]
    fn the_full_query_as_a_phrase_outranks_the_same_words_apart() {
        let (_dir, store) = store_with(&[
            "# Apart\n\ndeployment notes\n\npipeline notes\n\ndeployment pipeline again\n",
            "# Together\n\nthe deployment pipeline broke\n",
        ]);
        let found = hits(&store, "deployment pipeline", SearchMode::Keyword);
        assert_eq!(found[0].title, "Apart");
        let apart_and_together_both_phrase_tier =
            found.iter().all(|h| h.rank.tier == Strategy::Phrase);
        assert!(apart_and_together_both_phrase_tier);

        let (_dir, store) = store_with(&[
            "# Apart\n\ndeployment notes\n\npipeline notes\n",
            "# Together\n\nthe deployment pipeline broke\n",
        ]);
        let found = hits(&store, "deployment pipeline", SearchMode::Keyword);
        assert_eq!(found[0].title, "Together");
        assert_eq!(found[0].rank.tier, Strategy::Phrase);
        assert_eq!(found[1].rank.tier, Strategy::Word);
    }

    #[test]
    fn word_boundaries_hold_at_line_edges_and_punctuation() {
        let (_dir, store) = store_with(&[
            "# Start\n\nmilk is here\n",
            "# End\n\nwe need milk\n",
            "# Punctuated\n\n(milk), please\n",
            "# Joined\n\nmilk_shake\n",
        ]);
        let found = hits(&store, "milk", SearchMode::Keyword);
        let tier_of = |title: &str| found.iter().find(|h| h.title == title).unwrap().rank.tier;
        assert_eq!(tier_of("Start"), Strategy::Phrase);
        assert_eq!(tier_of("End"), Strategy::Phrase);
        assert_eq!(tier_of("Punctuated"), Strategy::Phrase);
        assert_eq!(tier_of("Joined"), Strategy::Partial);
    }

    #[test]
    fn a_quoted_phrase_never_drops_to_fuzzy_tier() {
        let (_dir, store) = store_with(&[
            "# Bounded\n\nthe deployment pipeline broke\n",
            "# Glued\n\nxdeployment pipelinex\n",
        ]);
        let found = hits(&store, "\"deployment pipeline\"", SearchMode::Keyword);
        assert_eq!(found[0].title, "Bounded");
        assert_eq!(found[0].rank.tier, Strategy::Phrase);
        assert_eq!(found[1].rank.tier, Strategy::Partial);
        assert!(found.iter().all(|h| h.rank.tier != Strategy::Fuzzy));
    }

    #[test]
    fn more_terms_matched_still_beats_a_stronger_tier() {
        let (_dir, store) = store_with(&[
            "# Both\n\nalphabet soup\n\nbeta\n",
            "# One\n\nalpha alone\n",
        ]);
        let found = hits(&store, "alpha beta", SearchMode::Keyword);
        assert_eq!(found[0].title, "Both");
        assert_eq!(found[0].rank.terms_matched, 2);
    }

    #[test]
    fn regex_matches_with_ranges() {
        let (_dir, store) = store_with_pages();
        let found = hits(&store, r"m\w+k", SearchMode::Regex);
        assert!(!found.is_empty());
        let hit = found.iter().find(|h| h.snippet.contains("milk")).unwrap();
        assert_eq!(matched(hit, 0), "milk");
    }

    #[test]
    fn regex_reports_a_title_match_once() {
        let (_dir, store) = store_with_pages();
        let found = hits(&store, "Grocery", SearchMode::Regex);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].line_no, 0);
        assert_eq!(found[0].snippet, "Grocery List");
    }

    #[test]
    fn regex_keeps_one_hit_per_line() {
        let (_dir, store) = store_with(&["# Log\n\nmilk\nmilk\n"]);
        assert_eq!(hits(&store, "milk", SearchMode::Regex).len(), 2);
    }

    #[test]
    fn invalid_regex_is_an_error() {
        let (_dir, store) = store_with_pages();
        assert!(search(&store, r"([unclosed", SearchMode::Regex).is_err());
    }

    #[test]
    fn empty_query_returns_nothing() {
        let (_dir, store) = store_with_pages();
        assert!(hits(&store, "   ", SearchMode::Keyword).is_empty());
        assert!(hits(&store, "\"\"", SearchMode::Keyword).is_empty());
    }

    #[test]
    fn long_lines_are_windowed() {
        let (_dir, store) = store_with(&[&format!(
            "# Long\n\n{}needle{}\n",
            "x".repeat(500),
            "y".repeat(500)
        )]);
        let found = hits(&store, "needle", SearchMode::Regex);
        assert!(found[0].snippet.chars().count() <= 240);
        assert_eq!(matched(&found[0], 0), "needle");
    }
}
