import { App, expect, test } from "../app";

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

test("mouse back and forward buttons revisit opened pages", async ({ app }) => {
  await app.newTitledPage("Alpha", 1);
  await app.newTitledPage("Beta", 2);
  await app.newTitledPage("Gamma", 3);

  expect(await pressNavigationButton(app, 3)).toBe(true);
  await expect(app.titleInput).toHaveValue("Beta");
  expect(await pressNavigationButton(app, 3)).toBe(true);
  await expect(app.titleInput).toHaveValue("Alpha");
  expect(await pressNavigationButton(app, 4)).toBe(true);
  await expect(app.titleInput).toHaveValue("Beta");
});
