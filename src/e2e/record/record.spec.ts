import { expect, test as base, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { App, hasGit } from "../app";

// Records the README/onboarding demo GIFs by driving the real release exe
// over CDP, dumping frames as PNGs, and handing them to ffmpeg for a
// palette + dithered encode (ffmpeg's palettegen/paletteuse pair beats a
// hand-rolled quantizer on quality, and its gifflags frame-diff a mostly
// static UI instead of re-encoding every pixel every frame). Requires
// ffmpeg on PATH (e.g. `winget install Gyan.FFmpeg`) — dev-machine tooling
// only, never bundled or run at app runtime. Writes straight into
// public/tutorial — the app's onboarding tour and the README (which links
// there directly) share these files, so there's only one copy in the repo.

const SETTINGS = path.join(process.env.APPDATA!, "MyNote", "settings.json");
const SETTINGS_BAK = `${SETTINGS}.gif-bak`;
const TUTORIAL_DIR = path.resolve(__dirname, "..", "..", "frontend", "public", "tutorial");
// Recorded at native window resolution, no upscaling — ffmpeg's
// palettegen/paletteuse dithering is what fixes the pixelated look (a flat
// nearest-color quantizer bands hard on antialiased UI text), not extra
// pixels. A bigger capture window was tried too, but since the UI's text and
// panel widths are CSS-fixed rather than proportional, a wider window just
// adds blank canvas — downscaling that back down shrinks the text instead of
// sharpening it.
const BASE_WIDTH = 1080;
const BASE_HEIGHT = 720;

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
      window: { x: 120, y: 80, width: BASE_WIDTH, height: BASE_HEIGHT, maximized: false },
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
    fs.mkdirSync(TUTORIAL_DIR, { recursive: true });

    const frameDir = fs.mkdtempSync(path.join(os.tmpdir(), "mynote-gif-"));
    try {
      const framePaths = this.frames.map((frame, i) => {
        const framePath = path.join(frameDir, `f${String(i).padStart(4, "0")}.png`);
        fs.writeFileSync(framePath, frame.png);
        return framePath;
      });

      const listLines = framePaths.map((framePath, i) => {
        const durationSec =
          (i + 1 < this.frames.length
            ? Math.max(30, this.frames[i + 1].t - this.frames[i].t)
            : 1500) / 1000; // hold the final state before the loop restarts
        return `file '${framePath.replace(/\\/g, "/")}'\nduration ${durationSec}`;
      });
      // the concat demuxer drops the last entry's duration unless the file is repeated
      listLines.push(`file '${framePaths[framePaths.length - 1].replace(/\\/g, "/")}'`);
      const listPath = path.join(frameDir, "frames.txt");
      fs.writeFileSync(listPath, listLines.join("\n"));

      const out = path.join(TUTORIAL_DIR, `${name}.gif`);
      const result = spawnSync(
        "ffmpeg",
        [
          "-y",
          "-f", "concat",
          "-safe", "0",
          "-i", listPath,
          "-vf",
          `split[a][b];[a]palettegen=stats_mode=diff:max_colors=256[p];` +
            `[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle`,
          "-gifflags", "+transdiff+offsetting", // only re-encode pixels that changed frame-to-frame
          "-loop", "0",
          out,
        ],
        { stdio: "pipe" },
      );
      if (result.status !== 0) {
        throw new Error(`ffmpeg failed (${result.status}): ${result.stderr.toString()}`);
      }
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
    } finally {
      fs.rmSync(frameDir, { recursive: true, force: true });
    }
    this.frames = [];
  }
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

/** Replaces the whole body, defeating the editor's auto-indent line by line
 * (typing a multi-line string straight in re-inserts each list marker). */
async function setBodyLines(page: Page, lines: string[]) {
  await page.keyboard.press("Control+2");
  await expect(page.locator(".cm-content")).toBeFocused();
  await page.keyboard.press("Control+a");
  await page.keyboard.type(lines[0]);
  for (const line of lines.slice(1)) {
    await page.keyboard.press("Enter");
    await page.keyboard.press("Shift+Home");
    if (line) await page.keyboard.type(line);
    else await page.keyboard.press("Delete");
  }
  await page.keyboard.press("Control+s");
  await page.keyboard.press("Escape");
  await expect(page.locator(".cm-content")).not.toBeFocused();
}

const CHECKLIST_V1 = [
  "Ship 2.4 on Friday.",
  "",
  "Pre-flight",
  "- freeze the release branch",
  "- run the full test matrix",
  "",
  "Release",
  "- tag v2.4.0 and push the tag",
  "- write the changelog entry",
];

const CHECKLIST_V2 = [
  "Ship 2.4 on Friday.",
  "",
  "Pre-flight",
  "- freeze the release branch",
  "- run the full test matrix on Windows and macOS",
  "- bump the version in tauri.conf.json",
  "",
  "Release",
  "- tag v2.4.0 and push the tag",
  "- build with build.bat, smoke-test the exe",
  "- write the changelog entry",
  "",
  "Rollback plan",
  "- keep 2.3.1 pinned in the archive folder",
  "- revert the tag, announce in #releases",
];

test("history: snapshots, diff, restore", async ({ app }) => {
  test.skip(!hasGit(), "git not found on PATH — history is inert without it");

  await app.enableGitSnapshots();
  await app.newPageWithBody("Scratch pad", "throwaway numbers from the sync", 1);
  await app.newTitledPage("Release checklist", 2);
  await setBodyLines(app.page, CHECKLIST_V1);
  await app.relaunch(); // close-commit: the first snapshot

  await expect(app.page.locator(".tree .row.selected .title")).toHaveText("Release checklist");
  await setBodyLines(app.page, CHECKLIST_V2);
  await app.page.keyboard.press("ArrowUp"); // onto Scratch pad
  await expect(app.page.locator(".tree .row.selected .title")).toHaveText("Scratch pad");
  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await app.relaunch(); // second snapshot, and the deletion lands in history

  const { page } = app; // relaunch swaps the CDP page — bind it after the last one
  await expect(page.locator(".tree .row")).toHaveCount(1);
  await expect(page.locator(".tree .row.selected .title")).toHaveText("Release checklist");

  const rec = new Recorder();
  rec.start(page);
  await sleep(700);

  // the accident the pane exists for: a whole section goes, and gets saved
  await page.keyboard.press("Control+2");
  await expect(page.locator(".cm-content")).toBeFocused();
  await page.keyboard.press("Control+End");
  for (let i = 0; i < 4; i++) await page.keyboard.press("Shift+ArrowUp");
  await page.keyboard.press("Shift+End");
  await sleep(700);
  await page.keyboard.press("Delete");
  await page.keyboard.press("Control+s");
  await sleep(1100);

  await page.keyboard.press("Control+h");
  await expect(page.locator(".history-layout")).toBeVisible();
  await expect(page.locator(".history-rail .picker-item")).toHaveCount(3, { timeout: 15_000 });
  await expect(page.locator(".diff.split")).toBeVisible();
  await sleep(2400); // read the split diff: green is what restoring brings back

  await page.keyboard.press("ArrowDown"); // step back to the older snapshot
  await sleep(2000);
  await page.keyboard.press("ArrowUp");
  await sleep(1200);

  await page.keyboard.press("2"); // inline diff
  await expect(page.locator(".diff.inline")).toBeVisible();
  await sleep(1800);
  await page.keyboard.press("1");
  await expect(page.locator(".diff.split")).toBeVisible();
  await sleep(1000);

  await page.keyboard.press("Tab"); // the other half: pages deleted since
  await expect(page.locator(".history-rail .picker-item")).toContainText(["Scratch pad"]);
  await sleep(1800);
  await page.keyboard.press("Tab");
  await expect(page.locator(".diff.split")).toBeVisible();
  await sleep(900);

  await page.keyboard.press("Enter"); // restore the newest snapshot
  await app.confirmDanger();
  await expect(page.locator(".history-layout")).toHaveCount(0);
  await expect(page.locator(".cm-content")).toContainText("Rollback plan");
  await sleep(2000);

  await rec.stopAndSave("history");
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
