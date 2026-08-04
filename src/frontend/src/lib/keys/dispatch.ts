import * as act from "../actions";
import { contextMenu, contextMenuKeys } from "../contextMenu.svelte";
import { app } from "../state/app.svelte";
import { isTextEntry } from "../textEntry";
import { chordKey, isHelpChord, modPressed } from "./platform";
import { resultsKeys } from "./resultsKeys";
import { treeKeys } from "./treeKeys";

export function handleGlobal(e: KeyboardEvent) {
  const mod = modPressed(e);
  const target = e.target as HTMLElement | null;

  // 0. an open context menu owns the keyboard, one layer above a modal — it can
  // be raised over any pane, and every other key would act behind it
  if (contextMenu.open) {
    contextMenuKeys(e);
    return;
  }

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
  if (mod && !e.altKey) {
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    switch (chordKey(e)) {
      // while typing, Ctrl+Z/Y stay native (editor/input text undo)
      case "z": if (typing) break; handled(); void (e.shiftKey ? act.redoLast() : act.undoLast()); return;
      case "y": if (typing) break; handled(); void act.redoLast(); return;
      case "k": handled(); act.openSearch(); return;
      case "f": handled(); act.openFind(); return;
      case "e": handled(); act.toggleMode(); return;
      case "s": handled(); void act.saveNow(); return;
      case "n": handled(); void (e.shiftKey ? act.newSection() : act.newPage()); return;
      case "j": handled(); act.openInsertHelper(); return;
      case "g": handled(); act.openSectionPicker(e.shiftKey ? "move" : "goto"); return;
      case "o": handled(); void act.openNotebookModal(); return;
      case "i": handled(); act.openImport(); return;
      case "h": handled(); void act.openHistory(e.shiftKey ? "deleted" : "page"); return;
      case ",": handled(); act.openModal("settings"); return;
      case "1": handled(); act.focusPane("tree"); return;
      case "2": handled(); act.focusPane("editor"); return;
      case "3": handled(); act.focusTitle(); return;
      case "PageUp": handled(); act.gotoSectionOffset(-1); return;
      case "PageDown": handled(); act.gotoSectionOffset(1); return;
      case "=": handled(); void act.zoomBy(0.1); return;
      case "-": handled(); void act.zoomBy(-0.1); return;
      case "0": handled(); void act.zoomReset(); return;
    }
  }

  if (e.key === "F3") {
    e.preventDefault();
    const findCtl = act.activeFindCtl();
    if (e.shiftKey) findCtl?.findPrev();
    else findCtl?.findNext();
    return;
  }

  if (e.key === "Escape") {
    // rename / filter inputs peel their own layer first
    if (target?.closest("[data-esc-local]")) return;
    e.preventDefault();
    act.closeCurrent();
    return;
  }

  if (isHelpChord(e) && !typing && !mod) {
    e.preventDefault();
    act.openModal("help");
    return;
  }

  if (e.key === "Tab" && !mod && !e.altKey) {
    // typing guard, except the search box which is part of the cycle ring
    const inSearchBox = !!target?.closest("[data-search-box]");
    if (!typing || inSearchBox) {
      e.preventDefault();
      act.cycleFocus(e.shiftKey ? -1 : 1);
    }
    return;
  }

  if ((e.key === "PageUp" || e.key === "PageDown") && !mod) {
    if (typing && target?.closest(".cm-editor")) return; // CM pages natively
    e.preventDefault();
    act.scrollMain(e.key === "PageUp" ? -1 : 1);
    return;
  }

  // 4. never hijack typing for pane-local keys
  if (typing) return;

  // 5. pane-local dispatch
  if (app.focus === "tree") treeKeys(e);
  else if (app.focus === "results" || app.focus === "search") resultsKeys(e);
}
