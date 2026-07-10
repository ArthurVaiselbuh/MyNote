import { defineConfig } from "@playwright/test";

// The suite drives the real release exe (output\mynote.exe) over CDP; one app
// instance, one debug port, one settings.json — everything must run serially.
export default defineConfig({
  testDir: "./tests",
  fullyParallel: false,
  workers: 1,
  timeout: 60_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
});
