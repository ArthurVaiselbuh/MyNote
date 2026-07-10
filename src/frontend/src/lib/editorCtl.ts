import type { FindPrefill } from "./state/app.svelte";

export interface EditorCtl {
  save(): Promise<void>;
  openFind(prefill?: FindPrefill | null): void;
  closeFind(): boolean;
  findOpen(): boolean;
  findNext(): void;
  findPrev(): void;
  insert(before: string, after?: string): void;
  setTitle(title: string): void;
}

export const editorCtl: { current: EditorCtl | null } = { current: null };
