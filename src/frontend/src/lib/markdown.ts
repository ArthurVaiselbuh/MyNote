import hljs from "highlight.js/lib/common";
import dos from "highlight.js/lib/languages/dos";
import powershell from "highlight.js/lib/languages/powershell";
import "highlight.js/styles/atom-one-dark.css";
import MarkdownIt from "markdown-it";
import type { RenderRule } from "markdown-it/lib/renderer.mjs";
import type StateInline from "markdown-it/lib/rules_inline/state_inline.mjs";
import { MOD_LABEL } from "./keys/platform";
import { isExternalHref } from "./regex";

hljs.registerLanguage("powershell", powershell);
hljs.registerLanguage("dos", dos);

// Custom URI schemes resolve differently per webview: Windows (WebView2) exposes
// them as http://<scheme>.localhost/, macOS/Linux (WKWebView/webkit2gtk) as
// <scheme>://localhost/. Match Tauri's own userAgent-based rule so images load
// everywhere.
const noteAssetBase = navigator.userAgent.includes("Windows")
  ? "http://note-asset.localhost/"
  : "note-asset://localhost/";

function highlightCode(code: string, lang: string): string {
  if (!lang || !hljs.getLanguage(lang)) return "";
  return hljs.highlight(code, { language: lang, ignoreIllegals: true }).value;
}

const md = new MarkdownIt({
  html: false,
  linkify: false,
  breaks: false,
  highlight: highlightCode,
});

const renderToken: RenderRule = (tokens, idx, opts, _env, self) =>
  self.renderToken(tokens, idx, opts);

const defaultImage = md.renderer.rules.image ?? renderToken;
const defaultLinkOpen = md.renderer.rules.link_open ?? renderToken;

md.renderer.rules.link_open = (tokens, idx, opts, env, self) => {
  const href = tokens[idx].attrGet("href") ?? "";
  if (isExternalHref(href)) {
    tokens[idx].attrSet("title", `${MOD_LABEL}+Click to open in browser`);
  }
  return defaultLinkOpen(tokens, idx, opts, env, self);
};

md.renderer.rules.image = (tokens, idx, opts, env, self) => {
  const token = tokens[idx];
  const src = token.attrGet("src") ?? "";
  if (src.startsWith("assets/")) {
    token.attrSet("src", noteAssetBase + src.slice("assets/".length));
  }
  return defaultImage(tokens, idx, opts, env, self);
};

// Pandoc-style attribute blocks, whitelisted: `![alt](src){width=420}` sizes an
// image, `[text]{.red}` / `[text]{style="color:#hex"}` colors a span. html
// stays off; anything not matching these exact forms renders as literal text.
export const COLOR_PALETTE: Record<string, string> = {
  red: "#e06c75",
  orange: "#d19a66",
  yellow: "#e5c07b",
  green: "#98c379",
  cyan: "#56b6c2",
  blue: "#61afef",
  purple: "#c678dd",
  gray: "#7f848e",
};

export const COLOR_PALETTE_GRADIENT = `linear-gradient(90deg, ${Object.values(COLOR_PALETTE).join(",")})`;

const WIDTH_RE = /^\{\s*width=(\d{1,4})\s*\}/;

function imageWidth(state: StateInline, silent: boolean): boolean {
  if (state.src.charCodeAt(state.pos) !== 0x7b /* { */) return false;
  if (state.pending) return false;
  const prev = state.tokens[state.tokens.length - 1];
  if (!prev || prev.type !== "image") return false;
  const m = WIDTH_RE.exec(state.src.slice(state.pos, state.posMax));
  if (!m || Number(m[1]) === 0) return false;
  if (!silent) prev.attrSet("width", m[1]);
  state.pos += m[0].length;
  return true;
}

const COLOR_ATTR_RE = new RegExp(
  `^\\{\\s*(?:\\.(${Object.keys(COLOR_PALETTE).join("|")})` +
    `|style=(["'])color:\\s*#([0-9a-fA-F]{3,4}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})\\2)\\s*\\}`,
);

function colorSpan(state: StateInline, silent: boolean): boolean {
  // declining in silent mode leaves bracket-matching to parseLinkLabel's own
  // counter, so spans nest inside link labels like any other brackets
  if (silent) return false;
  if (state.src.charCodeAt(state.pos) !== 0x5b /* [ */) return false;
  const labelEnd = state.md.helpers.parseLinkLabel(state, state.pos);
  if (labelEnd < 0) return false;
  // every ordinary link and image reaches this point; the char test rejects them
  // before the attribute block is sliced out of the source
  if (state.src.charCodeAt(labelEnd + 1) !== 0x7b /* { */) return false;
  const m = COLOR_ATTR_RE.exec(state.src.slice(labelEnd + 1, state.posMax));
  if (!m) return false;
  const open = state.push("color_span_open", "span", 1);
  // attr is built only from palette values / the captured hex digits — the
  // input text is never echoed into the attribute
  open.attrs = [["style", "color:" + (m[1] ? COLOR_PALETTE[m[1]] : "#" + m[3])]];
  const oldPosMax = state.posMax;
  state.pos += 1;
  state.posMax = labelEnd;
  state.md.inline.tokenize(state);
  state.posMax = oldPosMax;
  state.push("color_span_close", "span", -1);
  state.pos = labelEnd + 1 + m[0].length;
  return true;
}

md.inline.ruler.push("image_width", imageWidth);
md.inline.ruler.push("color_span", colorSpan);

const HAS_CONTENT = /\S/;
const FENCE_RE = /^\s*(```|~~~)/;

// markdown collapses blank-line runs into one paragraph break; the preview keeps
// them as nbsp spacer paragraphs so typed vertical gaps match the editor. The
// spacers shift every following line, so the rewrite hands back the body line
// each rendered line came from - that map is what LINE_ATTR reports to the DOM.
function preserveBlankRuns(body: string): { text: string; bodyLines: number[] } {
  const out: string[] = [];
  const bodyLines: number[] = [];
  const emit = (line: string, bodyLine: number) => {
    out.push(line);
    bodyLines.push(bodyLine);
  };
  let inFence = false;
  let afterBlank = false;
  body.split("\n").forEach((line, bodyLine) => {
    const blank = !inFence && !HAS_CONTENT.test(line);
    if (blank) {
      if (afterBlank) emit("\u00a0", bodyLine);
      emit("", bodyLine);
    } else {
      if (FENCE_RE.test(line)) inFence = !inFence;
      emit(line, bodyLine);
    }
    afterBlank = blank;
  });
  return { text: out.join("\n"), bodyLines };
}

/** Block elements carry the 0-based body line they start on, so the preview and
 * the editor can be pointed at the same place in the page. */
export const LINE_ATTR = "data-line";

md.core.ruler.push("body_line", (state) => {
  const { bodyLines } = state.env as { bodyLines: number[] };
  for (const token of state.tokens) {
    if (!token.map || !token.tag || token.nesting < 0) continue;
    token.attrSet(LINE_ATTR, String(bodyLines[token.map[0]]));
  }
});

export function renderBody(body: string): string {
  const { text, bodyLines } = preserveBlankRuns(body);
  return md.render(text, { bodyLines });
}

/** The body-line span of the preview block containing `bodyLine` — a paragraph,
 * list item, or fenced code block collapses several source lines into one
 * rendered element, so the editor (one CM block per source line) needs this to
 * know how many of its own lines correspond to that single preview block. */
export function blockRangeAt(body: string, bodyLine: number): { start: number; end: number } {
  const { text, bodyLines } = preserveBlankRuns(body);
  const starts = md
    .parse(text, { bodyLines })
    .map((t) => t.attrGet(LINE_ATTR))
    .filter((v): v is string => v !== null)
    .map(Number);
  const totalLines = body.split("\n").length;
  let start = 0;
  let end = totalLines;
  for (let i = 0; i < starts.length && starts[i] <= bodyLine; i++) {
    start = starts[i];
    end = i + 1 < starts.length ? starts[i + 1] : totalLines;
  }
  return { start, end };
}
