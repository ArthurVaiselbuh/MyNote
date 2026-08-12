import { expect, fillerBody, hasGit, NO_GIT_REASON, test, type App } from "../app";

test.skip(!hasGit(), NO_GIT_REASON);

// git-backed reads are slower than the suite's default expect budget
const RAIL_READY = { timeout: 10_000 };

const rail = (app: App) => app.page.locator(".history-rail .picker-item");
const historyLayout = (app: App) => app.page.locator(".history-layout");

/** One page with `v1` committed by a close snapshot and `v2` live on disk.
 * `fromInput` drives real keystrokes for the body instead — only needed when
 * the test itself asserts on CodeMirror's undo history (see setBodyFromInput). */
async function pageWithTwoRevisions(
  app: App,
  title: string,
  v1: string,
  v2: string,
  fromInput = false,
) {
  await app.enableGitSnapshots();
  if (fromInput) {
    await app.newPageWithBodyFromInput(title, v1, 1);
    await app.relaunch();
    await app.setBodyFromInput(v2);
  } else {
    await app.newPageWithBody(title, v1, 1);
    await app.relaunch();
    await app.setBody(v2);
  }
}

/** Deletes the selected page and relaunches twice, so the deletion is both
 * purged from disk and close-committed into history. */
async function deleteSelectedAndCommit(app: App) {
  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toEqual([]);
  await app.relaunch();
}

test("page history: diff shows the whole file, restore is undoable", async ({ app }) => {
  await pageWithTwoRevisions(app, "Doc", "v1 line\nkeep me", "v2 line\nkeep me", true);

  await app.page.keyboard.press("Control+h");
  await expect(historyLayout(app)).toBeVisible();
  await expect(rail(app)).toHaveCount(2, RAIL_READY);

  // the base defaults to what's on disk, with the newest snapshot pre-selected
  await expect(rail(app).nth(0)).toHaveClass(/\bbase\b/);
  await expect(rail(app).nth(1)).toHaveClass(/\bselected\b/);
  // commit shas are noise in the rail
  await expect(rail(app).nth(1)).not.toContainText(/[0-9a-f]{7}/);

  // whole-file coverage: the unchanged line survives the diff untouched
  const rows = app.page.locator(".diff-row");
  await expect(rows.filter({ hasText: "keep me" })).toHaveCount(1);
  await expect(rows.filter({ hasText: "keep me" }).locator(".tint-del, .tint-add")).toHaveCount(0);
  // left = now on disk, right = the revision: the diff reads as "what restoring does"
  await expect(rows.filter({ hasText: "v2 line" }).locator(".tint-del")).toHaveCount(1);
  await expect(rows.filter({ hasText: "v1 line" }).locator(".tint-add")).toHaveCount(1);

  // ? opens the cheat sheet narrowed to history keys, and Esc comes back here
  await expect(app.page.locator(".history-layout .hint")).toHaveCount(0);
  await app.openHelp();
  const groups = app.page.locator(".help-group");
  await expect(groups).toHaveCount(1);
  await expect(groups.locator("h3")).toHaveText("History");
  await app.page.keyboard.press("Escape");
  await expect(historyLayout(app)).toBeVisible();
  await expect(app.modalBackdrop).toHaveCount(0);
  await expect(rail(app).nth(1)).toHaveClass(/\bselected\b/);

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

  await expect(historyLayout(app)).toHaveCount(0);
  await expect(app.editorBody).toContainText("v1 line");
  await expect(app.editorBody).not.toContainText("v2 line");

  // it went through a normal editor transaction, so Ctrl+Z undoes it
  await expect(app.editorBody).toBeFocused();
  await app.page.keyboard.press("Control+z");
  await expect(app.editorBody).toContainText("v2 line");
});

test("PgUp / PgDn page the diff, which the modal would otherwise swallow", async ({ app }) => {
  const filler = fillerBody(80);
  await pageWithTwoRevisions(app, "Long", `v1 line\n${filler}`, `v2 line\n${filler}`);

  await app.page.keyboard.press("Control+h");
  await expect(rail(app)).toHaveCount(2, RAIL_READY);
  await expect(app.page.locator(".diff.split")).toBeVisible();

  const scrollTop = () => app.scrollTopOf(".history-content");
  expect(await scrollTop()).toBe(0);
  await app.page.keyboard.press("PageDown");
  await expect.poll(scrollTop).toBeGreaterThan(0);
  await app.page.keyboard.press("PageUp");
  await expect.poll(scrollTop).toBe(0);

  // the keys are on the history cheat sheet, not just the app-wide one
  await app.openHelp();
  await expect(
    app.page.locator(".help-group .help-row", { hasText: "Scroll active view down" }),
  ).toHaveCount(1);
});

test("page history from results view: restore lands in a live editor", async ({ app }) => {
  await pageWithTwoRevisions(app, "Doc", "v1 line\nkeep me", "v2 line\nkeep me");

  // enter the results view — the Editor unmounts here
  await app.search("keep");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await expect(app.editorBody).toHaveCount(0);

  // Ctrl+H steps back to the page view first, so the pane opens over a
  // mounted Editor instead of the results list
  await app.page.keyboard.press("Control+h");
  await expect(historyLayout(app)).toBeVisible();
  await expect(app.page.locator(".results")).toHaveCount(0);
  await expect(app.editorBody).toHaveCount(1);
  await expect(app.editorBody).toContainText("keep me");
  await expect(rail(app)).toHaveCount(2, RAIL_READY);

  // restoring the pre-selected snapshot must actually change the buffer
  await expect(app.page.locator(".history-restore")).toBeEnabled();
  await app.page.keyboard.press("Enter");
  await app.confirmDanger();

  await expect(historyLayout(app)).toHaveCount(0);
  await expect(app.editorBody).toContainText("v1 line");
  await expect(app.editorBody).not.toContainText("v2 line");
});

test("deleted pages: a purged page is listed and recoverable", async ({ app }) => {
  await app.enableGitSnapshots();
  await app.newPageWithBody("ToDelete", "keep this text", 1);
  await app.relaunch(); // commits the page
  await deleteSelectedAndCommit(app);

  await app.page.keyboard.press("Control+Shift+h");
  await expect(historyLayout(app)).toBeVisible();
  await expect(rail(app)).toContainText(["ToDelete"]);

  await app.page.keyboard.press("Enter");
  await app.confirmDanger();

  await expect(historyLayout(app)).toHaveCount(0);
  await expect.poll(() => app.treeIds()).toHaveLength(1);
  const [id] = await app.treeIds();
  expect(app.mdExists(id)).toBe(true);
  await expect(app.rowTitles).toContainText("ToDelete");
});

test("a recovered page's own history stays browsable across its delete gap", async ({ app }) => {
  await app.enableGitSnapshots();
  await app.newPageWithBody("Doc", "v1 line", 1);
  await app.relaunch(); // commits the page
  await deleteSelectedAndCommit(app);

  await app.page.keyboard.press("Control+Shift+h");
  await expect(historyLayout(app)).toBeVisible();
  await expect(rail(app)).toContainText(["Doc"]);
  await app.page.keyboard.press("Enter");
  await app.confirmDanger();
  await expect(historyLayout(app)).toHaveCount(0);
  await expect.poll(() => app.treeIds()).toHaveLength(1);

  await app.relaunch(); // close-commits the recovery, so it has its own revision

  await app.page.keyboard.press("Control+h");
  await expect(historyLayout(app)).toBeVisible();
  // now (on disk), recreate, delete, create — starts selected on "recreate"
  await expect(rail(app)).toHaveCount(4, RAIL_READY);

  // walking every revision must never collapse the rail down to an error —
  // one of them (the delete commit) has no file, which is expected, not a
  // failure of the whole pane
  for (let i = 0; i < 3; i++) {
    await app.page.keyboard.press("ArrowDown");
    await expect(rail(app)).toHaveCount(4);
  }
  await expect(app.page.locator(".history-rail .history-note")).toHaveCount(0);

  // step back onto the delete commit specifically: it reads as an absence,
  // not a hard error, and Restore is disabled while it's selected
  await app.page.keyboard.press("ArrowUp");
  await expect(
    app.page.locator(".history-pane .history-note", { hasText: "didn't exist at this revision" }),
  ).toBeVisible();
  await expect(app.page.locator(".history-restore")).toBeDisabled();
});
