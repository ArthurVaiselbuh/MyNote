import fs from "node:fs";
import path from "node:path";
import { expect, test } from "../app";

const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);

test("close prunes orphan assets but keeps referenced ones", async ({ app }) => {
  await app.newTitledPage("Pics", 1);
  const [id] = await app.treeIds();
  const dir = path.join(app.notebookDir, "assets", id);
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, "kept.png"), TINY_PNG);
  fs.writeFileSync(path.join(dir, "orphan.png"), TINY_PNG);
  await app.setBody(`![shot](assets/${id}/kept.png)`);

  await app.close();
  expect(fs.existsSync(path.join(dir, "kept.png"))).toBe(true);
  expect(fs.existsSync(path.join(dir, "orphan.png"))).toBe(false);
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
