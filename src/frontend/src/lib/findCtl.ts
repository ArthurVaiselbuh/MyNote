import type { FindPrefill } from "./state/app.svelte";

// shared shape between the editor's CodeMirror-backed find and the preview's
// DOM-highlight find, so callers can dispatch to whichever is active without
// caring which implementation backs it
export interface FindCtl {
  openFind(prefill?: FindPrefill | null): void;
  closeFind(): boolean;
  findOpen(): boolean;
  findNext(): void;
  findPrev(): void;
}
