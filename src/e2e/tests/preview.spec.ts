import { expect, test, type App } from "../app";

const PREVIEW = "#preview-scroll";

/** Blank-line runs render as paragraphs holding a single nbsp. */
const spacerCount = (app: App) =>
  app.page
    .locator(`${PREVIEW} p`)
    .evaluateAll((ps) => ps.filter((p) => p.textContent === "\u00a0").length);

async function pageWithImage(app: App, title: string, ref: (id: string) => string) {
  await app.newTitledPage(title, 1);
  const [id] = await app.treeIds();
  app.writeAsset(id, "tiny.png");
  await app.setBody(ref(id));
  await app.page.keyboard.press("Control+e");
  return id;
}

test("preview shows the body without a duplicate title H1", async ({ app }) => {
  await app.newPageWithBody("Alpha", "hello body only", 1);
  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator(PREVIEW)).toContainText("hello body only");
  await expect(app.page.locator(`${PREVIEW} h1`)).toHaveCount(0);
  await expect(app.titleInput).toHaveValue("Alpha");
});

test("blank-line runs render as spacer paragraphs", async ({ app }) => {
  await app.newPageWithBody("Gaps", "one\n\n\n\ntwo", 1); // 3 blank lines -> 2 spacers
  await app.page.keyboard.press("Control+e");

  await expect(app.page.locator(`${PREVIEW} p`)).toHaveCount(4);
  await expect.poll(() => spacerCount(app)).toBe(2);
});

test("code fences keep blank lines verbatim and get highlighted", async ({ app }) => {
  await app.newPageWithBody("Code", "```js\nconst x = 1;\n\n\nreturn x;\n```", 1);
  await app.page.keyboard.press("Control+e");

  const code = app.page.locator(`${PREVIEW} pre code`);
  await expect(code).toHaveCount(1);
  await expect
    .poll(() => code.evaluate((el) => el.textContent))
    .toContain("const x = 1;\n\n\nreturn x;");
  await expect(code.locator(".hljs-keyword").first()).toBeVisible(); // const/return
  // and no spacer paragraphs leaked out of the fence
  await expect.poll(() => spacerCount(app)).toBe(0);
});

test("images are served through the note-asset scheme", async ({ app }) => {
  const id = await pageWithImage(app, "Pics", (pageId) => `![shot](assets/${pageId}/tiny.png)`);

  const img = app.page.locator(`${PREVIEW} img`);
  await expect(img).toHaveAttribute("src", `http://note-asset.localhost/${id}/tiny.png`);
  // naturalWidth > 0 proves the Rust protocol handler served real bytes past CSP
  await expect
    .poll(() => img.evaluate((el) => (el as HTMLImageElement).naturalWidth))
    .toBeGreaterThan(0);
});

test("{width=N} sizes the image and is stripped from the text", async ({ app }) => {
  await pageWithImage(app, "Sized", (id) => `![shot](assets/${id}/tiny.png){width=120}`);

  const img = app.page.locator(`${PREVIEW} img`);
  await expect(img).toHaveAttribute("width", "120");
  await expect(img).toHaveAttribute("alt", "shot");
  await expect.poll(() => img.evaluate((el) => el.clientWidth)).toBe(120);
  await expect(app.page.locator(PREVIEW)).not.toContainText("{width=120}");
});

test("[text]{.name} renders a colored span, non-palette forms stay literal", async ({ app }) => {
  await app.newPageWithBody("Colors", "[warn **hard**]{.red} and [nope]{.notacolor}", 1);
  await app.page.keyboard.press("Control+e");

  const span = app.page.locator(`${PREVIEW} p span`);
  await expect(span).toHaveCount(1);
  await expect
    .poll(() => span.evaluate((el) => getComputedStyle(el).color))
    .toBe("rgb(224, 108, 117)"); // #e06c75
  await expect(span.locator("strong")).toHaveText("hard");
  await expect(app.page.locator(PREVIEW)).toContainText("[nope]{.notacolor}");
});
