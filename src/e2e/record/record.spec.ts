import { expect, test as base, type Page } from "@playwright/test";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { App, hasGit, NO_GIT_REASON, sleep, withScratchApp } from "../app";

// Records the README/onboarding demo GIFs by driving the real release exe over
// CDP, dumping frames as PNGs, and handing them to ffmpeg for a palette +
// dithered encode. Requires ffmpeg on PATH (e.g. `winget install Gyan.FFmpeg`)
// — dev-machine tooling only, never bundled or run at app runtime. Writes
// straight into public/tutorial, which the onboarding tour and the README both
// link at, so there is only one copy of each GIF in the repo.

const TUTORIAL_DIR = path.resolve(__dirname, "..", "..", "frontend", "public", "tutorial");
// Native window resolution, no upscaling: the UI's text and panel widths are
// CSS-fixed rather than proportional, so a bigger capture window only adds
// blank canvas. Dithering, not pixel count, is what keeps the text crisp.
const BASE_WIDTH = 1080;
const BASE_HEIGHT = 720;
const WINDOW = { x: 120, y: 80, width: BASE_WIDTH, height: BASE_HEIGHT, maximized: false };

const FRAME_INTERVAL_MS = 60;
const MIN_FRAME_MS = 30;
const FINAL_HOLD_MS = 1_500; // hold the last state before the loop restarts
// palettegen/paletteuse beats a flat nearest-color quantizer, which bands hard
// on antialiased UI text; transdiff re-encodes only pixels that changed.
const PALETTE_FILTER =
  `split[a][b];[a]palettegen=stats_mode=diff:max_colors=256[p];` +
  `[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle`;

class Recorder {
  private frames: { png: Buffer; t: number }[] = [];
  private stopped = false;
  private capturing: Promise<void> = Promise.resolve();

  start(page: Page) {
    this.stopped = false;
    this.capturing = (async () => {
      while (!this.stopped) {
        const t = Date.now();
        try {
          this.frames.push({ png: await page.screenshot(), t });
        } catch {
          // window mid-teardown; drop the frame
        }
        await sleep(FRAME_INTERVAL_MS);
      }
    })();
  }

  async stopAndSave(name: string) {
    this.stopped = true;
    await this.capturing;
    fs.mkdirSync(TUTORIAL_DIR, { recursive: true });

    const frameDir = fs.mkdtempSync(path.join(os.tmpdir(), "mynote-gif-"));
    try {
      const framePaths = this.frames.map((frame, i) => {
        const framePath = path.join(frameDir, `f${String(i).padStart(4, "0")}.png`);
        fs.writeFileSync(framePath, frame.png);
        return framePath.replace(/\\/g, "/");
      });

      const listLines = framePaths.map(
        (framePath, i) => `file '${framePath}'\nduration ${this.frameDurationSec(i)}`,
      );
      // the concat demuxer drops the last entry's duration unless the file is repeated
      listLines.push(`file '${framePaths[framePaths.length - 1]}'`);
      const listPath = path.join(frameDir, "frames.txt");
      fs.writeFileSync(listPath, listLines.join("\n"));

      const out = path.join(TUTORIAL_DIR, `${name}.gif`);
      const result = spawnSync(
        "ffmpeg",
        // prettier-ignore
        [
          "-y",
          "-f", "concat",
          "-safe", "0",
          "-i", listPath,
          "-vf", PALETTE_FILTER,
          "-gifflags", "+transdiff+offsetting",
          "-loop", "0",
          out,
        ],
        { stdio: "pipe", windowsHide: true },
      );
      if (result.status !== 0) {
        throw new Error(`ffmpeg failed (${result.status}): ${result.stderr.toString()}`);
      }
      console.log(`wrote ${out}: ${this.frames.length} frames, ${fs.statSync(out).size} bytes`);
      this.dumpSampleFrames(name);
    } finally {
      fs.rmSync(frameDir, { recursive: true, force: true });
      this.frames = [];
    }
  }

  private frameDurationSec(i: number) {
    const next = this.frames[i + 1];
    return (next ? Math.max(MIN_FRAME_MS, next.t - this.frames[i].t) : FINAL_HOLD_MS) / 1000;
  }

  private dumpSampleFrames(name: string) {
    const dir = process.env.GIF_FRAME_DUMP;
    if (!dir) return;
    fs.mkdirSync(dir, { recursive: true });
    for (const pct of [30, 55, 80, 99]) {
      const i = Math.min(this.frames.length - 1, Math.floor((this.frames.length * pct) / 100));
      fs.writeFileSync(path.join(dir, `${name}-${pct}.png`), this.frames[i].png);
    }
  }
}

/** Types one line from column zero, defeating the indent the editor inserts. */
async function typeFromColumnZero(page: Page, line: string, delay?: number) {
  await page.keyboard.press("Enter");
  await page.keyboard.press("Shift+Home");
  if (line) await page.keyboard.type(line, { delay });
  else await page.keyboard.press("Delete");
}

/** Replaces the whole body line by line (typing a multi-line string straight in
 * re-inserts each list marker). */
async function setBodyLines(app: App, lines: string[]) {
  await app.selectWholeBody();
  await app.page.keyboard.type(lines[0]);
  for (const line of lines.slice(1)) {
    await typeFromColumnZero(app.page, line);
  }
  await app.page.keyboard.press("Control+s");
  await app.page.keyboard.press("Escape");
  await expect(app.editorBody).not.toBeFocused();
}

const test = base.extend<{ app: App }>({
  // eslint-disable-next-line no-empty-pattern
  app: ({}, use, testInfo) => withScratchApp(testInfo, use, { window: WINDOW }),
});

test("edit-preview: type a code block, flip to preview", async ({ app }) => {
  const { page } = app;
  await app.newTitledPage("Quicksort notes", 1);
  await page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();
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
    await typeFromColumnZero(page, line, 24);
  }
  await sleep(900);

  await page.keyboard.press("Control+e"); // preview with highlighted code
  await expect(page.locator(".preview")).toBeVisible();
  await sleep(2200);
  await page.keyboard.press("Control+e"); // and back to the editor
  await expect(app.editorBody).toBeVisible();
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

  await app.newTitledSubpage("Architecture", 3, 45);
  await sleep(300);
  await page.keyboard.press("ArrowUp"); // parent again
  await sleep(300);
  await app.newTitledSubpage("Milestones", 4, 45);
  await sleep(300);
  await app.newTitledSubpage("Q3 targets", 5, 45); // nested one level deeper
  await sleep(500);

  for (let i = 0; i < 3; i++) {
    await page.keyboard.press("ArrowUp");
    await sleep(260);
  }
  await page.keyboard.press("ArrowLeft"); // fold the subtree
  await expect(app.rows).toHaveCount(2);
  await sleep(900);
  await page.keyboard.press("ArrowRight"); // unfold
  await expect(app.rows).toHaveCount(5);
  await sleep(900);

  await app.newSection("Research", 60);
  await sleep(400);
  await app.newTitledPage("Reading list", 1);
  await sleep(600);

  await page.keyboard.press("Control+PageUp"); // flip between sections
  await expect(app.sectionName).toContainText("Notes");
  await sleep(1100);
  await page.keyboard.press("Control+PageDown");
  await expect(app.sectionName).toContainText("Research");
  await sleep(900);

  await rec.stopAndSave("tree-sections");
});

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
  test.skip(!hasGit(), NO_GIT_REASON);

  await app.enableGitSnapshots();
  await app.newPageWithBodyFromInput("Scratch pad", "throwaway numbers from the sync", 1);
  await app.newTitledPage("Release checklist", 2);
  await setBodyLines(app, CHECKLIST_V1);
  await app.relaunch(); // close-commit: the first snapshot

  await expect(app.selectedTitle).toHaveText("Release checklist");
  await setBodyLines(app, CHECKLIST_V2);
  await app.page.keyboard.press("ArrowUp"); // onto Scratch pad
  await expect(app.selectedTitle).toHaveText("Scratch pad");
  await app.page.keyboard.press("Delete");
  await app.confirmDanger();
  await expect(app.selectedTitle).toHaveText("Release checklist");
  await app.relaunch(); // second snapshot, and the deletion lands in history

  const { page } = app; // relaunch swaps the CDP page — bind it after the last one
  await expect(app.rows).toHaveCount(1);
  await expect(app.selectedTitle).toHaveText("Release checklist");

  const rec = new Recorder();
  rec.start(page);
  await sleep(700);

  // the accident the pane exists for: a whole section goes, and gets saved
  await page.keyboard.press("Control+2");
  await expect(app.editorBody).toBeFocused();
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
  await expect(app.editorBody).toContainText("Rollback plan");
  await sleep(2000);

  await rec.stopAndSave("history");
});

test("help-search: the ? overlay and its live filter", async ({ app }) => {
  const { page } = app;
  await app.newTitledPage("Alpha", 1);

  const rec = new Recorder();
  rec.start(page);
  await sleep(700);

  await app.openHelp();
  await expect(page.locator(".modal-title")).toHaveText("Keyboard shortcuts");
  await expect(app.modal.locator("input")).toBeFocused();
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
