import { expect, test } from "../app";

async function treeWidth(app: { page: import("@playwright/test").Page }) {
  const box = await app.page.locator(".tree-pane").boundingBox();
  return box!.width;
}

test("splitter drag resizes the tree pane, persists, dblclick resets", async ({ app }) => {
  const before = await treeWidth(app);
  const splitter = app.page.locator(".splitter");
  const box = (await splitter.boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;

  await app.page.mouse.move(x, y);
  await app.page.mouse.down();
  await app.page.mouse.move(x + 120, y, { steps: 5 });
  await app.page.mouse.up();
  expect(Math.abs((await treeWidth(app)) - (before + 120))).toBeLessThanOrEqual(2);

  await app.relaunch();
  expect(Math.abs((await treeWidth(app)) - (before + 120))).toBeLessThanOrEqual(2);

  await app.page.locator(".splitter").dblclick();
  expect(Math.abs((await treeWidth(app)) - 300)).toBeLessThanOrEqual(2);
});
