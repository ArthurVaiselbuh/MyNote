import { expect, hasGit, test } from "../app";

test.skip(!hasGit(), "git not found on PATH — history is inert without it");

test("page history: diff shows the whole file, restore is undoable", async ({ app }) => {
  await app.enableGitSnapshots();
  await app.newPageWithBody("Doc", "v1 line\nkeep me", 1);
  // close snapshot commits v1
  await app.relaunch();

  await app.setBody("v2 line\nkeep me");

  await app.page.keyboard.press("Control+h");
  await expect(app.page.locator(".history-layout")).toBeVisible();
  await expect(app.page.locator(".history-rail .picker-item")).toHaveCount(2, { timeout: 10_000 });

  // the base defaults to what's on disk, with the newest snapshot pre-selected
  const rail = app.page.locator(".history-rail .picker-item");
  await expect(rail.nth(0)).toHaveClass(/\bbase\b/);
  await expect(rail.nth(1)).toHaveClass(/\bselected\b/);
  // commit shas are noise in the rail
  await expect(rail.nth(1)).not.toContainText(/[0-9a-f]{7}/);

  // whole-file coverage: the unchanged line survives the diff untouched
  const rows = app.page.locator(".diff-row");
  await expect(rows.filter({ hasText: "keep me" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "keep me" }).locator(".tint-del, .tint-add")).toHaveCount(0);
  // left = now on disk, right = the revision: the diff reads as "what restoring does"
  await expect(rows.filter({ hasText: "v2 line" }).locator(".tint-del")).toHaveCount(1);
  await expect(rows.filter({ hasText: "v1 line" }).locator(".tint-add")).toHaveCount(1);

  // ? opens the cheat sheet narrowed to history keys, and Esc comes back here
  await expect(app.page.locator(".history-layout .hint")).toHaveCount(0);
  // the help chord is Shift+the base "/" key — press("?") alone sends no shiftKey
  await app.page.keyboard.press("Shift+Slash");
  const groups = app.page.locator(".help-group");
  await expect(groups).toHaveCount(1);
  await expect(groups.locator("h3")).toHaveText("History");
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".history-layout")).toBeVisible();
  await expect(app.page.locator(".modal-backdrop")).toHaveCount(0);
  await expect(app.page.locator(".history-rail .picker-item").nth(1)).toHaveClass(/\bselected\b/);

  // mode switching
  await app.page.keyboard.press("2"); // inline
  await expect(app.page.locator(".diff.inline")).toBeVisible();
  await app.page.keyboard.press("3"); // rendered
  await expect(app.page.locator(".history-rendered")).toBeVisible();
  await app.page.keyboard.press("1"); // back to split
  await expect(app.page.locator(".diff.split")).toBeVisible();

  // the pre-selected revision is restorable without moving the selection
  await expect(app.page.locator(".history-restore")).toBeEnabled();
  await app.page.keyboard.press("Enter");
  await app.confirmDanger();

  await expect(app.page.locator(".history-layout")).toHaveCount(0);
  const cm = app.page.locator(".cm-content");
  await expect(cm).toContainText("v1 line");
  await expect(cm).not.toContainText("v2 line");

  // it went through a normal editor transaction, so Ctrl+Z undoes it
  await expect(cm).toBeFocused();
  await app.page.keyboard.press("Control+z");
  await expect(cm).toContainText("v2 line");
});

test("PgUp / PgDn page the diff, which the modal would otherwise swallow", async ({ app }) => {
  await app.enableGitSnapshots();
  const long = Array.from({ length: 80 }, (_, i) => `line ${i} of filler`).join("\n");
  await app.newPageWithBody("Long", `v1 line\n${long}`, 1);
  // close snapshot commits v1
  await app.relaunch();

  await app.setBody(`v2 line\n${long}`);

  await app.page.keyboard.press("Control+h");
  await expect(app.page.locator(".history-rail .picker-item")).toHaveCount(2, { timeout: 10_000 });
  await expect(app.page.locator(".diff.split")).toBeVisible();

  const scrollTop = () => app.page.locator(".history-content").evaluate((el) => el.scrollTop);
  expect(await scrollTop()).toBe(0);
  await app.page.keyboard.press("PageDown");
  await expect.poll(scrollTop).toBeGreaterThan(0);
  await app.page.keyboard.press("PageUp");
  await expect.poll(scrollTop).toBe(0);

  // the keys are on the history cheat sheet, not just the app-wide one
  await app.page.keyboard.press("Shift+Slash");
  await expect(app.page.locator(".help-group .help-row", { hasText: "Scroll active view down" })).toHaveCount(1);
});

test("page history from results view: restore lands in a live editor", async ({ app }) => {
  await app.enableGitSnapshots();
  await app.newPageWithBody("Doc", "v1 line\nkeep me", 1);
  // close snapshot commits v1
  await app.relaunch();

  await app.setBody("v2 line\nkeep me");

  // enter the results view — the Editor unmounts here
  await app.page.keyboard.press("Control+k");
  await expect(app.page.locator(".search-bar input")).toBeFocused();
  await app.page.keyboard.type("keep");
  await app.page.keyboard.press("Enter");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await expect(app.page.locator(".cm-content")).toHaveCount(0);

  // Ctrl+H steps back to the page view first, so the pane opens over a
  // mounted Editor instead of the results list
  await app.page.keyboard.press("Control+h");
  await expect(app.page.locator(".history-layout")).toBeVisible();
  await expect(app.page.locator(".results")).toHaveCount(0);
  await expect(app.page.locator(".cm-content")).toHaveCount(1);
  await expect(app.page.locator(".cm-content")).toContainText("keep me");
  await expect(app.page.locator(".history-rail .picker-item")).toHaveCount(2, { timeout: 10_000 });

  // restoring the pre-selected snapshot must actually change the buffer
  await expect(app.page.locator(".history-restore")).toBeEnabled();
  await app.page.keyboard.press("Enter");
  await app.confirmDanger();

  await expect(app.page.locator(".history-layout")).toHaveCount(0);
  const cm = app.page.locator(".cm-content");
  await expect(cm).toContainText("v1 line");
  await expect(cm).not.toContainText("v2 line");
});

test("deleted pages: a purged page is listed and recoverable", async ({ app }) => {
  await app.enableGitSnapshots();
  await app.newPageWithBody("ToDelete", "keep this text", 1);
  await app.relaunch(); // commits the page

  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toEqual([]);

  await app.relaunch(); // purges the file + close-commits the deletion

  await app.page.keyboard.press("Control+Shift+h");
  await expect(app.page.locator(".history-layout")).toBeVisible();
  await expect(app.page.locator(".history-rail .picker-item")).toContainText(["ToDelete"]);

  await app.page.keyboard.press("Enter");
  await app.confirmDanger();

  await expect(app.page.locator(".history-layout")).toHaveCount(0);
  await expect.poll(() => app.treeIds()).toHaveLength(1);
  const [id] = await app.treeIds();
  expect(app.mdExists(id)).toBe(true);
  await expect(app.page.locator(".row .title")).toContainText("ToDelete");
});

test("a recovered page's own history stays browsable across its delete gap", async ({ app }) => {
  await app.enableGitSnapshots();
  await app.newPageWithBody("Doc", "v1 line", 1);
  await app.relaunch(); // commits the page

  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toEqual([]);
  await app.relaunch(); // purges the file + close-commits the deletion

  await app.page.keyboard.press("Control+Shift+h");
  await expect(app.page.locator(".history-layout")).toBeVisible();
  await expect(app.page.locator(".history-rail .picker-item")).toContainText(["Doc"]);
  await app.page.keyboard.press("Enter");
  await app.confirmDanger();
  await expect(app.page.locator(".history-layout")).toHaveCount(0);
  await expect.poll(() => app.treeIds()).toHaveLength(1);

  await app.relaunch(); // close-commits the recovery, so it has its own revision

  await app.page.keyboard.press("Control+h");
  await expect(app.page.locator(".history-layout")).toBeVisible();
  const rail = app.page.locator(".history-rail .picker-item");
  // now (on disk), recreate, delete, create — starts selected on "recreate"
  await expect(rail).toHaveCount(4, { timeout: 10_000 });

  // walking every revision must never collapse the rail down to an error —
  // one of them (the delete commit) has no file, which is expected, not a
  // failure of the whole pane
  for (let i = 0; i < 3; i++) {
    await app.page.keyboard.press("ArrowDown");
    await expect(rail).toHaveCount(4);
  }
  await expect(app.page.locator(".history-rail .history-note")).toHaveCount(0);

  // step back onto the delete commit specifically: it reads as an absence,
  // not a hard error, and Restore is disabled while it's selected
  await app.page.keyboard.press("ArrowUp");
  await expect(app.page.locator(".history-pane .history-note", { hasText: "didn't exist at this revision" })).toBeVisible();
  await expect(app.page.locator(".history-restore")).toBeDisabled();
});
