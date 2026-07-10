import * as act from "./actions";
import { app } from "./state/app.svelte";
import { isDescendant, locate } from "./treeUtils";

interface DragStart {
  x: number;
  y: number;
  id: string;
  row: HTMLElement;
  pointerId: number;
}

let start: DragStart | null = null;
let dragging = false;

export function dndDown(e: PointerEvent, id: string) {
  if (e.button !== 0 || app.treeFilter || app.renamingId) return;
  const target = e.target as HTMLElement;
  if (target.closest("input, button")) return;
  const row = target.closest(".row") as HTMLElement | null;
  if (!row) return;
  start = { x: e.clientX, y: e.clientY, id, row, pointerId: e.pointerId };
}

export function dndMove(e: PointerEvent) {
  if (!start) return;
  if (!dragging) {
    if (Math.abs(e.clientX - start.x) + Math.abs(e.clientY - start.y) < 5) return;
    dragging = true;
    app.dragId = start.id;
    start.row.setPointerCapture(start.pointerId);
  }
  const el = document.elementFromPoint(e.clientX, e.clientY);
  const row = el?.closest(".row") as HTMLElement | null;
  const targetId = row?.dataset.id;
  if (!row || !targetId || targetId === app.dragId) {
    app.dropTarget = null;
    return;
  }
  const section = act.currentSection();
  if (!section || isDescendant(section.pages, app.dragId!, targetId)) {
    app.dropTarget = null;
    return;
  }
  const rect = row.getBoundingClientRect();
  const rel = (e.clientY - rect.top) / rect.height;
  const zone = rel < 0.3 ? "before" : rel > 0.7 ? "after" : "inside";
  app.dropTarget = { id: targetId, zone };
}

export function dndUp() {
  const drop = app.dropTarget;
  const dragId = app.dragId;
  start = null;
  if (!dragging) return;
  dragging = false;
  app.dragId = null;
  app.dropTarget = null;
  if (!drop || !dragId) return;

  const section = act.currentSection();
  if (!section) return;
  const target = locate(section.pages, drop.id);
  if (!target) return;

  let parentId: string | null;
  let index: number;
  if (drop.zone === "inside") {
    parentId = target.node.id;
    index = target.node.children.filter((c) => c.id !== dragId).length;
  } else {
    parentId = target.parentId;
    const siblings = target.siblings.filter((n) => n.id !== dragId);
    const idx = siblings.findIndex((n) => n.id === drop.id);
    index = drop.zone === "before" ? idx : idx + 1;
  }
  void act.movePage(dragId, section.id, parentId, index);
  app.selectedId = dragId;
}

export function dndCancel() {
  start = null;
  dragging = false;
  app.dragId = null;
  app.dropTarget = null;
}
