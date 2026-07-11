import { defineConfig } from "@playwright/test";

// Config for the README GIF recorder (record/record.spec.ts) — not part of the
// regression suite. Run with: npx playwright test -c record.config.ts
export default defineConfig({
  testDir: "./record",
  fullyParallel: false,
  workers: 1,
  timeout: 300_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
});
