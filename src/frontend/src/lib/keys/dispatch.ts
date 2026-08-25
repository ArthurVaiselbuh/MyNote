import * as act from "../actions";
import { contextMenu, contextMenuKeys } from "../contextMenu.svelte";
import { app } from "../state/app.svelte";
import { isCodeMirror, isTextEntry } from "../textEntry";
import { chordOf, commandForChord, isTextChord, mouseChordOf } from "./bindings";
import { resultsKeys, runResultsCommand } from "./resultsKeys";
import { runTreeCommand, treeKeys } from "./treeKeys";

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

  const typing = isTextEntry(target);

  // A global or pane chord fires even while typing, but only if it can't be
  // text: a command rebound to a bare letter must not swallow that letter in
  // the editor.
  const chord = chordOf(e);
  const bindable = chord && (!typing || !isTextChord(chord)) ? chord : null;

  // 3. global shortcuts fire regardless of focus
  const global = bindable && commandForChord("global", bindable);
  if (global && runGlobal(global, typing)) {
    e.preventDefault();
    e.stopPropagation();
    return;
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

  const pane = bindable && commandForChord("pane", bindable);
  if (pane) {
    if (typing && isCodeMirror(target)) return; // CM pages natively
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

export function handleMouseButton(e: MouseEvent) {
  if (e.button === 3 || e.button === 4) e.preventDefault();
  if (contextMenu.open || app.capturingChord || app.modal !== "none") return;
  const chord = mouseChordOf(e);
  if (!chord) return;

  const global = commandForChord("global", chord);
  if (global && runGlobal(global, false)) {
    e.preventDefault();
    e.stopPropagation();
    return;
  }

  const pane = commandForChord("pane", chord);
  if (pane) {
    e.preventDefault();
    e.stopPropagation();
    act.scrollMain(pane === "pane.scrollUp" ? -1 : 1);
    return;
  }

  const handled =
    app.focus === "tree"
      ? runTreeCommand(commandForChord("tree", chord))
      : app.focus === "results" || app.focus === "search"
        ? runResultsCommand(commandForChord("results", chord))
        : false;
  if (handled) {
    e.preventDefault();
    e.stopPropagation();
  }
}

/** Runs a global command; false means it declined and the key should fall through. */
function runGlobal(id: string, typing: boolean): boolean {
  switch (id) {
    // while typing, undo/redo stay native (editor/input text undo)
    case "app.undo": if (typing) return false; void act.undoLast(); return true;
    case "app.redo": if (typing) return false; void act.redoLast(); return true;
    case "app.search": act.openSearch(); return true;
    case "app.find": act.openFind(); return true;
    case "app.findNext": act.activePaneCtl()?.findNext(); return true;
    case "app.findPrev": act.activePaneCtl()?.findPrev(); return true;
    case "page.back": act.navigateViewedPages(-1); return true;
    case "page.forward": act.navigateViewedPages(1); return true;
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
