import type { ImportPreview, Notebook, RecentNotebook, SearchHit, Settings } from "../api";
import { defaultSettings } from "../api";

export type Pane = "tree" | "editor" | "search" | "results";
export type ModalName =
  | "none"
  | "help"
  | "settings"
  | "colors"
  | "sectionPicker"
  | "confirm"
  | "insert"
  | "colorPicker"
  | "import"
  | "openNotebook"
  | "welcome";

export interface ConfirmRequest {
  message: string;
  action: () => void;
  label?: string;
}

export interface FindPrefill {
  text: string;
  regex: boolean;
}

export const app = $state({
  focus: "tree" as Pane,
  view: "page" as "page" | "results",
  mode: "edit" as "edit" | "preview",

  notebook: null as Notebook | null,
  root: "",
  sectionIdx: 0,
  currentPageId: null as string | null,
  selectedId: null as string | null,
  renamingId: null as string | null,
  renamingSection: false,
  creatingSection: false,

  modal: "none" as ModalName,
  sectionPickerMode: "goto" as "goto" | "move",
  sectionPickerRenaming: null as string | null,
  confirm: null as ConfirmRequest | null,
  importMode: "md" as "mht" | "md",
  importSource: "",
  importPaths: [] as string[],
  importPreview: null as ImportPreview | null,
  importBusy: false,
  recentNotebooks: [] as RecentNotebook[],

  treeFilter: "",
  filterActive: false,

  searchQuery: "",
  searchMode: "fuzzy" as "fuzzy" | "regex",
  searchError: "",
  results: [] as SearchHit[],
  resultsSel: 0,

  findPrefill: null as FindPrefill | null,
  settings: { ...defaultSettings } as Settings,
  status: "",

  dragId: null as string | null,
  dropTarget: null as { id: string; zone: "before" | "after" | "inside" } | null,

  editorFocusReq: 0,
  titleFocusReq: 0,
  searchFocusReq: 0,
  filterFocusReq: 0,
});
