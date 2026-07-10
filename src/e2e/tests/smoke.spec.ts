import fs from "node:fs";
import { expect, test } from "../app";

test("boots a fresh notebook with the default section", async ({ app }) => {
  await expect(app.page.locator(".section-strip .name")).toContainText("Notes");
  await expect(app.page.locator(".tree-empty")).toContainText("Ctrl+N");
});

test("title + body edits hit disk and survive a restart", async ({ app }) => {
  await app.page.keyboard.press("Control+n");
  const title = app.page.locator(".title-input");
  await expect(title).toBeFocused();
  // the async page load resets the bound value to "Untitled" and collapses
  // the auto-selection — wait for it to land, then re-select explicitly
  await expect(title).toHaveValue("Untitled");
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("Alpha");

  await app.page.keyboard.press("Control+2");
  await expect(app.page.locator(".cm-content")).toBeFocused();
  await app.page.keyboard.type("hello persistence");
  await app.page.keyboard.press("Control+s");
  await expect(app.page.locator(".status-toast")).toContainText("saved");

  const [id] = await app.treeIds();
  const content = fs.readFileSync(app.mdPath(id), "utf8");
  expect(content).toContain("# Alpha");
  expect(content).toContain("hello persistence");

  await app.relaunch();
  await expect(app.page.locator(".tree .row .title")).toHaveText(["Alpha"]);
  await expect(app.page.locator(".cm-content")).toContainText("hello persistence");
});
