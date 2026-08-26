import {
  chromium,
  expect,
  test as base,
  type Browser,
  type Page,
  type TestInfo,
} from "@playwright/test";
import { execFileSync, execSync, spawn, type ChildProcess } from "node:child_process";
import fs from "node:fs";
import net from "node:net";
import path from "node:path";

const EXE = path.resolve(__dirname, "..", "..", "output", "mynote.exe");
const SETTINGS = path.join(process.env.APPDATA!, "MyNote", "settings.json");
const SETTINGS_BAK = `${SETTINGS}.e2e-bak`;
const CDP_PORT = 9222;
const CDP_URL = `http://127.0.0.1:${CDP_PORT}`;
/** 1x1 PNG — the smallest thing the note-asset protocol can be asked to serve. */
const TINY_PNG = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==",
  "base64",
);

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** `line 0 of filler` … — a body long enough to scroll. */
export const fillerBody = (lines: number) =>
  Array.from({ length: lines }, (_, i) => `line ${i} of filler`).join("\n");

export class App {
  page!: Page;
  readonly notebookDir: string;
  private readonly dataDir: string;
  private child: ChildProcess | null = null;
  private browser: Browser | null = null;
  private exited: Promise<Exit> = Promise.resolve({ code: 0, signal: null });
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
    fs.mkdirSync(this.dataDir, { recursive: true });
    let stderr = "";
    const child = spawn(EXE, [], {
      env: {
        ...process.env,
        // wry always sets WebView2's AdditionalBrowserArguments COM option itself
        // (even just its own defaults), which makes WebView2 ignore
        // WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS outright — MyNote reads this
        // instead (see lib.rs::run's setup closure) to build that option itself.
        MYNOTE_E2E_CDP_PORT: String(CDP_PORT),
        WEBVIEW2_USER_DATA_FOLDER: this.dataDir,
      },
      stdio: ["ignore", "ignore", "pipe"],
      windowsHide: true,
    });
    this.child = child;
    child.stderr!.on("data", (chunk: Buffer) => (stderr += chunk.toString()));
    this.closing = false;
    this.exited = new Promise<Exit>((resolve) =>
      child.once("exit", (code, signal) => {
        if (!this.closing) {
          this.unexpectedExit =
            `mynote.exe exited on its own mid-test (code ${code}, signal ${signal})` +
            (stderr ? `\nstderr:\n${stderr}` : "");
        }
        resolve({ code, signal });
      }),
    );
    try {
      // Race the CDP attach against the process exiting early — a crash-on-launch
      // otherwise looks identical to a slow-to-start exe (both time out on connect),
      // hiding the exit code/stderr that would tell them apart.
      this.browser = await Promise.race([
        connectWithRetry(),
        this.exited.then(({ code, signal }) => {
          throw new Error(
            `mynote.exe exited before opening a CDP port (code ${code}, signal ${signal})` +
              (stderr ? `\nstderr:\n${stderr}` : ""),
          );
        }),
      ]);
    } catch (e) {
      throw new Error(`${(e as Error).message}\n${cdpDiagnostics()}`);
    }
    this.page = await firstPage(this.browser);
    this.page.on("pageerror", (e) => console.log(`[pageerror] ${e.message}`));
    this.page.on("console", (m) => {
      if (m.type() === "error") console.log(`[console.error] ${m.text()}`);
    });
    // "Notes" (not "—") means the notebook finished loading, not just the shell
    await expect(this.sectionName).toContainText("Notes", { timeout: 45_000 });
    await this.dismissWelcome();
  }

  /** First run pops the onboarding tour, which would eat keystrokes. Closing it
   * is also what marks it seen, so a relaunch in the same data dir stays clear. */
  private async dismissWelcome() {
    const welcome = this.page.locator(".welcome-backdrop");
    if (await welcome.count()) {
      await this.page.keyboard.press("Escape");
      await expect(welcome).toHaveCount(0);
    }
  }

  /** Graceful close through Tauri so the backend runs its on-close purge,
   * falling back to a hard kill of the whole process tree. */
  async close() {
    if (!this.child?.pid) return;
    this.closing = true;
    const pid = this.child.pid;
    await this.page
      ?.evaluate(() =>
        (
          window as typeof window & {
            __TAURI_INTERNALS__: {
              invoke(command: string, args: Record<string, unknown>): Promise<unknown>;
            };
          }
        ).__TAURI_INTERNALS__.invoke("plugin:window|close", { label: "main" }),
      )
      .catch(() => {});
    await this.browser?.close().catch(() => {});
    this.browser = null;
    const exitedCleanly = await Promise.race([
      this.exited.then(() => true),
      sleep(5_000).then(() => false),
    ]);
    if (!exitedCleanly) {
      console.log(`[fixture] pid ${pid} ignored close — killing its process tree`);
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

  get rows() {
    return this.page.locator(".tree .row");
  }

  get rowTitles() {
    return this.page.locator(".tree .row .title");
  }

  get selectedTitle() {
    return this.page.locator(".tree .row.selected .title");
  }

  get titleInput() {
    return this.page.locator(".title-input");
  }

  get editorBody() {
    return this.page.locator(".cm-content");
  }

  get sectionName() {
    return this.page.locator(".section-strip .name");
  }

  get modal() {
    return this.page.locator(".modal");
  }

  get modalBackdrop() {
    return this.page.locator(".modal-backdrop");
  }

  /** A renaming row shows an input instead of its title, so it can no longer be
   * found by text — only one rename is ever open at a time. */
  get renameInput() {
    return this.page.locator(".tree .row input.rename");
  }

  row(title: string) {
    return this.page.locator(".tree .row", { hasText: title });
  }

  treeIds(): Promise<string[]> {
    return this.rows.evaluateAll((rows) => rows.map((r) => (r as HTMLElement).dataset.id!));
  }

  treeDepths(): Promise<number[]> {
    return this.rows.evaluateAll((rows) => rows.map((r) => r.querySelectorAll(".guide").length));
  }

  /** Text of the editor line the primary caret currently sits on. */
  caretLineText(): Promise<string> {
    return this.page.locator(".cm-editor").evaluate((root) => {
      const caret = root.querySelector(".cm-cursor-primary");
      if (!caret) return "";
      const y = caret.getBoundingClientRect().top + 1;
      return (
        [...root.querySelectorAll<HTMLElement>(".cm-line")].find((line) => {
          const box = line.getBoundingClientRect();
          return box.top <= y && box.bottom >= y;
        })?.textContent ?? ""
      );
    });
  }

  scrollTopOf(scroller: string): Promise<number> {
    return this.page.locator(scroller).evaluate((el) => el.scrollTop);
  }

  scrollTo(scroller: string, top: number) {
    return this.page.locator(scroller).evaluate((el, to) => (el.scrollTop = to), top);
  }

  mdPath(id: string) {
    return path.join(this.notebookDir, `${id}.md`);
  }

  mdExists(id: string) {
    return fs.existsSync(this.mdPath(id));
  }

  /** Drops a real image into a page's asset dir, returning its path on disk. */
  writeAsset(pageId: string, name: string): string {
    const dir = path.join(this.notebookDir, "assets", pageId);
    fs.mkdirSync(dir, { recursive: true });
    const file = path.join(dir, name);
    fs.writeFileSync(file, TINY_PNG);
    return file;
  }

  get agentsPath() {
    return path.join(this.notebookDir, "AGENTS.md");
  }

  readMd(id: string): string {
    return fs.readFileSync(this.mdPath(id), "utf8");
  }

  readNotebookJson(): unknown {
    return this.readJson("notebook.json");
  }

  readUserJson(): { viewPositions?: Record<string, Record<string, number>> } {
    return this.readJson("notebook.user.json");
  }

  private readJson<T>(name: string): T {
    return JSON.parse(fs.readFileSync(path.join(this.notebookDir, name), "utf8")) as T;
  }

  /** Waits for the confirm dialog and accepts it via its autofocused button. */
  async confirmDanger() {
    const danger = this.page.locator(".modal .danger");
    await expect(danger).toBeFocused();
    await this.page.keyboard.press("Enter");
  }

  /** Ctrl+N, wait for the row to appear, Esc back to the tree. */
  async newPageToTree(expectedCount: number) {
    await this.startPage("Control+n", expectedCount);
    await this.page.keyboard.press("Escape");
    await expect(this.titleInput).not.toBeFocused();
  }

  /** Ctrl+N with a real title, committed to tree + disk; ends in the tree. */
  newTitledPage(title: string, expectedCount: number, typeDelay?: number) {
    return this.createTitledPage("Control+n", title, expectedCount, typeDelay);
  }

  /** Ctrl+Enter with a real title, nested under the selection; ends in the tree. */
  newTitledSubpage(title: string, expectedCount: number, typeDelay?: number) {
    return this.createTitledPage("Control+Enter", title, expectedCount, typeDelay);
  }

  private async createTitledPage(
    chord: string,
    title: string,
    expectedCount: number,
    typeDelay?: number,
  ) {
    await this.startPage(chord, expectedCount);
    await this.page.keyboard.press("Control+a");
    await this.page.keyboard.type(title, { delay: typeDelay });
    await this.page.keyboard.press("Control+s");
    await expect(this.selectedTitle).toHaveText(title);
    await this.page.keyboard.press("Escape");
    await expect(this.titleInput).not.toBeFocused();
  }

  /** Creates a page and leaves the caret in its title input, settled. */
  private async startPage(chord: string, expectedCount: number) {
    await this.page.keyboard.press(chord);
    await expect(this.rows).toHaveCount(expectedCount);
    // the title input grabs focus via rAF — let it land before the next key, or
    // that key races it and gets eaten by the typing guard
    await expect(this.titleInput).toBeFocused();
    // the async page load then resets the bound value to "Untitled" and
    // collapses the auto-selection — wait for it or typing races the reset
    await expect(this.titleInput).toHaveValue("Untitled");
  }

  /** Ctrl+2 into the body with everything selected, ready to be overtyped. */
  async selectWholeBody() {
    await this.page.keyboard.press("Control+2");
    await expect(this.editorBody).toBeFocused();
    await this.page.keyboard.press("Control+a");
  }

  async setBody(body: string) {
    await this.selectWholeBody();
    await this.page.keyboard.insertText(body);
    await this.page.keyboard.press("Control+s");
    await this.page.keyboard.press("Escape");
    await expect(this.editorBody).not.toBeFocused();
  }

  /** Slower than setBody: lets the UI handle the input, one real keystroke at
   * a time, instead of one bulk insert. Some tests rely on that. */
  async setBodyFromInput(body: string) {
    await this.selectWholeBody();
    await this.page.keyboard.type(body);
    await this.page.keyboard.press("Control+s");
    await this.page.keyboard.press("Escape");
    await expect(this.editorBody).not.toBeFocused();
  }

  async newPageWithBody(title: string, body: string, expectedCount: number) {
    await this.newTitledPage(title, expectedCount);
    await this.setBody(body);
  }

  async newPageWithBodyFromInput(title: string, body: string, expectedCount: number) {
    await this.newTitledPage(title, expectedCount);
    await this.setBodyFromInput(body);
  }

  /** Ctrl+Shift+N, name the section (bare Enter keeps "New Section"); ends in
   * the new section with it current. */
  async newSection(name?: string, typeDelay?: number) {
    await this.page.keyboard.press("Control+Shift+N");
    await expect(this.page.locator(".section-strip input")).toBeFocused();
    if (name) await this.page.keyboard.type(name, { delay: typeDelay });
    await this.page.keyboard.press("Enter");
    await expect(this.sectionName).toContainText(name ?? "New Section");
  }

  /** Opens the page whose tree row carries `title`, in whatever view is up. */
  async openPage(title: string) {
    await this.row(title).click();
    await expect(this.titleInput).toHaveValue(title);
  }

  /** Ctrl+K, run `query`, and wait for the results view to answer. */
  async search(query: string) {
    await this.page.keyboard.press("Control+k");
    await expect(this.page.locator(".search-bar input")).toBeFocused();
    await this.page.keyboard.type(query);
    await this.page.keyboard.press("Enter");
  }

  /** The `?` overlay. The chord is Shift+the base "/" key — `press("?")` alone
   * sends no shiftKey, so the dispatcher never sees the binding. */
  async openHelp() {
    await this.page.keyboard.press("Shift+Slash");
    await expect(this.page.locator(".help-group").first()).toBeVisible();
  }

  /** Ctrl+, → Keybindings → Customize…; ends on the binding list. */
  async openKeybindings() {
    await this.page.keyboard.press("Control+,");
    await this.page.locator("#set-keybindings").click();
    await expect(this.page.locator(".keybind-list")).toBeVisible();
  }

  keybindRow(commandId: string) {
    return this.page.locator(`.keybind-row[data-command="${commandId}"]`);
  }

  /** Captures `chord` onto a command from the open keybindings pane. */
  async rebind(commandId: string, chord: string, shownAs: string) {
    const row = this.keybindRow(commandId);
    await row.locator(".keybind-change").click();
    await this.page.keyboard.press(chord);
    await expect(row.locator("kbd")).toHaveText(shownAs);
  }

  /** Esc out of the settings stack (keybindings → settings → closed). */
  async closeSettings() {
    await this.page.keyboard.press("Escape");
    await this.page.keyboard.press("Escape");
    await expect(this.modalBackdrop).toHaveCount(0);
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
    await expect(this.modalBackdrop).toHaveCount(0);
  }
}

/** Whether the machine running the suite has a usable `git` on PATH — the
 * same gate `git.rs::available()` applies, checked from the test side so
 * history specs can skip cleanly instead of failing on a disabled checkbox. */
export function hasGit(): boolean {
  try {
    execSync("git --version", { stdio: "ignore", windowsHide: true });
    return true;
  } catch {
    return false;
  }
}

export const NO_GIT_REASON = "git not found on PATH — history is inert without it";

type Exit = { code: number | null; signal: NodeJS.Signals | null };

function quietly(cmd: string) {
  try {
    execSync(cmd, {
      timeout: 30_000,
      stdio: ["ignore", "ignore", "ignore"],
      windowsHide: true,
    });
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

/** Names the process listening on the CDP port, for the "still in use" error. */
function portHolder(): { pid: number; description: string } | null {
  try {
    const script =
      `$owner = @(Get-NetTCPConnection -LocalPort ${CDP_PORT} -State Listen -ErrorAction SilentlyContinue)[0].OwningProcess; ` +
      `if ($owner) { $p = Get-CimInstance Win32_Process -Filter "ProcessId=$owner" -ErrorAction SilentlyContinue; ` +
      `$par = @(Get-CimInstance Win32_Process -Filter "ProcessId=$($p.ParentProcessId)" -ErrorAction SilentlyContinue)[0]; ` +
      `"$owner|$($p.Name)|$($par.Name)" }; exit 0`;
    const out = execFileSync("powershell", ["-NoProfile", "-Command", script], {
      encoding: "utf8",
      timeout: 10_000,
      windowsHide: true,
    }).trim();
    if (!out) return null;
    const [pid, name, parent] = out.split("|");
    return {
      pid: Number(pid),
      description: `${name || "unknown process"} (pid ${pid}${parent ? `, parent ${parent}` : ""})`,
    };
  } catch {
    return null;
  }
}

const PORT_FREE_POLLS = 50;
const PORT_FREE_POLL_MS = 200;

async function waitForPortFree() {
  for (let i = 0; i < PORT_FREE_POLLS; i++) {
    if (!(await portIsOpen())) return;
    await sleep(PORT_FREE_POLL_MS);
  }
  const holder = portHolder();
  throw new Error(
    `CDP port ${CDP_PORT} is still in use by ${holder?.description ?? "an unidentified process"} ` +
      `after ${(PORT_FREE_POLLS * PORT_FREE_POLL_MS) / 1000}s — likely an orphaned ` +
      `WebView2/mynote from a killed run. Close it and retry, e.g.:\n` +
      `  powershell -Command "Stop-Process -Id ${holder?.pid ?? "<pid>"} -Force"`,
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
    const out = execSync(cmd, {
      encoding: "utf8",
      timeout: 10_000,
      windowsHide: true,
    }).trim();
    return out || "<no output>";
  } catch (e) {
    return `<failed to run: ${(e as Error).message}>`;
  }
}

/** WebView2 can come up fully (rendering, writing its profile) while its
 * DevTools port silently fails to bind — Chromium doesn't treat that as
 * fatal. Dump enough state to tell "exe never started" apart from "exe is
 * running but the CDP port is unreachable" the next time this fails. */
function cdpDiagnostics(): string {
  return [
    "--- CDP failure diagnostics ---",
    `mynote.exe processes:\n${runDiagnostic('tasklist /FI "IMAGENAME eq mynote.exe" /V /FO LIST')}`,
    `msedgewebview2.exe processes:\n${runDiagnostic('tasklist /FI "IMAGENAME eq msedgewebview2.exe" /FO LIST')}`,
    `sockets on port ${CDP_PORT}:\n${runDiagnostic(`netstat -ano | findstr :${CDP_PORT}`)}`,
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
    windowsHide: true,
  });
  return out.toLowerCase().includes("mynote.exe");
}

function swapSettingsToScratch(notebookDir: string, overrides: object) {
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
    JSON.stringify({ notebookPath: notebookDir, window: null, logLevel: "verbose", ...overrides }),
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

/** Runs one test against a freshly launched exe over a scratch notebook, with
 * the user's own `settings.json` swapped out and restored around it. */
export async function withScratchApp(
  testInfo: TestInfo,
  use: (app: App) => Promise<void>,
  settingsOverrides: object = {},
) {
  if (!fs.existsSync(EXE)) {
    throw new Error(`missing ${EXE} — run scripts/build.ps1 first`);
  }
  if (mynoteRunning()) {
    throw new Error("MyNote is already running — close it before running the e2e suite");
  }
  const notebookDir = testInfo.outputPath("notebook");
  const dataDir = testInfo.outputPath("wv2-data");
  fs.mkdirSync(notebookDir, { recursive: true });
  swapSettingsToScratch(notebookDir, settingsOverrides);
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
}

export const test = base.extend<{ app: App }>({
  // eslint-disable-next-line no-empty-pattern
  app: ({}, use, testInfo) => withScratchApp(testInfo, use),
});

export { expect };
