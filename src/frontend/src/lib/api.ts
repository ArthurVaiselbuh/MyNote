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

export interface ViewPos {
  editorScrollTop: number;
  editorCursor: number;
  previewScrollTop: number;
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

export interface SearchResults {
  hits: SearchHit[];
  // the literal keywords/phrases the query split into — empty in regex mode
  terms: string[];
}

export interface UndoOutcome {
  label: string;
  sectionId: string | null;
  pageId: string | null;
}

export interface PagePreview {
  title: string;
  duplicate: boolean;
  children: PagePreview[];
}

export interface SectionPreview {
  name: string;
  exists: boolean;
  error: string | null;
  pages: PagePreview[];
}

export interface ImportPreview {
  sections: SectionPreview[];
  pageCount: number;
  duplicateCount: number;
}

export interface ImportOutcome {
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
  headingColor: string;
  focusAlpha: number;
  scrollSpeed: number;
  treeWidth: number;
  peekWidth: number;
  logLevel: string;
  minimizeToTray: boolean;
  startOnLogin: boolean;
  // command id -> chord list; only commands the user changed. An empty list is
  // an explicit "unassigned" — see keys/bindings.ts
  keybindings: Record<string, string[]>;
  window: WindowGeom | null;
}

export interface GitStatus {
  available: boolean;
  enabled: boolean;
  repo: boolean;
  intervalSecs: number;
}

export interface PageRevision {
  sha: string;
  at: number;
  subject: string;
}

export interface RevisionText {
  text: string;
  truncated: boolean;
  missing: boolean;
}

export interface DeletedChild {
  id: string;
  title: string;
  children: DeletedChild[];
}

export interface DeletedItem {
  sha: string;
  at: number;
  id: string;
  title: string;
  sectionId: string | null;
  sectionName: string | null;
  sectionExists: boolean;
  parentId: string | null;
  parentExists: boolean;
  resolved: boolean;
  pageCount: number;
  children: DeletedChild[];
}

export interface DeletedHistory {
  items: DeletedItem[];
  hitCap: boolean;
}

export interface RestoreOutcome {
  sectionId: string;
  pageId: string;
  pageCount: number;
  assetCount: number;
  renamed: boolean;
}

export const defaultSettings: Settings = {
  notebookPath: null,
  recentNotebooks: [],
  zoom: 1.0,
  textColor: "#d4d4d4",
  backgroundColor: "#1e1f22",
  panelColor: "#26282b",
  accentColor: "#5aa0f2",
  headingColor: "#d4d4d4",
  focusAlpha: 0.5,
  scrollSpeed: 1.0,
  treeWidth: 300,
  peekWidth: 460,
  logLevel: "info",
  minimizeToTray: false,
  startOnLogin: false,
  keybindings: {},
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
  getViewPositions: () => invoke<Record<string, ViewPos>>("get_view_positions"),
  setViewPositions: (entries: [string, ViewPos][]) =>
    invoke<void>("set_view_positions", { entries }),
  searchPages: (query: string, mode: "fuzzy" | "regex") =>
    invoke<SearchResults>("search_pages", { query, mode }),
  saveImage: (pageId: string, data: string, ext: string) =>
    invoke<string>("save_image", { pageId, data, ext }),
  inspectMht: (paths: string[]) =>
    invoke<ImportPreview>("inspect_mht", { paths }),
  importMht: (paths: string[]) =>
    invoke<ImportOutcome>("import_mht", { paths }),
  inspectMd: (root: string) => invoke<ImportPreview>("inspect_md", { root }),
  importMd: (root: string) => invoke<ImportOutcome>("import_md", { root }),
  resetAgentsMd: () => invoke<void>("reset_agents_md"),
  getSettings: () => invoke<Settings>("get_settings"),
  setSettings: (settings: Settings) => invoke<void>("set_settings", { settings }),
  getGitStatus: () => invoke<GitStatus>("get_git_status"),
  setGitSnapshots: (enabled: boolean, intervalSecs?: number) =>
    invoke<GitStatus>("set_git_snapshots", { enabled, intervalSecs }),
  pageRevisions: (id: string) => invoke<PageRevision[]>("page_revisions", { id }),
  revisionText: (id: string, sha: string) => invoke<RevisionText>("revision_text", { id, sha }),
  deletedPages: () => invoke<DeletedHistory>("deleted_pages"),
  deletedPageText: (sha: string, id: string) => invoke<RevisionText>("deleted_page_text", { sha, id }),
  restoreDeletedPage: (sha: string, id: string, fallbackSectionId: string | null) =>
    invoke<RestoreOutcome>("restore_deleted_page", { sha, id, fallbackSectionId }),
};
