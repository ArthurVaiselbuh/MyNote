import fs from "node:fs";
import path from "node:path";
import { expect, test } from "../app";

const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);

test("preview shows the body without a duplicate title H1", async ({ app }) => {
  await app.newPageWithBody("Alpha", "hello body only", 1);
  await app.page.keyboard.press("Control+e");
  await expect(app.page.locator("#preview-scroll")).toContainText("hello body only");
  await expect(app.page.locator("#preview-scroll h1")).toHaveCount(0);
  await expect(app.page.locator(".title-input")).toHaveValue("Alpha");
});

test("blank-line runs render as spacer paragraphs", async ({ app }) => {
  await app.newTitledPage("Gaps", 1);
  await app.setBody("one\n\n\n\ntwo"); // 3 blank lines -> 2 spacers
  await app.page.keyboard.press("Control+e");

  await expect(app.page.locator("#preview-scroll p")).toHaveCount(4);
  await expect
    .poll(() =>
      app.page
        .locator("#preview-scroll p")
        .evaluateAll((ps) => ps.filter((p) => p.textContent === " ").length),
    )
    .toBe(2);
});

test("code fences keep blank lines verbatim and get highlighted", async ({ app }) => {
  await app.newTitledPage("Code", 1);
  await app.setBody("```js\nconst x = 1;\n\n\nreturn x;\n```");
  await app.page.keyboard.press("Control+e");

  const code = app.page.locator("#preview-scroll pre code");
  await expect(code).toHaveCount(1);
  await expect
    .poll(() => code.evaluate((el) => el.textContent))
    .toContain("const x = 1;\n\n\nreturn x;");
  await expect(code.locator(".hljs-keyword").first()).toBeVisible(); // const/return
  // and no spacer paragraphs leaked out of the fence
  await expect
    .poll(() =>
      app.page
        .locator("#preview-scroll p")
        .evaluateAll((ps) => ps.filter((p) => p.textContent === " ").length),
    )
    .toBe(0);
});

test("images are served through the note-asset scheme", async ({ app }) => {
  await app.newTitledPage("Pics", 1);
  const [id] = await app.treeIds();
  const dir = path.join(app.notebookDir, "assets", id);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "tiny.png"), TINY_PNG);

  await app.setBody(`![shot](assets/${id}/tiny.png)`);
  await app.page.keyboard.press("Control+e");

  const img = app.page.locator("#preview-scroll img");
  await expect(img).toHaveAttribute("src", `http://note-asset.localhost/${id}/tiny.png`);
  // naturalWidth > 0 proves the Rust protocol handler served real bytes past CSP
  await expect
    .poll(() => img.evaluate((el) => (el as HTMLImageElement).naturalWidth))
    .toBeGreaterThan(0);
});
