import { expect, test } from "../app";

test("boots a fresh notebook with the default section", async ({ app }) => {
  await expect(app.sectionName).toContainText("Notes");
  await expect(app.page.locator(".tree-empty")).toContainText("Ctrl+N");
});

// Deliberately drives the raw keystrokes rather than App's page helpers — this
// is the one test that proves the bare create-edit-save path reaches disk.
test("title + body edits hit disk and survive a restart", async ({ app }) => {
  await app.page.keyboard.press("Control+n");
  await expect(app.titleInput).toBeFocused();
  await expect(app.titleInput).toHaveValue("Untitled");
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("Alpha");

  await app.page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();
  await app.page.keyboard.type("hello persistence");
  await app.page.keyboard.press("Control+s");
  await expect(app.page.locator(".status-toast")).toContainText("saved");

  const [id] = await app.treeIds();
  const content = app.readMd(id);
  expect(content).toContain("# Alpha");
  expect(content).toContain("hello persistence");

  await app.relaunch();
  await expect(app.rowTitles).toHaveText(["Alpha"]);
  await expect(app.editorBody).toContainText("hello persistence");
});
