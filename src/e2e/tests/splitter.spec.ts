import { expect, test, type App } from "../app";

const DRAG_PX = 120;
const DEFAULT_TREE_WIDTH = 300; // actions.ts::PANE_WIDTHS.tree
const TOLERANCE_PX = 2;

async function treeWidth(app: App) {
  const box = await app.page.locator(".tree-pane").boundingBox();
  return box!.width;
}

const expectTreeWidth = async (app: App, expected: number) =>
  expect(Math.abs((await treeWidth(app)) - expected)).toBeLessThanOrEqual(TOLERANCE_PX);

test("splitter drag resizes the tree pane, persists, dblclick resets", async ({ app }) => {
  const before = await treeWidth(app);
  const box = (await app.page.locator(".splitter").boundingBox())!;
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;

  await app.page.mouse.move(x, y);
  await app.page.mouse.down();
  await app.page.mouse.move(x + DRAG_PX, y, { steps: 5 });
  await app.page.mouse.up();
  await expectTreeWidth(app, before + DRAG_PX);

  await app.relaunch();
  await expectTreeWidth(app, before + DRAG_PX);

  await app.page.locator(".splitter").dblclick();
  await expectTreeWidth(app, DEFAULT_TREE_WIDTH);
});
