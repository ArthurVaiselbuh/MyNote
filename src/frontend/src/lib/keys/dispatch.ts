import * as act from "../actions";
import { editorCtl } from "../editorCtl";
import { app } from "../state/app.svelte";
import { resultsKeys } from "./resultsKeys";
import { treeKeys } from "./treeKeys";

function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement) return true;
  if (target.isContentEditable) return true;
  return target.closest(".cm-editor") !== null;
}

export function handleGlobal(e: KeyboardEvent) {
  const ctrl = e.ctrlKey || e.metaKey;
  const target = e.target as HTMLElement | null;

  // 1+2. an open modal (incl. insert helper) owns the keyboard; only Esc is global
  if (app.modal !== "none") {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      act.closeModal();
    }
    return;
  }

  const typing = isTyping(e.target);

  // 3. global shortcuts fire regardless of focus
  if (ctrl && !e.altKey) {
    const handled = () => {
      e.preventDefault();
      e.stopPropagation();
    };
    switch (e.key.toLowerCase()) {
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
      case "o": handled(); void act.openNotebookDialog(); return;
      case "i": handled(); void act.importMhtDialog(); return;
      case ",": handled(); act.openModal("settings"); return;
      case "1": handled(); act.focusPane("tree"); return;
      case "2": handled(); act.focusPane("editor"); return;
      case "3": handled(); act.focusTitle(); return;
      case "pageup": handled(); act.gotoSectionOffset(-1); return;
      case "pagedown": handled(); act.gotoSectionOffset(1); return;
      case "=": case "+": handled(); void act.zoomBy(0.1); return;
      case "-": handled(); void act.zoomBy(-0.1); return;
      case "0": handled(); void act.zoomReset(); return;
    }
  }

  if (e.key === "F3") {
    e.preventDefault();
    if (e.shiftKey) editorCtl.current?.findPrev();
    else editorCtl.current?.findNext();
    return;
  }

  if (e.key === "Escape") {
    // rename / filter inputs peel their own layer first
    if (target?.closest("[data-esc-local]")) return;
    e.preventDefault();
    act.closeCurrent();
    return;
  }

  if (e.key === "?" && !typing && !ctrl) {
    e.preventDefault();
    act.openModal("help");
    return;
  }

  if (e.key === "Tab" && !ctrl && !e.altKey) {
    // typing guard, except the search box which is part of the cycle ring
    const inSearchBox = !!target?.closest("[data-search-box]");
    if (!typing || inSearchBox) {
      e.preventDefault();
      act.cycleFocus(e.shiftKey ? -1 : 1);
    }
    return;
  }

  if ((e.key === "PageUp" || e.key === "PageDown") && !ctrl) {
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
