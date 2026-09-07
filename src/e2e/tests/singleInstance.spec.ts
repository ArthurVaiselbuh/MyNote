import { spawn, execFileSync } from "node:child_process";
import path from "node:path";
import type { Page } from "@playwright/test";
import { expect, test, withScratchApp } from "../app";

const EXE = path.resolve(__dirname, "../../../output/MyNote.exe");

function invoke<T>(page: Page, command: string, args: Record<string, unknown> = {}) {
  return page.evaluate(
    ({ command, args }) =>
      (window as typeof window & {
        __TAURI_INTERNALS__: { invoke<T>(command: string, args: Record<string, unknown>): Promise<T> };
      }).__TAURI_INTERNALS__.invoke<T>(command, args),
    { command, args },
  );
}

async function launchAgain() {
  const child = spawn(EXE, [], { stdio: "ignore", windowsHide: true });
  try {
    await expect.poll(() => child.exitCode, { timeout: 10_000 }).toBe(0);
  } finally {
    if (child.exitCode === null && child.pid) {
      execFileSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], {
        stdio: "ignore", windowsHide: true,
      });
    }
  }
}

async function closeAndReveal(page: Page) {
  await invoke(page, "plugin:window|close", { label: "main" });
  await expect.poll(() => invoke(page, "plugin:window|is_visible", { label: "main" })).toBe(false);
  await launchAgain();
  await expect.poll(() => invoke(page, "plugin:window|is_visible", { label: "main" })).toBe(true);
  if (hasForegroundDesktop()) {
    await expect.poll(() => invoke(page, "plugin:window|is_focused", { label: "main" })).toBe(true);
  } else {
    test.info().annotations.push({ type: "focus", description: "Native focus assertion requires an interactive Windows desktop." });
  }
}

function hasForegroundDesktop() {
  const foregroundWindow = execFileSync("powershell", ["-NoProfile", "-Command", `
Add-Type -TypeDefinition 'using System; using System.Runtime.InteropServices; public class FocusProbe { [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); }';
[FocusProbe]::GetForegroundWindow().ToInt64()
`], { encoding: "utf8", windowsHide: true });
  return foregroundWindow.trim() !== "0";
}

test("single instance applies immediately and can be disabled and re-enabled", async ({ app }) => {
  await app.newTitledPage("Still here", 1);
  await app.page.keyboard.press("Control+,");
  const checkbox = app.page.getByLabel("Single instance", { exact: true });
  await expect(checkbox).not.toBeChecked();
  for (let attempt = 0; attempt < 2; attempt++) {
    await checkbox.check();
    await expect.poll(async () => (await invoke<{ singleInstance: boolean }>(app.page, "get_settings")).singleInstance).toBe(true);
    await closeAndReveal(app.page);
    await expect(app.rowTitles).toHaveText(["Still here"]);
    await checkbox.uncheck();
    await expect.poll(async () => (await invoke<{ singleInstance: boolean }>(app.page, "get_settings")).singleInstance).toBe(false);
  }
});

test("saved tray preference enables single instance on startup", async ({}, testInfo) => {
  await withScratchApp(testInfo, async (app) => {
    try {
      await closeAndReveal(app.page);
      const settings = await invoke<{ singleInstance: boolean; minimizeToTray?: boolean }>(app.page, "get_settings");
      expect(settings.singleInstance).toBe(true);
      expect(settings.minimizeToTray).toBeUndefined();
    } finally {
      const settings = await invoke<Record<string, unknown>>(app.page, "get_settings");
      await invoke(app.page, "set_settings", { settings: { ...settings, singleInstance: false } });
    }
  }, { minimizeToTray: true });
});
