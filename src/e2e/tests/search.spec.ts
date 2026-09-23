import { expect, fillerBody, test } from "../app";
import fs from "node:fs";
import path from "node:path";

test("search exclusions refresh results, survive restart, and stay visible when collapsed", async ({ app }, testInfo) => {
  await app.newPageWithBody("Included", "milkshake", 1);
  await app.newSection("Archive");
  await app.newPageWithBody("Archived", "milk", 1);
  await app.search("milk");
  const hits = app.page.locator(".results .hit");
  await expect(hits).toHaveCount(2);
  const advanced = app.page.getByRole("button", { name: /^Advanced/ });
  await advanced.click();
  await app.page.getByRole("button", { name: "Excluded sections (0)" }).click();
  const dialog = app.page.getByRole("dialog");
  await expect(dialog.getByRole("textbox", { name: "Filter sections" })).toBeFocused();
  await dialog.getByRole("checkbox", { name: "Archive", exact: true }).check();
  await expect(hits).toHaveCount(1);
  await expect(hits).toContainText("Included");
  await app.page.keyboard.press("Escape");
  await expect(app.page.getByRole("button", { name: "Excluded sections (1)" })).toBeFocused();
  await app.page.keyboard.press("Tab");
  await app.page.keyboard.press("Enter");
  await expect(dialog).toContainText("Excluded search strategies");
  await dialog.getByRole("checkbox", { name: /^Partial word/ }).check();
  await expect(hits).toHaveCount(0);
  await app.page.screenshot({ path: testInfo.outputPath("excluded-strategies.png") });
  await dialog.getByRole("button", { name: "Done" }).click();
  await expect(app.page.locator("#search-advanced input[type=color]")).toHaveCount(0);
  await app.page.keyboard.press("Control+Comma");
  await app.page.getByText("Colors & theme", { exact: true }).locator("..").getByRole("button").click();
  await app.page.getByLabel("Search exclusion indicator").fill("#cc44aa");
  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Escape");
  await expect(advanced).toHaveCSS("border-top-color", "rgb(204, 68, 170)");
  await app.page.screenshot({ path: testInfo.outputPath("advanced-search.png") });
  await advanced.click();
  await expect(app.page.locator("#search-advanced")).toHaveCount(0);
  await expect(advanced).toHaveClass(/exclusions-active/);
  const userState = () => JSON.parse(fs.readFileSync(path.join(app.notebookDir, "notebook.user.json"), "utf8"));
  await expect.poll(() => userState().searchPreferences).toMatchObject({
    excludedStrategies: ["partial"],
  });
  expect(userState().searchPreferences).not.toHaveProperty("indicatorColor");
  expect(userState().searchPreferences.excludedSectionIds).toHaveLength(1);
  expect(app.readNotebookJson()).not.toHaveProperty("searchPreferences");

  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Control+PageUp");
  await expect(app.sectionName).toContainText("Notes");
  await app.relaunch();
  await app.search("milk");
  const reopenedAdvanced = app.page.getByRole("button", { name: "Advanced (2)", exact: true });
  await expect(reopenedAdvanced).toHaveClass(/exclusions-active/);
  await expect(reopenedAdvanced).toHaveCSS("border-top-color", "rgb(204, 68, 170)");
  await expect(app.page.locator(".results .hit")).toHaveCount(0);
  await app.page.locator(".search-bar .mode", { hasText: "regex" }).click();
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await expect(app.page.locator(".results .hit")).toContainText("Included");
  await reopenedAdvanced.click();
  await app.page.getByRole("button", { name: "Clear exclusions", exact: true }).click();
  await expect(app.page.locator(".results .hit")).toHaveCount(2);
  await expect(app.page.getByRole("button", { name: "Advanced", exact: true })).not.toHaveClass(/exclusions-active/);
});

test("advanced search is keyboard accessible without changing the search Tab ring", async ({ app }, testInfo) => {
  await app.newPageWithBody("Note", "milk", 1);
  await app.search("milk");
  await expect(app.page.locator(".results")).toHaveClass(/focused/);
  await app.page.keyboard.press("a");
  const sections = app.page.getByRole("button", { name: "Excluded sections (0)" });
  await expect(sections).toBeFocused();
  await app.page.keyboard.press("Enter");
  const dialog = app.page.getByRole("dialog");
  const filter = dialog.getByRole("textbox", { name: "Filter sections" });
  await expect(filter).toBeFocused();
  await app.page.keyboard.press("Shift+Tab");
  await expect(dialog.getByRole("button", { name: "Done" })).toBeFocused();
  await app.page.keyboard.press("Tab");
  await expect(filter).toBeFocused();
  await app.page.keyboard.press("Tab");
  await app.page.keyboard.press("Space");
  await expect(app.page.locator(".results .hit")).toHaveCount(0);
  await expect(dialog.getByRole("checkbox", { name: "Notes", exact: true })).toBeFocused();
  await app.page.screenshot({ path: testInfo.outputPath("excluded-sections-keyboard.png") });
  await app.page.keyboard.press("Escape");
  await expect(sections).toHaveCount(0);
  await expect(app.page.getByRole("button", { name: "Excluded sections (1)" })).toBeFocused();
  await app.page.keyboard.press("Control+k");
  await expect(app.page.locator(".search-bar input")).toBeFocused();
  await app.page.keyboard.press("Tab");
  await expect(app.page.locator(".results")).toHaveClass(/focused/);
});

test("fuzzy and regex search open the right page", async ({ app }) => {
  await app.newPageWithBody("Grocery", "buy milk and eggs", 1);
  await app.newPageWithBody("Meeting", "quarterly roadmap review", 2);

  // fuzzy: only Grocery's body contains a "milk" subsequence
  await app.search("milk");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await expect(app.page.locator(".results .hit .meta")).toContainText("Grocery");
  await expect(app.page.locator(".results .hit .snippet mark")).toContainText("milk");

  // Enter on the focused result opens the page in preview, query prehighlighted
  await app.page.keyboard.press("Enter");
  await expect(app.titleInput).toHaveValue("Grocery");
  await expect(app.page.locator(".editor-wrap")).toHaveClass(/focused/);
  await expect(app.page.locator(".preview")).toBeVisible();
  await expect(app.page.locator(".preview mark.find-hit")).toHaveText("milk");

  // regex mode
  const search = app.page.locator(".search-bar input");
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
  await expect(app.titleInput).toHaveValue("Grocery");
});

test("the peek renders the selected result and follows the selection", async ({ app }) => {
  await app.newPageWithBody("Groceries", "buy milk milk milk and eggs", 1);
  await app.newPageWithBody("Dairy", "milk comes from cows", 2);

  await app.search("milk");
  await expect(app.page.locator(".results .hit")).toHaveCount(2);

  // more occurrences ranks Groceries first, so it is what the peek opens on
  const peek = app.page.locator(".peek");
  await expect(peek.locator(".peek-head")).toContainText("Groceries");
  await expect(peek.locator("#peek-scroll")).toContainText("buy milk milk milk and eggs");
  await expect(peek.locator("mark.find-hit")).toHaveCount(3);
  await expect(peek.locator("mark.find-hit.current")).toHaveCount(1);

  // N steps matches inside the peeked page without changing the selection
  await app.page.keyboard.press("n");
  await expect(peek.locator("mark.find-hit").nth(1)).toHaveClass(/current/);
  await expect(peek.locator(".peek-head")).toContainText("Groceries");

  await app.page.keyboard.press("ArrowDown");
  await expect(peek.locator(".peek-head")).toContainText("Dairy");
  await expect(peek.locator("#peek-scroll")).toContainText("milk comes from cows");

  // opening the result leaves the peek behind for the real editor
  await app.page.keyboard.press("Enter");
  await expect(peek).toHaveCount(0);
  await expect(app.titleInput).toHaveValue("Dairy");
});

test("PgDn scrolls the peek, not the results list", async ({ app }) => {
  await app.newPageWithBody("Long", `needle\n${fillerBody(200)}`, 1);

  await app.search("needle");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);

  const scrollTop = () => app.scrollTopOf("#peek-scroll");
  await expect(app.page.locator(".peek mark.find-hit")).toHaveCount(1);
  const before = await scrollTop();
  await app.page.keyboard.press("PageDown");
  await expect.poll(scrollTop).toBeGreaterThan(before);
});

test("refining the query updates the peek's highlights without reloading the page", async ({ app }) => {
  await app.newPageWithBody("Note", "milk chocolate and milkshake", 1);

  await app.search("milk");
  const peek = app.page.locator(".peek");
  await expect(peek.locator(".peek-head")).toContainText("Note");
  await expect(peek.locator("mark.find-hit")).toHaveCount(2);

  // same single page stays the top (and only) hit, so the peek's cached body
  // is reused; only the highlight terms should change
  await app.search("milkshake");
  await expect(peek.locator(".peek-head")).toContainText("Note");
  await expect(peek.locator("mark.find-hit")).toHaveCount(1);
  await expect(peek.locator("mark.find-hit").first()).toHaveText("milkshake");
});

test("keywords rank by how many matched; quotes match the phrase whole", async ({ app }) => {
  await app.newPageWithBody("Loud", "alpha alpha alpha", 1);
  await app.newPageWithBody("Quiet", "alpha and beta", 2);

  await app.search("alpha beta");
  const meta = app.page.locator(".results .hit .meta");
  await expect(meta).toHaveCount(2);
  await expect(meta.first()).toContainText("Quiet");

  await app.search('"alpha and beta"');
  await expect(meta).toHaveCount(1);
  await expect(meta).toContainText("Quiet");
});
