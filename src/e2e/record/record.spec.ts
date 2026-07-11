import { expect, test as base, type Page } from "@playwright/test";
import fs from "node:fs";
import path from "node:path";
import { PNG } from "pngjs";
// @ts-expect-error gifenc ships no type declarations
import { GIFEncoder, quantize, applyPalette } from "gifenc";
import { App } from "../app";

// Records the README demo GIFs by driving the real release exe over CDP and
// encoding captured frames with gifenc (no ffmpeg needed on the machine).

const SETTINGS = path.join(process.env.APPDATA!, "MyNote", "settings.json");
const SETTINGS_BAK = `${SETTINGS}.gif-bak`;
const DOCS = path.resolve(__dirname, "..", "..", "..", "docs");
const TARGET_WIDTH = 880;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

function swapSettings(notebookDir: string) {
  fs.mkdirSync(path.dirname(SETTINGS), { recursive: true });
  if (fs.existsSync(SETTINGS_BAK)) {
    fs.copyFileSync(SETTINGS_BAK, SETTINGS);
    fs.rmSync(SETTINGS_BAK);
  }
  if (fs.existsSync(SETTINGS)) fs.copyFileSync(SETTINGS, SETTINGS_BAK);
  fs.writeFileSync(
    SETTINGS,
    JSON.stringify({
      notebookPath: notebookDir,
      window: { x: 120, y: 80, width: 1080, height: 720, maximized: false },
    }),
  );
}

function restoreSettings() {
  if (fs.existsSync(SETTINGS_BAK)) {
    fs.copyFileSync(SETTINGS_BAK, SETTINGS);
    fs.rmSync(SETTINGS_BAK);
  } else {
    fs.rmSync(SETTINGS, { force: true });
  }
}

class Recorder {
  private frames: { png: Buffer; t: number }[] = [];
  private stopFlag = false;
  private loop: Promise<void> = Promise.resolve();

  start(page: Page) {
    this.stopFlag = false;
    this.loop = (async () => {
      while (!this.stopFlag) {
        const t = Date.now();
        try {
          this.frames.push({ png: await page.screenshot(), t });
        } catch {
          // window mid-teardown; drop the frame
        }
        await sleep(60);
      }
    })();
  }

  async stopAndSave(name: string) {
    this.stopFlag = true;
    await this.loop;
    fs.mkdirSync(DOCS, { recursive: true });
    const gif = GIFEncoder();
    for (let i = 0; i < this.frames.length; i++) {
      const src = PNG.sync.read(this.frames[i].png);
      const scale = Math.min(1, TARGET_WIDTH / src.width);
      const dw = Math.round(src.width * scale);
      const dh = Math.round(src.height * scale);
      const rgba = boxResize(src.data, src.width, src.height, dw, dh);
      const palette = quantize(rgba, 256);
      const index = applyPalette(rgba, palette);
      const delay =
        i + 1 < this.frames.length
          ? Math.max(30, this.frames[i + 1].t - this.frames[i].t)
          : 1500; // hold the final state before the loop restarts
      gif.writeFrame(index, dw, dh, { palette, delay });
    }
    gif.finish();
    const out = path.join(DOCS, `${name}.gif`);
    fs.writeFileSync(out, Buffer.from(gif.bytes()));
    console.log(`wrote ${out}: ${this.frames.length} frames, ${fs.statSync(out).size} bytes`);
    if (process.env.GIF_FRAME_DUMP) {
      fs.mkdirSync(process.env.GIF_FRAME_DUMP, { recursive: true });
      for (const pct of [30, 55, 80, 99]) {
        const i = Math.min(this.frames.length - 1, Math.floor((this.frames.length * pct) / 100));
        fs.writeFileSync(
          path.join(process.env.GIF_FRAME_DUMP, `${name}-${pct}.png`),
          this.frames[i].png,
        );
      }
    }
    this.frames = [];
  }
}

function boxResize(src: Buffer, sw: number, sh: number, dw: number, dh: number) {
  if (dw === sw && dh === sh) return new Uint8ClampedArray(src);
  const dst = new Uint8ClampedArray(dw * dh * 4);
  for (let y = 0; y < dh; y++) {
    const y0 = Math.floor((y * sh) / dh);
    const y1 = Math.max(y0 + 1, Math.ceil(((y + 1) * sh) / dh));
    for (let x = 0; x < dw; x++) {
      const x0 = Math.floor((x * sw) / dw);
      const x1 = Math.max(x0 + 1, Math.ceil(((x + 1) * sw) / dw));
      let r = 0;
      let g = 0;
      let b = 0;
      let n = 0;
      for (let sy = y0; sy < y1; sy++) {
        for (let sx = x0; sx < x1; sx++) {
          const o = (sy * sw + sx) * 4;
          r += src[o];
          g += src[o + 1];
          b += src[o + 2];
          n++;
        }
      }
      const d = (y * dw + x) * 4;
      dst[d] = r / n;
      dst[d + 1] = g / n;
      dst[d + 2] = b / n;
      dst[d + 3] = 255;
    }
  }
  return dst;
}

/** Types a code line replacing whatever indent the editor auto-inserted. */
async function typeCodeLine(page: Page, line: string) {
  await page.keyboard.press("Enter");
  await page.keyboard.press("Shift+Home");
  await page.keyboard.type(line, { delay: 24 });
}

const test = base.extend<{ app: App }>({
  // eslint-disable-next-line no-empty-pattern
  app: async ({}, use, testInfo) => {
    const notebookDir = testInfo.outputPath("notebook");
    const dataDir = testInfo.outputPath("wv2-data");
    fs.mkdirSync(notebookDir, { recursive: true });
    swapSettings(notebookDir);
    const app = new App(notebookDir, dataDir);
    try {
      await app.launch();
      await use(app);
    } finally {
      await app.close().catch(() => {});
      restoreSettings();
    }
  },
});

test("edit-preview: type a code block, flip to preview", async ({ app }) => {
  const { page } = app;
  await app.newTitledPage("Quicksort notes", 1);
  await page.keyboard.press("Control+2");
  await expect(page.locator(".cm-content")).toBeFocused();
  await page.keyboard.press("Control+End"); // keep the seeded date stamp on top
  await page.keyboard.type(
    "Refactoring the sort helpers tonight.\n\nAlways recurse into the smaller half first — keeps the stack at O(log n).\n",
  );

  const rec = new Recorder();
  rec.start(page);
  await sleep(700);

  await page.keyboard.press("Enter");
  await page.keyboard.press("Control+j"); // insert helper, filtered live
  await expect(page.locator(".modal-title")).toHaveText("Insert (Ctrl+J)");
  await sleep(900);
  await page.keyboard.type("code", { delay: 380 });
  await sleep(900);
  await page.keyboard.press("ArrowDown"); // "Inline code" → "Code block"
  await sleep(600);
  await page.keyboard.press("Enter"); // fence inserted, caret inside
  await expect(page.locator(".modal-title")).toHaveCount(0);
  await sleep(700);

  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("End");
  await page.keyboard.type("python", { delay: 60 }); // tag the opening fence
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("End");
  await page.keyboard.type("def quicksort(xs):", { delay: 24 });
  for (const line of [
    "    if len(xs) <= 1:",
    "        return xs",
    "    pivot, *rest = xs",
    "    small = [x for x in rest if x < pivot]",
    "    large = [x for x in rest if x >= pivot]",
    "    return quicksort(small) + [pivot] + quicksort(large)",
  ]) {
    await typeCodeLine(page, line);
  }
  await sleep(900);

  await page.keyboard.press("Control+e"); // preview with highlighted code
  await expect(page.locator(".preview")).toBeVisible();
  await sleep(2200);
  await page.keyboard.press("Control+e"); // and back to the editor
  await expect(page.locator(".cm-content")).toBeVisible();
  await sleep(800);

  await rec.stopAndSave("edit-preview");
});

test("tree-sections: subpages, folding, section switching", async ({ app }) => {
  const { page } = app;
  await app.newTitledPage("Project Falcon", 1);
  await app.newTitledPage("Meeting notes", 2);
  await page.keyboard.press("ArrowUp"); // back onto Project Falcon

  const rec = new Recorder();
  rec.start(page);
  await sleep(700);

  const subpage = async (title: string, expectedCount: number) => {
    await page.keyboard.press("Control+Enter");
    await expect(page.locator(".tree .row")).toHaveCount(expectedCount);
    const input = page.locator(".title-input");
    await expect(input).toBeFocused();
    await expect(input).toHaveValue("Untitled");
    await page.keyboard.press("Control+a");
    await page.keyboard.type(title, { delay: 45 });
    await page.keyboard.press("Control+s");
    await expect(page.locator(".tree .row.selected .title")).toHaveText(title);
    await page.keyboard.press("Escape");
    await expect(input).not.toBeFocused();
  };

  await subpage("Architecture", 3);
  await sleep(300);
  await page.keyboard.press("ArrowUp"); // parent again
  await sleep(300);
  await subpage("Milestones", 4);
  await sleep(300);
  await subpage("Q3 targets", 5); // nested one level deeper
  await sleep(500);

  for (let i = 0; i < 3; i++) {
    await page.keyboard.press("ArrowUp");
    await sleep(260);
  }
  await page.keyboard.press("ArrowLeft"); // fold the subtree
  await expect(page.locator(".tree .row")).toHaveCount(2);
  await sleep(900);
  await page.keyboard.press("ArrowRight"); // unfold
  await expect(page.locator(".tree .row")).toHaveCount(5);
  await sleep(900);

  await page.keyboard.press("Control+Shift+n"); // new section
  const rename = page.locator(".section-strip input");
  await expect(rename).toBeFocused();
  await page.keyboard.type("Research", { delay: 60 });
  await page.keyboard.press("Enter");
  await expect(page.locator(".section-strip .name")).toContainText("Research");
  await sleep(400);
  await app.newTitledPage("Reading list", 1);
  await sleep(600);

  await page.keyboard.press("Control+PageUp"); // flip between sections
  await expect(page.locator(".section-strip .name")).toContainText("Notes");
  await sleep(1100);
  await page.keyboard.press("Control+PageDown");
  await expect(page.locator(".section-strip .name")).toContainText("Research");
  await sleep(900);

  await rec.stopAndSave("tree-sections");
});

test("help-search: the ? overlay and its live filter", async ({ app }) => {
  const { page } = app;
  await app.newTitledPage("Alpha", 1);

  const rec = new Recorder();
  rec.start(page);
  await sleep(700);

  await page.keyboard.press("?");
  await expect(page.locator(".modal-title")).toHaveText("Keyboard shortcuts");
  const filter = page.locator(".modal input");
  await expect(filter).toBeFocused();
  await sleep(1400);

  // slow typing so several frames land between keystrokes — the GIF must
  // visibly show the shortcut list narrowing character by character
  await page.keyboard.type("move", { delay: 420 });
  await sleep(1600);
  await page.keyboard.press("Control+a");
  await page.keyboard.type("zoom", { delay: 420 });
  await sleep(1600);
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Delete");
  await sleep(900);

  await page.keyboard.press("Escape");
  await expect(page.locator(".modal-title")).toHaveCount(0);
  await sleep(600);

  await rec.stopAndSave("help-search");
});
