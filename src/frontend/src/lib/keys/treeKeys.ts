import * as act from "../actions";
import { chordKey, modPressed } from "./platform";

export function treeKeys(e: KeyboardEvent) {
  const mod = modPressed(e);
  const handled = () => e.preventDefault();

  if (e.altKey) {
    if (e.key === "ArrowLeft") { handled(); void act.moveSelectedToAdjacentSection(-1); }
    if (e.key === "ArrowRight") { handled(); void act.moveSelectedToAdjacentSection(1); }
    if (e.key === "ArrowUp") { handled(); void act.moveSelected(-1); }
    if (e.key === "ArrowDown") { handled(); void act.moveSelected(1); }
    return;
  }

  switch (chordKey(e)) {
    case "/": handled(); act.openTreeFilter(); return;
    case "ArrowUp": handled(); act.selectOffset(-1); return;
    case "ArrowDown": handled(); act.selectOffset(1); return;
    case "ArrowLeft": handled(); void act.collapseOrParent(); return;
    case "ArrowRight": handled(); void act.expandOrChild(); return;
    case "Enter": handled(); mod ? void act.newSubpage() : act.activateSelected(); return;
    case "F2": handled(); act.startRename(); return;
    case "Delete": handled(); act.deleteSelected(); return;
    case "]": if (mod) { handled(); void act.demoteSelected(); } return;
    case "[": if (mod) { handled(); void act.promoteSelected(); } return;
  }
}
