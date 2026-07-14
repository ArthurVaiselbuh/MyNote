import { invoke } from "@tauri-apps/api/core";

export interface PageNode {
  id: string;
  title: string;
  expanded: boolean;
  children: PageNode[];
}

export interface Section {
  id: string;
  name: string;
  pages: PageNode[];
}

export interface LastView {
  sectionId: string | null;
  pageId: string | null;
}

export interface Notebook {
  sections: Section[];
  lastView: LastView;
}

export interface NotebookInfo {
  root: string;
  notebook: Notebook;
}

export interface SearchHit {
  pageId: string;
  sectionId: string;
  sectionName: string;
  title: string;
  lineNo: number;
  snippet: string;
  ranges: [number, number][];
}

export interface UndoOutcome {
  label: string;
  sectionId: string | null;
  pageId: string | null;
}

export interface MhtPagePreview {
  title: string;
  duplicate: boolean;
}

export interface MhtFilePreview {
  path: string;
  sectionName: string;
  sectionExists: boolean;
  pages: MhtPagePreview[];
  error: string | null;
}

export interface MhtImportOutcome {
  sectionIds: string[];
  pageCount: number;
}

export interface WindowGeom {
  x: number;
  y: number;
  width: number;
  height: number;
  maximized: boolean;
}

export interface RecentNotebook {
  path: string;
  name: string;
  exists: boolean;
}

export interface Settings {
  notebookPath: string | null;
  recentNotebooks: string[];
  zoom: number;
  textColor: string;
  backgroundColor: string;
  panelColor: string;
  accentColor: string;
  focusAlpha: number;
  scrollSpeed: number;
  logLevel: string;
  window: WindowGeom | null;
}

export const defaultSettings: Settings = {
  notebookPath: null,
  recentNotebooks: [],
  zoom: 1.0,
  textColor: "#d4d4d4",
  backgroundColor: "#1e1f22",
  panelColor: "#26282b",
  accentColor: "#5aa0f2",
  focusAlpha: 0.5,
  scrollSpeed: 1.0,
  logLevel: "info",
  window: null,
};

export const api = {
  openNotebook: (path?: string) =>
    invoke<NotebookInfo>("open_notebook", { path: path ?? null }),
  createNotebook: (path: string) =>
    invoke<NotebookInfo>("create_notebook", { path }),
  listRecentNotebooks: () => invoke<RecentNotebook[]>("list_recent_notebooks"),
  getTree: () => invoke<Notebook>("get_tree"),
  createSection: (name: string) => invoke<Section>("create_section", { name }),
  renameSection: (id: string, name: string) =>
    invoke<void>("rename_section", { id, name }),
  moveSection: (id: string, index: number) =>
    invoke<void>("move_section", { id, index }),
  deleteSection: (id: string) => invoke<void>("delete_section", { id }),
  createPage: (sectionId: string, parentId: string | null, afterId: string | null) =>
    invoke<PageNode>("create_page", { sectionId, parentId, afterId }),
  readPage: (id: string) => invoke<string>("read_page", { id }),
  writePage: (id: string, content: string) =>
    invoke<string>("write_page", { id, content }),
  renamePage: (id: string, title: string) =>
    invoke<void>("rename_page", { id, title }),
  deletePage: (id: string) => invoke<void>("delete_page", { id }),
  movePage: (id: string, toSection: string, parentId: string | null, index: number) =>
    invoke<void>("move_page", { id, toSection, parentId, index }),
  undo: () => invoke<UndoOutcome | null>("undo"),
  redo: () => invoke<UndoOutcome | null>("redo"),
  setExpanded: (id: string, expanded: boolean) =>
    invoke<void>("set_expanded", { id, expanded }),
  setLastView: (sectionId: string | null, pageId: string | null) =>
    invoke<void>("set_last_view", { sectionId, pageId }),
  searchPages: (query: string, mode: "fuzzy" | "regex") =>
    invoke<SearchHit[]>("search_pages", { query, mode }),
  saveImage: (pageId: string, data: string, ext: string) =>
    invoke<string>("save_image", { pageId, data, ext }),
  inspectMht: (paths: string[]) =>
    invoke<MhtFilePreview[]>("inspect_mht", { paths }),
  importMht: (paths: string[]) =>
    invoke<MhtImportOutcome>("import_mht", { paths }),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
};
