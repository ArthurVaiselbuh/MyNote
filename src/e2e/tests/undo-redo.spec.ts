import { expect, test, type App } from "../app";

// Two root pages; B ends selected with tree focus.
async function twoPages(app: App): Promise<[string, string]> {
  await app.newPageToTree(1);
  await app.newPageToTree(2);
  return (await app.treeIds()) as [string, string];
}

test("move page undo/redo restores order", async ({ app }) => {
  const [A, B] = await twoPages(app);
  await app.page.keyboard.press("Alt+ArrowUp");
  await expect.poll(() => app.treeIds()).toEqual([B, A]);

  await app.page.keyboard.press("Control+z");
  await expect.poll(() => app.treeIds()).toEqual([A, B]);
  await expect(app.page.locator(".status-toast")).toContainText("moved");

  await app.page.keyboard.press("Control+y");
  await expect.poll(() => app.treeIds()).toEqual([B, A]);
});

test("delete page undo/redo, file stays on disk while deleted", async ({ app }) => {
  const [A, B] = await twoPages(app);
  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toEqual([A]);
  expect(app.mdExists(B)).toBe(true);

  await app.page.keyboard.press("Control+z");
  await expect.poll(() => app.treeIds()).toEqual([A, B]);
  await expect(app.page.locator(".status-toast")).toContainText("restored");

  await app.page.keyboard.press("Control+y");
  await expect.poll(() => app.treeIds()).toEqual([A]);
  expect(app.mdExists(B)).toBe(true);

  await app.page.keyboard.press("Control+z");
  await expect.poll(() => app.treeIds()).toEqual([A, B]);
});

test("demote to subpage and undo", async ({ app }) => {
  await twoPages(app);
  await app.page.keyboard.press("Control+]");
  await expect.poll(() => app.treeDepths()).toEqual([0, 1]);

  await app.page.keyboard.press("Control+z");
  await expect.poll(() => app.treeDepths()).toEqual([0, 0]);
});

test("Ctrl+Z while typing stays native editor undo", async ({ app }) => {
  const [A, B] = await twoPages(app);
  await app.page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();
  await app.page.keyboard.type("guardprobe");
  await expect(app.editorBody).toContainText("guardprobe");

  for (let i = 0; i < 5; i++) {
    if (!(await app.editorBody.textContent())!.includes("guardprobe")) break;
    await app.page.keyboard.press("Control+z");
  }
  await expect(app.editorBody).not.toContainText("guardprobe");
  expect(await app.treeIds()).toEqual([A, B]);
});

test("delete section undo restores its pages and jumps back", async ({ app }) => {
  const [A, B] = await twoPages(app);
  await app.newSection();

  await app.page.keyboard.press("Control+g");
  await expect(app.page.locator(".picker-list")).toBeVisible();
  await expect(app.modal.locator("input")).toBeFocused();
  await app.page.keyboard.press("ArrowUp"); // current New Section is preselected -> Notes
  await app.page.keyboard.press("Shift+Delete"); // delete "Notes"
  await app.confirmDanger();
  await expect(app.sectionName).toContainText("1/1");
  expect(app.mdExists(A) && app.mdExists(B)).toBe(true);

  await app.page.keyboard.press("Control+z");
  await expect(app.sectionName).toContainText("Notes");
  await expect.poll(() => app.treeIds()).toEqual([A, B]);
});

test("clean close purges deleted files, keeps the rest", async ({ app }) => {
  const [A, B] = await twoPages(app);
  await app.page.keyboard.press("Delete"); // B is selected
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toEqual([A]);
  expect(app.mdExists(B)).toBe(true);

  await app.close();
  expect(app.mdExists(B)).toBe(false);
  expect(app.mdExists(A)).toBe(true);
  const flat = JSON.stringify(app.readNotebookJson());
  expect(flat).toContain(A);
  expect(flat).not.toContain(B);
});
