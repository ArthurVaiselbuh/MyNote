import type { FindPrefill } from "./state/app.svelte";
import type { ModeAnchor } from "./viewPos";

// Whichever pane owns the main view registers itself here on mount and clears on
// unmount, so callers can drive find and paging without caring which
// implementation — CodeMirror, DOM highlights — backs the pane on screen.
export interface PaneCtl {
  openFind(prefill?: FindPrefill | null): void;
  closeFind(): boolean;
  findOpen(): boolean;
  findNext(): void;
  findPrev(): void;
  /** The element PgUp/PgDn pages. */
  scroller(): HTMLElement | null;
}

export interface EditorCtl extends PaneCtl {
  save(): Promise<void>;
  /** The line the editor is showing at its top edge, for the preview to land on. */
  anchor(): ModeAnchor | null;
  insert(before: string, after?: string): void;
  setTitle(title: string): void;
  /** Replaces the whole buffer as one normal edit (undoable via Ctrl+Z,
   * picked up by the usual autosave) — used to restore a page revision. */
  replaceAll(content: string): void;
}

export interface PreviewCtl extends PaneCtl {
  /** What the editor needs to land where the reader was when the mode flips. */
  anchor(): ModeAnchor | null;
}

export const editorCtl: { current: EditorCtl | null } = { current: null };
export const previewCtl: { current: PreviewCtl | null } = { current: null };
// The results peek has no find box of its own — it always highlights the search
// terms — so it satisfies PaneCtl only to make F3/Shift+F3 step its matches.
export const peekCtl: { current: PaneCtl | null } = { current: null };
