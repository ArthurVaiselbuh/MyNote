import fs from "node:fs";
import path from "node:path";
import { expect, test } from "../app";

const PNG = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

test("dropping text, images, and files into the editor uses paste semantics", async ({ app }, testInfo) => {
  await app.newTitledPage("Dropped", 1);
  const [id] = await app.treeIds();
  await app.setBody("first line\nsecond line\nthird line");
  const imageSource = testInfo.outputPath("picture.png");
  const attachmentSource = testInfo.outputPath("_policy.json");
  const attachmentContents = '{"policy":"attachment contents"}';
  fs.writeFileSync(imageSource, Buffer.from(PNG, "base64"));
  fs.writeFileSync(attachmentSource, attachmentContents);
  const browserSession = await app.page.context().newCDPSession(app.page);

  async function dropOnLine(line: number, kind: "text" | "image" | "file") {
    const bounds = await app.page.locator(".cm-line").nth(line).boundingBox();
    expect(bounds).not.toBeNull();
    const data = {
      items: kind === "text" ? [{ mimeType: "text/plain", data: "dropped " }] : [],
      files: kind === "text" ? [] : [kind === "image" ? imageSource : attachmentSource],
      dragOperationsMask: 1,
    };
    for (const type of ["dragEnter", "dragOver", "drop"] as const) {
      await browserSession.send("Input.dispatchDragEvent", {
        type,
        x: bounds!.x + 1,
        y: bounds!.y + bounds!.height / 2,
        data,
      });
    }
  }

  await dropOnLine(1, "text");
  await expect(app.editorBody).toContainText("dropped second line");

  await dropOnLine(0, "image");
  await expect(app.editorBody).toContainText(`![](assets/${id}/`);

  await dropOnLine(2, "file");
  if (await app.modal.isVisible()) await app.modal.getByRole("button", { name: "Attach" }).click();
  await expect(app.editorBody).toContainText(`[_policy.json](files/${id}/_policy.json)`);

  await app.page.keyboard.press("Control+s");
  await expect.poll(() => app.readMd(id)).toMatch(new RegExp(`assets/${id}/[^)]+\\.png`));
  const markdown = app.readMd(id);
  const [firstLine, secondLine, thirdLine] = markdown.split("\n").slice(2);
  expect(firstLine).toContain(`![](assets/${id}/`);
  expect(secondLine).toContain("dropped second line");
  expect(thirdLine).toContain(`[_policy.json](files/${id}/_policy.json)`);
  const imagePath = markdown.match(new RegExp(`assets/${id}/[^)]+\\.png`))![0];
  expect(fs.existsSync(path.join(app.notebookDir, imagePath))).toBe(true);
  expect(fs.readFileSync(path.join(app.notebookDir, "files", id, "_policy.json"), "utf8"))
    .toBe(attachmentContents);
  await browserSession.detach();
});
