import { app } from "./state/app.svelte";
import { isTextEntry } from "./textEntry";

// A right-click popup, deliberately NOT an app.modal: modals own the keyboard
// and sit on the Esc ladder, and a context menu that joined that ladder would
// perturb the four-pane focus model. It keeps app.focus untouched and gets its
// keys from an explicit branch at the top of dispatch.ts instead.
export interface MenuItem {
  label: string;
  keys?: string;
  run: () => void;
  danger?: boolean;
}

export type MenuEntry = MenuItem | "separator";

export const contextMenu = $state({
  open: false,
  x: 0,
  y: 0,
  entries: [] as MenuEntry[],
  sel: -1,
});

export function openContextMenu(e: MouseEvent, entries: MenuEntry[]) {
  // a rename/filter input sits inside the panes that raise menus — there the
  // webview's native menu stays, so ours must not stack on top of it
  if (app.modal !== "none" || isTextEntry(e.target) || entries.length === 0) return;
  contextMenu.x = e.clientX;
  contextMenu.y = e.clientY;
  contextMenu.entries = entries;
  contextMenu.sel = -1;
  contextMenu.open = true;
}

export function closeContextMenu() {
  contextMenu.open = false;
  contextMenu.entries = [];
  contextMenu.sel = -1;
}

export function runContextItem(item: MenuItem) {
  closeContextMenu();
  item.run();
}

function itemIndices(): number[] {
  return contextMenu.entries
    .map((entry, i) => (entry === "separator" ? -1 : i))
    .filter((i) => i >= 0);
}

function moveSelection(dir: number) {
  const indices = itemIndices();
  if (indices.length === 0) return;
  const pos = indices.indexOf(contextMenu.sel);
  const next = pos < 0 ? (dir > 0 ? 0 : indices.length - 1) : pos + dir;
  contextMenu.sel = indices[((next % indices.length) + indices.length) % indices.length];
}

function runSelection() {
  const entry = contextMenu.entries[contextMenu.sel];
  if (entry && entry !== "separator") runContextItem(entry);
}

export function contextMenuKeys(e: KeyboardEvent) {
  switch (e.key) {
    case "ArrowDown":
      e.preventDefault();
      moveSelection(1);
      return;
    case "ArrowUp":
      e.preventDefault();
      moveSelection(-1);
      return;
    case "Enter":
      e.preventDefault();
      runSelection();
      return;
    case "Escape":
    case "Tab":
      e.preventDefault();
      closeContextMenu();
      return;
  }
}
