import type { FindCtl } from "./findCtl";

export interface EditorCtl extends FindCtl {
  save(): Promise<void>;
  insert(before: string, after?: string): void;
  setTitle(title: string): void;
}

export const editorCtl: { current: EditorCtl | null } = { current: null };
