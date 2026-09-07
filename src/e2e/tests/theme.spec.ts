import { expect, test } from "../app";

test("theme switching preserves custom palettes across restart and resets only the active mode", async ({ app }) => {
  await app.newPageWithBody("Theme test", "Theme changes preserve this text.", 1);
  const page = app.page;
  await page.keyboard.press("Control+Comma");
  await page.getByText("Colors & theme", { exact: true }).locator("..").getByRole("button").click();
  await expect(page.locator("#set-theme")).toHaveValue("dark");
  await page.locator("#set-text").fill("#c0ffee");
  await page.locator("#set-theme").selectOption("light");
  await expect(page.locator(".app")).toHaveCSS("background-color", "rgb(250, 251, 252)");
  await expect(page.locator(".cm-editor")).toHaveCSS("color-scheme", "light");
  await expect(page.locator(".cm-content")).toContainText("Theme changes preserve this text.");
  await page.screenshot({ path: "test-results/light-mode.png" });
  await page.locator("#set-text").fill("#123456");
  await page.locator("#set-theme").selectOption("dark");
  await expect(page.locator(".app")).toHaveCSS("color", "rgb(192, 255, 238)");
  await page.locator("#set-theme").selectOption("light");
  await expect(page.locator(".app")).toHaveCSS("color", "rgb(18, 52, 86)");
  await app.relaunch();
  await expect(app.page.locator(".app")).toHaveAttribute("data-theme", "light");
  await expect(app.page.locator(".app")).toHaveCSS("color", "rgb(18, 52, 86)");
  await app.page.keyboard.press("Control+Comma");
  await app.page.getByText("Colors & theme", { exact: true }).locator("..").getByRole("button").click();
  await app.page.getByRole("button", { name: "Reset to defaults" }).click();
  await expect(app.page.locator("#set-text")).toHaveValue("#303640");
  await app.page.locator("#set-theme").selectOption("dark");
  await expect(app.page.locator(".app")).toHaveCSS("color", "rgb(192, 255, 238)");
});

