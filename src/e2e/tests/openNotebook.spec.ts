import { expect, test } from "../app";

test("Ctrl+O opens the notebook pane with logo and recents; Esc closes", async ({ app }) => {
  await app.page.keyboard.press("Control+o");
  await expect(app.page.locator(".open-nb")).toBeVisible();

  // the logo must actually resolve (asset shipped in dist + CSP allows it)
  const logoWidth = await app.page
    .locator(".open-nb-logo")
    .evaluate((el) => (el as HTMLImageElement).naturalWidth);
  expect(logoWidth).toBeGreaterThan(0);

  // boot registered the scratch notebook as the current, most recent entry
  const current = app.page.locator(".recent-item", { hasText: "current" });
  await expect(current).toHaveCount(1);
  await expect(current.locator(".nb-path")).toContainText("notebook");

  // arrows walk from the recents into the action rows
  const selected = app.page.locator(".recent-item.selected");
  await expect(selected).toHaveText(/current/);
  await app.page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText(/New notebook/);
  await app.page.keyboard.press("ArrowDown");
  await expect(selected).toHaveText(/Open notebook/);

  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".open-nb")).toHaveCount(0);
});

test("Enter on a recent entry opens that notebook and closes the pane", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.page.keyboard.press("Control+o");
  await expect(app.page.locator(".recent-item.selected")).toHaveText(/current/);

  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".open-nb")).toHaveCount(0);
  await expect(app.sectionName).toContainText("Notes");
  await expect(app.rowTitles).toHaveText("Alpha");
});
