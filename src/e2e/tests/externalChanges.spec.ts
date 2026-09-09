import fs from "node:fs";
import path from "node:path";
import { expect, test } from "../app";

test("saved pages reload automatically and global undo and redo restore each version", async ({ app }) => {
  await app.newPageWithBody("Local", "saved body", 1);
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  fs.writeFileSync(app.mdPath(id), "# External title\n\nexternal body");
  const dialog = app.page.getByRole("dialog", { name: "Files changed externally" });
  await expect(app.titleInput).toHaveValue("External title");
  await expect(app.editorBody).toContainText("external body");
  await expect(dialog).toHaveCount(0);
  await app.page.keyboard.press("Control+1");
  await app.page.keyboard.press("Control+z");
  await expect(app.titleInput).toHaveValue("Local");
  await expect(app.editorBody).toContainText("saved body");
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  await app.page.keyboard.press("Control+y");
  await expect(app.titleInput).toHaveValue("External title");
  await expect(app.editorBody).toContainText("external body");
  await expect.poll(() => app.readMd(id)).toContain("external body");
  await expect(dialog).toHaveCount(0);
});

test("external page edits pause autosave and reload the editor and title", async ({ app }) => {
  await app.newPageWithBody("Local", "saved body", 1);
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  await app.selectWholeBody();
  await app.page.keyboard.insertText("unsaved local body");
  fs.writeFileSync(app.mdPath(id), "# External title\n\nexternal body");
  const saveError = await app.page.evaluate(async (id) => {
    const ipc = (window as unknown as {
      __TAURI_INTERNALS__: { invoke(command: string, args: unknown): Promise<unknown> };
    }).__TAURI_INTERNALS__;
    try {
      await ipc.invoke("write_page", { id, content: "# Local\n\nunsaved local body" });
      return null;
    } catch (error) {
      return String(error);
    }
  }, id);
  expect(saveError).toContain("changed externally");
  const dialog = app.page.getByRole("dialog", { name: "Files changed externally" });
  await expect(dialog).toBeVisible();
  await app.page.waitForTimeout(3300);
  expect(app.readMd(id)).toContain("external body");
  await dialog.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(app.titleInput).toHaveValue("External title");
  await expect(app.editorBody).toContainText("external body");
  await expect(app.rowTitles).toHaveText("External title");
});

test("overwrite restores a deleted page from the unsaved editor buffer", async ({ app }) => {
  await app.newPageWithBody("Local", "saved body", 1);
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  await app.selectWholeBody();
  await app.page.keyboard.insertText("keep my unsaved body");
  fs.unlinkSync(app.mdPath(id));
  const dialog = app.page.getByRole("dialog", { name: "Files changed externally" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(dialog.getByRole("alert")).toContainText(`Error loading ${id}.md- `);
  await dialog.getByRole("button", { name: "Overwrite", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect.poll(() => app.readMd(id)).toContain("keep my unsaved body");
});

test("notebook reload updates the tree and overwrite restores loaded metadata", async ({ app }) => {
  await app.newPageWithBody("Local", "saved body", 1);
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  const notebookPath = path.join(app.notebookDir, "notebook.json");
  const notebook = JSON.parse(fs.readFileSync(notebookPath, "utf8"));
  const originalSection = notebook.sections[0].name;
  notebook.sections[0].name = "External section";
  fs.writeFileSync(notebookPath, JSON.stringify(notebook));
  const dialog = app.page.getByRole("dialog", { name: "Files changed externally" });
  await expect(app.sectionName).toContainText("External section");
  await expect(dialog).toHaveCount(0);
  await app.page.keyboard.press("Control+1");
  await app.page.keyboard.press("Control+z");
  await expect(app.sectionName).toContainText(originalSection);
  await app.page.keyboard.press("Control+y");
  await expect(app.sectionName).toContainText("External section");
  fs.writeFileSync(notebookPath, "invalid JSON");
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole("alert")).toContainText("Error loading notebook.json- ");
  await dialog.getByRole("button", { name: "Overwrite", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(JSON.parse(fs.readFileSync(notebookPath, "utf8")).sections[0].name).toBe("External section");
});

test("auto-reloading an externally removed open page can be undone and redone", async ({ app }) => {
  await app.newPageWithBody("Removed", "saved body", 1);
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  const notebookPath = path.join(app.notebookDir, "notebook.json");
  const notebook = JSON.parse(fs.readFileSync(notebookPath, "utf8"));
  notebook.sections[0].pages = [];
  fs.writeFileSync(notebookPath, JSON.stringify(notebook));
  fs.unlinkSync(app.mdPath(id));
  const dialog = app.page.getByRole("dialog", { name: "Files changed externally" });
  await expect(app.rows).toHaveCount(0);
  await expect(dialog).toHaveCount(0);
  expect(fs.existsSync(app.mdPath(id))).toBe(false);
  await app.page.keyboard.press("Control+1");
  await app.page.keyboard.press("Control+z");
  await expect(app.rowTitles).toHaveText("Removed");
  await expect(app.editorBody).toContainText("saved body");
  await app.page.keyboard.press("Control+y");
  await expect(app.rows).toHaveCount(0);
  await app.newTitledPage("Next page", 1);
  await expect(dialog).toHaveCount(0);
});

test("reloading only notebook metadata preserves unsaved page edits", async ({ app }) => {
  await app.newPageWithBody("Local", "saved body", 1);
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("saved body");
  await app.selectWholeBody();
  await app.page.keyboard.insertText("keep this local edit");
  const notebookPath = path.join(app.notebookDir, "notebook.json");
  const notebook = JSON.parse(fs.readFileSync(notebookPath, "utf8"));
  notebook.sections[0].name = "Changed section";
  fs.writeFileSync(notebookPath, JSON.stringify(notebook));
  const dialog = app.page.getByRole("dialog", { name: "Files changed externally" });
  await expect(dialog).toBeVisible();
  await dialog.getByRole("button", { name: "Reload", exact: true }).click();
  await expect(dialog).toHaveCount(0);
  await expect(app.editorBody).toContainText("keep this local edit");
  await app.page.keyboard.press("Control+s");
  await expect.poll(() => app.readMd(id)).toContain("keep this local edit");
});
