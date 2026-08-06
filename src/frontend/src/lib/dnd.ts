import * as act from "./actions";
import { app } from "./state/app.svelte";
import { findNode, hasDescendant, locate, type Located } from "./treeUtils";

type DropZone = NonNullable<typeof app.dropTarget>["zone"];

interface Press {
  x: number;
  y: number;
  id: string;
  row: HTMLElement;
  pointerId: number;
}

const DRAG_THRESHOLD_PX = 5;
const EDGE_BAND = 0.3;

let press: Press | null = null;

export function dndDown(e: PointerEvent, id: string) {
  if (e.button !== 0 || app.treeFilter || app.renamingId) return;
  const target = e.target as HTMLElement;
  if (target.closest("input, button")) return;
  const row = target.closest(".row") as HTMLElement | null;
  if (!row) return;
  press = { x: e.clientX, y: e.clientY, id, row, pointerId: e.pointerId };
}

export function dndMove(e: PointerEvent) {
  if (!press) return;
  let dragId = app.dragId;
  if (!dragId) {
    if (Math.abs(e.clientX - press.x) + Math.abs(e.clientY - press.y) < DRAG_THRESHOLD_PX) return;
    dragId = press.id;
    app.dragId = dragId;
    press.row.setPointerCapture(press.pointerId);
  }
  app.dropTarget = dropTargetUnder(e, dragId);
}

export function dndUp() {
  const drop = app.dropTarget;
  const dragId = app.dragId;
  press = null;
  if (!dragId) return;
  app.dragId = null;
  app.dropTarget = null;
  if (!drop) return;

  const section = act.currentSection();
  if (!section) return;
  const target = locate(section.pages, drop.id);
  if (!target) return;

  const { parentId, index } = dropSlot(target, dragId, drop.zone);
  void act.movePage(dragId, section.id, parentId, index);
  app.selectedId = dragId;
}

export function dndCancel() {
  press = null;
  app.dragId = null;
  app.dropTarget = null;
}

function dropTargetUnder(e: PointerEvent, dragId: string): typeof app.dropTarget {
  const under = document.elementFromPoint(e.clientX, e.clientY);
  const row = under?.closest(".row") as HTMLElement | null;
  const targetId = row?.dataset.id;
  if (!row || !targetId || targetId === dragId) return null;

  const section = act.currentSection();
  const dragged = section && findNode(section.pages, dragId);
  if (!dragged || hasDescendant(dragged, targetId)) return null;

  const rect = row.getBoundingClientRect();
  const rel = (e.clientY - rect.top) / rect.height;
  const zone = rel < EDGE_BAND ? "before" : rel > 1 - EDGE_BAND ? "after" : "inside";
  return { id: targetId, zone };
}

// movePage counts the index in the destination list *after* the dragged page
// has been detached, so it is excluded from that list before counting.
function dropSlot(target: Located, dragId: string, zone: DropZone) {
  if (zone === "inside") {
    return {
      parentId: target.node.id,
      index: target.node.children.filter((c) => c.id !== dragId).length,
    };
  }
  const siblings = target.siblings.filter((n) => n.id !== dragId);
  const at = siblings.findIndex((n) => n.id === target.node.id);
  return { parentId: target.parentId, index: zone === "before" ? at : at + 1 };
}
