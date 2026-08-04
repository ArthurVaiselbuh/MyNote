import * as act from "../actions";
import { commandFor } from "./bindings";

export function treeKeys(e: KeyboardEvent) {
  switch (commandFor("tree", e)) {
    case "tree.filter": act.openTreeFilter(); break;
    case "tree.selectUp": act.selectOffset(-1); break;
    case "tree.selectDown": act.selectOffset(1); break;
    case "tree.collapse": void act.collapseOrParent(); break;
    case "tree.expand": void act.expandOrChild(); break;
    case "tree.open": act.activateSelected(); break;
    case "tree.newSubpage": void act.newSubpage(); break;
    case "tree.rename": act.startRename(); break;
    case "tree.moveUp": void act.moveSelected(-1); break;
    case "tree.moveDown": void act.moveSelected(1); break;
    case "tree.demote": void act.demoteSelected(); break;
    case "tree.promote": void act.promoteSelected(); break;
    case "tree.delete": act.deleteSelected(); break;
    case "tree.toPrevSection": void act.moveSelectedToAdjacentSection(-1); break;
    case "tree.toNextSection": void act.moveSelectedToAdjacentSection(1); break;
    default: return;
  }
  e.preventDefault();
}
