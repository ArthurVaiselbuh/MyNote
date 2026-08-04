import { expect, test } from "../app";

const NEW_PAGE = "New page";

function rowFor(app: { page: import("@playwright/test").Page }, desc: string) {
  return app.page.locator(".keybind-row", { hasText: desc }).first();
}

async function openKeybindings(app: { page: import("@playwright/test").Page }) {
  await app.page.keyboard.press("Control+,");
  await app.page.locator(".settings-row", { hasText: "Keyboard shortcuts" }).getByRole("button").click();
  await expect(app.page.locator(".keybind-list")).toBeVisible();
}

test("rebinding a shortcut retires the old chord and reaches every label", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await openKeybindings(app);

  const row = rowFor(app, NEW_PAGE);
  await expect(row.locator("kbd")).toHaveText("Ctrl+N");
  await row.getByRole("button", { name: "Change…" }).click();
  await expect(row).toHaveClass(/capturing/);
  await app.page.keyboard.press("Control+Shift+P");
  await expect(row.locator("kbd")).toHaveText("Ctrl+Shift+P");

  await app.page.keyboard.press("Escape"); // back to settings
  await app.page.keyboard.press("Escape");

  // the old chord is gone, the new one works
  await app.page.keyboard.press("Control+n");
  await expect(app.page.locator(".tree .row")).toHaveCount(1);
  await app.page.keyboard.press("Control+Shift+P");
  await expect(app.page.locator(".tree .row")).toHaveCount(2);

  // and the help overlay reads from the same table — but the new page's title
  // focus lands on a rAF, so leave it before pressing a non-modifier chord
  await expect(app.page.locator(".title-input")).toBeFocused();
  await app.page.keyboard.press("Escape");
  await expect(app.page.locator(".title-input")).not.toBeFocused();
  await app.page.keyboard.press("Shift+Slash");
  await expect(app.page.locator(".help-row", { hasText: NEW_PAGE }).locator("kbd")).toHaveText(
    "Ctrl+Shift+P",
  );
  await app.page.keyboard.press("Escape");
});

test("a shortcut can be unassigned, and reset all restores the defaults", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await openKeybindings(app);

  const row = rowFor(app, NEW_PAGE);
  await row.getByRole("button", { name: "Clear" }).click();
  await expect(row.locator(".keybind-unassigned")).toBeVisible();
  await expect(row.locator("kbd")).toHaveCount(0);

  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Control+n");
  await expect(app.page.locator(".tree .row")).toHaveCount(1); // nothing happened

  await openKeybindings(app);
  await app.page.getByRole("button", { name: "Reset all to defaults" }).click();
  await expect(rowFor(app, NEW_PAGE).locator("kbd")).toHaveText("Ctrl+N");

  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Escape");
  await app.page.keyboard.press("Control+n");
  await expect(app.page.locator(".tree .row")).toHaveCount(2);
});

test("the system-wide shortcut starts unassigned and refuses a bare chord", async ({ app }) => {
  const FRONT = "Bring MyNote to the front";
  await openKeybindings(app);

  const row = rowFor(app, FRONT);
  await expect(row.locator(".keybind-unassigned")).toBeVisible();

  await row.getByRole("button", { name: "Change…" }).click();
  await app.page.keyboard.press("b");
  await expect(app.page.locator(".keybind-note")).toContainText("Ctrl or Alt");
  await expect(row).toHaveClass(/capturing/); // still waiting for a usable chord

  await app.page.keyboard.press("Control+Alt+F9");
  await expect(row.locator("kbd")).toHaveText("Ctrl+Alt+F9");

  await app.relaunch();
  await openKeybindings(app);
  await expect(rowFor(app, FRONT).locator("kbd")).toHaveText("Ctrl+Alt+F9");
});

test("claiming a taken chord moves it off the command that held it", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await openKeybindings(app);

  const save = rowFor(app, "Save");
  await save.getByRole("button", { name: "Change…" }).click();
  await app.page.keyboard.press("Control+E"); // Edit / Preview owns this

  await expect(save.locator("kbd")).toHaveText("Ctrl+E");
  await expect(rowFor(app, "Edit / Preview").locator(".keybind-unassigned")).toBeVisible();
});
