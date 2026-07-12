import hljs from "highlight.js/lib/common";
import dos from "highlight.js/lib/languages/dos";
import powershell from "highlight.js/lib/languages/powershell";
import "highlight.js/styles/atom-one-dark.css";
import MarkdownIt from "markdown-it";

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
