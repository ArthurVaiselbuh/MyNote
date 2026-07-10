import * as act from "../actions";

export function treeKeys(e: KeyboardEvent) {
  const ctrl = e.ctrlKey;
  const handled = () => e.preventDefault();

  if (e.altKey) {
    if (e.key === "ArrowLeft") { handled(); void act.moveSelectedToAdjacentSection(-1); }
    if (e.key === "ArrowRight") { handled(); void act.moveSelectedToAdjacentSection(1); }
    if (e.key === "ArrowUp") { handled(); void act.moveSelected(-1); }
    if (e.key === "ArrowDown") { handled(); void act.moveSelected(1); }
    return;
  }

  switch (e.key) {
    case "/": handled(); act.openTreeFilter(); return;
    case "ArrowUp": handled(); act.selectOffset(-1); return;
    case "ArrowDown": handled(); act.selectOffset(1); return;
    case "ArrowLeft": handled(); void act.collapseOrParent(); return;
    case "ArrowRight": handled(); void act.expandOrChild(); return;
    case "Enter": handled(); ctrl ? void act.newSubpage() : act.activateSelected(); return;
    case "F2": handled(); act.startRename(); return;
    case "Delete": handled(); act.deleteSelected(); return;
    case "]": if (ctrl) { handled(); void act.demoteSelected(); } return;
    case "[": if (ctrl) { handled(); void act.promoteSelected(); } return;
  }
}
