import { app } from "../state/app.svelte";
import { ALT_LABEL, MOD_LABEL, SHIFT_LABEL, chordKey, isMac } from "./platform";

// Single source of truth for every binding in the app: the dispatcher matches
// against it, the help overlay and every tooltip render from it, and the
// keybindings pane edits it. A chord is stored layout-stably — the key half is
// whatever chordKey() yields (see platform.ts), never the printed character.
//
// Serialized form is "Mod+Alt+Shift+key" with the modifiers in that fixed
// order; "Mod" is Ctrl on Windows/Linux and Cmd on macOS. User overrides live
// in settings.json::keybindings as command id -> chord list, where an empty
// list means the command is deliberately unassigned. Commands absent from that
// map keep their defaults, so bindings added in a later version light up
// without touching the user's file.

export type BindingContext = "global" | "system" | "pane" | "tree" | "results" | "history";

export type Command = {
  id: string;
  ctx: BindingContext;
  desc: string;
  defaults: string[];
  /** extra terms the help/keybindings filter matches but doesn't show */
  search?: string;
  /** hidden entirely when the named feature is unavailable (see app.git) */
  gate?: "git";
};

export const COMMANDS: Command[] = [
  { id: "app.search", ctx: "global", desc: "Global search", defaults: ["Mod+k"] },
  { id: "app.find", ctx: "global", desc: "Find in page", defaults: ["Mod+f"] },
  { id: "app.findNext", ctx: "global", desc: "Next match", defaults: ["F3"] },
  { id: "app.findPrev", ctx: "global", desc: "Previous match", defaults: ["Shift+F3"] },
  { id: "app.toggleMode", ctx: "global", desc: "Edit / Preview", defaults: ["Mod+e"] },
  { id: "app.save", ctx: "global", desc: "Save", defaults: ["Mod+s"] },
  { id: "app.undo", ctx: "global", desc: "Undo delete & move", defaults: ["Mod+z"] },
  { id: "app.redo", ctx: "global", desc: "Redo delete & move", defaults: ["Mod+y", "Mod+Shift+z"] },
  { id: "page.new", ctx: "global", desc: "New page", defaults: ["Mod+n"] },
  { id: "section.new", ctx: "global", desc: "New section", defaults: ["Mod+Shift+n"] },
  { id: "focus.tree", ctx: "global", desc: "Focus tree", defaults: ["Mod+1"] },
  { id: "focus.editor", ctx: "global", desc: "Focus editor", defaults: ["Mod+2"] },
  { id: "focus.title", ctx: "global", desc: "Focus title", defaults: ["Mod+3"] },
  {
    id: "app.insertHelper",
    ctx: "global",
    desc: "Insert helper",
    defaults: ["Mod+j"],
    search: "color colour image width size resize table date link",
  },
  { id: "section.prev", ctx: "global", desc: "Previous section", defaults: ["Mod+PageUp"] },
  { id: "section.next", ctx: "global", desc: "Next section", defaults: ["Mod+PageDown"] },
  { id: "section.goto", ctx: "global", desc: "Go to section", defaults: ["Mod+g"] },
  { id: "page.moveToSection", ctx: "global", desc: "Move page to section", defaults: ["Mod+Shift+g"] },
  { id: "notebook.open", ctx: "global", desc: "Open notebook", defaults: ["Mod+o"] },
  {
    id: "notebook.import",
    ctx: "global",
    desc: "Import",
    defaults: ["Mod+i"],
    search: "onenote markdown mht folder pages sections",
  },
  {
    id: "history.open",
    ctx: "global",
    desc: "Page history",
    defaults: ["Mod+h"],
    gate: "git",
    search: "revision version diff restore snapshot git",
  },
  {
    id: "history.openDeleted",
    ctx: "global",
    desc: "Recover deleted pages",
    defaults: ["Mod+Shift+h"],
    gate: "git",
    search: "undelete trash restore recover",
  },
  { id: "app.settings", ctx: "global", desc: "Settings", defaults: ["Mod+,"] },
  // the main row's Ctrl++ arrives as Mod+Shift+=; the numpad's as Mod+= (chordKey folds + to =)
  { id: "app.zoomIn", ctx: "global", desc: "Zoom in", defaults: ["Mod+=", "Mod+Shift+="] },
  { id: "app.zoomOut", ctx: "global", desc: "Zoom out", defaults: ["Mod+-"] },
  { id: "app.zoomReset", ctx: "global", desc: "Reset zoom", defaults: ["Mod+0"] },
  { id: "app.help", ctx: "global", desc: "Shortcut overlay", defaults: ["Shift+/"] },

  {
    id: "app.showWindow",
    ctx: "system",
    desc: "Bring MyNote to the front",
    defaults: [],
    search: "global system-wide hotkey foreground restore tray background focus",
  },

  { id: "pane.scrollUp", ctx: "pane", desc: "Scroll active view up", defaults: ["PageUp"] },
  { id: "pane.scrollDown", ctx: "pane", desc: "Scroll active view down", defaults: ["PageDown"] },

  { id: "tree.filter", ctx: "tree", desc: "Filter pages", defaults: ["/"] },
  { id: "tree.selectUp", ctx: "tree", desc: "Select previous page", defaults: ["ArrowUp"] },
  { id: "tree.selectDown", ctx: "tree", desc: "Select next page", defaults: ["ArrowDown"] },
  { id: "tree.collapse", ctx: "tree", desc: "Collapse / go to parent", defaults: ["ArrowLeft"] },
  { id: "tree.expand", ctx: "tree", desc: "Expand / go to child", defaults: ["ArrowRight"] },
  { id: "tree.open", ctx: "tree", desc: "Open in editor", defaults: ["Enter"] },
  { id: "tree.newSubpage", ctx: "tree", desc: "New subpage", defaults: ["Mod+Enter"] },
  { id: "tree.rename", ctx: "tree", desc: "Rename", defaults: ["F2"] },
  { id: "tree.moveUp", ctx: "tree", desc: "Move up", defaults: ["Alt+ArrowUp"] },
  { id: "tree.moveDown", ctx: "tree", desc: "Move down", defaults: ["Alt+ArrowDown"] },
  {
    id: "tree.demote",
    ctx: "tree",
    desc: "Demote",
    defaults: ["Mod+]"],
    search: "subpage indent child",
  },
  {
    id: "tree.promote",
    ctx: "tree",
    desc: "Promote",
    defaults: ["Mod+["],
    search: "outdent unindent parent",
  },
  { id: "tree.delete", ctx: "tree", desc: "Delete", defaults: ["Delete"] },
  { id: "tree.toPrevSection", ctx: "tree", desc: "Move to previous section", defaults: ["Alt+ArrowLeft"] },
  { id: "tree.toNextSection", ctx: "tree", desc: "Move to next section", defaults: ["Alt+ArrowRight"] },

  { id: "results.selectUp", ctx: "results", desc: "Select previous result", defaults: ["ArrowUp"] },
  { id: "results.selectDown", ctx: "results", desc: "Select next result", defaults: ["ArrowDown"] },
  { id: "results.open", ctx: "results", desc: "Open result", defaults: ["Enter"] },
  { id: "results.refine", ctx: "results", desc: "Refine query", defaults: ["/"] },
  { id: "results.nextMatch", ctx: "results", desc: "Next match in the preview", defaults: ["n"], search: "peek" },
  { id: "results.prevMatch", ctx: "results", desc: "Previous match in the preview", defaults: ["p"], search: "peek" },

  { id: "history.selUp", ctx: "history", desc: "Select previous revision", defaults: ["ArrowUp"] },
  { id: "history.selDown", ctx: "history", desc: "Select next revision", defaults: ["ArrowDown"] },
  { id: "history.baseUp", ctx: "history", desc: "Move the comparison base up", defaults: ["Shift+ArrowUp"] },
  { id: "history.baseDown", ctx: "history", desc: "Move the comparison base down", defaults: ["Shift+ArrowDown"] },
  { id: "history.setBase", ctx: "history", desc: "Set base to selection", defaults: ["b"] },
  { id: "history.modeSplit", ctx: "history", desc: "Side-by-side diff", defaults: ["1"] },
  { id: "history.modeInline", ctx: "history", desc: "Inline diff", defaults: ["2"] },
  { id: "history.modeRendered", ctx: "history", desc: "Rendered view", defaults: ["3"] },
  { id: "history.modeText", ctx: "history", desc: "Text view", defaults: ["4"] },
  { id: "history.cycleMode", ctx: "history", desc: "Cycle view mode", defaults: ["v"] },
  { id: "history.nextChange", ctx: "history", desc: "Next change", defaults: ["n", "F3"] },
  { id: "history.prevChange", ctx: "history", desc: "Previous change", defaults: ["p", "Shift+F3"] },
  { id: "history.switchTab", ctx: "history", desc: "This page / deleted pages", defaults: ["Tab"] },
  { id: "history.restore", ctx: "history", desc: "Restore selected", defaults: ["Enter", "r"] },
  { id: "history.help", ctx: "history", desc: "This cheat sheet", defaults: ["Shift+/"] },
];

const BY_ID = new Map(COMMANDS.map((c) => [c.id, c]));

export const CONTEXT_LABELS: Record<BindingContext, string> = {
  global: "Global — work from anywhere, even while typing",
  system: "System-wide — works while another app has the keyboard",
  pane: "Panes",
  tree: "Page tree",
  results: "Search results",
  history: "History pane",
};

// Esc and Tab drive the focus ladder and dismiss every modal — rebinding them
// could strand the user inside a pane with no way out, so they stay fixed and
// are listed read-only wherever shortcuts are shown.
export type FixedBinding = { keys: string; desc: string; ctx: BindingContext };

export const FIXED_BINDINGS: FixedBinding[] = [
  { keys: `Tab / ${SHIFT_LABEL}+Tab`, desc: "Cycle panes", ctx: "pane" },
  { keys: "Esc", desc: "Back out one layer", ctx: "pane" },
  { keys: "Esc", desc: "Close history", ctx: "history" },
];

// ---------- chords ----------

export type ParsedChord = { mod: boolean; alt: boolean; shift: boolean; key: string };

export function parseChord(chord: string): ParsedChord {
  const parsed: ParsedChord = { mod: false, alt: false, shift: false, key: "" };
  // the key half can itself be "+", which splitting leaves as empty parts
  for (const part of chord.split("+").map((p) => p || "+")) {
    if (part === "Mod") parsed.mod = true;
    else if (part === "Alt") parsed.alt = true;
    else if (part === "Shift") parsed.shift = true;
    else parsed.key = part;
  }
  return parsed;
}

export function formatChord(chord: string): string {
  const { mod, alt, shift, key } = parseChord(chord);
  const parts: string[] = [];
  if (mod) parts.push(MOD_LABEL);
  if (alt) parts.push(ALT_LABEL);
  let keyLabel = KEY_LABELS[key] ?? (key.length === 1 ? key.toUpperCase() : key);
  if (shift) {
    // a bare shifted punctuation key reads as what it prints — "?" not "Shift+/"
    const printed = !mod && !alt ? SHIFTED_PRINTED[key] : undefined;
    if (printed) keyLabel = printed;
    else parts.push(SHIFT_LABEL);
  }
  parts.push(keyLabel);
  return parts.join("+");
}

const KEY_LABELS: Record<string, string> = {
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  PageUp: "PgUp",
  PageDown: "PgDn",
  Delete: "Del",
  Escape: "Esc",
  " ": "Space",
  Insert: "Ins",
};

const SHIFTED_PRINTED: Record<string, string> = {
  "/": "?",
  "=": "+",
  "-": "_",
  "[": "{",
  "]": "}",
  "\\": "|",
  ";": ":",
  "'": '"',
  "`": "~",
  ",": "<",
  ".": ">",
};

const MODIFIER_KEYS = new Set(["Control", "Shift", "Alt", "Meta", "CapsLock", "Dead"]);

/**
 * The chord an event represents, or null when it is only a modifier. The
 * secondary modifier (Ctrl on macOS, the Windows key elsewhere) is folded into
 * an "Other+" prefix that no binding can carry, so Ctrl+K on a Mac stays inert
 * instead of firing the Cmd+K command.
 */
export function chordOf(e: KeyboardEvent): string | null {
  const key = chordKey(e);
  if (MODIFIER_KEYS.has(key)) return null;
  const parts: string[] = [];
  if (isMac ? e.ctrlKey : e.metaKey) parts.push("Other");
  if (isMac ? e.metaKey : e.ctrlKey) parts.push("Mod");
  if (e.altKey) parts.push("Alt");
  if (e.shiftKey) parts.push("Shift");
  parts.push(key);
  return parts.join("+");
}

// Keys a text field needs for itself — the printable ones plus the caret and
// editing keys. PageUp/PageDown are deliberately absent: they page the main
// view even while typing, as they always have.
const TYPING_KEYS = new Set([
  "Enter", "Tab", "Backspace", "Delete", "Home", "End", " ",
  "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight",
]);

/** Whether a chord could be the user typing, so a binding on it must yield. */
export function isTextChord(chord: string): boolean {
  const { mod, alt, key } = parseChord(chord);
  if (mod || alt) return false;
  return key.length === 1 || TYPING_KEYS.has(key);
}

// ---------- resolution ----------

const NO_OVERRIDES: Record<string, string[]> = {};

function overrides(): Record<string, string[]> {
  return app.settings.keybindings ?? NO_OVERRIDES;
}

export function commandOf(id: string): Command | undefined {
  return BY_ID.get(id);
}

function defaultsOf(id: string): string[] {
  return BY_ID.get(id)?.defaults ?? [];
}

function sameChords(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((chord, i) => chord === b[i]);
}

export function chordsOf(id: string): string[] {
  const custom = overrides()[id];
  return custom ? [...custom] : defaultsOf(id);
}

export function isDefault(id: string): boolean {
  const custom = overrides()[id];
  return !custom || sameChords(custom, defaultsOf(id));
}

/** The chord shown in tooltips and menus — the first one, or "" when unassigned. */
export function labelOf(id: string): string {
  const [first] = chordsOf(id);
  return first ? formatChord(first) : "";
}

/** " (Ctrl+N)" for tooltips, or "" so an unassigned command shows no empty parens. */
export function hintOf(id: string): string {
  const label = labelOf(id);
  return label ? ` (${label})` : "";
}

/** Every chord, for the help overlay and the keybindings pane. */
export function labelsOf(id: string): string {
  return chordsOf(id).map(formatChord).join(" / ");
}

let indexedOverrides: unknown = null;
let index = new Map<BindingContext, Map<string, string>>();

function chordIndex(): Map<BindingContext, Map<string, string>> {
  const current = overrides();
  if (current === indexedOverrides) return index;
  indexedOverrides = current;
  index = new Map();
  for (const command of COMMANDS) {
    const ctxIndex = index.get(command.ctx) ?? new Map<string, string>();
    index.set(command.ctx, ctxIndex);
    for (const chord of chordsOf(command.id)) {
      if (!ctxIndex.has(chord)) ctxIndex.set(chord, command.id);
    }
  }
  return index;
}

export function commandForChord(ctx: BindingContext, chord: string): string | null {
  return chordIndex().get(ctx)?.get(chord) ?? null;
}

export function commandFor(ctx: BindingContext, e: KeyboardEvent): string | null {
  const chord = chordOf(e);
  return chord ? commandForChord(ctx, chord) : null;
}

// ---------- editing ----------

// Global and pane bindings reach every pane, so a pane-local chord that repeats
// one of them would never fire. Two panes may safely share a chord; the history
// pane owns the keyboard outright, so globals don't reach it — only the pane
// scroll keys, which it pages itself.
const REACH: Record<BindingContext, BindingContext[]> = {
  global: ["global", "system", "pane", "tree", "results"],
  system: ["global", "system", "pane", "tree", "results", "history"],
  pane: ["global", "system", "pane", "tree", "results", "history"],
  tree: ["global", "system", "pane", "tree"],
  results: ["global", "system", "pane", "results"],
  history: ["system", "pane", "history"],
};

/** Why `chord` can't be assigned to `id`, phrased for the user, or null. */
export function rejectionOf(id: string, chord: string): string | null {
  if (chord.startsWith("Other+")) return "That modifier isn't usable in a shortcut.";
  const { mod, alt, key } = parseChord(chord);
  if (key === "Tab") return "Tab can't be reassigned — it drives focus everywhere.";
  if (commandOf(id)?.ctx !== "system") return null;
  if (mod || alt) return null;
  return `A system-wide shortcut needs ${MOD_LABEL} or ${ALT_LABEL} — otherwise it swallows that key in every app.`;
}

/** The command a chord would collide with if assigned to `id`, if any. */
export function conflictOf(id: string, chord: string): Command | null {
  const command = commandOf(id);
  if (!command) return null;
  const reach = new Set(REACH[command.ctx]);
  for (const other of COMMANDS) {
    if (other.id === id || !reach.has(other.ctx)) continue;
    if (chordsOf(other.id).includes(chord)) return other;
  }
  return null;
}

function writeChords(id: string, chords: string[]) {
  const next = { ...overrides() };
  if (sameChords(chords, defaultsOf(id))) delete next[id];
  else next[id] = chords;
  app.settings.keybindings = next;
}

/** Assigns `chord` as the only chord for `id`, stealing it from whatever held it. */
export function assignChord(id: string, chord: string) {
  const stolen = conflictOf(id, chord);
  if (stolen) writeChords(stolen.id, chordsOf(stolen.id).filter((c) => c !== chord));
  writeChords(id, [chord]);
}

export function unassign(id: string) {
  writeChords(id, []);
}

export function resetToDefault(id: string) {
  writeChords(id, defaultsOf(id));
}

export function resetAllToDefaults() {
  app.settings.keybindings = {};
}
