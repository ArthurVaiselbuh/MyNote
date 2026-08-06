use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use percent_encoding::percent_decode_str;
use std::borrow::Cow;
use std::fs;
use std::path::Path;

use crate::assets;
use crate::err;
use crate::import::{
    name_or, section_exists, DupTracker, ImportOutcome, ImportPreview, PagePreview, SectionPreview,
};
use crate::store::{self, new_id, PageNode, Store};

// image references survive HTML->Markdown conversion as \u{1}src\u{1} markers
// and are resolved against the MIME parts only at import time, when the
// destination page id (and thus the assets folder) exists
const IMG_MARKER: char = '\u{1}';

pub fn inspect(store: &Store, paths: &[String]) -> ImportPreview {
    let mut dup = DupTracker::new(store);
    let sections = paths
        .iter()
        .map(|path| {
            let name = section_name_of(path);
            let exists = section_exists(store, &name);
            let (pages, error) = match load_pages(path, Attachments::Skip) {
                Ok((pages, _)) => (
                    pages
                        .into_iter()
                        .map(|p| PagePreview {
                            duplicate: dup.mark(&p.title),
                            title: p.title,
                            children: vec![],
                        })
                        .collect(),
                    None,
                ),
                Err(e) => (vec![], Some(e)),
            };
            SectionPreview {
                name,
                exists,
                error,
                pages,
            }
        })
        .collect();
    ImportPreview::from_sections(sections)
}

pub fn import(store: &mut Store, paths: &[String]) -> Result<ImportOutcome, String> {
    let mut parsed = Vec::new();
    for path in paths {
        let (pages, parts) =
            load_pages(path, Attachments::Decode).map_err(|e| format!("{path}: {e}"))?;
        parsed.push((section_name_of(path), pages, parts));
    }
    let mut outcome = ImportOutcome::default();
    for (name, pages, parts) in parsed {
        let mut nodes = Vec::new();
        for page in pages {
            let id = new_id();
            let body = resolve_images(store, &id, &page.blocks, &parts)?;
            fs::write(
                store.page_path(&id),
                format!("# {}\n\n{}", page.title, body),
            )
            .map_err(err)?;
            nodes.push(PageNode {
                id,
                title: page.title,
                expanded: true,
                children: vec![],
            });
        }
        outcome.add_section(store, name, nodes);
    }
    store.save()?;
    Ok(outcome)
}

fn section_name_of(path: &str) -> String {
    name_or(Path::new(path).file_stem(), "Imported")
}

fn load_pages(
    path: &str,
    attachments: Attachments,
) -> Result<(Vec<RawPage>, Vec<MimePart>), String> {
    let raw = fs::read(path).map_err(err)?;
    let raw = String::from_utf8_lossy(&raw);
    let doc = parse_mht(&raw, attachments)?;
    let pages = html_to_pages(&doc.html);
    if pages.is_empty() {
        return Err("no pages found (not a OneNote single-file export?)".into());
    }
    Ok((pages, doc.parts))
}

// ---------- MIME ----------

// the preview only needs page titles, so it parses the same MHT without paying
// the base64 decode of every embedded image
#[derive(Clone, Copy, PartialEq)]
enum Attachments {
    Decode,
    Skip,
}

struct MimePart {
    location: String,
    location_file_name: String,
    content_type: String,
    bytes: Vec<u8>,
}

struct MhtDoc {
    html: String,
    parts: Vec<MimePart>,
}

fn parse_mht(raw: &str, attachments: Attachments) -> Result<MhtDoc, String> {
    let (head, body) = split_headers(raw);
    let headers = parse_headers(head);
    let ctype = header(&headers, "content-type").unwrap_or("");
    let Some(boundary) = boundary_of(ctype) else {
        let enc = header(&headers, "content-transfer-encoding").unwrap_or("");
        return Ok(MhtDoc {
            html: into_utf8_lossy(decode_body(enc, body)),
            parts: vec![],
        });
    };
    let mut html = None;
    let mut parts = Vec::new();
    for chunk in split_multipart(body, &boundary) {
        let (part_head, part_body) = split_headers(chunk);
        let part_headers = parse_headers(part_head);
        let part_type = header(&part_headers, "content-type").unwrap_or("");
        let enc = header(&part_headers, "content-transfer-encoding").unwrap_or("");
        let location = header(&part_headers, "content-location").unwrap_or("");
        if part_type.to_lowercase().starts_with("text/html") && html.is_none() {
            html = Some(into_utf8_lossy(decode_body(enc, part_body)));
            continue;
        }
        let location = percent_decode_str(location).decode_utf8_lossy().into_owned();
        parts.push(MimePart {
            location_file_name: file_name_of(&location),
            location,
            content_type: part_type.to_string(),
            bytes: match attachments {
                Attachments::Decode => decode_body(enc, part_body),
                Attachments::Skip => vec![],
            },
        });
    }
    Ok(MhtDoc {
        html: html.ok_or("no HTML part in MHT file")?,
        parts,
    })
}

fn into_utf8_lossy(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes)
        .unwrap_or_else(|invalid| String::from_utf8_lossy(invalid.as_bytes()).into_owned())
}

fn split_headers(raw: &str) -> (&str, &str) {
    let crlf = raw.find("\r\n\r\n").map(|i| (i, 4));
    let lf = raw.find("\n\n").map(|i| (i, 2));
    match crlf.into_iter().chain(lf).min_by_key(|&(at, _)| at) {
        Some((at, sep_len)) => (&raw[..at], &raw[at + sep_len..]),
        None => ("", raw),
    }
}

fn parse_headers(block: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in block.lines() {
        if line.starts_with(' ') || line.starts_with('\t') {
            if let Some(last) = out.last_mut() {
                last.1.push(' ');
                last.1.push_str(line.trim());
            }
        } else if let Some((k, v)) = line.split_once(':') {
            out.push((k.trim().to_lowercase(), v.trim().to_string()));
        }
    }
    out
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
}

fn boundary_of(ctype: &str) -> Option<String> {
    let lower = ctype.to_lowercase();
    if !lower.contains("multipart") {
        return None;
    }
    let at = lower.find("boundary=")?;
    let value = ctype[at + "boundary=".len()..].trim_start();
    let boundary = if let Some(stripped) = value.strip_prefix('"') {
        stripped.split('"').next().unwrap_or("")
    } else {
        value
            .split(|c: char| c == ';' || c.is_whitespace())
            .next()
            .unwrap_or("")
    };
    (!boundary.is_empty()).then(|| boundary.to_string())
}

fn split_multipart<'a>(body: &'a str, boundary: &str) -> Vec<&'a str> {
    let marker = format!("--{boundary}");
    let mut chunks = Vec::new();
    for segment in body.split(&marker).skip(1) {
        let segment = segment.trim_start_matches(['\r', '\n']);
        if segment.starts_with("--") {
            break;
        }
        chunks.push(segment);
    }
    chunks
}

fn decode_body(encoding: &str, body: &str) -> Vec<u8> {
    match encoding.to_lowercase().as_str() {
        "base64" => {
            let mut compact = String::with_capacity(body.len());
            compact.extend(body.chars().filter(|c| !c.is_whitespace()));
            STANDARD.decode(compact).unwrap_or_default()
        }
        "quoted-printable" => decode_qp(body),
        _ => body.as_bytes().to_vec(),
    }
}

fn decode_qp(s: &str) -> Vec<u8> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'=' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let rest = &bytes[i + 1..];
        if rest.starts_with(b"\r\n") {
            i += 3;
        } else if rest.starts_with(b"\n") {
            i += 2;
        } else if let Some(byte) = rest.get(..2).and_then(hex_pair) {
            out.push(byte);
            i += 3;
        } else {
            out.push(b'=');
            i += 1;
        }
    }
    out
}

fn hex_pair(pair: &[u8]) -> Option<u8> {
    Some(hex_val(pair[0])? * 16 + hex_val(pair[1])?)
}

fn hex_val(c: u8) -> Option<u8> {
    char::from(c).to_digit(16).map(|d| d as u8)
}

// ---------- HTML tokenizer ----------

enum Tok<'a> {
    Open { name: Cow<'a, str>, attrs: &'a str },
    Close(Cow<'a, str>),
    Text(&'a str),
}

fn tokenize(html: &str) -> Vec<Tok<'_>> {
    let mut toks = Vec::new();
    let len = html.len();
    let mut i = 0;
    while i < len {
        if html.as_bytes()[i] == b'<' {
            if html[i..].starts_with("<!--") {
                i = html[i..].find("-->").map(|j| i + j + 3).unwrap_or(len);
                continue;
            }
            let end = tag_end(html, i);
            let inner = &html[i + 1..end];
            i = (end + 1).min(len);
            if inner.starts_with('!') || inner.starts_with('?') {
                continue;
            }
            let closing = inner.starts_with('/');
            let inner = inner.trim_start_matches('/');
            let name_end = inner
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(inner.len());
            let name = lowercased(&inner[..name_end]);
            if name.is_empty() {
                continue;
            }
            if closing {
                toks.push(Tok::Close(name));
            } else {
                toks.push(Tok::Open {
                    name,
                    attrs: &inner[name_end..],
                });
            }
        } else {
            let next = html[i..].find('<').map(|j| i + j).unwrap_or(len);
            toks.push(Tok::Text(&html[i..next]));
            i = next;
        }
    }
    toks
}

fn lowercased(text: &str) -> Cow<'_, str> {
    if text.bytes().any(|b| b.is_ascii_uppercase()) {
        Cow::Owned(text.to_lowercase())
    } else {
        Cow::Borrowed(text)
    }
}

fn tag_end(html: &str, start: usize) -> usize {
    let bytes = html.as_bytes();
    let mut quote: Option<u8> = None;
    let mut i = start + 1;
    while i < bytes.len() {
        match quote {
            Some(q) => {
                if bytes[i] == q {
                    quote = None;
                }
            }
            None => match bytes[i] {
                b'"' | b'\'' => quote = Some(bytes[i]),
                b'>' => return i,
                _ => {}
            },
        }
        i += 1;
    }
    bytes.len().saturating_sub(1)
}

fn attr<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let bytes = attrs.as_bytes();
    for at in 0..bytes.len() {
        let eq = at + name.len();
        if bytes.get(eq) != Some(&b'=') || !bytes[at..eq].eq_ignore_ascii_case(name.as_bytes()) {
            continue;
        }
        let starts_a_name = at == 0 || {
            let before = bytes[at - 1];
            !before.is_ascii_alphanumeric() && before != b'-'
        };
        if !starts_a_name {
            continue;
        }
        let value = attrs[eq + 1..].trim_start();
        return Some(match value.chars().next() {
            Some(quote @ ('"' | '\'')) => value[1..].split(quote).next().unwrap_or(""),
            _ => value.split(char::is_whitespace).next().unwrap_or(""),
        });
    }
    None
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let semi = rest
            .find(';')
            .filter(|&end| end > 1 && end <= 10 && rest[1..end].is_ascii());
        let decoded = semi.and_then(|end| entity_char(&rest[1..end]).map(|c| (c, end)));
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn entity_char(entity: &str) -> Option<char> {
    match entity {
        "nbsp" => Some('\u{a0}'),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        _ => {
            let code = entity.strip_prefix('#')?;
            let value = match code.strip_prefix(['x', 'X']) {
                Some(hexcode) => u32::from_str_radix(hexcode, 16).ok()?,
                None => code.parse().ok()?,
            };
            char::from_u32(value)
        }
    }
}

// ---------- HTML -> Markdown ----------

pub struct RawPage {
    pub title: String,
    pub blocks: Vec<String>,
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Inline {
    bold: bool,
    italic: bool,
}

#[derive(Default)]
struct Para {
    runs: Vec<(Inline, String)>,
    is_title: bool,
    is_meta: bool,
}

#[derive(Default)]
struct PageBuild {
    title: String,
    blocks: Vec<String>,
}

// OneNote's exporter marks a page's heading and its date/time stamp by inline
// style rather than by tag
const TITLE_STYLE: &str = "font-size:20.0pt";
const META_STYLE: &str = "color:#767676";
// every export closes with a title-less footer page
const FOOTER_TEXT: &str = "Created with OneNote.";

fn finish_page(page: &mut Option<PageBuild>, pages: &mut Vec<RawPage>) {
    let Some(built) = page.take() else { return };
    let filled: Vec<&str> = built
        .blocks
        .iter()
        .filter(|b| !b.is_empty())
        .map(String::as_str)
        .collect();
    if built.title.is_empty() && (filled.is_empty() || filled == [FOOTER_TEXT]) {
        return;
    }
    pages.push(RawPage {
        title: if built.title.is_empty() {
            "Untitled".into()
        } else {
            built.title
        },
        blocks: built.blocks,
    });
}

fn html_to_pages(html: &str) -> Vec<RawPage> {
    let mut pages: Vec<RawPage> = Vec::new();
    let mut page: Option<PageBuild> = None;
    let mut in_body = false;
    let mut div_depth = 0usize;
    let mut inline_stack: Vec<Inline> = vec![Inline::default()];
    let mut para: Option<Para> = None;
    let mut link: Option<(String, String)> = None;
    let mut table: Option<Vec<Vec<String>>> = None;

    for tok in tokenize(html) {
        match tok {
            Tok::Open { name, attrs } => match name.as_ref() {
                "body" => in_body = true,
                "div" if in_body => {
                    if div_depth == 0 {
                        finish_page(&mut page, &mut pages);
                        page = Some(PageBuild::default());
                    }
                    div_depth += 1;
                }
                "p" if page.is_some() && table.is_none() => {
                    let style = attr(attrs, "style").unwrap_or_default();
                    para = Some(Para {
                        is_title: style.contains(TITLE_STYLE),
                        is_meta: style.contains(META_STYLE),
                        ..Para::default()
                    });
                }
                "span" | "font" => {
                    let style = attr(attrs, "style").unwrap_or_default();
                    let mut nested = inline_stack.last().copied().unwrap_or_default();
                    nested.bold |= style.contains("font-weight:bold");
                    nested.italic |= style.contains("font-style:italic");
                    inline_stack.push(nested);
                }
                "b" | "strong" | "i" | "em" => {
                    let mut nested = inline_stack.last().copied().unwrap_or_default();
                    nested.bold |= matches!(name.as_ref(), "b" | "strong");
                    nested.italic |= matches!(name.as_ref(), "i" | "em");
                    inline_stack.push(nested);
                }
                "a" => {
                    link = Some((
                        attr(attrs, "href").unwrap_or_default().to_string(),
                        String::new(),
                    ))
                }
                "br" => {
                    if let Some(p) = para.take() {
                        flush_para(p, &mut page);
                        para = Some(Para::default());
                    }
                }
                "img" => {
                    if let Some(src) = attr(attrs, "src") {
                        let run = format!("{IMG_MARKER}{src}{IMG_MARKER}");
                        match (&mut table, &mut para) {
                            (Some(rows), _) => append_cell(rows, &run),
                            (None, Some(p)) => p.runs.push((Inline::default(), run)),
                            (None, None) => {
                                if let Some(pg) = page.as_mut() {
                                    pg.blocks.push(run);
                                }
                            }
                        }
                    }
                }
                "table" if page.is_some() => {
                    if let Some(p) = para.take() {
                        flush_para(p, &mut page);
                    }
                    table = Some(vec![]);
                }
                "tr" => {
                    if let Some(rows) = table.as_mut() {
                        rows.push(vec![]);
                    }
                }
                "td" | "th" => {
                    if let Some(rows) = table.as_mut() {
                        if let Some(row) = rows.last_mut() {
                            row.push(String::new());
                        }
                    }
                }
                _ => {}
            },
            Tok::Close(name) => match name.as_ref() {
                "body" => {
                    finish_page(&mut page, &mut pages);
                    in_body = false;
                }
                "div" if in_body => {
                    div_depth = div_depth.saturating_sub(1);
                }
                "p" => {
                    if let Some(p) = para.take() {
                        flush_para(p, &mut page);
                    }
                }
                "span" | "font" | "b" | "strong" | "i" | "em" => {
                    if inline_stack.len() > 1 {
                        inline_stack.pop();
                    }
                }
                "a" => {
                    if let Some((href, text)) = link.take() {
                        let text = collapse_ws(&text);
                        let rendered = if href.is_empty() {
                            text
                        } else if text.is_empty() || text == href {
                            format!("<{href}>")
                        } else {
                            format!("[{text}]({href})")
                        };
                        if let Some(p) = para.as_mut() {
                            p.runs.push((Inline::default(), rendered));
                        } else if let Some(rows) = table.as_mut() {
                            append_cell(rows, &rendered);
                        }
                    }
                }
                "table" => {
                    if let Some(rows) = table.take() {
                        if let Some(pg) = page.as_mut() {
                            let rendered = render_table(rows);
                            if !rendered.is_empty() {
                                pg.blocks.push(rendered);
                            }
                        }
                    }
                }
                _ => {}
            },
            Tok::Text(raw) => {
                if page.is_none() {
                    continue;
                }
                if let Some((_, link_text)) = link.as_mut() {
                    link_text.push_str(&decode_entities(raw));
                } else if let Some(rows) = table.as_mut() {
                    append_cell(rows, &collapse_ws(&decode_entities(raw)));
                } else if let Some(p) = para.as_mut() {
                    let style = inline_stack.last().copied().unwrap_or_default();
                    p.runs.push((style, normalized_run(raw)));
                }
            }
        }
    }
    finish_page(&mut page, &mut pages);
    pages
}

// keeps a single leading/trailing space so words split across tags stay apart
fn normalized_run(raw: &str) -> String {
    let decoded = decode_entities(raw);
    let mut run = collapse_ws(&decoded);
    if decoded.ends_with(char::is_whitespace) && !run.is_empty() {
        run.push(' ');
    }
    if decoded.starts_with(char::is_whitespace) {
        run.insert(0, ' ');
    }
    run
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for word in text.split_whitespace() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

fn append_cell(rows: &mut [Vec<String>], text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(cell) = rows.last_mut().and_then(|r| r.last_mut()) {
        if !cell.is_empty() {
            cell.push(' ');
        }
        cell.push_str(text);
    }
}

fn flush_para(para: Para, page: &mut Option<PageBuild>) {
    let Some(pg) = page.as_mut() else { return };
    if para.is_title {
        let plain = collapse_ws(&para.runs.iter().map(|(_, t)| t.as_str()).collect::<String>());
        if plain.is_empty() {
            return;
        }
        if pg.title.is_empty() {
            pg.title = plain;
        } else {
            pg.blocks.push(format!("## {plain}"));
        }
        return;
    }
    let text = render_runs(&para.runs);
    let text = text.trim();
    if text.is_empty() {
        // an empty paragraph is an intentional blank line; kept as a marker so
        // the page keeps its vertical gaps (join_blocks turns it into a newline)
        pg.blocks.push(String::new());
        return;
    }
    if para.is_meta {
        pg.blocks.push(format!("*{text}*"));
    } else {
        pg.blocks.push(bulletize(text));
    }
}

fn render_runs(runs: &[(Inline, String)]) -> String {
    let mut merged: Vec<(Inline, Cow<str>)> = Vec::new();
    for (style, text) in runs {
        match merged.last_mut() {
            Some((last_style, last_text)) if last_style == style => {
                last_text.to_mut().push_str(text)
            }
            _ => merged.push((*style, Cow::Borrowed(text.as_str()))),
        }
    }
    let mut out = String::new();
    for (style, text) in merged {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            out.push_str(&text);
            continue;
        }
        if text.starts_with(' ') {
            out.push(' ');
        }
        let emphasis = match (style.bold, style.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            (false, false) => "",
        };
        out.push_str(emphasis);
        out.push_str(trimmed);
        out.push_str(emphasis);
        if text.ends_with(' ') {
            out.push(' ');
        }
    }
    out
}

fn bulletize(text: &str) -> String {
    for bullet in ['·', '•', '§', '\u{f0b7}'] {
        if let Some(rest) = text.strip_prefix(bullet) {
            return format!("- {}", rest.trim_start());
        }
    }
    if text.starts_with('#') {
        return format!("\\{text}");
    }
    text.to_string()
}

fn render_table(rows: Vec<Vec<String>>) -> String {
    let rows: Vec<Vec<String>> = rows.into_iter().filter(|r| !r.is_empty()).collect();
    let Some(cols) = rows.iter().map(|r| r.len()).max() else {
        return String::new();
    };
    let line = |row: &[String]| {
        let cells: Vec<String> = (0..cols)
            .map(|i| {
                row.get(i)
                    .map(|c| c.trim().replace('|', "\\|"))
                    .filter(|c| !c.is_empty())
                    .unwrap_or_else(|| " ".into())
            })
            .collect();
        format!("| {} |", cells.join(" | "))
    };
    let mut out = vec![line(&rows[0])];
    out.push(format!("|{}", " --- |".repeat(cols)));
    for row in &rows[1..] {
        out.push(line(row));
    }
    out.join("\n")
}

// ---------- image resolution ----------

fn resolve_images(
    store: &Store,
    page_id: &str,
    blocks: &[String],
    parts: &[MimePart],
) -> Result<String, String> {
    let mut counter = 0usize;
    let mut out_blocks = Vec::new();
    for block in blocks {
        if block.is_empty() {
            out_blocks.push(String::new());
            continue;
        }
        let expanded = expand_markers(store, page_id, block, parts, &mut counter)?;
        // a block emptied by an unresolved image is dropped, not kept as a gap
        if !expanded.trim().is_empty() {
            out_blocks.push(expanded);
        }
    }
    Ok(join_blocks(&out_blocks))
}

fn expand_markers(
    store: &Store,
    page_id: &str,
    block: &str,
    parts: &[MimePart],
    counter: &mut usize,
) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = block;
    while let Some(at) = rest.find(IMG_MARKER) {
        out.push_str(&rest[..at]);
        let after = &rest[at + 1..];
        let Some(close) = after.find(IMG_MARKER) else {
            rest = after;
            break;
        };
        let src = &after[..close];
        rest = &after[close + 1..];
        if let Some(rel) = save_part(store, page_id, src, parts, counter)? {
            out.push_str("![](");
            out.push_str(&rel);
            out.push(')');
        }
    }
    out.push_str(rest);
    Ok(out)
}

// the preview renders blank-line runs verbatim, so imported gaps survive
fn join_blocks(blocks: &[String]) -> String {
    let mut out = String::new();
    let mut pending_blank_lines = 0usize;
    for block in blocks {
        if block.is_empty() {
            pending_blank_lines += 1;
            continue;
        }
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str(&"\n".repeat(pending_blank_lines));
        pending_blank_lines = 0;
        out.push_str(block);
    }
    out.push('\n');
    out
}

fn save_part(
    store: &Store,
    page_id: &str,
    src: &str,
    parts: &[MimePart],
    counter: &mut usize,
) -> Result<Option<String>, String> {
    let src = percent_decode_str(src).decode_utf8_lossy();
    let src_name = file_name_of(&src);
    let part = parts.iter().find(|p| {
        p.location.ends_with(src.as_ref())
            || (!src_name.is_empty() && p.location_file_name == src_name)
    });
    let Some(part) = part else { return Ok(None) };
    if part.bytes.is_empty() {
        return Ok(None);
    }
    let ext = image_ext(&src_name, &part.content_type);
    let dir = store.assets_dir(page_id);
    fs::create_dir_all(&dir).map_err(err)?;
    *counter += 1;
    let name = format!("img-{counter:03}.{ext}");
    fs::write(dir.join(&name), &part.bytes).map_err(err)?;
    Ok(Some(store::assets_rel(page_id, &name)))
}

fn file_name_of(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or("").to_lowercase()
}

fn image_ext(file_name: &str, content_type: &str) -> &'static str {
    let from_name = file_name.rsplit('.').next().unwrap_or("");
    assets::written_ext(from_name).unwrap_or_else(|| assets::ext_for_mime(content_type))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Section;
    use tempfile::tempdir;

    const SINGLE_PART: &str = "MIME-Version: 1.0\r\nContent-Location: file:///C:/X/Work Notes.htm\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Type: text/html; charset=\"utf-8\"\r\n\r\n<html>\r\n<head></head>\r\n<body lang=3Den-US style=3D'font-family:Calibri'>\r\n<div style=3D'direction:ltr;border-width:100%'>\r\n<div>\r\n<p style=3D'margin:0in;font-family:\"Calibri Light\";font-size:20.0pt'>First =\r\nPage</p>\r\n<p style=3D'font-size:10.0pt;color:#767676'>Thursday, 10 October 2019</p>\r\n<p style=3D'font-size:10.0pt;color:#767676'>10:20</p>\r\n<p style=3D'font-size:11.0pt'>Hello <span style=3D'font-weight:bold'>bold</span> world</p>\r\n<p style=3D'font-size:11.0pt'>&nbsp;</p>\r\n<p style=3D'font-size:11.0pt'>=C2=B7 bullet item</p>\r\n<p style=3D'font-size:11.0pt'>=E2=80=A2 dot item</p>\r\n<p style=3D'font-size:11.0pt'>See <a href=3D\"https://example.com\">the site</a> now</p>\r\n</div>\r\n</div>\r\n<p style=3D'margin:0in'>&nbsp;</p>\r\n<div style=3D'direction:ltr;border-width:100%'>\r\n<div>\r\n<p style=3D'font-size:20.0pt'>&nbsp;</p>\r\n<p>It=E2=80=99s the second page</p>\r\n</div>\r\n</div>\r\n<div style=3D'direction:ltr;border-width:100%'>\r\n<div>\r\n<p style=3D'font-size:20.0pt'>&nbsp;</p>\r\n<p>Created with OneNote.</p>\r\n</div>\r\n</div>\r\n</body>\r\n</html>\r\n";

    fn write_fixture(dir: &std::path::Path, name: &str, content: &str) -> String {
        let path = dir.join(name);
        fs::write(&path, content).unwrap();
        path.to_string_lossy().to_string()
    }

    fn imported_section<'a>(store: &'a Store, outcome: &ImportOutcome) -> &'a Section {
        store
            .notebook
            .sections
            .iter()
            .find(|s| s.id == outcome.section_ids[0])
            .unwrap()
    }

    #[test]
    fn qp_decodes_hex_and_soft_breaks() {
        let decoded = String::from_utf8(decode_qp("It=E2=80=99s a bro=\r\nken=3D line")).unwrap();
        assert_eq!(decoded, "It\u{2019}s a broken= line");
    }

    #[test]
    fn splits_pages_and_converts_markdown() {
        let doc = parse_mht(SINGLE_PART, Attachments::Decode).unwrap();
        let pages = html_to_pages(&doc.html);
        assert_eq!(pages.len(), 2);

        assert_eq!(pages[0].title, "First Page");
        assert_eq!(pages[0].blocks[0], "*Thursday, 10 October 2019*");
        assert_eq!(pages[0].blocks[1], "*10:20*");
        assert_eq!(pages[0].blocks[2], "Hello **bold** world");
        assert_eq!(pages[0].blocks[3], "");
        assert_eq!(pages[0].blocks[4], "- bullet item");
        assert_eq!(pages[0].blocks[5], "- dot item");
        assert_eq!(pages[0].blocks[6], "See [the site](https://example.com) now");

        // the trailing "Created with OneNote." footer page is dropped
        assert_eq!(pages[1].title, "Untitled");
        assert_eq!(pages[1].blocks[0], "It\u{2019}s the second page");
    }

    #[test]
    fn inspect_flags_duplicates() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let sid = store.notebook.sections[0].id.clone();
        let existing = store.create_page(&sid, None, None).unwrap();
        store.rename_page(&existing.id, "First Page").unwrap();
        store.rename_section(&sid, "Work Notes").unwrap();

        let path = write_fixture(dir.path(), "Work Notes.mht", SINGLE_PART);
        let preview = inspect(&store, &[path]);
        assert_eq!(preview.sections.len(), 1);
        let section = &preview.sections[0];
        assert_eq!(section.name, "Work Notes");
        assert!(section.exists);
        assert!(section.error.is_none());
        assert!(section.pages[0].duplicate);
        assert!(!section.pages[1].duplicate);
    }

    #[test]
    fn inspect_reports_unparseable_file() {
        let dir = tempdir().unwrap();
        let store = Store::open(dir.path()).unwrap();
        let path = write_fixture(dir.path(), "junk.mht", "not an mht at all");
        let preview = inspect(&store, &[path]);
        assert!(preview.sections[0].error.is_some());
    }

    #[test]
    fn import_creates_section_with_pages() {
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let path = write_fixture(dir.path(), "Work Notes.mht", SINGLE_PART);

        let outcome = import(&mut store, &[path]).unwrap();
        assert_eq!(outcome.page_count, 2);
        assert_eq!(outcome.section_ids.len(), 1);

        let section = imported_section(&store, &outcome);
        assert_eq!(section.name, "Work Notes");
        assert_eq!(section.pages.len(), 2);
        assert_eq!(section.pages[0].title, "First Page");
        let first_page_id = section.pages[0].id.clone();

        let content = store.read_page(&first_page_id).unwrap();
        assert!(content.starts_with("# First Page\n\n"));
        // the &nbsp; paragraph survives as one extra blank line
        assert!(content.contains("Hello **bold** world\n\n\n- bullet item"));

        drop(store);
        let reopened = Store::open(dir.path()).unwrap();
        assert!(reopened.find_page(&first_page_id).is_some());
    }

    #[test]
    fn import_extracts_multipart_images_to_assets() {
        const TINY_PNG_B64: &str =
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
        let html_part = "Content-Location: file:///C:/X/Pics.htm\r\nContent-Transfer-Encoding: quoted-printable\r\nContent-Type: text/html; charset=\"utf-8\"\r\n\r\n<html><body>\r\n<div style=3D'border-width:100%'>\r\n<div>\r\n<p style=3D'font-size:20.0pt'>Pics</p>\r\n<p><img width=3D5 src=3D\"Pics_files/image001.png\"></p>\r\n</div>\r\n</div>\r\n</body></html>\r\n";
        let mht = format!(
            "MIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"----=_Part\"\r\n\r\n------=_Part\r\n{html_part}\r\n------=_Part\r\nContent-Location: file:///C:/X/Pics_files/image001.png\r\nContent-Transfer-Encoding: base64\r\nContent-Type: image/png\r\n\r\n{TINY_PNG_B64}\r\n------=_Part--\r\n"
        );
        let dir = tempdir().unwrap();
        let mut store = Store::open(dir.path()).unwrap();
        let path = write_fixture(dir.path(), "Pics.mht", &mht);

        let outcome = import(&mut store, &[path]).unwrap();
        let page_id = &imported_section(&store, &outcome).pages[0].id;
        let content = store.read_page(page_id).unwrap();
        let rel = store::assets_rel(page_id, "img-001.png");
        assert!(content.contains(&format!("![]({rel})")));
        assert!(dir.path().join(&rel).exists());
    }

    #[test]
    fn blank_paragraph_runs_become_extra_newlines() {
        let html = "<html><body><div style='border-width:100%'><div><p style='font-size:20.0pt'>T</p><p>a</p><p>&nbsp;</p><p>&nbsp;</p><p>b</p><p>&nbsp;</p></div></div></body></html>";
        let pages = html_to_pages(html);
        assert_eq!(pages[0].blocks, vec!["a", "", "", "b", ""]);
        // two markers -> two extra newlines; trailing blanks are dropped
        assert_eq!(join_blocks(&pages[0].blocks), "a\n\n\n\nb\n");
    }

    #[test]
    fn tables_render_as_markdown() {
        let table_html = "<html><body><div style='border-width:100%'><div><p style='font-size:20.0pt'>T</p><table><tr><td><p>a</p></td><td><p>b</p></td></tr><tr><td><p>c</p></td><td><p>d</p></td></tr></table></div></div></body></html>";
        let pages = html_to_pages(table_html);
        assert_eq!(pages[0].blocks[0], "| a | b |\n| --- | --- |\n| c | d |");
    }
}
