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

test("AGENTS.md is seeded once, user edits survive, settings can overwrite", async ({ app }) => {
  const template = fs.readFileSync(app.agentsPath, "utf8");
  expect(template).toContain("MyNote notebook");

  fs.writeFileSync(app.agentsPath, "# my own agent notes\n");
  await app.relaunch();
  expect(fs.readFileSync(app.agentsPath, "utf8")).toBe("# my own agent notes\n");

  await app.page.keyboard.press("Control+,");
  await app.page.getByRole("button", { name: "Overwrite…" }).click();
  await app.confirmDanger();
  await expect(app.page.locator(".status-toast")).toContainText("AGENTS.md overwritten");
  expect(fs.readFileSync(app.agentsPath, "utf8")).toBe(template);
});

test("close never touches files the app did not create", async ({ app }) => {
  await app.newTitledPage("Mine", 1);
  // an agent-style stray page file and a plain markdown file in the root
  const strayUuid = path.join(app.notebookDir, "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee.md");
  const strayPlain = path.join(app.notebookDir, "todo.md");
  fs.writeFileSync(strayUuid, "# Not registered\n");
  fs.writeFileSync(strayPlain, "# Scratch notes\n");

  await app.close();
  expect(fs.existsSync(strayUuid)).toBe(true);
  expect(fs.existsSync(strayPlain)).toBe(true);
});
