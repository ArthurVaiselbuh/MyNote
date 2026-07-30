import type { GitStatus, ImportPreview, Notebook, RecentNotebook, SearchHit, Settings } from "../api";
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
  | "welcome"
  | "history";

export interface ConfirmRequest {
  message: string;
  action: () => void;
  label?: string;
  /** Modal to step back to on Cancel/Esc instead of closing everything. */
  returnTo?: ModalName;
}

export type HelpContext = "app" | "history";
export type HistoryTab = "page" | "deleted";
export type HistoryMode = "split" | "inline" | "rendered" | "text";

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
  searchTerms: [] as string[],
  results: [] as SearchHit[],
  resultsSel: 0,

  findPrefill: null as FindPrefill | null,
  settings: { ...defaultSettings } as Settings,
  status: "",
  statusIsError: false,

  // which cheat sheet the ? overlay shows; "history" narrows it to the history
  // pane's own keys and sends Esc back to that pane
  helpContext: "app" as HelpContext,

  git: null as GitStatus | null,
  historyTab: "page" as HistoryTab,
  historyMode: "split" as HistoryMode,
  // rail indices into ["now (on disk)", ...revisions]; 0 is always "now".
  // base is the left/before side, sel the right/after side: the split diff reads
  // as "what restoring the selected revision would do", so base defaults to "now".
  historyRevSel: 1,
  historyRevBase: 0,
  // index into the flattened (depth-first) display list on the deleted tab
  historyDeletedSel: 0,

  dragId: null as string | null,
  dropTarget: null as { id: string; zone: "before" | "after" | "inside" } | null,

  editorFocusReq: 0,
  titleFocusReq: 0,
  searchFocusReq: 0,
  filterFocusReq: 0,
});
