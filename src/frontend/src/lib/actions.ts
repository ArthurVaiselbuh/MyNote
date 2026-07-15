import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { api, type NotebookInfo, type Section, type UndoOutcome } from "./api";
import { editorCtl } from "./editorCtl";
import { log } from "./log";
import { app, type ModalName, type Pane } from "./state/app.svelte";
import { countPages, countSubtree, findNode, flatten, locate, sectionOfPage } from "./treeUtils";

// ---------- notebook / boot ----------

export async function boot() {
  try {
    app.settings = await api.getSettings();
    await applyZoom();
  } catch (e) {
    app.status = String(e);
  }
  try {
    const info = await api.openNotebook();
    applyNotebook(info);
  } catch (e) {
    app.status = String(e);
    await openNotebookModal();
  }
}

function applyNotebook(info: NotebookInfo) {
  app.notebook = info.notebook;
  app.root = info.root;
  app.settings.notebookPath = info.root;
  const last = info.notebook.lastView;
  const idx = info.notebook.sections.findIndex((s) => s.id === last.sectionId);
  app.sectionIdx = idx >= 0 ? idx : 0;
  const pageId =
    last.pageId && sectionOfPage(info.notebook, last.pageId) ? last.pageId : null;
  app.currentPageId = pageId;
  app.selectedId = pageId;
  app.view = "page";
  app.focus = "tree";
}

export async function openNotebookModal() {
  log.verbose("open notebook picker");
  try {
    app.recentNotebooks = await api.listRecentNotebooks();
  } catch {
    app.recentNotebooks = [];
  }
  app.modal = "openNotebook";
}

async function switchNotebook(open: () => Promise<NotebookInfo>) {
  await editorCtl.current?.save();
  const info = await open();
  closeModal();
  app.currentPageId = null;
  applyNotebook(info);
}

export async function openNotebookAt(path: string) {
  try {
    await switchNotebook(() => api.openNotebook(path));
  } catch (e) {
    app.status = String(e);
  }
}

export async function browseNotebookFile() {
  log.verbose("open notebook file dialog");
  const file = await openDialog({
    title: "Open notebook — select its notebook.json",
    filters: [{ name: "MyNote notebook (notebook.json)", extensions: ["json"] }],
  });
  if (typeof file !== "string") return;
  await openNotebookAt(file);
}

export async function createNotebookDialog() {
  log.verbose("open new-notebook folder dialog");
  const dir = await openDialog({
    directory: true,
    title: "Choose an empty folder for the new notebook",
  });
  if (typeof dir !== "string") return;
  try {
    await switchNotebook(() => api.createNotebook(dir));
  } catch (e) {
    app.status = String(e);
  }
}

export async function refreshTree() {
  try {
    const notebook = await api.getTree();
    app.notebook = notebook;
    if (app.sectionIdx >= notebook.sections.length) {
      app.sectionIdx = Math.max(0, notebook.sections.length - 1);
    }
  } catch (e) {
    app.status = String(e);
  }
}

// ---------- sections ----------

export function currentSection(): Section | null {
  return app.notebook?.sections[app.sectionIdx] ?? null;
}

function wrapIndex(idx: number, length: number): number {
  return ((idx % length) + length) % length;
}

export function gotoSection(idx: number) {
  const sections = app.notebook?.sections ?? [];
  if (sections.length === 0) return;
  app.sectionIdx = wrapIndex(idx, sections.length);
  const first = currentSection()?.pages[0] ?? null;
  app.selectedId = first?.id ?? null;
  app.currentPageId = first?.id ?? null;
  app.view = "page";
  saveLastView();
}

export function gotoSectionOffset(offset: number) {
  gotoSection(app.sectionIdx + offset);
}

export function newSection() {
  app.renamingSection = false;
  app.creatingSection = true;
}

export async function createSectionNamed(name: string) {
  if (!app.creatingSection) return;
  app.creatingSection = false;
  try {
    const section = await api.createSection(name.trim() || "New Section");
    await refreshTree();
    const idx = app.notebook?.sections.findIndex((s) => s.id === section.id) ?? -1;
    if (idx >= 0) app.sectionIdx = idx;
    app.currentPageId = null;
    app.selectedId = null;
  } catch (e) {
    app.status = String(e);
  }
  app.focus = "tree";
}

export function cancelNewSection() {
  app.creatingSection = false;
  app.focus = "tree";
}

export async function renameSection(id: string, name: string) {
  try {
    await api.renameSection(id, name);
    await refreshTree();
  } catch (e) {
    app.status = String(e);
  }
}

export async function moveSection(id: string, index: number) {
  const keepId = currentSection()?.id ?? null;
  try {
    await api.moveSection(id, index);
    await refreshTree();
    if (keepId) {
      const idx = app.notebook?.sections.findIndex((s) => s.id === keepId) ?? -1;
      if (idx >= 0) app.sectionIdx = idx;
    }
  } catch (e) {
    app.status = String(e);
  }
}

export function deleteSectionWithConfirm(id: string) {
  const section = app.notebook?.sections.find((s) => s.id === id);
  if (!section) return;
  askConfirm(
    `Delete section "${section.name}" and its ${countPages(section)} page(s)?`,
    async () => {
      if (app.currentPageId && sectionOfPage(app.notebook!, app.currentPageId)?.id === id) {
        app.currentPageId = null;
        app.selectedId = null;
      }
      await api.deleteSection(id);
      await refreshTree();
    },
  );
}

// ---------- import ----------

export function openImport() {
  log.verbose("open import pane");
  app.importPreview = null;
  app.importSource = "";
  app.importPaths = [];
  app.importBusy = false;
  app.modal = "import";
}

export function setImportMode(mode: "mht" | "md") {
  if (app.importMode === mode) return;
  app.importMode = mode;
  app.importPreview = null;
  app.importSource = "";
  app.importPaths = [];
}

export async function chooseImportSource() {
  const mht = app.importMode === "mht";
  log.verbose(mht ? "choose OneNote files" : "choose markdown folder");
  const picked = await openDialog(
    mht
      ? {
          multiple: true,
          title: "Import OneNote export",
          filters: [{ name: "OneNote single-file export", extensions: ["mht", "mhtml"] }],
        }
      : { directory: true, title: "Choose a folder of Markdown files" },
  );
  const paths = typeof picked === "string" ? [picked] : picked;
  if (!paths || paths.length === 0) return;
  app.importPaths = paths;
  app.importSource = mht ? sourceLabel(paths) : (paths[0] ?? "");
  app.importBusy = true;
  app.importPreview = null;
  try {
    app.importPreview = mht ? await api.inspectMht(paths) : await api.inspectMd(paths[0]);
  } catch (e) {
    app.status = String(e);
  } finally {
    app.importBusy = false;
  }
}

function sourceLabel(paths: string[]): string {
  const names = paths.map((p) => p.replace(/^.*[\\/]/, ""));
  return names.length <= 2 ? names.join(", ") : `${names.length} files`;
}

export async function runImport() {
  const preview = app.importPreview;
  if (!preview || preview.pageCount === 0) return;
  const mht = app.importMode === "mht";
  const paths = app.importPaths;
  closeModal();
  try {
    const outcome = mht ? await api.importMht(paths) : await api.importMd(paths[0]);
    await refreshTree();
    const idx =
      app.notebook?.sections.findIndex((s) => s.id === outcome.sectionIds[0]) ?? -1;
    if (idx >= 0) gotoSection(idx);
    flashStatus(
      `imported ${outcome.pageCount} page(s) into ${outcome.sectionIds.length} section(s)`,
    );
  } catch (e) {
    app.status = String(e);
  }
}

function flashStatus(message: string) {
  app.status = message;
  setTimeout(() => {
    if (app.status === message) app.status = "";
  }, 4000);
}

// ---------- pages ----------

export function selectAndOpen(id: string) {
  app.selectedId = id;
  app.currentPageId = id;
  app.view = "page";
  saveLastView();
}

export function openPageById(pageId: string) {
  if (!app.notebook) return;
  const section = sectionOfPage(app.notebook, pageId);
  if (!section) return;
  app.sectionIdx = app.notebook.sections.indexOf(section);
  selectAndOpen(pageId);
}

export async function newPage() {
  const section = currentSection();
  if (!section) return;
  const sel = app.selectedId ? locate(section.pages, app.selectedId) : null;
  await createPageIn(section.id, sel?.parentId ?? null, sel?.node.id ?? null);
}

export async function newSubpage() {
  const section = currentSection();
  if (!section || !app.selectedId) return newPage();
  await createPageIn(section.id, app.selectedId, null);
}

async function createPageIn(sectionId: string, parentId: string | null, afterId: string | null) {
  try {
    const node = await api.createPage(sectionId, parentId, afterId);
    await refreshTree();
    selectAndOpen(node.id);
    app.focus = "editor";
    app.titleFocusReq++;
  } catch (e) {
    app.status = String(e);
  }
}

export function updateTreeTitle(id: string, title: string) {
  if (!app.notebook) return;
  for (const section of app.notebook.sections) {
    const node = findNode(section.pages, id);
    if (node) {
      node.title = title;
      return;
    }
  }
}

export function startRename(id?: string) {
  const target = id ?? app.selectedId;
  if (!target) return;
  app.renamingId = target;
}

export async function commitRename(id: string, value: string) {
  if (app.renamingId !== id) return;
  app.renamingId = null;
  const title = value.trim() || "Untitled";
  try {
    await api.renamePage(id, title);
    updateTreeTitle(id, title);
    if (app.currentPageId === id) editorCtl.current?.setTitle(title);
  } catch (e) {
    app.status = String(e);
  }
  app.focus = "tree";
}

export function deleteSelected() {
  const section = currentSection();
  if (!section || !app.selectedId) return;
  const found = locate(section.pages, app.selectedId);
  if (!found) return;
  const count = countSubtree(found.node);
  const suffix = count > 1 ? ` and ${count - 1} subpage(s)` : "";
  askConfirm(`Delete "${found.node.title}"${suffix}?`, async () => {
    const rows = visibleRows();
    const pos = rows.findIndex((r) => r.node.id === found.node.id);
    const neighbor =
      rows
        .slice(pos + 1)
        .find((r) => !findNode([found.node], r.node.id))?.node.id ??
      rows[pos - 1]?.node.id ??
      null;
    await api.deletePage(found.node.id);
    await refreshTree();
    if (app.currentPageId === found.node.id) app.currentPageId = neighbor;
    app.selectedId = neighbor;
    if (neighbor) saveLastView();
  });
}

export async function movePage(
  id: string,
  toSection: string,
  parentId: string | null,
  index: number,
) {
  try {
    await api.movePage(id, toSection, parentId, index);
    await refreshTree();
  } catch (e) {
    app.status = String(e);
  }
}

// ---------- undo / redo (tree ops) ----------

export async function undoLast() {
  await applyHistory(api.undo, "nothing to undo");
}

export async function redoLast() {
  await applyHistory(api.redo, "nothing to redo");
}

async function applyHistory(
  call: () => Promise<UndoOutcome | null>,
  emptyMessage: string,
) {
  try {
    await editorCtl.current?.save();
    const outcome = await call();
    if (!outcome) {
      flashStatus(emptyMessage);
      return;
    }
    await refreshTree();
    const notebook = app.notebook;
    if (notebook) {
      const idx = outcome.sectionId
        ? notebook.sections.findIndex((s) => s.id === outcome.sectionId)
        : -1;
      if (idx >= 0) app.sectionIdx = idx;
      if (app.currentPageId && !sectionOfPage(notebook, app.currentPageId)) {
        app.currentPageId = null;
        app.selectedId = null;
      }
      if (outcome.pageId && sectionOfPage(notebook, outcome.pageId)) {
        selectAndOpen(outcome.pageId);
      }
    }
    flashStatus(outcome.label);
  } catch (e) {
    app.status = String(e);
  }
}

// ---------- tree navigation ----------

export function visibleRows() {
  const section = currentSection();
  return section ? flatten(section.pages, app.treeFilter) : [];
}

export function selectOffset(offset: number) {
  const rows = visibleRows();
  if (rows.length === 0) return;
  const pos = rows.findIndex((r) => r.node.id === app.selectedId);
  const next = pos < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, pos + offset));
  const id = rows[next].node.id;
  app.selectedId = id;
  app.currentPageId = id;
  app.view = "page";
  saveLastView();
}

export function activateSelected() {
  if (!app.selectedId) return;
  selectAndOpen(app.selectedId);
  focusPane("editor");
}

export async function toggleExpand(id: string) {
  const section = currentSection();
  if (!section) return;
  const node = findNode(section.pages, id);
  if (!node || node.children.length === 0) return;
  node.expanded = !node.expanded;
  try {
    await api.setExpanded(id, node.expanded);
  } catch (e) {
    app.status = String(e);
  }
}

export async function collapseOrParent() {
  const section = currentSection();
  if (!section || !app.selectedId) return;
  const found = locate(section.pages, app.selectedId);
  if (!found) return;
  if (found.node.expanded && found.node.children.length > 0) {
    found.node.expanded = false;
    await api.setExpanded(found.node.id, false).catch(() => {});
  } else if (found.parentId) {
    app.selectedId = found.parentId;
    app.currentPageId = found.parentId;
    saveLastView();
  }
}

export async function expandOrChild() {
  const section = currentSection();
  if (!section || !app.selectedId) return;
  const found = locate(section.pages, app.selectedId);
  if (!found || found.node.children.length === 0) return;
  if (!found.node.expanded) {
    found.node.expanded = true;
    await api.setExpanded(found.node.id, true).catch(() => {});
  } else {
    const child = found.node.children[0];
    app.selectedId = child.id;
    app.currentPageId = child.id;
    saveLastView();
  }
}

export async function moveSelected(offset: number) {
  const section = currentSection();
  if (!section || !app.selectedId || app.treeFilter) return;
  const found = locate(section.pages, app.selectedId);
  if (!found) return;
  const target = found.index + offset;
  if (target < 0 || target >= found.siblings.length) return;
  // index is interpreted after detach, so moving down keeps the raw target
  await movePage(found.node.id, section.id, found.parentId, target);
}

export async function demoteSelected() {
  const section = currentSection();
  if (!section || !app.selectedId || app.treeFilter) return;
  const found = locate(section.pages, app.selectedId);
  if (!found || found.index === 0) return;
  const newParent = found.siblings[found.index - 1];
  await movePage(found.node.id, section.id, newParent.id, newParent.children.length);
}

export async function promoteSelected() {
  const section = currentSection();
  if (!section || !app.selectedId || app.treeFilter) return;
  const found = locate(section.pages, app.selectedId);
  if (!found || !found.parentId) return;
  const parent = locate(section.pages, found.parentId);
  if (!parent) return;
  await movePage(found.node.id, section.id, parent.parentId, parent.index + 1);
}

export async function moveSelectedToSection(target: Section) {
  const id = app.selectedId;
  if (!id || !app.notebook) return;
  const source = sectionOfPage(app.notebook, id);
  if (!source || source.id === target.id) return;
  await movePage(id, target.id, null, target.pages.length);
  const landed = app.notebook ? sectionOfPage(app.notebook, id) : null;
  if (!landed) return;
  app.sectionIdx = app.notebook!.sections.indexOf(landed);
  selectAndOpen(id);
}

export async function moveSelectedToAdjacentSection(offset: number) {
  const sections = app.notebook?.sections ?? [];
  if (sections.length < 2 || !app.selectedId) return;
  await moveSelectedToSection(sections[wrapIndex(app.sectionIdx + offset, sections.length)]);
}

// ---------- search ----------

export function openSearch() {
  log.verbose("open search");
  app.view = "results";
  app.focus = "search";
  app.searchFocusReq++;
}

export async function runSearch() {
  const query = app.searchQuery.trim();
  if (!query) {
    app.results = [];
    app.searchError = "";
    return;
  }
  try {
    app.results = await api.searchPages(query, app.searchMode);
    app.searchError = "";
    app.resultsSel = 0;
  } catch (e) {
    app.results = [];
    app.searchError = String(e);
  }
}

export function openResult(idx: number) {
  const hit = app.results[idx];
  if (!hit || !app.notebook) return;
  const sectionIdx = app.notebook.sections.findIndex((s) => s.id === hit.sectionId);
  if (sectionIdx < 0) return;
  app.sectionIdx = sectionIdx;
  // the editor remounts when leaving the results view and consumes the prefill on load
  app.findPrefill = { text: app.searchQuery.trim(), regex: app.searchMode === "regex" };
  app.view = "page";
  app.selectedId = hit.pageId;
  app.currentPageId = hit.pageId;
  app.focus = "editor";
  saveLastView();
}

// ---------- focus / view ----------

export function focusPane(pane: Pane) {
  log.verbose(`focus ${pane}`);
  // only the editor forces the page view — focusing the tree must not kick
  // the user out of results, or the results Tab ring could never cycle
  if (pane === "editor") app.view = "page";
  app.focus = pane;
  if (pane === "editor") {
    if (app.mode === "edit") app.editorFocusReq++;
  } else if (pane === "search") {
    app.searchFocusReq++;
  } else {
    blurActive();
  }
}

export function focusTitle() {
  app.view = "page";
  app.focus = "editor";
  app.titleFocusReq++;
}

export function cycleFocus(dir: number) {
  const ring: Pane[] =
    app.view === "page" ? ["tree", "editor"] : ["tree", "search", "results"];
  const idx = Math.max(0, ring.indexOf(app.focus));
  focusPane(ring[wrapIndex(idx + dir, ring.length)]);
}

export function toggleMode() {
  if (app.view !== "page" || !app.currentPageId) return;
  app.mode = app.mode === "edit" ? "preview" : "edit";
  app.focus = "editor";
  if (app.mode === "edit") app.editorFocusReq++;
}

export function openFind() {
  if (app.view !== "page" || !app.currentPageId) return;
  app.focus = "editor";
  editorCtl.current?.openFind();
}

export async function saveNow() {
  await editorCtl.current?.save();
  app.status = "saved";
  setTimeout(() => {
    if (app.status === "saved") app.status = "";
  }, 1500);
}

export function closeCurrent() {
  if (app.modal !== "none") {
    closeModal();
    return;
  }
  if (editorCtl.current?.findOpen()) {
    editorCtl.current.closeFind();
    if (app.mode === "edit") app.editorFocusReq++;
    return;
  }
  if (app.view === "results") {
    focusPane("editor");
    return;
  }
  if (app.focus === "editor") {
    blurActive();
    app.focus = "tree";
    return;
  }
  if (app.treeFilter || app.filterActive) {
    app.treeFilter = "";
    app.filterActive = false;
    return;
  }
}

export function scrollMain(dir: number) {
  let el: Element | null = null;
  if (app.view === "results") {
    el = document.getElementById("results-scroll");
  } else if (app.mode === "preview") {
    el = document.getElementById("preview-scroll");
  } else {
    el = document.querySelector(".cm-scroller");
  }
  if (el instanceof HTMLElement) {
    el.scrollBy({ top: dir * el.clientHeight * 0.85 });
  }
}

export function openTreeFilter() {
  app.filterActive = true;
  app.filterFocusReq++;
}

function blurActive() {
  const el = document.activeElement;
  if (el instanceof HTMLElement) el.blur();
}

// ---------- modals ----------

export function openModal(name: ModalName) {
  log.verbose(`open modal ${name}`);
  app.modal = name;
}

export function openSectionPicker(mode: "goto" | "move") {
  if (mode === "move" && !app.selectedId) return;
  log.verbose(`open section picker (${mode})`);
  app.sectionPickerMode = mode;
  app.sectionPickerRenaming = null;
  app.modal = "sectionPicker";
}

export function escapeModal() {
  if (app.modal === "sectionPicker" && app.sectionPickerRenaming) {
    app.sectionPickerRenaming = null;
    return;
  }
  if (app.modal === "colorPicker") {
    app.modal = "insert";
    return;
  }
  if (app.modal === "colors") {
    app.modal = "settings";
    return;
  }
  closeModal();
}

export function closeModal() {
  app.modal = "none";
  app.confirm = null;
  app.importPreview = null;
  if (app.focus === "editor" && app.mode === "edit") app.editorFocusReq++;
}

export function askConfirm(message: string, action: () => void | Promise<void>) {
  app.confirm = { message, action: () => void action() };
  app.modal = "confirm";
}

export function openInsertHelper() {
  if (app.view !== "page" || !app.currentPageId) return;
  log.verbose("open insert helper");
  if (app.mode !== "edit") app.mode = "edit";
  app.modal = "insert";
}

// ---------- settings / zoom ----------

export async function persistSettings() {
  try {
    await api.setSettings(JSON.parse(JSON.stringify(app.settings)));
  } catch (e) {
    app.status = String(e);
  }
}

export async function applyZoom() {
  try {
    await getCurrentWebview().setZoom(app.settings.zoom);
  } catch {
    // zoom is unsupported in some environments; ignore
  }
}

export async function zoomBy(delta: number) {
  app.settings.zoom = Math.min(2.5, Math.max(0.5, +(app.settings.zoom + delta).toFixed(2)));
  await applyZoom();
  await persistSettings();
}

export async function zoomReset() {
  app.settings.zoom = 1.0;
  await applyZoom();
  await persistSettings();
}

// ---------- misc ----------

export function saveLastView() {
  const section = currentSection();
  void api.setLastView(section?.id ?? null, app.currentPageId).catch(() => {});
}
