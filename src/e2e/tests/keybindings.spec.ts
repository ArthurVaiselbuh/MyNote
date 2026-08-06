import { expect, test } from "../app";

const NEW_PAGE = "New page";
const BRING_TO_FRONT = "Bring MyNote to the front";

test("rebinding a shortcut retires the old chord and reaches every label", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.openKeybindings();

  const row = app.keybindRow(NEW_PAGE);
  await expect(row.locator("kbd")).toHaveText("Ctrl+N");
  await row.getByRole("button", { name: "Change…" }).click();
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
  await expect(app.page.locator(".help-row", { hasText: NEW_PAGE }).locator("kbd")).toHaveText(
    "Ctrl+Shift+P",
  );
  await app.page.keyboard.press("Escape");
});

test("a shortcut can be unassigned, and reset all restores the defaults", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.openKeybindings();

  const row = app.keybindRow(NEW_PAGE);
  await row.getByRole("button", { name: "Clear" }).click();
  await expect(row.locator(".keybind-unassigned")).toBeVisible();
  await expect(row.locator("kbd")).toHaveCount(0);

  await app.closeSettings();
  await app.page.keyboard.press("Control+n");
  await expect(app.rows).toHaveCount(1); // nothing happened

  await app.openKeybindings();
  await app.page.getByRole("button", { name: "Reset all to defaults" }).click();
  await expect(app.keybindRow(NEW_PAGE).locator("kbd")).toHaveText("Ctrl+N");

  await app.closeSettings();
  await app.page.keyboard.press("Control+n");
  await expect(app.rows).toHaveCount(2);
});

test("the system-wide shortcut starts unassigned and refuses a bare chord", async ({ app }) => {
  await app.openKeybindings();

  const row = app.keybindRow(BRING_TO_FRONT);
  await expect(row.locator(".keybind-unassigned")).toBeVisible();

  await row.getByRole("button", { name: "Change…" }).click();
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

  await app.rebind("Save", "Control+E", "Ctrl+E"); // Edit / Preview owns this chord
  await expect(app.keybindRow("Edit / Preview").locator(".keybind-unassigned")).toBeVisible();
});
