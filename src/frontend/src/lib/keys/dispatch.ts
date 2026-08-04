import * as act from "../actions";
import { contextMenu, contextMenuKeys } from "../contextMenu.svelte";
import { app } from "../state/app.svelte";
import { isTextEntry } from "../textEntry";
import { chordOf, commandFor, isTextChord } from "./bindings";
import { resultsKeys } from "./resultsKeys";
import { treeKeys } from "./treeKeys";

// A global chord fires even while typing, but only if it can't be text: a
// command rebound to a bare letter must not swallow that letter in the editor.
function firesWhileTyping(e: KeyboardEvent): boolean {
  const chord = chordOf(e);
  return !!chord && !isTextChord(chord);
}

export function handleGlobal(e: KeyboardEvent) {
  const target = e.target as HTMLElement | null;

  // 0. an open context menu owns the keyboard, one layer above a modal — it can
  // be raised over any pane, and every other key would act behind it
  if (contextMenu.open) {
    contextMenuKeys(e);
    return;
  }

  // the keybindings pane is recording a chord — every key belongs to it,
  // including the Esc that cancels the recording
  if (app.capturingChord) return;

  // 1+2. an open modal (incl. insert helper) owns the keyboard; only Esc is
  // global (closes it, or steps a stacked modal back one layer)
  if (app.modal !== "none") {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      act.escapeModal();
    }
    return;
  }

  const typing = isTextEntry(e.target);

  // 3. global shortcuts fire regardless of focus
  const global = commandFor("global", e);
  if (global && (!typing || firesWhileTyping(e))) {
    if (runGlobal(global, typing)) {
      e.preventDefault();
      e.stopPropagation();
      return;
    }
  }

  if (e.key === "Escape") {
    // rename / filter inputs peel their own layer first
    if (target?.closest("[data-esc-local]")) return;
    e.preventDefault();
    act.closeCurrent();
    return;
  }

  if (e.key === "Tab" && !e.ctrlKey && !e.metaKey && !e.altKey) {
    // typing guard, except the search box which is part of the cycle ring
    const inSearchBox = !!target?.closest("[data-search-box]");
    if (!typing || inSearchBox) {
      e.preventDefault();
      act.cycleFocus(e.shiftKey ? -1 : 1);
    }
    return;
  }

  const pane = commandFor("pane", e);
  if (pane && (!typing || firesWhileTyping(e))) {
    if (typing && target?.closest(".cm-editor")) return; // CM pages natively
    e.preventDefault();
    act.scrollMain(pane === "pane.scrollUp" ? -1 : 1);
    return;
  }

  // 4. never hijack typing for pane-local keys
  if (typing) return;

  // 5. pane-local dispatch
  if (app.focus === "tree") treeKeys(e);
  else if (app.focus === "results" || app.focus === "search") resultsKeys(e);
}

/** Runs a global command; false means it declined and the key should fall through. */
function runGlobal(id: string, typing: boolean): boolean {
  switch (id) {
    // while typing, undo/redo stay native (editor/input text undo)
    case "app.undo": if (typing) return false; void act.undoLast(); return true;
    case "app.redo": if (typing) return false; void act.redoLast(); return true;
    case "app.search": act.openSearch(); return true;
    case "app.find": act.openFind(); return true;
    case "app.findNext": act.activeFindCtl()?.findNext(); return true;
    case "app.findPrev": act.activeFindCtl()?.findPrev(); return true;
    case "app.toggleMode": act.toggleMode(); return true;
    case "app.save": void act.saveNow(); return true;
    case "page.new": void act.newPage(); return true;
    case "section.new": act.newSection(); return true;
    case "app.insertHelper": act.openInsertHelper(); return true;
    case "section.goto": act.openSectionPicker("goto"); return true;
    case "page.moveToSection": act.openSectionPicker("move"); return true;
    case "notebook.open": void act.openNotebookModal(); return true;
    case "notebook.import": act.openImport(); return true;
    case "history.open": void act.openHistory("page"); return true;
    case "history.openDeleted": void act.openHistory("deleted"); return true;
    case "app.settings": act.openModal("settings"); return true;
    case "app.help": act.openModal("help"); return true;
    case "focus.tree": act.focusPane("tree"); return true;
    case "focus.editor": act.focusPane("editor"); return true;
    case "focus.title": act.focusTitle(); return true;
    case "section.prev": act.gotoSectionOffset(-1); return true;
    case "section.next": act.gotoSectionOffset(1); return true;
    case "app.zoomIn": void act.zoomBy(0.1); return true;
    case "app.zoomOut": void act.zoomBy(-0.1); return true;
    case "app.zoomReset": void act.zoomReset(); return true;
  }
  return false;
}
