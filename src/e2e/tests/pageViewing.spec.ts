import { expect, test, type App } from "../app";

async function pressNavigationButton(app: App, button: 3 | 4) {
  return app.page.evaluate((mouseButton) => {
    const event = new MouseEvent("mousedown", {
      button: mouseButton,
      bubbles: true,
      cancelable: true,
    });
    document.body.dispatchEvent(event);
    return event.defaultPrevented;
  }, button);
}

async function createCollapsedParentWithChild(app: App) {
  await app.newTitledPage("Parent", 1);
  await app.newTitledSubpage("Child", 2);
  await app.page.keyboard.press("ArrowLeft");
  await expect(app.selectedTitle).toHaveText("Parent");
  await app.page.keyboard.press("ArrowLeft");
  await expect(app.rowTitles).toHaveText(["Parent"]);
}

test("view history expands the ancestors of a hidden page", async ({ app }) => {
  await createCollapsedParentWithChild(app);
  await app.newTitledPage("Other", 2);

  expect(await pressNavigationButton(app, 3)).toBe(true);
  await expect(app.titleInput).toHaveValue("Parent");
  expect(await pressNavigationButton(app, 3)).toBe(true);
  await expect(app.titleInput).toHaveValue("Child");
  await expect(app.rowTitles).toHaveText(["Parent", "Child", "Other"]);
  await expect(app.selectedTitle).toHaveText("Child");
});

test("search uses the same visible page selection path", async ({ app }) => {
  await createCollapsedParentWithChild(app);

  await app.search("Child");
  await expect(app.page.locator(".results .hit")).toHaveCount(1);
  await app.page.keyboard.press("Enter");

  await expect(app.titleInput).toHaveValue("Child");
  await expect(app.rowTitles).toHaveText(["Parent", "Child"]);
  await expect(app.selectedTitle).toHaveText("Child");
});
