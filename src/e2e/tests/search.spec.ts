import { expect, test } from "../app";

test("fuzzy and regex search open the right page", async ({ app }) => {
  await app.newPageWithBody("Grocery", "buy milk and eggs", 1);
  await app.newPageWithBody("Meeting", "quarterly roadmap review", 2);

  // fuzzy: only Grocery's body contains a "milk" subsequence
  await app.page.keyboard.press("Control+k");
  const search = app.page.locator(".search-bar input");
  await expect(search).toBeFocused();
  await app.page.keyboard.type("milk");
  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await expect(app.page.locator(".results .hit .meta")).toContainText("Grocery");
  await expect(app.page.locator(".results .hit .snippet mark")).toContainText("milk");

  // Enter on the focused result opens the page in preview, query prehighlighted
  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".title-input")).toHaveValue("Grocery");
  await expect(app.page.locator(".editor-wrap")).toHaveClass(/focused/);
  await expect(app.page.locator(".preview")).toBeVisible();
  await expect(app.page.locator(".preview mark.find-hit")).toHaveText("milk");

  // regex mode
  await app.page.keyboard.press("Control+k");
  await expect(search).toBeFocused();
  await app.page.locator(".search-bar .mode", { hasText: "regex" }).click();
  await expect(search).toBeFocused();
  await app.page.keyboard.type("road\\w+");
  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await expect(app.page.locator(".results .hit .meta")).toContainText("Meeting");

  // Esc peels results back to the page view
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".results")).toHaveCount(0);
  await expect(app.page.locator(".title-input")).toHaveValue("Grocery");
});

test("keywords rank by how many matched; quotes match the phrase whole", async ({ app }) => {
  await app.newPageWithBody("Loud", "alpha alpha alpha", 1);
  await app.newPageWithBody("Quiet", "alpha and beta", 2);

  await app.page.keyboard.press("Control+k");
  const search = app.page.locator(".search-bar input");
  await expect(search).toBeFocused();
  await app.page.keyboard.type("alpha beta");
  await app.page.keyboard.press("Enter");
  const meta = app.page.locator(".results .hit .meta");
  await expect(meta).toHaveCount(2);
  await expect(meta.first()).toContainText("Quiet");

  await app.page.keyboard.press("Control+k");
  await expect(search).toBeFocused();
  await app.page.keyboard.type('"alpha and beta"');
  await app.page.keyboard.press("Enter");
  await expect(meta).toHaveCount(1);
  await expect(meta).toContainText("Quiet");
});
