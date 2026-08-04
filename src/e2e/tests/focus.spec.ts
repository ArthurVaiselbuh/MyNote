import { expect, test } from "../app";

test("Tab cycles panes; Esc peels editor back to tree", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await expect(app.page.locator(".tree-pane")).toHaveClass(/focused/);

  await app.page.keyboard.press("Tab");
  await expect(app.page.locator(".editor-wrap")).toHaveClass(/focused/);
  // Tab now goes into CodeMirror (indent), so leave via Esc instead
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".tree-pane")).toHaveClass(/focused/);

  // results view ring cycles search -> results -> tree -> search
  await app.page.keyboard.press("Control+k");
  await expect(app.page.locator(".search-bar input")).toBeFocused();
  await app.page.keyboard.press("Tab"); // search box is part of the ring
  await expect(app.page.locator(".results")).toHaveClass(/focused/);
  await app.page.keyboard.press("Tab");
  await expect(app.page.locator(".tree-pane")).toHaveClass(/focused/);
  await expect(app.page.locator(".results")).toHaveCount(1); // still in results view
  await app.page.keyboard.press("Tab");
  await expect(app.page.locator(".search-bar")).toHaveClass(/focused/);

  // Esc peels results back to the page view with the editor focused
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".results")).toHaveCount(0);
  await expect(app.page.locator(".editor-wrap")).toHaveClass(/focused/);
});

test("? opens the context-aware, filterable help overlay", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  // the help chord is Shift+the base "/" key — press("?") alone sends no shiftKey
  await app.page.keyboard.press("Shift+Slash");
  await expect(app.page.locator(".modal-title")).toHaveText("Keyboard shortcuts");
  await expect(app.page.locator(".help-group h3").first()).toHaveText("Tree (focused pane)");

  const filter = app.page.locator(".modal input");
  await expect(filter).toBeFocused();
  await app.page.keyboard.type("undo");
  await expect(app.page.locator(".help-row", { hasText: "Undo delete & move" })).toBeVisible();
  await expect(app.page.locator(".help-row", { hasText: "Zoom" })).toHaveCount(0);

  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".modal-title")).toHaveCount(0);
});
