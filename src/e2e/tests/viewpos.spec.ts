import type { Page } from "@playwright/test";
import { expect, test } from "../app";

// headings, so every body line is its own block in the preview with no blank
// lines between them — editor line and preview block line up one to one
const LONG_BODY = Array.from({ length: 80 }, (_, i) => `## line ${i} of the long page`).join("\n");

const PREVIEW = "#preview-scroll";
const EDITOR = ".cm-scroller";

/** Text of the first block still visible at the top edge of a scroller. */
const topVisibleText = (page: Page, scroller: string, block: string) =>
  page.locator(scroller).evaluate(
    (el, sel) =>
      [...el.querySelectorAll<HTMLElement>(sel)]
        .find((node) => node.getBoundingClientRect().bottom > el.getBoundingClientRect().top)
        ?.textContent?.trim() ?? "",
    block,
  );

test("preview scroll position is remembered per page", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.newPageWithBody("Short", "just one line", 2);

  await app.openPage("Long");
  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(`${PREVIEW} h2`)).toHaveCount(80);
  await app.scrollTo(PREVIEW, 400);
  const parked = await app.scrollTopOf(PREVIEW);
  expect(parked).toBeGreaterThan(0);

  await app.openPage("Short");
  await expect.poll(() => app.scrollTopOf(PREVIEW)).toBe(0);

  await app.openPage("Long");
  await expect.poll(() => app.scrollTopOf(PREVIEW)).toBeCloseTo(parked, 0);
});

test("editor scroll position is remembered per page", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.newPageWithBody("Short", "just one line", 2);

  await app.openPage("Long");
  await app.scrollTo(EDITOR, 350);
  const parked = await app.scrollTopOf(EDITOR);
  expect(parked).toBeGreaterThan(0);

  await app.openPage("Short");
  await app.openPage("Long");
  await expect.poll(() => app.scrollTopOf(EDITOR)).toBeCloseTo(parked, 0);
});

test("view positions live in notebook.user.json and survive a restart", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.openPage("Long");
  await app.scrollTo(EDITOR, 300);
  const parked = await app.scrollTopOf(EDITOR);
  await app.page.keyboard.press("Control+s"); // flushes the debounced write

  const [id] = await app.treeIds();
  await expect
    .poll(() => app.readUserJson().viewPositions ?? {})
    .toHaveProperty([id, "editorScrollTop"], parked);
  expect(JSON.stringify(app.readNotebookJson())).not.toContain("viewPositions");

  await app.relaunch();
  await expect(app.titleInput).toHaveValue("Long");
  await expect.poll(() => app.scrollTopOf(EDITOR)).toBeCloseTo(parked, 0);
});

test("switching to the editor lands the caret on the preview's find match", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.openPage("Long");
  await app.page.keyboard.press("Control+e");

  await app.page.keyboard.press("Control+f");
  await app.page.keyboard.type("line 42");
  await expect(app.page.locator(".preview-find .count")).toHaveText("1/1");
  await expect(app.page.locator(`${PREVIEW} mark.find-hit.current`)).toHaveText("line 42");

  await app.page.keyboard.press("Control+e");
  await expect(app.editorBody).toBeFocused();
  await expect.poll(() => app.caretLineText()).toBe("## line 42 of the long page");
});

test("switching to preview keeps the line the editor was showing", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.openPage("Long");
  await app.scrollTo(EDITOR, 400);
  const topInEditor = await topVisibleText(app.page, EDITOR, ".cm-line");
  expect(topInEditor).toMatch(/^## line \d+/);

  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(`${PREVIEW} h2`)).toHaveCount(80);
  await expect
    .poll(() => topVisibleText(app.page, PREVIEW, "h2"))
    .toBe(topInEditor.replace("## ", ""));
});
