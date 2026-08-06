import { expect, test, type App } from "../app";

const menu = (app: App) => app.page.locator(".context-menu");
const item = (app: App, label: string) => menu(app).locator(".item", { hasText: label });

test("the tree menu acts on the right-clicked row without opening it", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.newTitledPage("Beta", 2);
  await expect(app.titleInput).toHaveValue("Beta");

  await app.row("Alpha").click({ button: "right" });
  await expect(menu(app)).toBeVisible();
  await expect(item(app, "New subpage")).toBeVisible();
  await expect(item(app, "Move to section…")).toBeVisible();
  await expect(menu(app).locator(".sep").first()).toBeVisible();
  await expect(item(app, "Delete")).toHaveClass(/danger/);

  // selected, but the editor stays on Beta — a menu the user may just dismiss
  // must not navigate as a side effect
  await expect(app.selectedTitle).toHaveText("Alpha");
  await expect(app.titleInput).toHaveValue("Beta");

  await item(app, "Rename").click();
  await expect(menu(app)).toHaveCount(0);
  await expect(app.renameInput).toBeFocused();
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("Renamed");
  await app.page.keyboard.press("Enter");
  await expect(app.rowTitles).toHaveText(["Renamed", "Beta"]);
});

test("arrows step over separators and wrap, Enter runs the selection", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.row("Alpha").click({ button: "right" });
  const selected = menu(app).locator(".item.selected");

  // nothing is preselected, so the first ArrowUp wraps to the last item
  await app.page.keyboard.press("ArrowUp");
  await expect(selected).toHaveText(/Delete/);
  await expect(selected).toHaveClass(/danger/);

  await app.page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText(/New page/);

  // New subpage, then the separator is skipped rather than selected
  await app.page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText(/New subpage/);
  await app.page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText(/Rename/);

  await app.page.keyboard.press("Enter");
  await expect(menu(app)).toHaveCount(0);
  await expect(app.renameInput).toBeFocused();
});

test("Esc closes the menu without spending a step of the focus ladder", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.page.keyboard.press("Control+2");
  const editor = app.page.locator(".editor-wrap");
  await expect(editor).toHaveClass(/focused/);

  await app.page.locator(".section-strip").click({ button: "right" });
  await expect(menu(app)).toBeVisible();
  // a context menu is not a modal — it never joins the Esc ladder
  await expect(app.modalBackdrop).toHaveCount(0);
  await expect(editor).toHaveClass(/focused/);

  await app.page.keyboard.press("Escape");
  await expect(menu(app)).toHaveCount(0);
  await expect(editor).toHaveClass(/focused/);

  // the ladder was postponed, not broken: the next Esc still peels editor→tree
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".tree-pane")).toHaveClass(/focused/);
});

test("clicking outside dismisses the menu without running anything", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.row("Alpha").click({ button: "right" });
  await expect(menu(app)).toBeVisible();

  await app.titleInput.click();
  await expect(menu(app)).toHaveCount(0);
  await expect(app.rows).toHaveCount(1);
  await expect(app.rowTitles).toHaveText(["Alpha"]);
});

test("text fields keep the native menu, and a modal suppresses ours", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);

  await app.page.keyboard.press("F2");
  await expect(app.renameInput).toBeFocused();
  await app.renameInput.click({ button: "right" });
  await expect(menu(app)).toHaveCount(0);
  await app.page.keyboard.press("Escape");

  await app.page.keyboard.press("Control+,");
  await expect(app.modalBackdrop).toBeVisible();
  // the backdrop would swallow a real right-click, so drive the strip's own
  // handler directly — what is under test is openContextMenu's modal guard
  await app.page.locator(".section-strip").dispatchEvent("contextmenu");
  await expect(menu(app)).toHaveCount(0);
});

test("the preview menu offers link actions only over a link", async ({ app }) => {
  await app.newPageWithBody("Linky", "[Example](https://example.com)\n\nordinary paragraph", 1);
  await app.page.keyboard.press("Control+e");
  const preview = app.page.locator(".preview");
  await expect(preview).toBeVisible();

  await preview.locator("a").click({ button: "right" });
  await expect(item(app, "Open link in browser")).toBeVisible();
  await expect(item(app, "Copy link address")).toBeVisible();
  await expect(item(app, "Edit page")).toBeVisible();
  await app.page.keyboard.press("Escape");

  await preview.locator("p", { hasText: "ordinary paragraph" }).click({ button: "right" });
  await expect(menu(app)).toBeVisible();
  await expect(item(app, "Open link in browser")).toHaveCount(0);
  await expect(item(app, "Copy link address")).toHaveCount(0);
  await expect(item(app, "Edit page")).toBeVisible();
  await expect(item(app, "Find in page")).toBeVisible();
});

test("the results menu retargets the selection and opens in the editor", async ({ app }) => {
  await app.newPageWithBody("Groceries", "buy milk milk milk and eggs", 1);
  await app.newPageWithBody("Dairy", "milk comes from cows", 2);

  await app.search("milk");
  const hits = app.page.locator(".results .hit");
  await expect(hits).toHaveCount(2);
  await expect(app.page.locator(".peek .peek-head")).toContainText("Groceries");

  await hits.nth(1).click({ button: "right" });
  await expect(hits.nth(1)).toHaveClass(/selected/);
  await expect(app.page.locator(".peek .peek-head")).toContainText("Dairy");

  await item(app, "Open in editor").click();
  await expect(menu(app)).toHaveCount(0);
  await expect(app.page.locator(".results")).toHaveCount(0);
  await expect(app.titleInput).toHaveValue("Dairy");
  await expect(app.page.locator(".editor-wrap")).toHaveClass(/focused/);
  await expect(app.editorBody).toBeVisible();
});

test("menu hints follow a rebound chord", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.row("Alpha").click({ button: "right" });
  await expect(item(app, "New page").locator(".keys")).toHaveText("Ctrl+N");
  await app.page.keyboard.press("Escape");

  await app.openKeybindings();
  await app.rebind("New page", "Control+Shift+P", "Ctrl+Shift+P");
  await app.closeSettings();

  await app.row("Alpha").click({ button: "right" });
  await expect(item(app, "New page").locator(".keys")).toHaveText("Ctrl+Shift+P");
});
