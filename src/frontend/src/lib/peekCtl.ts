import type { FindCtl } from "./findCtl";

// The results peek has no find box of its own — it always highlights the search
// terms — so it satisfies FindCtl only to make F3/Shift+F3 step its matches.
export const peekCtl: { current: FindCtl | null } = { current: null };
