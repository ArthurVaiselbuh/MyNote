import { defineConfig } from "@playwright/test";

// The suite drives the real release exe (output\mynote.exe) over CDP; one app
// instance, one debug port, one settings.json — everything must run serially.
export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  // a history test relaunches the exe three times, and each launch waits on a
  // real cold start — the budget has to cover that on a loaded CI runner
  timeout: 180_000,
  expect: { timeout: 10_000 },
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI
    ? [["list"], ["github"], ["json", { outputFile: "test-results/results.json" }]]
    : [["list"]],
});
