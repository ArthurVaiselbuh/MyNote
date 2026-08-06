import type { Page } from "@playwright/test";
import fs from "node:fs";
import { expect, test } from "../app";

// headings, so every body line is its own block in the preview with no blank
// lines between them — editor line and preview block line up one to one
const LONG_BODY = Array.from({ length: 80 }, (_, i) => `## line ${i} of the long page`).join("\n");

const PREVIEW = "#preview-scroll";
const EDITOR = ".cm-scroller";

const scrollTopOf = (page: Page, scroller: string) =>
  page.locator(scroller).evaluate((el) => el.scrollTop);

const scrollTo = (page: Page, scroller: string, top: number) =>
  page.locator(scroller).evaluate((el, to) => (el.scrollTop = to), top);

/** Text of the first block still visible at the top edge of a scroller. */
const topVisibleText = (page: Page, scroller: string, block: string) =>
  page.locator(scroller).evaluate(
    (el, sel) =>
      [...el.querySelectorAll<HTMLElement>(sel)]
        .find((node) => node.getBoundingClientRect().bottom > el.getBoundingClientRect().top)
        ?.textContent?.trim() ?? "",
    block,
  );

async function openPage(page: Page, title: string) {
  await page.locator(".tree .row", { hasText: title }).click();
  await expect(page.locator(".title-input")).toHaveValue(title);
}

test("preview scroll position is remembered per page", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.newPageWithBody("Short", "just one line", 2);

  await openPage(app.page, "Long");
  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(`${PREVIEW} h2`)).toHaveCount(80);
  await scrollTo(app.page, PREVIEW, 400);
  const parked = await scrollTopOf(app.page, PREVIEW);
  expect(parked).toBeGreaterThan(0);

  await openPage(app.page, "Short");
  await expect.poll(() => scrollTopOf(app.page, PREVIEW)).toBe(0);

  await openPage(app.page, "Long");
  await expect.poll(() => scrollTopOf(app.page, PREVIEW)).toBeCloseTo(parked, 0);
});

test("editor scroll position is remembered per page", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await app.newPageWithBody("Short", "just one line", 2);

  await openPage(app.page, "Long");
  await scrollTo(app.page, EDITOR, 350);
  const parked = await scrollTopOf(app.page, EDITOR);
  expect(parked).toBeGreaterThan(0);

  await openPage(app.page, "Short");
  await openPage(app.page, "Long");
  await expect.poll(() => scrollTopOf(app.page, EDITOR)).toBeCloseTo(parked, 0);
});

test("view positions live in notebook.user.json and survive a restart", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await openPage(app.page, "Long");
  await scrollTo(app.page, EDITOR, 300);
  const parked = await scrollTopOf(app.page, EDITOR);
  await app.page.keyboard.press("Control+s"); // flushes the debounced write

  await expect
    .poll(() => JSON.parse(fs.readFileSync(app.userStatePath, "utf8")).viewPositions ?? {})
    .toHaveProperty([(await app.treeIds())[0], "editorScrollTop"], parked);
  expect(JSON.stringify(app.readNotebookJson())).not.toContain("viewPositions");

  await app.relaunch();
  await expect(app.page.locator(".title-input")).toHaveValue("Long");
  await expect.poll(() => scrollTopOf(app.page, EDITOR)).toBeCloseTo(parked, 0);
});

test("switching to the editor lands the caret on the preview's find match", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await openPage(app.page, "Long");
  await app.page.keyboard.press("Control+e");

  await app.page.keyboard.press("Control+f");
  await app.page.keyboard.type("line 42");
  await expect(app.page.locator(".preview-find .count")).toHaveText("1/1");
  await expect(app.page.locator(`${PREVIEW} mark.find-hit.current`)).toHaveText("line 42");

  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(".cm-content")).toBeFocused();
  await expect
    .poll(() =>
      app.page.locator(".cm-editor").evaluate((root) => {
        const caret = root.querySelector(".cm-cursor-primary");
        if (!caret) return "";
        const y = caret.getBoundingClientRect().top + 1;
        return (
          [...root.querySelectorAll<HTMLElement>(".cm-line")].find((line) => {
            const box = line.getBoundingClientRect();
            return box.top <= y && box.bottom >= y;
          })?.textContent ?? ""
        );
      }),
    )
    .toBe("## line 42 of the long page");
});

test("switching to preview keeps the line the editor was showing", async ({ app }) => {
  await app.newPageWithBody("Long", LONG_BODY, 1);
  await openPage(app.page, "Long");
  await scrollTo(app.page, EDITOR, 400);
  const topInEditor = await topVisibleText(app.page, EDITOR, ".cm-line");
  expect(topInEditor).toMatch(/^## line \d+/);

  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(`${PREVIEW} h2`)).toHaveCount(80);
  await expect
    .poll(() => topVisibleText(app.page, PREVIEW, "h2"))
    .toBe(topInEditor.replace("## ", ""));
});
