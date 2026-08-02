import { ALT_LABEL, MOD_LABEL, SHIFT_LABEL } from "./platform";

// Single source of truth for every shortcut's on-screen label, shared by the
// help overlay and component tooltips so labels stay platform-correct in one
// place. Intended to grow into the backing data for user-configurable
// keybindings — keep it declarative.
// `search` holds extra terms matched by the help filter but not shown — synonyms.
// `gate` hides the row entirely when the named feature isn't available (git.rs
// version history is opt-in and needs system git — see app.git in app.svelte.ts).
export type Shortcut = { keys: string; desc: string; search?: string; gate?: "git" };

const M = MOD_LABEL;
const A = ALT_LABEL;
const S = SHIFT_LABEL;

export const GLOBAL_SHORTCUTS: Shortcut[] = [
  { keys: `${M}+K`, desc: "Global search" },
  { keys: `${M}+F`, desc: "Find in page" },
  { keys: `${M}+E`, desc: "Edit / Preview" },
  { keys: `${M}+S`, desc: "Save" },
  { keys: `${M}+Z / Y`, desc: "Undo / redo delete & move" },
  { keys: `${M}+N`, desc: "New page" },
  { keys: `${M}+${S}+N`, desc: "New section" },
  { keys: `${M}+1 / 2 / 3`, desc: "Focus tree / editor / title" },
  { keys: `${M}+J`, desc: "Insert helper", search: "color colour image width size resize table date link" },
  { keys: `${M}+PgUp / PgDn`, desc: "Prev / next section" },
  { keys: `${M}+G`, desc: "Go to section" },
  { keys: `${M}+${S}+G`, desc: "Move page to section" },
  { keys: `${M}+O`, desc: "Open notebook" },
  { keys: `${M}+I`, desc: "Import", search: "onenote markdown mht folder pages sections" },
  { keys: `${M}+H`, desc: "Page history", gate: "git", search: "revision version diff restore snapshot git" },
  { keys: `${M}+${S}+H`, desc: "Recover deleted pages", gate: "git", search: "undelete trash restore recover" },
  { keys: `${M}+,`, desc: "Settings" },
  { keys: `${M}+= / − / 0`, desc: "Zoom in / out / reset" },
  { keys: `F3 / ${S}+F3`, desc: "Next / prev match" },
  { keys: "?", desc: "This overlay" },
];

export const PANE_SHORTCUTS: Shortcut[] = [
  { keys: `Tab / ${S}+Tab`, desc: "Cycle panes" },
  { keys: "PgUp / PgDn", desc: "Scroll active view" },
  { keys: "Esc", desc: "Back out one layer" },
];

export const TREE_SHORTCUTS: Shortcut[] = [
  { keys: "/", desc: "Filter pages" },
  { keys: "↑ / ↓", desc: "Select page" },
  { keys: "← / →", desc: "Collapse / expand" },
  { keys: "Enter", desc: "Open in editor" },
  { keys: `${M}+Enter`, desc: "New subpage" },
  { keys: "F2", desc: "Rename" },
  { keys: `${A}+↑ / ↓`, desc: "Move up / down" },
  { keys: `${M}+] / [`, desc: "Demote / promote", search: "subpage indent outdent child" },
  { keys: "Del", desc: "Delete" },
  { keys: `${A}+← / →`, desc: "Move to prev / next section" },
  { keys: `${M}+${S}+G`, desc: "Move to section…" },
];

export const EDITOR_SHORTCUTS: Shortcut[] = [
  { keys: `${M}+F`, desc: "Find in page" },
  { keys: `F3 / ${S}+F3`, desc: "Next / prev match" },
  { keys: `${M}+J`, desc: "Insert helper", search: "color colour image width size resize table date link" },
  { keys: "Esc", desc: "Close find / focus tree" },
];

export const RESULTS_SHORTCUTS: Shortcut[] = [
  { keys: "↑ / ↓", desc: "Select result" },
  { keys: "PgUp / PgDn", desc: "Scroll the preview", search: "peek page" },
  { keys: `N / P (also F3 / ${S}+F3)`, desc: "Next / prev match in the preview", search: "peek" },
  { keys: "Enter", desc: "Open result" },
  { keys: "/", desc: "Refine query" },
  { keys: "Esc", desc: "Back to page" },
];

export const HISTORY_SHORTCUTS: Shortcut[] = [
  { keys: "↑ / ↓", desc: "Select revision" },
  { keys: `${S}+↑ / ↓`, desc: "Move the comparison base" },
  { keys: "B", desc: "Set base to selection" },
  { keys: "1 / 2 / 3 / 4", desc: "Side-by-side / inline / rendered / text" },
  { keys: "V", desc: "Cycle view mode" },
  { keys: `N / P (also F3 / ${S}+F3)`, desc: "Next / previous change" },
  { keys: "Tab", desc: "This page / deleted pages" },
  { keys: "Enter / R", desc: "Restore selected" },
  { keys: "?", desc: "This cheat sheet" },
  { keys: "Esc", desc: "Close history" },
];
