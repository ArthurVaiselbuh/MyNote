import { beforeEach, describe, expect, it, vi } from "vitest";
import { checkExternalChanges } from "./externalChanges";

const mocks = vi.hoisted(() => {
  const notebook = { sections: [{ id: "section", pages: [{ id: "page", children: [] }] }] };
  return {
    notebook,
    app: {
      notebook, root: "notebook", interactionBlocked: false,
      externalChanges: null as [boolean, string | null] | null,
      externalReloading: false, externalChangeError: "", currentPageId: "page",
      selectedId: "page", pendingPage: null, sectionIdx: 0, status: "",
    },
    editor: {
      hasUnsavedChanges: vi.fn(() => false),
      printContent: vi.fn(() => ({ title: "Local", body: "saved body" })),
      setEditingBlocked: vi.fn(), discardChanges: vi.fn(),
      load: vi.fn(async () => true), save: vi.fn(async () => true),
    },
    api: {
      checkExternalChanges: vi.fn<() => Promise<[boolean, string | null]>>(),
      resolveExternalChanges: vi.fn(),
    },
  };
});

vi.mock("./api", () => ({ api: mocks.api }));
vi.mock("./state/app.svelte", () => ({ app: mocks.app }));
vi.mock("./paneCtl", () => ({ editorCtl: { current: mocks.editor } }));
vi.mock("./actions", () => ({ openPageById: vi.fn() }));

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(mocks.app, {
    notebook: mocks.notebook, externalChanges: null, externalReloading: false,
    externalChangeError: "", status: "",
  });
  mocks.editor.hasUnsavedChanges.mockReturnValue(false);
  mocks.api.checkExternalChanges.mockResolvedValue([false, "page"]);
  mocks.api.resolveExternalChanges.mockResolvedValue([mocks.notebook, "page"]);
});

describe("external changes", () => {
  it("automatically reloads a saved page and releases editing", async () => {
    await checkExternalChanges();
    expect(mocks.api.resolveExternalChanges).toHaveBeenCalledWith(true, "# Local\n\nsaved body");
    expect(mocks.editor.load).toHaveBeenCalledWith("page", true);
    expect(mocks.app.externalChanges).toBeNull();
    expect(mocks.app.externalReloading).toBe(false);
    expect(mocks.editor.setEditingBlocked).toHaveBeenLastCalledWith(false);
  });

  it("leaves unsaved local edits for the conflict dialog", async () => {
    mocks.editor.hasUnsavedChanges.mockReturnValue(true);
    await checkExternalChanges();
    expect(mocks.api.resolveExternalChanges).not.toHaveBeenCalled();
    expect(mocks.app.externalChanges).toEqual([false, "page"]);
    expect(mocks.editor.setEditingBlocked).toHaveBeenCalledWith(true);
  });

  it("checks for local edits after the disk check returns", async () => {
    let finishCheck!: (changes: [boolean, string | null]) => void;
    mocks.api.checkExternalChanges.mockReturnValueOnce(new Promise((resolve) => { finishCheck = resolve; }));
    const checking = checkExternalChanges();
    mocks.editor.hasUnsavedChanges.mockReturnValue(true);
    finishCheck([false, "page"]);
    await checking;
    expect(mocks.api.resolveExternalChanges).not.toHaveBeenCalled();
  });

  it("keeps a failed automatic reload blocked with an error for the dialog", async () => {
    mocks.api.resolveExternalChanges.mockRejectedValueOnce("invalid JSON");
    await checkExternalChanges();
    expect(mocks.app.externalChangeError).toBe("invalid JSON");
    expect(mocks.app.externalChanges).toEqual([false, "page"]);
    expect(mocks.app.externalReloading).toBe(false);
    expect(mocks.editor.setEditingBlocked).toHaveBeenLastCalledWith(true);
  });
});
