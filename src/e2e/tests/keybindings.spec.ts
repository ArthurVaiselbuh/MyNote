import { App, expect, test } from "../app";

// keybindRow/rebind/help-row locators key off these command ids (see keys/bindings.ts),
// not the desc text, so a copy tweak there can't silently break these tests
const NEW_PAGE = "page.new";
const BRING_TO_FRONT = "app.showWindow";
const PREVIOUS_PAGE = "page.back";
const NEXT_PAGE = "page.forward";

async function pressMouseButton(
  app: App,
  button: number,
  modifiers: Pick<MouseEventInit, "ctrlKey" | "altKey" | "shiftKey"> = {},
) {
  return app.page.evaluate(
    ({ mouseButton, init }) => {
      const event = new MouseEvent("mousedown", {
        button: mouseButton,
        bubbles: true,
        cancelable: true,
        ...init,
      });
      document.body.dispatchEvent(event);
      return event.defaultPrevented;
    },
    { mouseButton: button, init: modifiers },
  );
}

test("rebinding a shortcut retires the old chord and reaches every label", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.openKeybindings();

  const row = app.keybindRow(NEW_PAGE);
  await expect(row.locator("kbd")).toHaveText("Ctrl+N");
  await row.locator(".keybind-change").click();
  await expect(row).toHaveClass(/capturing/);
  await app.page.keyboard.press("Control+Shift+P");
  await expect(row.locator("kbd")).toHaveText("Ctrl+Shift+P");

  await app.closeSettings();

  // the old chord is gone, the new one works
  await app.page.keyboard.press("Control+n");
  await expect(app.rows).toHaveCount(1);
  await app.page.keyboard.press("Control+Shift+P");
  await expect(app.rows).toHaveCount(2);

  // and the help overlay reads from the same table — but the new page's title
  // focus lands on a rAF, so leave it before pressing a non-modifier chord
  await expect(app.titleInput).toBeFocused();
  await app.page.keyboard.press("Escape");
  await expect(app.titleInput).not.toBeFocused();
  await app.openHelp();
  await expect(app.page.locator(`.help-row[data-command="${NEW_PAGE}"]`).locator("kbd")).toHaveText(
    "Ctrl+Shift+P",
  );
  await app.page.keyboard.press("Escape");
});

test("a shortcut can be unassigned, and reset all restores the defaults", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.openKeybindings();

  const row = app.keybindRow(NEW_PAGE);
  await row.locator(".keybind-clear").click();
  await expect(row.locator(".keybind-unassigned")).toBeVisible();
  await expect(row.locator("kbd")).toHaveCount(0);

  await app.closeSettings();
  await app.page.keyboard.press("Control+n");
  await expect(app.rows).toHaveCount(1); // nothing happened

  await app.openKeybindings();
  await app.page.locator(".keybind-reset-all").click();
  await expect(app.keybindRow(NEW_PAGE).locator("kbd")).toHaveText("Ctrl+N");

  await app.closeSettings();
  await app.page.keyboard.press("Control+n");
  await expect(app.rows).toHaveCount(2);
});

test("the system-wide shortcut starts unassigned and refuses a bare chord", async ({ app }) => {
  await app.openKeybindings();

  const row = app.keybindRow(BRING_TO_FRONT);
  await expect(row.locator(".keybind-unassigned")).toBeVisible();

  await row.locator(".keybind-change").click();
  await app.page.keyboard.press("b");
  await expect(app.page.locator(".keybind-note")).toContainText("Ctrl or Alt");
  await expect(row).toHaveClass(/capturing/); // still waiting for a usable chord

  await app.page.keyboard.press("Control+Alt+F9");
  await expect(row.locator("kbd")).toHaveText("Ctrl+Alt+F9");

  await app.relaunch();
  await app.openKeybindings();
  await expect(app.keybindRow(BRING_TO_FRONT).locator("kbd")).toHaveText("Ctrl+Alt+F9");
});

test("claiming a taken chord moves it off the command that held it", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.openKeybindings();

  await app.rebind("app.save", "Control+E", "Ctrl+E"); // app.toggleMode owns this chord
  await expect(app.keybindRow("app.toggleMode").locator(".keybind-unassigned")).toBeVisible();
});

test("mouse buttons can be captured, dispatched, and persisted", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.newTitledPage("Beta", 2);
  await app.newTitledPage("Gamma", 3);
  await app.openKeybindings();

  const row = app.keybindRow(PREVIOUS_PAGE);
  await expect(row.locator("kbd")).toHaveText("Mouse Back");
  await row.locator(".keybind-change").click();
  expect(await pressMouseButton(app, 1)).toBe(true);
  await expect(row.locator("kbd")).toHaveText("Mouse Middle");
  await expect(app.titleInput).toHaveValue("Gamma");

  await app.closeSettings();
  expect(await pressMouseButton(app, 3)).toBe(true);
  await expect(app.titleInput).toHaveValue("Gamma");
  expect(await pressMouseButton(app, 1)).toBe(true);
  await expect(app.titleInput).toHaveValue("Beta");

  await app.relaunch();
  await app.openKeybindings();
  await expect(app.keybindRow(PREVIOUS_PAGE).locator("kbd")).toHaveText("Mouse Middle");
});

test("mouse capture rejects normal clicks and wheel scrolling", async ({ app }) => {
  await app.openKeybindings();

  const row = app.keybindRow(PREVIOUS_PAGE);
  await row.locator(".keybind-change").click();
  expect(await pressMouseButton(app, 0)).toBe(true);
  await expect(app.page.locator(".keybind-note")).toContainText("Primary and secondary");
  await expect(row).toHaveClass(/capturing/);

  expect(await pressMouseButton(app, 2)).toBe(true);
  await expect(row).toHaveClass(/capturing/);
  await app.page.evaluate(() =>
    document.body.dispatchEvent(new WheelEvent("wheel", { deltaY: 100, bubbles: true })),
  );
  await expect(app.page.locator(".keybind-note")).toContainText("Wheel scrolling");
  await expect(row).toHaveClass(/capturing/);

  expect(await pressMouseButton(app, 3)).toBe(true);
  await expect(row.locator("kbd")).toHaveText("Mouse Back");
});

test("claiming a mouse button moves it off its previous command", async ({ app }) => {
  await app.openKeybindings();

  const back = app.keybindRow(PREVIOUS_PAGE);
  await back.locator(".keybind-change").click();
  expect(await pressMouseButton(app, 4)).toBe(true);
  await expect(back.locator("kbd")).toHaveText("Mouse Forward");
  await expect(app.keybindRow(NEXT_PAGE).locator(".keybind-unassigned")).toBeVisible();
});
