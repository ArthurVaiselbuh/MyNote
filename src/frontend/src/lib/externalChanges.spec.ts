import { beforeEach, describe, expect, it, vi } from "vitest";
import { checkExternalChanges, resolveExternalChanges } from "./externalChanges";
import { setPageForView } from "./actions";

const mocks = vi.hoisted(() => {
  const notebook = { sections: [{ id: "section", pages: [{ id: "page", children: [] }] }] };
  return {
    notebook,
    editorMounted: true,
    app: {
      notebook, root: "notebook", interactionBlocked: false,
      externalChanges: null as [boolean, string | null] | null,
      externalResolution: "idle", externalChangeError: "", currentPageId: "page" as string | null,
      selectedId: "page", pendingPage: null, sectionIdx: 0, status: "", view: "page",
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
vi.mock("./paneCtl", () => ({ editorCtl: { get current() { return mocks.editorMounted ? mocks.editor : null; } } }));
vi.mock("./actions", () => ({ setPageForView: vi.fn() }));

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(mocks.app, {
    notebook: mocks.notebook, externalChanges: null, externalResolution: "idle",
    externalChangeError: "", status: "", interactionBlocked: false,
    currentPageId: "page", selectedId: "page", sectionIdx: 0, view: "page",
  });
  mocks.editorMounted = true;
  mocks.editor.hasUnsavedChanges.mockReturnValue(false);
  mocks.editor.load.mockResolvedValue(true);
  mocks.editor.save.mockResolvedValue(true);
  mocks.api.checkExternalChanges.mockResolvedValue([false, "page"]);
  mocks.api.resolveExternalChanges.mockResolvedValue([mocks.notebook, "page"]);
});

describe("external changes", () => {
  it("automatically reloads a saved page and releases editing", async () => {
    await checkExternalChanges();
    expect(mocks.api.resolveExternalChanges).toHaveBeenCalledWith(true, "# Local\n\nsaved body");
    expect(mocks.editor.load).toHaveBeenCalledWith("page", true);
    expect(mocks.app.externalChanges).toBeNull();
    expect(mocks.app.externalResolution).toBe("idle");
    expect(mocks.app.interactionBlocked).toBe(false);
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
    expect(mocks.app.externalResolution).toBe("idle");
    expect(mocks.app.interactionBlocked).toBe(false);
    expect(mocks.editor.setEditingBlocked).toHaveBeenLastCalledWith(true);
  });

  it("serializes manual resolution and owns its busy state", async () => {
    mocks.app.externalChanges = [false, "page"];
    let finishResolve!: (value: unknown) => void;
    mocks.api.resolveExternalChanges.mockReturnValueOnce(new Promise((resolve) => { finishResolve = resolve; }));
    const resolving = resolveExternalChanges(true);
    expect(mocks.app.externalResolution).toBe("reload");
    expect(mocks.app.interactionBlocked).toBe(true);
    await resolveExternalChanges(false);
    expect(mocks.api.resolveExternalChanges).toHaveBeenCalledTimes(1);
    finishResolve([mocks.notebook, "page"]);
    await resolving;
    expect(mocks.app.externalResolution).toBe("idle");
    expect(mocks.app.interactionBlocked).toBe(false);
  });

  it("retries editor refresh after the backend has already accepted a reload", async () => {
    mocks.editor.load.mockResolvedValueOnce(false);
    await checkExternalChanges();
    expect(mocks.app.externalChangeError).toContain("Could not reload page");
    expect(mocks.app.externalChanges).toEqual([false, "page"]);
    expect(mocks.editor.setEditingBlocked).toHaveBeenLastCalledWith(true);
    mocks.api.resolveExternalChanges.mockResolvedValueOnce([mocks.notebook, null]);
    await resolveExternalChanges(true);
    expect(mocks.editor.load).toHaveBeenCalledTimes(2);
    expect(mocks.editor.load).toHaveBeenLastCalledWith("page", true);
    expect(mocks.app.externalChanges).toBeNull();
    expect(mocks.app.externalChangeError).toBe("");
  });

  it("retains selection when clearing an externally removed page fails", async () => {
    mocks.api.resolveExternalChanges.mockResolvedValueOnce([{ sections: [] }, null]);
    mocks.editor.load.mockResolvedValueOnce(false);
    await checkExternalChanges();
    expect(mocks.app.currentPageId).toBe("page");
    expect(mocks.app.externalChanges).not.toBeNull();
    expect(mocks.app.externalChangeError).toContain("Could not reload page");
  });

  it("keeps unsaved content when only metadata was reloaded", async () => {
    mocks.app.externalChanges = [true, null];
    mocks.api.resolveExternalChanges.mockResolvedValueOnce([mocks.notebook, null]);
    await resolveExternalChanges(true);
    expect(mocks.editor.discardChanges).not.toHaveBeenCalled();
    expect(mocks.editor.load).not.toHaveBeenCalled();
    expect(mocks.app.externalChanges).toBeNull();
  });

  it("preserves results view when the selected page is externally removed", async () => {
    mocks.editorMounted = false;
    mocks.app.view = "results";
    mocks.api.resolveExternalChanges.mockResolvedValueOnce([{ sections: [] }, null]);
    await checkExternalChanges();
    expect(mocks.app.currentPageId).toBeNull();
    expect(mocks.app.view).toBe("results");
    expect(setPageForView).not.toHaveBeenCalled();
    expect(mocks.app.externalChanges).toBeNull();
  });

  it("keeps the conflict available if saving after overwrite fails", async () => {
    mocks.app.externalChanges = [false, "page"];
    mocks.editor.save.mockResolvedValueOnce(false);
    await resolveExternalChanges(false);
    expect(mocks.app.externalChangeError).toContain("Could not save page");
    expect(mocks.app.externalChanges).not.toBeNull();
    expect(mocks.app.interactionBlocked).toBe(false);
    expect(mocks.editor.setEditingBlocked).toHaveBeenLastCalledWith(true);
  });
});
