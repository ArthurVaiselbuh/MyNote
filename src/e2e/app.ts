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
  private closing = false;
  /** Set when the exe dies on its own. Every Playwright call then fails with a
   * bare "Target page… has been closed", which names the assertion that
   * happened to be in flight instead of the exit — keep the real cause. */
  unexpectedExit: string | null = null;

  constructor(notebookDir: string, dataDir: string) {
    this.notebookDir = notebookDir;
    this.dataDir = dataDir;
  }

  async launch() {
    await waitForPortFree();
    const webviewLogFile = path.join(this.dataDir, "webview2-chrome-debug.log");
    fs.mkdirSync(this.dataDir, { recursive: true });
    let stderr = "";
    this.child = spawn(EXE, [], {
      env: {
        ...process.env,
        // wry always sets WebView2's AdditionalBrowserArguments COM option itself
        // (even just its own defaults), which makes WebView2 ignore
        // WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS outright — MyNote reads these two
        // instead (see lib.rs::run's setup closure) to build that option itself.
        MYNOTE_E2E_CDP_PORT: String(CDP_PORT),
        MYNOTE_E2E_LOG_FILE: webviewLogFile,
        WEBVIEW2_USER_DATA_FOLDER: this.dataDir,
      },
      stdio: ["ignore", "ignore", "pipe"],
    });
    this.child.stderr!.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
    this.closing = false;
    const exited = new Promise<{ code: number | null; signal: NodeJS.Signals | null }>((resolve) =>
      this.child!.once("exit", (code, signal) => {
        if (!this.closing) {
          this.unexpectedExit =
            `mynote.exe exited on its own mid-test (code ${code}, signal ${signal})` +
            (stderr ? `\nstderr:\n${stderr}` : "");
        }
        resolve({ code, signal });
      }),
    );
    this.exited = exited.then(() => {});
    try {
      // Race the CDP attach against the process exiting early — a crash-on-launch
      // otherwise looks identical to a slow-to-start exe (both time out on connect),
      // hiding the exit code/stderr that would tell them apart.
      this.browser = await Promise.race([
        connectWithRetry(),
        exited.then(({ code, signal }) => {
          throw new Error(
            `mynote.exe exited before opening a CDP port (code ${code}, signal ${signal})` +
              (stderr ? `\nstderr:\n${stderr}` : ""),
          );
        }),
      ]);
    } catch (e) {
      throw new Error(`${(e as Error).message}\n${cdpDiagnostics(webviewLogFile)}`);
    }
    this.page = await firstPage(this.browser);
    this.page.on("pageerror", (e) => console.log(`[pageerror] ${e.message}`));
    this.page.on("console", (m) => {
      if (m.type() === "error") console.log(`[console.error] ${m.text()}`);
    });
    // "Notes" (not "—") means the notebook finished loading, not just the shell
    await expect(this.page.locator(".section-strip .name")).toContainText("Notes", {
      timeout: 45_000,
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

  /** Graceful close (WM_CLOSE) so the backend runs its on-close purge, falling
   * back to a hard kill of the whole tree. Both steps target the pid we
   * spawned rather than the name "mynote": a wedged instance that ignores
   * WM_CLOSE must still die, and killing the host alone orphans its
   * msedgewebview2 children, which keep the CDP port bound and poison every
   * later launch in the run. */
  async close() {
    if (!this.child?.pid) return;
    this.closing = true;
    const pid = this.child.pid;
    await this.browser?.close().catch(() => {});
    this.browser = null;
    quietly(
      `powershell -NoProfile -Command "(Get-Process -Id ${pid} -ErrorAction SilentlyContinue).CloseMainWindow()"`,
    );
    const exitedCleanly = await Promise.race([
      this.exited.then(() => true),
      sleep(15_000).then(() => false),
    ]);
    if (!exitedCleanly) {
      console.log(`[fixture] pid ${pid} ignored WM_CLOSE — killing its process tree`);
      killTree(pid);
      await Promise.race([this.exited, sleep(10_000)]);
    }
    this.child = null;
    // the host can exit while a WebView2 child still holds the port; the next
    // launch would then attach to nothing (or to the wrong instance)
    await waitForPortFree();
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

  get userStatePath() {
    return path.join(this.notebookDir, "notebook.user.json");
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

  /** Ctrl+, → tick "Version history" → wait for the interval field → close.
   * Idempotent: leaves snapshots enabled whether or not they already were. */
  async enableGitSnapshots() {
    await this.page.keyboard.press("Control+,");
    const checkbox = this.page.locator("#set-git");
    await expect(checkbox).toBeVisible();
    if (!(await checkbox.isChecked())) {
      await checkbox.click();
    }
    await expect(this.page.locator("#set-git-interval")).toBeVisible();
    await this.page.keyboard.press("Escape");
    await expect(this.page.locator(".modal-backdrop")).toHaveCount(0);
  }
}

/** Whether the machine running the suite has a usable `git` on PATH — the
 * same gate `git.rs::available()` applies, checked from the test side so
 * history specs can skip cleanly instead of failing on a disabled checkbox. */
export function hasGit(): boolean {
  try {
    execSync("git --version", { stdio: "ignore" });
    return true;
  } catch {
    return false;
  }
}

function quietly(cmd: string) {
  try {
    execSync(cmd, { timeout: 30_000, stdio: ["ignore", "ignore", "ignore"] });
  } catch {}
}

/** `/T` takes the WebView2 children with it — without it they orphan and keep
 * the CDP port bound long after the host is gone. */
function killTree(pid: number) {
  quietly(`taskkill /PID ${pid} /T /F`);
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
    // a loaded machine is exactly when connects are slowest, so a timeout must
    // read as "still in use" — the opposite guess hands the port to a launch
    // that then races the previous instance for it
    sock.setTimeout(2_000, () => finish(true));
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

const ATTACH_BUDGET_MS = 60_000;

async function connectWithRetry(): Promise<Browser> {
  const deadline = Date.now() + ATTACH_BUDGET_MS;
  do {
    try {
      return await chromium.connectOverCDP(CDP_URL, { timeout: 3_000 });
    } catch {
      await sleep(200);
    }
  } while (Date.now() < deadline);
  throw new Error(
    `could not attach to ${CDP_URL} within ${ATTACH_BUDGET_MS / 1000}s — did the exe start?`,
  );
}

function runDiagnostic(cmd: string): string {
  try {
    const out = execSync(cmd, { encoding: "utf8", timeout: 10_000 }).trim();
    return out || "<no output>";
  } catch (e) {
    return `<failed to run: ${(e as Error).message}>`;
  }
}

/** Last ~8000 chars of the chromium `--log-file` WebView2 was launched with —
 * that log records DevTools server bind attempts/failures directly, which is
 * a more definitive signal than inferring from process/socket state alone. */
function readLogTail(file: string, maxChars = 8_000): string {
  try {
    const content = fs.readFileSync(file, "utf8");
    return content.length > maxChars ? `…(truncated)…\n${content.slice(-maxChars)}` : content;
  } catch (e) {
    return `<could not read: ${(e as Error).message}>`;
  }
}

/** WebView2 can come up fully (rendering, writing its profile) while its
 * DevTools port silently fails to bind — Chromium doesn't treat that as
 * fatal. Dump enough state to tell "exe never started" apart from "exe is
 * running but the CDP port is unreachable" the next time this fails. */
function cdpDiagnostics(webviewLogFile: string): string {
  return [
    "--- CDP failure diagnostics ---",
    `mynote.exe processes:\n${runDiagnostic('tasklist /FI "IMAGENAME eq mynote.exe" /V /FO LIST')}`,
    `msedgewebview2.exe processes:\n${runDiagnostic('tasklist /FI "IMAGENAME eq msedgewebview2.exe" /FO LIST')}`,
    `sockets on port ${CDP_PORT}:\n${runDiagnostic(`netstat -ano | findstr :${CDP_PORT}`)}`,
    `webview2 chromium log (${webviewLogFile}):\n${readLogTail(webviewLogFile)}`,
  ].join("\n\n");
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
  fs.writeFileSync(
    SETTINGS,
    JSON.stringify({ notebookPath: notebookDir, window: null, logLevel: "verbose" }),
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

export const test = base.extend<{ app: App }>({
  // eslint-disable-next-line no-empty-pattern
  app: async ({}, use, testInfo) => {
    if (!fs.existsSync(EXE)) {
      throw new Error(`missing ${EXE} — run scripts/build.ps1 first`);
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
      if (app.unexpectedExit) {
        console.log(`\n--- app died during this test ---\n${app.unexpectedExit}`);
      }
      // a test must never hand the next one a live app or a bound CDP port
      await app.close().catch((e) => console.log(`[fixture] close failed: ${(e as Error).message}`));
      restoreSettings();
    }
  },
});

export { expect };
