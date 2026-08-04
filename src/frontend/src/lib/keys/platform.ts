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

// e.key is whatever character the *active* layout produces, so switching to a
// non-Latin one (Hebrew, Cyrillic, Greek) turns Ctrl+K into Ctrl+ל and every
// binding misses. chordKey() is the layout-stable name every binding matches on:
// an ASCII letter is taken as printed, so AZERTY/QWERTZ users still press the
// letter they see; everything else resolves from the physical key's US-QWERTY
// meaning. Punctuation can't go by what it prints even when that is ASCII — a
// Hebrew layout puts `,` on the Shift+`/` position, which would fire Settings
// from the help chord.
const US_QWERTY_BY_CODE: Record<string, string> = {
  KeyA: "a", KeyB: "b", KeyC: "c", KeyD: "d", KeyE: "e", KeyF: "f", KeyG: "g",
  KeyH: "h", KeyI: "i", KeyJ: "j", KeyK: "k", KeyL: "l", KeyM: "m", KeyN: "n",
  KeyO: "o", KeyP: "p", KeyQ: "q", KeyR: "r", KeyS: "s", KeyT: "t", KeyU: "u",
  KeyV: "v", KeyW: "w", KeyX: "x", KeyY: "y", KeyZ: "z",
  Digit0: "0", Digit1: "1", Digit2: "2", Digit3: "3", Digit4: "4",
  Digit5: "5", Digit6: "6", Digit7: "7", Digit8: "8", Digit9: "9",
  Minus: "-", Equal: "=", BracketLeft: "[", BracketRight: "]", Backslash: "\\",
  Semicolon: ";", Quote: "'", Backquote: "`", Comma: ",", Period: ".", Slash: "/",
};

// Bindings are written against the unshifted character and test e.shiftKey
// separately. Reached only by keys outside the table above — chiefly the numpad,
// whose `+` has to land on the same zoom binding as the main row's `=`.
const UNSHIFTED_BY_PRINTED: Record<string, string> = {
  "?": "/", "+": "=", _: "-", "{": "[", "}": "]", "|": "\\",
  ":": ";", '"': "'", "~": "`", "<": ",", ">": ".",
};

export function chordKey(e: KeyboardEvent): string {
  if (e.key.length !== 1) return e.key; // named keys (Arrow*, Enter, F3…) are layout-independent
  const printed = e.key.toLowerCase();
  if (printed >= "a" && printed <= "z") return printed;
  return US_QWERTY_BY_CODE[e.code] ?? UNSHIFTED_BY_PRINTED[printed] ?? printed;
}

export function isHelpChord(e: KeyboardEvent): boolean {
  return chordKey(e) === "/" && e.shiftKey;
}
