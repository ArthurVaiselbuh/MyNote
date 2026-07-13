import hljs from "highlight.js/lib/common";
import dos from "highlight.js/lib/languages/dos";
import powershell from "highlight.js/lib/languages/powershell";
import "highlight.js/styles/atom-one-dark.css";
import MarkdownIt from "markdown-it";
import type StateInline from "markdown-it/lib/rules_inline/state_inline.mjs";

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

const defaultImage =
  md.renderer.rules.image ??
  ((tokens, idx, opts, _env, self) => self.renderToken(tokens, idx, opts));

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

// markdown collapses blank-line runs into one paragraph break; the preview keeps
// them as nbsp spacer paragraphs so typed vertical gaps match the editor
function preserveBlankRuns(body: string): string {
  const out: string[] = [];
  let inFence = false;
  let blanks = 0;
  const flushBlanks = () => {
    if (blanks === 0) return;
    out.push("");
    for (let i = 1; i < blanks; i++) out.push("\u00a0", "");
    blanks = 0;
  };
  for (const line of body.split("\n")) {
    if (!inFence && line.trim() === "") {
      blanks++;
      continue;
    }
    flushBlanks();
    if (/^\s*(```|~~~)/.test(line)) inFence = !inFence;
    out.push(line);
  }
  flushBlanks();
  return out.join("\n");
}

export function renderBody(body: string): string {
  return md.render(preserveBlankRuns(body));
}
