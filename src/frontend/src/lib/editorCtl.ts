import type { FindCtl } from "./findCtl";
import type { ModeAnchor } from "./viewPos";

export interface EditorCtl extends FindCtl {
  save(): Promise<void>;
  /** The line the editor is showing at its top edge, for the preview to land on. */
  anchor(): ModeAnchor | null;
  insert(before: string, after?: string): void;
  setTitle(title: string): void;
  /** Replaces the whole buffer as one normal edit (undoable via Ctrl+Z,
   * picked up by the usual autosave) — used to restore a page revision. */
  replaceAll(content: string): void;
}

export const editorCtl: { current: EditorCtl | null } = { current: null };
