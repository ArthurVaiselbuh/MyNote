import { api, type ViewPos } from "./api";

// Where the user was last looking on each page, so switching pages (or flipping
// between the editor and the preview) resumes instead of jumping to the top.
// Persisted per notebook in notebook.user.json alongside the last view, written
// back on a debounce because preview scrolling changes a position continuously.

const PERSIST_MS = 1500;

const posByPage = new Map<string, ViewPos>();
const unsavedPages = new Set<string>();
let persistTimer: ReturnType<typeof setTimeout> | undefined;

export function viewPosOf(pageId: string): ViewPos {
  let pos = posByPage.get(pageId);
  if (!pos) {
    pos = { editorScrollTop: 0, editorCursor: 0, previewScrollTop: 0 };
    posByPage.set(pageId, pos);
  }
  return pos;
}

export function viewPosChanged(pageId: string) {
  unsavedPages.add(pageId);
  clearTimeout(persistTimer);
  persistTimer = setTimeout(() => void persistViewPositions(), PERSIST_MS);
}

export async function persistViewPositions() {
  clearTimeout(persistTimer);
  if (unsavedPages.size === 0) return;
  const entries: [string, ViewPos][] = [...unsavedPages].map((id) => [id, viewPosOf(id)]);
  unsavedPages.clear();
  try {
    await api.setViewPositions(entries);
  } catch {
    // a lost scroll offset is not worth surfacing — the next change retries
  }
}

/** Replaces the whole map with the notebook being opened. Anything still
 * unsaved belongs to the notebook being left, so callers flush *before* the
 * store swaps — by then this map's ids mean nothing to the backend. */
export async function loadViewPositions() {
  posByPage.clear();
  unsavedPages.clear();
  setModeAnchor(null);
  try {
    for (const [pageId, pos] of Object.entries(await api.getViewPositions())) {
      posByPage.set(pageId, pos);
    }
  } catch {
    // an unreadable user file just means every page starts at the top
  }
}

/** The spot the mode being left was showing, for the mode being entered to land
 * on. `offsetRatio` is fraction-of-block-height, not pixels — the editor and
 * preview render the same body line at different heights, so a raw pixel
 * offset from one doesn't mean the same thing in the other. `text` is the
 * matched string under the preview's find, so the editor can put the caret on
 * the hit itself rather than at the start of its line. */
export interface ModeAnchor {
  pageId: string;
  bodyLine: number;
  offsetRatio: number;
  text?: string;
}

let pendingAnchor: ModeAnchor | null = null;

export function setModeAnchor(next: ModeAnchor | null) {
  pendingAnchor = next;
}

export function takeModeAnchor(pageId: string): ModeAnchor | null {
  const taken = pendingAnchor;
  pendingAnchor = null;
  return taken?.pageId === pageId ? taken : null;
}
