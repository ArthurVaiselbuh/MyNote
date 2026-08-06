import type { FindCtl } from "./findCtl";
import type { ModeAnchor } from "./viewPos";

// The preview's control surface, mirroring editorCtl: find, plus what the
// editor needs to land where the reader was when the mode flips.
export interface PreviewCtl extends FindCtl {
  anchor(): ModeAnchor | null;
}

export const previewCtl: { current: PreviewCtl | null } = { current: null };
