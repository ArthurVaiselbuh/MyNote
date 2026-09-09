import { beforeEach, expect, it, vi } from "vitest";
import { runNotebookUpdate } from "./notebookUpdate";

const mocks = vi.hoisted(() => ({
  app: { interactionBlocked: false, externalChanges: null as unknown },
  editorCtl: { current: { setEditingBlocked: vi.fn() } },
}));

vi.mock("./state/app.svelte", () => ({ app: mocks.app }));
vi.mock("./paneCtl", () => ({ editorCtl: mocks.editorCtl }));

beforeEach(() => {
  mocks.app.interactionBlocked = false;
  mocks.app.externalChanges = null;
  mocks.editorCtl.current = { setEditingBlocked: vi.fn() };
});

it("releases the interaction lock when an update fails", async () => {
  await expect(runNotebookUpdate(async () => {
    expect(mocks.app.interactionBlocked).toBe(true);
    throw new Error("failed write");
  })).rejects.toThrow("failed write");
  expect(mocks.app.interactionBlocked).toBe(false);
  expect(mocks.editorCtl.current.setEditingBlocked).toHaveBeenLastCalledWith(false);
});

it("leaves editing blocked when an external conflict appears during an update", async () => {
  await runNotebookUpdate(async () => { mocks.app.externalChanges = [false, "page"]; });
  expect(mocks.app.interactionBlocked).toBe(false);
  expect(mocks.editorCtl.current.setEditingBlocked).toHaveBeenLastCalledWith(true);
});

it("releases a newly mounted editor and ignores overlapping updates", async () => {
  const overlapping = vi.fn();
  await runNotebookUpdate(async () => {
    await runNotebookUpdate(overlapping);
    mocks.editorCtl.current = { setEditingBlocked: vi.fn() };
  });
  expect(overlapping).not.toHaveBeenCalled();
  expect(mocks.editorCtl.current.setEditingBlocked).toHaveBeenLastCalledWith(false);
});
