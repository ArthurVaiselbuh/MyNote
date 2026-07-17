// The primary chord modifier is Cmd on macOS and Ctrl elsewhere — the standard
// desktop model. Every shortcut goes through modPressed()/MOD_LABEL so a future
// user-configurable keybinding layer has a single platform seam to build on.
export const isMac =
  typeof navigator !== "undefined" &&
  (/Mac/i.test(navigator.platform) || /Mac OS X/i.test(navigator.userAgent));

export function modPressed(e: KeyboardEvent | MouseEvent): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

export const MOD_LABEL = isMac ? "⌘" : "Ctrl";
export const ALT_LABEL = isMac ? "⌥" : "Alt";
export const SHIFT_LABEL = isMac ? "⇧" : "Shift";
