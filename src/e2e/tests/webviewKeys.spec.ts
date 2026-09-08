import { expect, test } from "../app";

test("select all stays within text controls and the focused preview", async ({ app }) => {
  await app.newPageWithBody("Selection", "Only note content", 1);
  await app.page.keyboard.press("Control+e");
  await app.page.locator("#preview-scroll").click();
  const selectedText = () => app.page.evaluate(() => window.getSelection()?.toString() ?? "");
  await app.page.keyboard.press("Control+a");
  await expect.poll(selectedText).toBe("Only note content");
  await app.page.keyboard.press("Escape");
  await expect.poll(selectedText).toBe("");
  await app.page.keyboard.press("Control+a");
  await expect.poll(selectedText).toBe("");
  await app.page.keyboard.press("Control+k");
  await app.page.keyboard.type("replace me");
  await app.page.keyboard.press("Control+a");
  await app.page.keyboard.type("replacement");
  await expect(app.page.locator("[data-search-box] input, input[data-search-box]")).toHaveValue("replacement");
  await app.search("content");
  await app.page.keyboard.press("Control+a");
  await expect.poll(selectedText).toBe("");
  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Control+j");
  await app.modal.locator(".modal-title").click();
  await app.page.keyboard.press("Control+a");
  await expect.poll(selectedText).toBe("");
});

test("reload shortcuts preserve the running document, including in dialogs", async ({ app }) => {
  await app.newPageWithBody("Reload", "Keep this buffer", 1);
  await app.page.evaluate(() => document.documentElement.dataset.reloadSentinel = "alive");
  for (const openDialog of [false, true]) {
    if (openDialog) await app.page.keyboard.press("Control+j");
    for (const chord of ["Control+r", "Control+Shift+r", "F5", "Control+F5", "Shift+F5"]) {
      await app.page.keyboard.press(chord);
    }
    await expect(app.page.locator("html")).toHaveAttribute("data-reload-sentinel", "alive");
  }
});

test("print uses the live page and can be reassigned", async ({ app }) => {
  await app.newPageWithBody("Printed title", "A **bold** paragraph\n\n- first\n- second", 1);
  await app.editorBody.click();
  await app.page.keyboard.press("Control+End");
  await app.page.keyboard.type(" unsaved");
  await app.page.evaluate(() => {
    window.print = () => { document.documentElement.dataset.printCalled = "yes"; };
  });
  await app.page.keyboard.press("Control+p");
  await expect(app.page.locator("html")).toHaveAttribute("data-print-called", "yes");
  await expect(app.page.locator(".print-note h1")).toHaveText("Printed title");
  await expect(app.page.locator(".print-note strong")).toHaveText("bold");
  await expect(app.page.locator(".print-note li")).toHaveCount(2);
  await expect(app.page.locator(".print-note li").last()).toHaveText("second unsaved");
  await app.page.emulateMedia({ media: "print" });
  await expect(app.page.locator(".app")).toBeHidden();
  await expect(app.page.locator(".print-note")).toBeVisible();
  await app.page.screenshot({ path: "test-results/print-layout.png" });
  await app.page.emulateMedia({ media: "screen" });
  await app.page.evaluate(() => {
    window.dispatchEvent(new Event("afterprint"));
    delete document.documentElement.dataset.printCalled;
  });
  await expect(app.page.locator(".print-note")).toHaveCount(0);
  await app.openKeybindings();
  await app.keybindRow("page.print").locator(".keybind-change").click();
  await app.page.keyboard.press("Control+Shift+p");
  await app.closeSettings();
  await app.page.keyboard.press("Control+p");
  await expect(app.page.locator(".print-note")).toHaveCount(0);
  await app.page.keyboard.press("Control+Shift+p");
  await expect(app.page.locator(".print-note")).toHaveCount(1);
  await app.page.evaluate(() => window.dispatchEvent(new Event("afterprint")));
});

test("browser fallbacks are prevented in dialogs without blocking app bindings", async ({ app }) => {
  await app.newTitledPage("Browser keys", 1);
  await app.page.keyboard.press("Control+j");
  const prevented = await app.page.evaluate(() => {
    return [
      { key: "p", ctrlKey: true }, { key: "s", ctrlKey: true },
      { key: "f", ctrlKey: true }, { key: "F3" },
      { key: "=", ctrlKey: true }, { key: "0", ctrlKey: true },
      { key: "ArrowLeft", altKey: true }, { key: "ArrowRight", altKey: true },
      { key: "F12" }, { key: "i", ctrlKey: true, shiftKey: true },
    ].map(init => {
      const event = new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true });
      document.activeElement!.dispatchEvent(event);
      return event.defaultPrevented;
    });
  });
  expect(prevented.every(Boolean)).toBe(true);
  await expect(app.page.locator(".print-note")).toHaveCount(0);
  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Control+f");
  await expect(app.page.locator(".cm-search")).toBeVisible();
});
