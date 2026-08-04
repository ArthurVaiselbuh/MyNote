import * as act from "./actions";
import { openContextMenu, type MenuEntry } from "./contextMenu.svelte";
import { ALT_LABEL, MOD_LABEL, SHIFT_LABEL } from "./keys/platform";
import { app } from "./state/app.svelte";
import { isTextEntry } from "./textEntry";

const M = MOD_LABEL;
const A = ALT_LABEL;
const S = SHIFT_LABEL;

const PAGE_LINK = /([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.md$/i;

// the openers select what was clicked before showing the menu, so they bail on
// a rename/filter input the same way openContextMenu does
export function openTreeMenu(e: MouseEvent) {
  if (isTextEntry(e.target)) return;
  const id = (e.target as HTMLElement).closest(".row")?.getAttribute("data-id");
  if (id) openTreeRowMenu(e, id);
  else openTreeBlankMenu(e);
}

function openTreeRowMenu(e: MouseEvent, id: string) {
  // select only — selectAndOpen would also navigate the editor to this page,
  // which a menu the user might just dismiss shouldn't do as a side effect
  app.selectedId = id;
  app.focus = "tree";
  const entries: MenuEntry[] = [
    { label: "New page", keys: `${M}+N`, run: () => void act.newPage() },
    { label: "New subpage", keys: `${M}+Enter`, run: () => void act.newSubpage() },
    "separator",
    { label: "Rename", keys: "F2", run: () => act.startRename(id) },
    { label: "Move up", keys: `${A}+↑`, run: () => void act.moveSelected(-1) },
    { label: "Move down", keys: `${A}+↓`, run: () => void act.moveSelected(1) },
    { label: "Demote", keys: `${M}+]`, run: () => void act.demoteSelected() },
    { label: "Promote", keys: `${M}+[`, run: () => void act.promoteSelected() },
    { label: "Move to section…", keys: `${M}+${S}+G`, run: () => act.openSectionPicker("move") },
  ];
  if (app.git?.available) {
    entries.push("separator", {
      label: "Page history…",
      keys: `${M}+H`,
      run: () => void act.openHistory("page"),
    });
  }
  entries.push("separator", {
    label: "Delete",
    keys: "Del",
    danger: true,
    run: () => act.deleteSelected(),
  });
  openContextMenu(e, entries);
}

function openTreeBlankMenu(e: MouseEvent) {
  openContextMenu(e, [
    { label: "New page", keys: `${M}+N`, run: () => void act.newPage() },
    { label: "New section", keys: `${M}+${S}+N`, run: () => act.newSection() },
  ]);
}

export function openSectionMenu(e: MouseEvent) {
  const section = act.currentSection();
  if (!section) {
    openContextMenu(e, [
      { label: "New section", keys: `${M}+${S}+N`, run: () => act.newSection() },
    ]);
    return;
  }
  openContextMenu(e, [
    { label: "New section", keys: `${M}+${S}+N`, run: () => act.newSection() },
    { label: "Rename section", run: () => act.startSectionRename() },
    { label: "Go to section…", keys: `${M}+G`, run: () => act.openSectionPicker("goto") },
    "separator",
    {
      label: "Delete section",
      danger: true,
      run: () => act.deleteSectionWithConfirm(section.id),
    },
  ]);
}

export function openPreviewMenu(e: MouseEvent) {
  const entries: MenuEntry[] = [];
  const link = (e.target as HTMLElement).closest("a");
  const href = link?.getAttribute("href") ?? "";
  const pageLink = href.match(PAGE_LINK);
  if (pageLink) {
    entries.push({ label: "Open page", run: () => act.openPageById(pageLink[1]) });
  } else if (/^https?:\/\//i.test(href)) {
    entries.push({ label: "Open link in browser", run: () => act.openExternalLink(href) });
  }
  if (href) {
    entries.push({ label: "Copy link address", run: () => void act.copyText(href) });
  }
  const selection = window.getSelection()?.toString() ?? "";
  if (selection) {
    entries.push({ label: "Copy", keys: `${M}+C`, run: () => void act.copyText(selection) });
  }
  if (entries.length) entries.push("separator");
  entries.push(
    { label: "Edit page", keys: `${M}+E`, run: () => act.toggleMode() },
    { label: "Find in page", keys: `${M}+F`, run: () => act.openFind() },
  );
  openContextMenu(e, entries);
}

export function openResultsMenu(e: MouseEvent) {
  if (isTextEntry(e.target)) return;
  const idx = Number((e.target as HTMLElement).closest(".hit")?.getAttribute("data-idx") ?? -1);
  if (idx < 0) return;
  app.resultsSel = idx;
  openContextMenu(e, [
    { label: "Open", keys: "Enter", run: () => act.openResult(idx) },
    { label: "Open in editor", run: () => act.openResultInEditor(idx) },
    "separator",
    { label: "Refine query", keys: "/", run: () => act.focusPane("search") },
  ]);
}
