import fs from "node:fs";
import { expect, test, type App } from "../app";

// Second section named "New Section", then back in "Notes" with the first
// page selected (Ctrl+PgUp's goto selects the section's first page).
async function addSectionAndReturn(app: App) {
  await app.page.keyboard.press("Control+Shift+N");
  await expect(app.page.locator(".section-strip input")).toBeFocused();
  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".section-strip .name")).toContainText("New Section");
  await app.page.keyboard.press("Control+PageUp");
  await expect(app.page.locator(".section-strip .name")).toContainText("Notes");
}

test("F2 rename rewrites the H1 in the file", async ({ app }) => {
  await app.newTitledPage("Original", 1);
  await app.page.keyboard.press("F2");
  const rename = app.page.locator(".tree .row input.rename");
  await expect(rename).toBeFocused();
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("Renamed");
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".tree .row .title")).toHaveText(["Renamed"]);
  const [id] = await app.treeIds();
  await expect.poll(() => fs.readFileSync(app.mdPath(id), "utf8")).toContain("# Renamed");
  await expect(app.page.locator(".title-input")).toHaveValue("Renamed");
});

test("Ctrl+Enter creates a subpage under the selection", async ({ app }) => {
  await app.newTitledPage("Parent", 1);
  await app.page.keyboard.press("Control+Enter");
  await expect(app.page.locator(".tree .row")).toHaveCount(2);
  await expect.poll(() => app.treeDepths()).toEqual([0, 1]);
});

test("deleting a page selects its next neighbor", async ({ app }) => {
  await app.newTitledPage("A", 1);
  await app.newTitledPage("B", 2);
  await app.newTitledPage("C", 3);
  await app.page.keyboard.press("ArrowUp"); // C -> B
  await expect(app.page.locator(".tree .row.selected .title")).toHaveText("B");

  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect(app.page.locator(".tree .row .title")).toHaveText(["A", "C"]);
  await expect(app.page.locator(".tree .row.selected .title")).toHaveText("C");
});

test("Alt+Right moves the page to the next section and follows it", async ({ app }) => {
  await app.newTitledPage("Wanderer", 1);
  await addSectionAndReturn(app);

  await app.page.keyboard.press("Alt+ArrowRight");
  await expect(app.page.locator(".section-strip .name")).toContainText("New Section");
  await expect(app.page.locator(".tree .row.selected .title")).toHaveText("Wanderer");
});

test("Ctrl+Shift+G moves the page via the picker and follows it", async ({ app }) => {
  await app.newTitledPage("Mover", 1);
  await addSectionAndReturn(app);

  await app.page.keyboard.press("Control+Shift+G");
  await expect(app.page.locator(".modal-title")).toHaveText("Move page to section");
  await expect(app.page.locator(".modal input")).toBeFocused();
  await app.page.keyboard.press("ArrowDown"); // Notes -> New Section
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".section-strip .name")).toContainText("New Section");
  await expect(app.page.locator(".tree .row.selected .title")).toHaveText("Mover");
});

test("Ctrl+G goes to a section without moving anything", async ({ app }) => {
  await app.newTitledPage("Stays", 1);
  await app.page.keyboard.press("Control+Shift+N");
  await expect(app.page.locator(".section-strip input")).toBeFocused();
  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".section-strip .name")).toContainText("New Section");

  await app.page.keyboard.press("Control+g");
  await expect(app.page.locator(".modal-title")).toHaveText("Go to section");
  await expect(app.page.locator(".modal input")).toBeFocused();
  await app.page.keyboard.press("ArrowUp"); // current New Section is preselected -> Notes
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".section-strip .name")).toContainText("Notes");
  await expect(app.page.locator(".tree .row .title")).toHaveText(["Stays"]);
});

test("Esc aborts creating a new section", async ({ app }) => {
  await app.page.keyboard.press("Control+Shift+N");
  await expect(app.page.locator(".section-strip input")).toBeFocused();
  await app.page.keyboard.press("Escape");

  await expect(app.page.locator(".section-strip input")).toHaveCount(0);
  await expect(app.page.locator(".section-strip .name")).toContainText("Notes");
  await expect(app.page.locator(".section-strip .name")).toContainText("1/1");
});

test("Alt+Up in the section picker reorders sections", async ({ app }) => {
  await app.page.keyboard.press("Control+Shift+N");
  await expect(app.page.locator(".section-strip input")).toBeFocused();
  await app.page.keyboard.press("Enter");
  // sections are now [Notes, New Section] with New Section current
  await expect(app.page.locator(".section-strip .name")).toContainText("New Section");
  await expect(app.page.locator(".section-strip .name")).toContainText("2/2");

  await app.page.keyboard.press("Control+g");
  await expect(app.page.locator(".modal input")).toBeFocused();
  // the current section (New Section) is preselected; Alt+Up moves it to the top
  await app.page.keyboard.press("Alt+ArrowUp");
  await expect(app.page.locator(".picker-item").first()).toContainText("New Section");
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".section-strip .name")).toContainText("New Section");
  await expect(app.page.locator(".section-strip .name")).toContainText("1/2");
});

test("F2 in the section picker renames a section", async ({ app }) => {
  await app.page.keyboard.press("Control+g");
  await expect(app.page.locator(".modal input")).toBeFocused();
  await app.page.keyboard.press("F2");
  const rename = app.page.locator(".picker-item input.picker-rename");
  await expect(rename).toBeFocused();
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("Renamed Notes");
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".picker-item").first()).toContainText("Renamed Notes");
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".modal")).toHaveCount(0);
  await expect(app.page.locator(".section-strip .name")).toContainText("Renamed Notes");
});

test("Esc leaves an in-progress rename before closing the section picker", async ({ app }) => {
  await app.page.keyboard.press("Control+g");
  await expect(app.page.locator(".modal input")).toBeFocused();
  await app.page.keyboard.press("F2");
  const rename = app.page.locator(".picker-item input.picker-rename");
  await expect(rename).toBeFocused();
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("scrapped");

  // first Esc peels only the rename: picker stays open, name unchanged
  await app.page.keyboard.press("Escape");
  await expect(rename).toHaveCount(0);
  await expect(app.page.locator(".modal")).toBeVisible();
  await expect(app.page.locator(".picker-item").first()).toContainText("Notes");
  // second Esc closes the picker
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".modal")).toHaveCount(0);
  await expect(app.page.locator(".section-strip .name")).toContainText("Notes");
});

test("/ filters the tree, Esc clears it", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.newTitledPage("Beta", 2);

  await app.page.keyboard.press("/");
  const filter = app.page.locator(".tree-filter");
  await expect(filter).toBeFocused();
  await app.page.keyboard.type("alp");
  await expect(app.page.locator(".tree .row .title")).toHaveText(["Alpha"]);

  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".tree .row")).toHaveCount(2);
  await expect(filter).toHaveCount(0);
});
