import { chromium, expect, test as base, type Browser, type Page } from "@playwright/test";
import { execFileSync, execSync, spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";

const EXE = path.resolve(__dirname, "..", "..", "output", "mynote.exe");
const SETTINGS = path.join(process.env.APPDATA!, "MyNote", "settings.json");
const SETTINGS_BAK = `${SETTINGS}.e2e-bak`;
const CDP_PORT = 9222;
const CDP_URL = `http://127.0.0.1:${CDP_PORT}`;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export class App {
  page!: Page;
  readonly notebookDir: string;
  private readonly dataDir: string;
  private child: ChildProcess | null = null;
  private browser: Browser | null = null;
  private exited: Promise<void> = Promise.resolve();

  constructor(notebookDir: string, dataDir: string) {
    this.notebookDir = notebookDir;
    this.dataDir = dataDir;
  }

  async launch() {
    await waitForPortFree();
    this.child = spawn(EXE, [], {
      env: {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${CDP_PORT}`,
        WEBVIEW2_USER_DATA_FOLDER: this.dataDir,
      },
      stdio: "ignore",
    });
    this.exited = new Promise((resolve) => this.child!.once("exit", () => resolve()));
    this.browser = await connectWithRetry();
    this.page = await firstPage(this.browser);
    this.page.on("pageerror", (e) => console.log(`[pageerror] ${e.message}`));
    this.page.on("console", (m) => {
      if (m.type() === "error") console.log(`[console.error] ${m.text()}`);
    });
    // "Notes" (not "—") means the notebook finished loading, not just the shell
    await expect(this.page.locator(".section-strip .name")).toContainText("Notes", {
      timeout: 15_000,
    });
    await this.dismissWelcome();
  }

  /** First run pops the onboarding tour, which would eat keystrokes — mark it
   * seen (survives relaunch in the shared data dir) and close it. */
  private async dismissWelcome() {
    await this.page.evaluate(() => localStorage.setItem("mynote.welcome.v1", "1"));
    const welcome = this.page.locator(".welcome-backdrop");
    if (await welcome.count()) {
      await this.page.keyboard.press("Escape");
      await expect(welcome).toHaveCount(0);
    }
  }

  /** Graceful close (WM_CLOSE) so the backend runs its on-close purge. */
  async close() {
    if (!this.child) return;
    await this.browser?.close().catch(() => {});
    this.browser = null;
    execSync(
      `powershell -NoProfile -Command "(Get-Process mynote -ErrorAction SilentlyContinue).CloseMainWindow()"`,
      { timeout: 10_000 },
    );
    await Promise.race([
      this.exited,
      sleep(10_000).then(() => {
        throw new Error("app did not exit after CloseMainWindow");
      }),
    ]);
    this.child = null;
  }

  async relaunch() {
    await this.close();
    await this.launch();
  }

  treeIds(): Promise<string[]> {
    return this.page
      .locator(".tree .row")
      .evaluateAll((rows) => rows.map((r) => (r as HTMLElement).dataset.id!));
  }

  treeDepths(): Promise<number[]> {
    return this.page
      .locator(".tree .row")
      .evaluateAll((rows) => rows.map((r) => r.querySelectorAll(".guide").length));
  }

  mdPath(id: string) {
    return path.join(this.notebookDir, `${id}.md`);
  }

  mdExists(id: string) {
    return fs.existsSync(this.mdPath(id));
  }

  readNotebookJson(): unknown {
    return JSON.parse(fs.readFileSync(path.join(this.notebookDir, "notebook.json"), "utf8"));
  }

  /** Waits for the confirm dialog and accepts it via its autofocused button. */
  async confirmDanger() {
    const danger = this.page.locator(".modal .danger");
    await expect(danger).toBeFocused();
    await this.page.keyboard.press("Enter");
  }

  /** Ctrl+N, wait for the row to appear, Esc back to the tree. */
  async newPageToTree(expectedCount: number) {
    await this.page.keyboard.press("Control+n");
    await expect(this.page.locator(".tree .row")).toHaveCount(expectedCount);
    // the title input grabs focus via rAF — let it land before Esc, or the
    // next keypress races it and gets eaten by the typing guard
    const title = this.page.locator(".title-input");
    await expect(title).toBeFocused();
    await this.page.keyboard.press("Escape");
    await expect(title).not.toBeFocused();
  }

  /** Ctrl+N with a real title, committed to tree + disk; ends in the tree. */
  async newTitledPage(title: string, expectedCount: number) {
    await this.page.keyboard.press("Control+n");
    await expect(this.page.locator(".tree .row")).toHaveCount(expectedCount);
    const input = this.page.locator(".title-input");
    await expect(input).toBeFocused();
    // the async page load resets the bound value to "Untitled" and collapses
    // the auto-selection — wait for it to land or typing races the reset
    await expect(input).toHaveValue("Untitled");
    await this.page.keyboard.press("Control+a");
    await this.page.keyboard.type(title);
    await this.page.keyboard.press("Control+s");
    await expect(this.page.locator(".tree .row.selected .title")).toHaveText(title);
    await this.page.keyboard.press("Escape");
    await expect(input).not.toBeFocused();
  }

  /** Replaces the whole body of the current page and saves; ends in the tree. */
  async setBody(body: string) {
    await this.page.keyboard.press("Control+2");
    const cm = this.page.locator(".cm-content");
    await expect(cm).toBeFocused();
    await this.page.keyboard.press("Control+a");
    await this.page.keyboard.type(body);
    await this.page.keyboard.press("Control+s");
    await this.page.keyboard.press("Escape");
    await expect(cm).not.toBeFocused();
  }

  async newPageWithBody(title: string, body: string, expectedCount: number) {
    await this.newTitledPage(title, expectedCount);
    await this.setBody(body);
  }
}

function portIsOpen(): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = net.connect({ host: "127.0.0.1", port: CDP_PORT });
    const finish = (open: boolean) => {
      sock.destroy();
      resolve(open);
    };
    sock.once("connect", () => finish(true));
    sock.once("error", () => finish(false));
    sock.setTimeout(500, () => finish(false));
  });
}

function portHolder(): { pid: number; name: string; parent: string } | null {
  try {
    const script =
      `$owner = @(Get-NetTCPConnection -LocalPort ${CDP_PORT} -State Listen -ErrorAction SilentlyContinue)[0].OwningProcess; ` +
      `if ($owner) { $p = Get-CimInstance Win32_Process -Filter "ProcessId=$owner" -ErrorAction SilentlyContinue; ` +
      `$par = @(Get-CimInstance Win32_Process -Filter "ProcessId=$($p.ParentProcessId)" -ErrorAction SilentlyContinue)[0]; ` +
      `"$owner|$($p.Name)|$($par.Name)" }; exit 0`;
    const out = execFileSync("powershell", ["-NoProfile", "-Command", script], {
      encoding: "utf8",
      timeout: 10_000,
    }).trim();
    if (!out) return null;
    const [pid, name, parent] = out.split("|");
    return { pid: Number(pid), name: name || "unknown process", parent: parent || "" };
  } catch {
    return null;
  }
}

async function waitForPortFree() {
  for (let i = 0; i < 50; i++) {
    if (!(await portIsOpen())) return;
    await sleep(200);
  }
  const holder = portHolder();
  const parent = holder?.parent ? `, parent ${holder.parent}` : "";
  const who = holder ? `${holder.name} (pid ${holder.pid}${parent})` : "an unidentified process";
  const pid = holder ? String(holder.pid) : "<pid>";
  throw new Error(
    `CDP port ${CDP_PORT} is still in use by ${who} after 10s — likely an orphaned ` +
      `WebView2/mynote from a killed run. Close it and retry, e.g.:\n` +
      `  powershell -Command "Stop-Process -Id ${pid} -Force"`,
  );
}

async function connectWithRetry(): Promise<Browser> {
  for (let i = 0; i < 75; i++) {
    try {
      return await chromium.connectOverCDP(CDP_URL, { timeout: 3_000 });
    } catch {
      await sleep(200);
    }
  }
  throw new Error(`could not attach to ${CDP_URL} — did the exe start?`);
}

async function firstPage(browser: Browser): Promise<Page> {
  for (let i = 0; i < 50; i++) {
    const page = browser.contexts()[0]?.pages()[0];
    if (page) return page;
    await sleep(100);
  }
  throw new Error("no page target appeared over CDP");
}

function mynoteRunning(): boolean {
  const out = execSync('tasklist /FI "IMAGENAME eq mynote.exe" /NH', {
    encoding: "utf8",
    timeout: 10_000,
  });
  return out.toLowerCase().includes("mynote.exe");
}

function swapSettingsToScratch(notebookDir: string) {
  fs.mkdirSync(path.dirname(SETTINGS), { recursive: true });
  // a leftover backup means a previous run crashed before restoring — the
  // live file is scratch junk, the backup is the user's real settings
  if (fs.existsSync(SETTINGS_BAK)) {
    fs.copyFileSync(SETTINGS_BAK, SETTINGS);
    fs.rmSync(SETTINGS_BAK);
  }
  if (fs.existsSync(SETTINGS)) fs.copyFileSync(SETTINGS, SETTINGS_BAK);
  fs.writeFileSync(SETTINGS, JSON.stringify({ notebookPath: notebookDir, window: null }));
}

function restoreSettings() {
  if (fs.existsSync(SETTINGS_BAK)) {
    fs.copyFileSync(SETTINGS_BAK, SETTINGS);
    fs.rmSync(SETTINGS_BAK);
  } else {
    fs.rmSync(SETTINGS, { force: true });
  }
}

export const test = base.extend<{ app: App }>({
  // eslint-disable-next-line no-empty-pattern
  app: async ({}, use, testInfo) => {
    if (!fs.existsSync(EXE)) {
      throw new Error(`missing ${EXE} — run build.bat first`);
    }
    if (mynoteRunning()) {
      throw new Error("MyNote is already running — close it before running the e2e suite");
    }
    const notebookDir = testInfo.outputPath("notebook");
    const dataDir = testInfo.outputPath("wv2-data");
    fs.mkdirSync(notebookDir, { recursive: true });
    swapSettingsToScratch(notebookDir);
    const app = new App(notebookDir, dataDir);
    try {
      await app.launch();
      await use(app);
    } finally {
      await app.close().catch(() => {
        try {
          execSync(
            'powershell -NoProfile -Command "Get-Process mynote -ErrorAction SilentlyContinue | Stop-Process"',
            { timeout: 10_000 },
          );
        } catch {}
      });
      restoreSettings();
    }
  },
});

export { expect };
