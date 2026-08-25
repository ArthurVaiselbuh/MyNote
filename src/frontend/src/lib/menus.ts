import * as act from "./actions";
import { openContextMenu, type MenuEntry } from "./contextMenu.svelte";
import { labelOf } from "./keys/bindings";
import { MOD_LABEL } from "./keys/platform";
import { attachmentFromHref, isExternalHref, pageIdFromHref } from "./regex";
import { app } from "./state/app.svelte";
import { isTextEntry } from "./textEntry";

// the openers select what was clicked before showing the menu, so they bail on
// a rename/filter input the same way openContextMenu does
export function openTreeMenu(e: MouseEvent) {
  if (isTextEntry(e.target)) return;
  const id = (e.target as HTMLElement).closest(".row")?.getAttribute("data-id");
  if (id) openTreeRowMenu(e, id);
  else openTreeBlankMenu(e);
}

function openTreeRowMenu(e: MouseEvent, id: string) {
  // select only — setPageForView would also navigate the editor to this page,
  // which a menu the user might just dismiss shouldn't do as a side effect
  app.selectedId = id;
  app.focus = "tree";
  const entries: MenuEntry[] = [
    { label: "New page", keys: labelOf("page.new"), run: () => void act.newPage() },
    { label: "New subpage", keys: labelOf("tree.newSubpage"), run: () => void act.newSubpage() },
    "separator",
    { label: "Rename", keys: labelOf("tree.rename"), run: () => act.startRename(id) },
    { label: "Move up", keys: labelOf("tree.moveUp"), run: () => void act.moveSelected(-1) },
    { label: "Move down", keys: labelOf("tree.moveDown"), run: () => void act.moveSelected(1) },
    { label: "Demote", keys: labelOf("tree.demote"), run: () => void act.demoteSelected() },
    { label: "Promote", keys: labelOf("tree.promote"), run: () => void act.promoteSelected() },
    { label: "Move to section…", keys: labelOf("page.moveToSection"), run: () => act.openSectionPicker("move") },
  ];
  if (app.git?.available) {
    entries.push("separator", {
      label: "Page history…",
      keys: labelOf("history.open"),
      run: () => void act.openHistory("page"),
    });
  }
  entries.push("separator", {
    label: "Delete",
    keys: labelOf("tree.delete"),
    danger: true,
    run: () => act.deleteSelected(),
  });
  openContextMenu(e, entries);
}

function openTreeBlankMenu(e: MouseEvent) {
  openContextMenu(e, [
    { label: "New page", keys: labelOf("page.new"), run: () => void act.newPage() },
    { label: "New section", keys: labelOf("section.new"), run: () => act.newSection() },
  ]);
}

export function openSectionMenu(e: MouseEvent) {
  const entries: MenuEntry[] = [
    { label: "New section", keys: labelOf("section.new"), run: () => act.newSection() },
  ];
  const section = act.currentSection();
  if (section) {
    entries.push(
      { label: "Rename section", run: () => act.startSectionRename() },
      { label: "Go to section…", keys: labelOf("section.goto"), run: () => act.openSectionPicker("goto") },
      "separator",
      {
        label: "Delete section",
        danger: true,
        run: () => act.deleteSectionWithConfirm(section.id),
      },
    );
  }
  openContextMenu(e, entries);
}

export function openPreviewMenu(e: MouseEvent) {
  const entries: MenuEntry[] = [];
  const link = (e.target as HTMLElement).closest("a");
  const href = link?.getAttribute("href") ?? "";
  const pageId = pageIdFromHref(href);
  if (pageId) {
    entries.push({ label: "Open page", run: () => act.openPageById(pageId) });
  } else if (attachmentFromHref(href)) {
    entries.push({ label: "Show in folder", run: () => act.revealAttachment(href) });
  } else if (isExternalHref(href)) {
    entries.push({ label: "Open link in browser", run: () => act.openExternalLink(href) });
  }
  if (href) {
    entries.push({ label: "Copy link address", run: () => void act.copyText(href) });
  }
  const selection = window.getSelection()?.toString() ?? "";
  if (selection) {
    // the native clipboard chord, not one of ours — no registry entry to read
    entries.push({ label: "Copy", keys: `${MOD_LABEL}+C`, run: () => void act.copyText(selection) });
  }
  if (entries.length) entries.push("separator");
  entries.push(
    { label: "Edit page", keys: labelOf("app.toggleMode"), run: () => act.toggleMode() },
    { label: "Find in page", keys: labelOf("app.find"), run: () => act.openFind() },
  );
  openContextMenu(e, entries);
}

export function openResultsMenu(e: MouseEvent) {
  if (isTextEntry(e.target)) return;
  const idx = Number((e.target as HTMLElement).closest(".hit")?.getAttribute("data-idx") ?? -1);
  if (idx < 0) return;
  app.resultsSel = idx;
  openContextMenu(e, [
    { label: "Open", keys: labelOf("results.open"), run: () => act.openResult(idx) },
    { label: "Open in editor", run: () => act.openResultInEditor(idx) },
    "separator",
    { label: "Refine query", keys: labelOf("results.refine"), run: () => act.focusPane("search") },
  ]);
}
