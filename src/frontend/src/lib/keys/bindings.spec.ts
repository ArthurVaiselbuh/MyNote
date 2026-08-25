import { beforeEach, describe, expect, it } from "vitest";
import {
  COMMANDS,
  assignChord,
  chordOf,
  chordsOf,
  commandFor,
  commandForChord,
  conflictOf,
  formatChord,
  isTextChord,
  labelOf,
  mouseChordOf,
  parseChord,
  rejectionOf,
  resetAllToDefaults,
  unassign,
} from "./bindings";
import { app } from "../state/app.svelte";
import { press } from "./testPress";

beforeEach(() => resetAllToDefaults());

describe("chordOf", () => {
  it("names a chord layout-stably", () => {
    expect(chordOf(press("k", "KeyK", { ctrlKey: true }))).toBe("Mod+k");
    expect(chordOf(press("ל", "KeyK", { ctrlKey: true }))).toBe("Mod+k");
    expect(chordOf(press("N", "KeyN", { ctrlKey: true, shiftKey: true }))).toBe("Mod+Shift+n");
    expect(chordOf(press("ArrowUp", "ArrowUp", { altKey: true }))).toBe("Alt+ArrowUp");
  });

  it("ignores a modifier pressed on its own", () => {
    expect(chordOf(press("Shift", "ShiftLeft", { shiftKey: true }))).toBe(null);
  });

  it("marks the secondary modifier so it can never match a binding", () => {
    expect(chordOf(press("k", "KeyK", { ctrlKey: true, metaKey: true }))).toBe("Other+Mod+k");
  });
});

describe("mouseChordOf", () => {
  function mouse(button: number, modifiers: MouseEventInit = {}) {
    return new MouseEvent("mousedown", { button, ...modifiers });
  }

  it("accepts middle and extended buttons but not primary or secondary click", () => {
    expect(mouseChordOf(mouse(0))).toBe(null);
    expect(mouseChordOf(mouse(2))).toBe(null);
    expect(mouseChordOf(mouse(1))).toBe("MouseMiddle");
    expect(mouseChordOf(mouse(3))).toBe("MouseBack");
    expect(mouseChordOf(mouse(4))).toBe("MouseForward");
    expect(mouseChordOf(mouse(5))).toBe("Mouse6");
  });

  it("uses the same modifiers as keyboard chords", () => {
    expect(mouseChordOf(mouse(3, { ctrlKey: true, shiftKey: true }))).toBe(
      "Mod+Shift+MouseBack",
    );
  });
});

describe("chord formatting", () => {
  it("round-trips through parse", () => {
    for (const chord of ["Mod+Shift+n", "Alt+ArrowLeft", "F3", "/", "Mod+,"]) {
      const { mod, alt, shift, key } = parseChord(chord);
      expect([mod, alt, shift, key]).toEqual([
        chord.includes("Mod+"),
        chord.includes("Alt+"),
        chord.includes("Shift+"),
        chord.split("+").pop(),
      ]);
    }
  });

  it("shows a bare shifted punctuation key as what it prints", () => {
    expect(formatChord("Shift+/")).toBe("?");
    expect(formatChord("Mod+Shift+/")).toBe("Ctrl+Shift+/");
  });

  it("uses arrows and short names for the named keys", () => {
    expect(formatChord("Alt+ArrowUp")).toBe("Alt+↑");
    expect(formatChord("Mod+PageDown")).toBe("Ctrl+PgDn");
    expect(formatChord("Delete")).toBe("Del");
    expect(formatChord("MouseMiddle")).toBe("Mouse Middle");
    expect(formatChord("Mod+MouseBack")).toBe("Ctrl+Mouse Back");
    expect(formatChord("Mouse6")).toBe("Mouse 6");
  });
});

describe("defaults", () => {
  it("resolves an event to its command per context", () => {
    expect(commandFor("global", press("k", "KeyK", { ctrlKey: true }))).toBe("app.search");
    expect(commandFor("tree", press("F2", "F2"))).toBe("tree.rename");
    expect(commandFor("tree", press("k", "KeyK", { ctrlKey: true }))).toBe(null);
    expect(commandFor("results", press("n", "KeyN"))).toBe("results.nextMatch");
    expect(commandForChord("global", "MouseBack")).toBe("page.back");
    expect(commandForChord("global", "MouseForward")).toBe("page.forward");
  });

  it("keeps ? off the settings binding on a layout that moves the comma", () => {
    expect(commandFor("global", press(",", "Slash", { shiftKey: true }))).toBe("app.help");
    expect(commandFor("global", press("ת", "Comma", { ctrlKey: true }))).toBe("app.settings");
  });

  it("leaves no chord serving two commands that can both hear it", () => {
    for (const command of COMMANDS) {
      for (const chord of command.defaults) {
        expect(conflictOf(command.id, chord)).toBe(null);
      }
    }
  });
});

describe("overrides", () => {
  it("rebinds a command and forgets its default", () => {
    assignChord("app.search", "Mod+Shift+p");
    expect(commandFor("global", press("p", "KeyP", { ctrlKey: true, shiftKey: true }))).toBe("app.search");
    expect(commandFor("global", press("k", "KeyK", { ctrlKey: true }))).toBe(null);
    expect(labelOf("app.search")).toBe("Ctrl+Shift+P");
  });

  it("takes a chord away from whatever held it", () => {
    assignChord("app.search", "Mod+f");
    expect(chordsOf("app.find")).toEqual([]);
    expect(commandFor("global", press("f", "KeyF", { ctrlKey: true }))).toBe("app.search");
  });

  it("persists mouse chords and applies the same conflict rules", () => {
    assignChord("page.back", "MouseMiddle");
    expect(chordsOf("page.back")).toEqual(["MouseMiddle"]);
    expect(commandForChord("global", "MouseMiddle")).toBe("page.back");

    assignChord("page.back", "MouseForward");
    expect(chordsOf("page.forward")).toEqual([]);
    expect(commandForChord("global", "MouseForward")).toBe("page.back");
  });

  it("unassigns without falling back to the default", () => {
    unassign("app.save");
    expect(chordsOf("app.save")).toEqual([]);
    expect(labelOf("app.save")).toBe("");
    expect(commandFor("global", press("s", "KeyS", { ctrlKey: true }))).toBe(null);
  });

  it("stores only what changed, so new defaults still apply", () => {
    unassign("app.save");
    expect(Object.keys(app.settings.keybindings)).toEqual(["app.save"]);
  });

  it("lets two panes share a chord but not a pane and a global", () => {
    expect(conflictOf("tree.open", "Enter")).toBe(null); // results.open also uses Enter
    expect(conflictOf("tree.rename", "Mod+k")?.id).toBe("app.search");
  });
});

describe("the system-wide shortcut", () => {
  it("stays unassigned and inert until the user sets it", () => {
    expect(chordsOf("app.showWindow")).toEqual([]);
    expect(commandFor("global", press("F5", "F5"))).toBe(null);
  });

  it("refuses a chord the OS would swallow in every app", () => {
    expect(rejectionOf("app.showWindow", "n")).not.toBe(null);
    expect(rejectionOf("app.showWindow", "Shift+F5")).not.toBe(null);
    expect(rejectionOf("app.showWindow", "Mod+Alt+n")).toBe(null);
    expect(rejectionOf("app.search", "n")).toBe(null);
    expect(rejectionOf("app.showWindow", "Mod+MouseBack")).toContain("Mouse buttons");
  });

  it("collides with in-app chords, which the OS would never deliver", () => {
    expect(conflictOf("app.showWindow", "Mod+k")?.id).toBe("app.search");
    expect(conflictOf("app.showWindow", "b")?.id).toBe("history.setBase");
    assignChord("app.showWindow", "Mod+k");
    expect(chordsOf("app.search")).toEqual([]);
  });
});

describe("isTextChord", () => {
  it("flags the chords a keystroke could be typing instead", () => {
    expect(isTextChord("/")).toBe(true);
    expect(isTextChord("Shift+/")).toBe(true);
    expect(isTextChord("Enter")).toBe(true);
    expect(isTextChord("Mod+k")).toBe(false);
    expect(isTextChord("F3")).toBe(false);
    expect(isTextChord("PageUp")).toBe(false); // pages the view even while typing
  });
});
