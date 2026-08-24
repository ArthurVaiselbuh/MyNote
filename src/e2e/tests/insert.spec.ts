import { expect, test } from "../app";

test("Ctrl+J Color opens the picker and a swatch wraps the selection", async ({ app }) => {
  await app.newTitledPage("Tint", 1);

  // new pages carry a date-stamp body — replace it, then re-select the text
  await app.selectWholeBody();
  await app.page.keyboard.type("turn me cyan");
  await app.page.keyboard.press("Control+a");

  await app.page.keyboard.press("Control+j");
  await expect(app.modal.locator("input")).toBeFocused();
  await app.page.keyboard.type("color");
  await expect(app.page.locator(".insert-item")).toHaveCount(1);
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".color-grid")).toBeFocused();
  await app.page.keyboard.press("ArrowDown"); // red -> cyan (one grid row down)
  await app.page.keyboard.press("Enter");
  await expect(app.modal).toHaveCount(0);

  await app.page.keyboard.press("Control+s");
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("[turn me cyan]{.cyan}");

  await app.page.keyboard.press("Control+e");
  const span = app.page.locator("#preview-scroll p span");
  await expect(span).toHaveText("turn me cyan");
  await expect
    .poll(() => span.evaluate((el) => getComputedStyle(el).color))
    .toBe("rgb(86, 182, 194)"); // #56b6c2
});

test("color picker lists named colors + custom; Esc steps back to insert", async ({ app }) => {
  await app.newTitledPage("Palette", 1);
  await app.page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();

  await app.page.keyboard.press("Control+j");
  await expect(app.modal.locator("input")).toBeFocused();
  // the synonym still finds the single Color… entry
  await app.page.keyboard.type("colour");
  await expect(app.page.locator(".insert-item")).toHaveCount(1);
  await expect(app.page.locator(".insert-item .swatch")).toHaveCount(1);
  await app.page.keyboard.press("Enter");

  await expect(app.page.locator(".color-grid")).toBeFocused();
  await expect(app.page.locator(".color-cell")).toHaveCount(9); // 8 named + custom
  await expect(app.page.locator(".color-cell.custom input[type=color]")).toHaveCount(1);

  await app.page.keyboard.press("Escape"); // peels back to the insert helper
  await expect(app.page.locator(".color-grid")).toHaveCount(0);
  await expect(app.modal.locator("input")).toBeFocused();

  await app.page.keyboard.press("Escape");
  await expect(app.modal).toHaveCount(0);
});

test("Ctrl+J Code block size wraps the selection and shrinks its preview", async ({ app }) => {
  await app.newTitledPage("Compact", 1);
  await app.selectWholeBody();
  await app.page.keyboard.type("const compact = true;");
  await app.page.keyboard.press("Control+a");

  await app.page.keyboard.press("Control+j");
  await expect(app.modal.locator("input")).toBeFocused();
  await app.page.keyboard.type("code block size");
  await expect(app.page.locator(".insert-item")).toHaveCount(1);
  await app.page.keyboard.press("Enter");

  await app.page.keyboard.press("Control+s");
  const [id] = await app.treeIds();
  await expect.poll(() => app.readMd(id)).toContain("``` {size=12}\nconst compact = true;\n```");

  await app.page.keyboard.press("Control+e");
  const code = app.page.locator("#preview-scroll pre code");
  await expect.poll(() => code.evaluate((el) => getComputedStyle(el).fontSize)).toBe("12px");
});
