import fs from "node:fs";
import path from "node:path";
import { expect, test } from "../app";

test("close prunes orphan assets but keeps referenced ones", async ({ app }) => {
  await app.newTitledPage("Pics", 1);
  const [id] = await app.treeIds();
  const kept = app.writeAsset(id, "kept.png");
  const orphan = app.writeAsset(id, "orphan.png");
  await app.setBody(`![shot](assets/${id}/kept.png)`);

  await app.close();
  expect(fs.existsSync(kept)).toBe(true);
  expect(fs.existsSync(orphan)).toBe(false);
});

test("a page deleted this session permanently removes its Markdown and assets", async ({ app }) => {
  await app.newTitledPage("Doomed", 1);
  const [id] = await app.treeIds();
  const image = app.writeAsset(id, "doomed.png");
  await app.setBody(`![doomed](assets/${id}/doomed.png)`);
  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toEqual([]);

  await app.close();
  expect(fs.existsSync(app.mdPath(id))).toBe(false);
  expect(fs.existsSync(path.join(app.notebookDir, "trash", id))).toBe(false);
  expect(fs.existsSync(image)).toBe(false);
});

test("close retains an image owned by a deleted page when another page still links it", async ({ app }) => {
  await app.newTitledPage("Image owner", 1);
  const [owner] = await app.treeIds();
  const shared = app.writeAsset(owner, "shared.png");
  await app.newTitledPage("Image user", 2);
  const [, borrower] = await app.treeIds();
  await app.setBody(`![shared](assets/${owner}/shared.png)`);

  await app.row("Image owner").click();
  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect.poll(() => app.treeIds()).toHaveLength(1);

  await app.close();
  expect(fs.existsSync(shared)).toBe(true);
  expect(app.readMd(borrower)).toContain(`assets/${owner}/shared.png`);
  expect(fs.existsSync(app.mdPath(owner))).toBe(false);
  expect(fs.existsSync(path.join(app.notebookDir, "trash", owner))).toBe(false);
});

test("AGENTS.md is seeded once, user edits survive, settings can overwrite", async ({ app }) => {
  const template = fs.readFileSync(app.agentsPath, "utf8");
  expect(template).toContain("MyNote notebook");

  fs.writeFileSync(app.agentsPath, "# my own agent notes\n");
  await app.relaunch();
  expect(fs.readFileSync(app.agentsPath, "utf8")).toBe("# my own agent notes\n");

  await app.page.keyboard.press("Control+,");
  await app.page.locator("#set-agents-overwrite").click();
  await app.confirmDanger();
  await expect(app.page.locator(".status-toast")).toContainText("AGENTS.md overwritten");
  expect(fs.readFileSync(app.agentsPath, "utf8")).toBe(template);
});

test("close trashes page files it can't vouch for, and Empty is what deletes them", async ({
  app,
}) => {
  await app.newTitledPage("Mine", 1);
  // an agent-style stray page file and a plain markdown file in the root
  const strayId = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
  const strayUuid = path.join(app.notebookDir, `${strayId}.md`);
  const strayPlain = path.join(app.notebookDir, "todo.md");
  fs.writeFileSync(strayUuid, "# Not registered\n");
  fs.writeFileSync(strayPlain, "# Scratch notes\n");

  await app.relaunch();
  const trashed = path.join(app.notebookDir, "trash", strayId, `${strayId}.md`);
  expect(fs.existsSync(strayUuid)).toBe(false);
  expect(fs.readFileSync(trashed, "utf8")).toBe("# Not registered\n");
  expect(fs.existsSync(strayPlain)).toBe(true);

  await app.page.keyboard.press("Control+,");
  await app.page.locator("#set-trash-empty").click();
  await app.confirmDanger();
  await expect(app.page.locator(".status-toast")).toContainText("deleted 1 file");
  expect(fs.existsSync(trashed)).toBe(false);
});
