import type { Page } from "@playwright/test";
import { expect, test } from "../app";

// headings, so every body line is its own block in the preview with no blank
// lines between them — editor line and preview block line up one to one
const LONG_BODY = Array.from({ length: 80 }, (_, i) => `## line ${i} of the long page`).join("\n");

// each paragraph is 8 source lines (one editor block each) that CommonMark
// merges into a single, much taller preview block — the mismatch a raw pixel
// offset gets wrong when carried from one view's block height to the other's
const paragraph = (n: number) =>
  Array.from({ length: 8 }, (_, i) => `paragraph ${n} line ${i} of some reasonably long body text`).join(
    "\n",
  );
// 60, not 15 — the target paragraph needs plenty of room below it too, or
// there isn't enough scrollable range left for the browser to honor a deep
// scroll into it and the test would pass by accident (clamped to max scroll)
const PARA_BODY = Array.from({ length: 60 }, (_, n) => paragraph(n)).join("\n\n");

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

test("Control+E, Control+E round trip restores the exact caret spot, not just the top line", async ({
  app,
}) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.openPage("Long");
  await app.page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();

  // land the caret at the END of line 40 — scrolling this far down the
  // viewport no longer starts at line 40, so a fix that only restores the
  // top-of-viewport line (rather than the real caret) would mislocate it
  await app.page.keyboard.press("Control+Home");
  for (let i = 0; i < 40; i++) await app.page.keyboard.press("ArrowDown");
  await app.page.keyboard.press("End");
  await expect.poll(() => app.caretLineText()).toBe("## line 40 of the long page");

  await app.page.keyboard.press("Control+e");
  await app.page.keyboard.press("Control+e");
  await expect(app.editorBody).toBeFocused();
  await expect.poll(() => app.caretLineText()).toBe("## line 40 of the long page");

  await app.page.keyboard.type(" INSERTED");
  await app.page.keyboard.press("Control+s");

  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("## line 40 of the long page INSERTED");
});

test("Control+E round trip keeps the editor scroll in the same paragraph, not a different one", async ({
  app,
}) => {
  await app.newPageWithBody("Prose", PARA_BODY, 1);
  await app.openPage("Prose");
  // setBody() leaves the editor scrolled to the bottom (cursor lands there
  // after typing the whole body) — start from a known top-of-document anchor
  await app.scrollTo(EDITOR, 0);
  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(`${PREVIEW} p`)).toHaveCount(60);

  // scroll deep into paragraph 30's block — its rendered height in the preview
  // is many times one editor line, exactly where a raw-pixel offset (instead
  // of a fraction of the block's own height) sends the editor to a wildly
  // wrong line on the way back
  await app.page.locator(`${PREVIEW} p`, { hasText: "paragraph 30 line 0" }).evaluate((p, sel) => {
    const container = p.closest(sel)!;
    const into = p.getBoundingClientRect().top - container.getBoundingClientRect().top;
    container.scrollTop += into + p.getBoundingClientRect().height * 0.6;
  }, PREVIEW);

  await app.page.keyboard.press("Control+e");
  await expect.poll(() => topVisibleText(app.page, EDITOR, ".cm-line")).toMatch(/^paragraph 30 /);
});

test("Control+E round trip restores the exact scroll when the top line is mid-paragraph, not just its first line", async ({
  app,
}) => {
  await app.newPageWithBody("Prose", PARA_BODY, 1);
  await app.openPage("Prose");
  await app.page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();

  // put the caret on paragraph 30's 4th source line (not its first) and scroll
  // it to the viewport top — offsetRatio measured against a single CM line
  // (instead of the whole merged preview paragraph) sends the round trip to the
  // wrong line here. 9 lines/paragraph (8 + blank separator) × 30 + 3
  await app.page.keyboard.press("Control+Home");
  for (let i = 0; i < 9 * 30 + 3; i++) await app.page.keyboard.press("ArrowDown");
  await expect.poll(() => app.caretLineText()).toBe("paragraph 30 line 3 of some reasonably long body text");
  await app.page.evaluate((sel) => {
    const scroller = document.querySelector(sel)!;
    const caret = document.querySelector(".cm-cursor-primary")!;
    scroller.scrollTop += caret.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
  }, EDITOR);
  const parked = await app.scrollTopOf(EDITOR);
  expect(parked).toBeGreaterThan(0);

  await app.page.keyboard.press("Control+e");
  await app.page.keyboard.press("Control+e");
  await expect(app.editorBody).toBeFocused();
  // sub-pixel rounding across the round trip is fine — a wrong line or a wrong
  // paragraph, which is what this test guards against, is off by many pixels
  await expect.poll(() => app.scrollTopOf(EDITOR)).toBeCloseTo(parked, -1);
});
