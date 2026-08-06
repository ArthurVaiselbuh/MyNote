import { describe, expect, it } from "vitest";
import { chordKey } from "./platform";
import { press } from "./testPress";

describe("chordKey — Latin layouts keep the printed character", () => {
  it("lowercases letters", () => {
    expect(chordKey(press("k", "KeyK"))).toBe("k");
    expect(chordKey(press("K", "KeyK", { shiftKey: true }))).toBe("k");
  });

  it("passes named keys through untouched", () => {
    expect(chordKey(press("PageUp", "PageUp"))).toBe("PageUp");
    expect(chordKey(press("ArrowDown", "ArrowDown"))).toBe("ArrowDown");
    expect(chordKey(press("F3", "F3"))).toBe("F3");
  });

  it("follows the printed character on AZERTY, not the physical key", () => {
    expect(chordKey(press("a", "KeyQ"))).toBe("a");
  });
});

describe("chordKey — non-Latin layouts fall back to the physical key", () => {
  it("maps Hebrew letters back to their US-QWERTY meaning", () => {
    expect(chordKey(press("ל", "KeyK"))).toBe("k");
    expect(chordKey(press("ם", "KeyO"))).toBe("o");
  });

  it("maps Cyrillic letters back too", () => {
    expect(chordKey(press("л", "KeyK"))).toBe("k");
  });

  it("recovers punctuation bindings", () => {
    expect(chordKey(press("ף", "BracketLeft"))).toBe("[");
    expect(chordKey(press("ן", "BracketRight"))).toBe("]");
    expect(chordKey(press("ת", "Comma"))).toBe(",");
  });

  it("ignores ASCII punctuation the layout moved to another key", () => {
    expect(chordKey(press(",", "Slash", { shiftKey: true }))).toBe("/");
    expect(chordKey(press(".", "Slash"))).toBe("/");
  });
});

describe("chordKey — shifted punctuation normalizes to its base key", () => {
  it("reports the unshifted character", () => {
    expect(chordKey(press("?", "Slash", { shiftKey: true }))).toBe("/");
    expect(chordKey(press("+", "Equal", { shiftKey: true }))).toBe("=");
    expect(chordKey(press("{", "BracketLeft", { shiftKey: true }))).toBe("[");
  });

  it("keeps numpad +/- on the zoom bindings", () => {
    expect(chordKey(press("+", "NumpadAdd"))).toBe("=");
    expect(chordKey(press("-", "NumpadSubtract"))).toBe("-");
  });
});
